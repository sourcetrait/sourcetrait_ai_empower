use crate::*;

// ============================================================================
// LintViolation + cap
// ============================================================================

/// Maximum number of typed lint violations emitted before the walker
/// short-circuits with a `LintViolation::More` sentinel. Truthful
/// truncation: agent sees up to 3 typed violations + an explicit
/// "there were more" sentinel, never a misleading impression of
/// exhaustive coverage. Cap is small because lint violations are
/// agent-fixable defects; surfacing more than a handful at once
/// overloads the next-step decision rather than informing it.
pub(crate) const LINT_VIOLATION_CAP: usize = 3;

/// What: a single agent-facing lint violation. Tagged enum with a
/// `kind` discriminator (`hardcoded_variable` / `denied_command` /
/// `more`) and per-variant `position` + optional `source` fields
/// inlined into the wire shape. `More` is a sentinel signaling
/// truncation -- the walker appends it once the cap fires.
///
/// Why: typed envelope per the_user 2026-06-02 -- agents branch on
/// `kind` (a string enum) rather than parsing rendered text. Plain
/// inline `position` + `source` fields (rather than nesting under a
/// `Where` struct) keep the wire terse and the JsonSchema simple.
/// `Where` is the Rust-side ergonomic carrier built by the walker;
/// the wire shape is what this enum derives.
///
/// Where: emitted by `lint_body` / `lint_block` walkers. Carried by
/// `Error::LintViolations { violations }` (handler-side body lint)
/// and `Error::LibraryViolations { structural, lint }` (library
/// validator).
#[derive(Debug, Clone, ser::Serialize, schema::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LintViolation {
    HardcodedVariable {
        position: [usize; 2],
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<WhereSource>,
    },
    DeniedCommand {
        position: [usize; 2],
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<WhereSource>,
    },
    /// leg 4: a node's summary line (the doc one-liner above `export def
    /// main`, or a mod.nu leading comment) exceeds the 80-char cap.
    /// Emitted by the library validator only; `source` is the file
    /// (`WhereSource::Mod(rel_path)`), `position` the summary line.
    SummaryLength {
        position: [usize; 2],
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<WhereSource>,
    },
    More,
}

impl LintViolation {
    /// What: construct `HardcodedVariable` from a `Where` value.
    /// Convenience that bridges the walker's `Where` carrier to the
    /// wire-shape enum variant (flattening the carrier's fields).
    ///
    /// Why: walker code wants to pass a single `Where` value through
    /// the recursion; the variant has its fields inlined for terser
    /// wire output. This bridge keeps both readable.
    ///
    /// Where: called by `check_path` whenever the hardcoded-path
    /// rule fires.
    pub(crate) fn hardcoded_variable(w: Where) -> Self {
        Self::HardcodedVariable {
            position: w.position,
            source: w.source,
        }
    }

    /// What: construct `DeniedCommand` from a `Where` value.
    /// Mirror of `hardcoded_variable`.
    ///
    /// Why: same rationale.
    ///
    /// Where: called by `check_external_head` whenever the denylist
    /// rule fires.
    pub(crate) fn denied_command(w: Where) -> Self {
        Self::DeniedCommand {
            position: w.position,
            source: w.source,
        }
    }

    /// What: construct `SummaryLength` from a `Where` value (leg 4).
    /// Mirror of `hardcoded_variable` for an over-long doc summary.
    ///
    /// Why: the library validator emits this when a node's summary (doc
    /// one-liner) exceeds the 80-char cap; same `{position, source}`
    /// carrier as the other lint violations.
    ///
    /// Where: called by `library::validate_function_file_ast` /
    /// `validate_mod_nu_ast` after extracting the node's doc.
    pub(crate) fn summary_length(w: Where) -> Self {
        Self::SummaryLength {
            position: w.position,
            source: w.source,
        }
    }
}

// ============================================================================
// Rule tables
// ============================================================================

/// Path prefixes whose location is contract-fixed; the agent has no
/// alternative and the lint should pass these uniformly. `/run/` is
/// intentionally NOT here -- runtime sockets/state live under
/// `$XDG_RUNTIME_DIR` which the agent should lift to args (the_user
/// 2026-05-31).
const ALLOWLIST_PATH_PREFIXES: &[&str] = &["/dev/", "/etc/", "/proc/", "/sys/"];

/// External commands the agent should not invoke. The fix is to use
/// idiomatic nushell or lift the work to typed args/helpers.
const DENYLIST_EXTERNALS: &[&str] = &[
    "awk", "bash", "cp", "date", "echo", "find", "grep", "jq", "ls", "nu", "rg", "rm", "sed", "sh",
    "sort", "which",
];

/// Internal-decl names that accept a `--regex` named flag. Presence of
/// the flag on a call to one of these decls gates skip of the path-rule
/// on positional[0] (the regex pattern). Slice 5.5 tightened to
/// positional[0] only so a hardcoded path at later positionals (e.g.
/// the replacement arg of `str replace`) still surfaces.
///
/// Slice 5.6 audit of `nu-command/src/` at tag 0.113.1 confirmed the
/// canonical list: every Signature with `.switch("regex", ...)` or
/// `.named("regex", ...)`. Plugins not covered (probe per-plugin if
/// adding plugin-aware lint surfaces).
const REGEX_RECEIVERS: &[&str] = &[
    "find",
    "idx search",
    "parse",
    "split column",
    "split list",
    "split row",
    "str replace",
];

/// Decls whose positional[0] is a parse-time-const file/module path -
/// `use` / `overlay use` / `source` / `source-env`. The path cannot be
/// lifted to args (a `source $args.p` / `use $args.p` is
/// `not_a_constant`), so flagging it is a false positive (the_user
/// 2026-06-14). A RESOLVED `use`/`overlay use` parses to
/// `Expr::ImportPattern` / `Expr::Overlay` (skipped by the walker's
/// catch-all); but on module-NOT-found it falls back to a plain
/// `Expr::Call` with the path as positional[0] (parse_keywords.rs
/// parse_use: import-pattern Nothing -> `Expr::Call(call)`) - this
/// receiver-skip covers that fallback, and source/source-env always.
const PARSE_PATH_RECEIVERS: &[&str] = &["use", "overlay use", "source", "source-env"];

// ============================================================================
// Entry points
// ============================================================================

/// What: lint `body` against the hardcoded-path and denied-external
/// rules and return up to `LINT_VIOLATION_CAP` typed violations plus
/// a `LintViolation::More` sentinel if the body had more. Wraps the
/// body in `def __lint_body [args: record<args_schema>] { <body> }`,
/// parses via the supplied full-shell `ParseEngine`, locates the
/// def's body block, and walks it.
///
/// Why: handler-side lint runs BEFORE template synthesis so
/// violations surface as a typed `Error::LintViolations` (via
/// `error_to_call_result`) instead of as worker-side parse errors.
/// Cap + sentinel per the_user 2026-06-02 -- truthful truncation.
///
/// Where: called by `server::tool::NuSh::run` /
/// `NuSh::interact` immediately before template synthesis.
pub(crate) fn lint_body(
    parse_engine: &ParseEngine,
    args_type: &str,
    body: &str,
    source: Option<WhereSource>,
) -> Vec<LintViolation> {
    let (wrapped, prefix_len) = wrap_as_def_body(body, args_type);
    let engine_state = parse_engine.engine_state();
    let mut ws = nu::StateWorkingSet::new(engine_state);
    let outer = nu::parse(&mut ws, Some("body.nu"), wrapped.as_bytes(), false);

    // If the wrapper parse failed badly enough that the def Block didn't
    // materialize, return no lint violations -- the downstream worker
    // parse will surface the same parse error to the agent in a more
    // appropriate channel. Lint reports rules, not parse errors.
    let body_block_id = match find_def_body_id(&outer, &ws) {
        Some(id) => id,
        None => return Vec::new(),
    };

    let mut violations = Vec::new();
    let body_block = ws.get_block(body_block_id);
    let _ = walk_block(
        body_block,
        &ws,
        body,
        prefix_len,
        source.as_ref(),
        &mut violations,
    );
    violations
}

// ============================================================================
// Internal walkers
// ============================================================================

/// Locate the body `BlockId` of the synthetic
/// `def __lint_body [...] { BODY }` after a successful wrap-and-parse.
/// Returns `None` if the parse left no recognizable `def` call at the
/// top level.
fn find_def_body_id(outer: &nu::Block, ws: &nu::StateWorkingSet) -> Option<nu::BlockId> {
    for p in &outer.pipelines {
        for elem in &p.elements {
            if let nu::Expr::Call(call) = &elem.expr.expr {
                let decl = ws.get_decl(call.decl_id);
                if decl.name() == "def" {
                    for arg in &call.arguments {
                        let inner = match arg {
                            nu::Argument::Positional(e) => e,
                            _ => continue,
                        };
                        match &inner.expr {
                            nu::Expr::Block(id) | nu::Expr::Closure(id) => {
                                return Some(*id);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    None
}

/// Push a violation or the `More` sentinel; signal early-return via
/// `ControlFlow::Break(())` when the cap is hit. The walker chains `?`
/// on each push to short-circuit recursion.
fn push_violation(
    violations: &mut Vec<LintViolation>,
    candidate: LintViolation,
) -> ControlFlow<()> {
    if violations.len() >= LINT_VIOLATION_CAP {
        violations.push(LintViolation::More);
        ControlFlow::Break(())
    } else {
        violations.push(candidate);
        ControlFlow::Continue(())
    }
}

fn walk_block(
    block: &nu::Block,
    ws: &nu::StateWorkingSet,
    body: &str,
    prefix_len: usize,
    source: Option<&WhereSource>,
    violations: &mut Vec<LintViolation>,
) -> ControlFlow<()> {
    for p in &block.pipelines {
        for elem in &p.elements {
            walk_expr(&elem.expr, ws, body, prefix_len, source, violations)?;
        }
    }
    ControlFlow::Continue(())
}

fn walk_expr(
    e: &nu::Expression,
    ws: &nu::StateWorkingSet,
    body: &str,
    prefix_len: usize,
    source: Option<&WhereSource>,
    violations: &mut Vec<LintViolation>,
) -> ControlFlow<()> {
    match &e.expr {
        // Value-shape variants the parser classified as path-likely: the
        // path-rule fires uniformly on the carried literal.
        nu::Expr::Directory(s, _) | nu::Expr::Filepath(s, _) | nu::Expr::GlobPattern(s, _) => {
            check_path(s, e.span.start, body, prefix_len, source, violations)?;
        }
        // String / RawString in any position: lint-everything-stringy.
        nu::Expr::String(s) | nu::Expr::RawString(s) => {
            check_path(s, e.span.start, body, prefix_len, source, violations)?;
        }
        // String interpolation: lint each literal part; recurse into
        // dynamic parts.
        nu::Expr::StringInterpolation(parts) => {
            for part in parts {
                walk_expr(part, ws, body, prefix_len, source, violations)?;
            }
        }
        // Internal call: walk every argument. The regex-receiver skip
        // applies tightly: only positional[0] of String/RawString type
        // gets the skip in a REGEX_RECEIVERS call with `--regex` present.
        nu::Expr::Call(call) => {
            let decl = ws.get_decl(call.decl_id);
            let name = decl.name();
            let regex_skip = REGEX_RECEIVERS.contains(&name)
                && call
                    .arguments
                    .iter()
                    .any(|a| matches!(a, nu::Argument::Named((n, _, _)) if n.item == "regex"));
            // use/overlay use/source/source-env: positional[0] is a
            // parse-time-const path; exempt it (the_user 2026-06-14). A
            // resolved use/overlay is Expr::ImportPattern/Overlay
            // (catch-all skip); the module-not-found fallback is a Call.
            let parse_path_skip = PARSE_PATH_RECEIVERS.contains(&name);
            let mut positional_idx = 0usize;
            for arg in &call.arguments {
                match arg {
                    nu::Argument::Positional(ae) => {
                        let skip_this = positional_idx == 0
                            && (parse_path_skip
                                || (regex_skip
                                    && matches!(
                                        ae.expr,
                                        nu::Expr::String(_) | nu::Expr::RawString(_),
                                    )));
                        positional_idx += 1;
                        if skip_this {
                            continue;
                        }
                        walk_expr(ae, ws, body, prefix_len, source, violations)?;
                    }
                    nu::Argument::Named((_n, _s, value)) => {
                        if let Some(ae) = value {
                            walk_expr(ae, ws, body, prefix_len, source, violations)?;
                        }
                    }
                    nu::Argument::Unknown(ae) | nu::Argument::Spread(ae) => {
                        walk_expr(ae, ws, body, prefix_len, source, violations)?;
                    }
                }
            }
        }
        // External call: denylist on head (basename-aware so an
        // absolute-path head like `^/usr/bin/awk` doesn't bypass) +
        // walk head through path-rule + recurse into ext args.
        nu::Expr::ExternalCall(head, ext_args) => {
            check_external_head(head, body, prefix_len, source, violations)?;
            walk_expr(head, ws, body, prefix_len, source, violations)?;
            for arg in ext_args.iter() {
                let inner = match arg {
                    nu::ExternalArgument::Regular(e) | nu::ExternalArgument::Spread(e) => e,
                };
                walk_expr(inner, ws, body, prefix_len, source, violations)?;
            }
        }
        // $args.path-style access: walk the head only (path tail members
        // are cell-path keys, not lintable literals).
        nu::Expr::FullCellPath(fcp) => {
            walk_expr(&fcp.head, ws, body, prefix_len, source, violations)?;
        }
        // Binary op: walk lhs always; walk rhs only when op is not regex.
        nu::Expr::BinaryOp(lhs, op, rhs) => {
            walk_expr(lhs, ws, body, prefix_len, source, violations)?;
            let is_regex_op = if let nu::Expr::Operator(operator) = &op.expr {
                matches!(
                    operator,
                    nu::Operator::Comparison(nu::Comparison::RegexMatch)
                        | nu::Operator::Comparison(nu::Comparison::NotRegexMatch),
                )
            } else {
                false
            };
            if !is_regex_op {
                walk_expr(rhs, ws, body, prefix_len, source, violations)?;
            }
        }
        // Block-like Exprs: descend into the resolved block's pipelines.
        nu::Expr::Block(id)
        | nu::Expr::Closure(id)
        | nu::Expr::Subexpression(id)
        | nu::Expr::RowCondition(id) => {
            let b = ws.get_block(*id);
            walk_block(b, ws, body, prefix_len, source, violations)?;
        }
        // Unary not: recurse on inner.
        nu::Expr::UnaryNot(inner) => {
            walk_expr(inner, ws, body, prefix_len, source, violations)?;
        }
        // Collect: wraps an inner Expression with a var binding; the
        // inner is where any literal lives.
        nu::Expr::Collect(_, inner) => {
            walk_expr(inner, ws, body, prefix_len, source, violations)?;
        }
        // List literal: recurse on every item.
        nu::Expr::List(items) => {
            for item in items {
                match item {
                    nu::ListItem::Item(ae) | nu::ListItem::Spread(_, ae) => {
                        walk_expr(ae, ws, body, prefix_len, source, violations)?;
                    }
                }
            }
        }
        // Table literal: walk columns + every row's cells.
        nu::Expr::Table(t) => {
            for col in t.columns.iter() {
                walk_expr(col, ws, body, prefix_len, source, violations)?;
            }
            for row in t.rows.iter() {
                for cell in row.iter() {
                    walk_expr(cell, ws, body, prefix_len, source, violations)?;
                }
            }
        }
        // Record literal: every Pair has both a key and a value
        // Expression to walk; Spread has one inner Expression.
        nu::Expr::Record(items) => {
            for item in items {
                match item {
                    nu::RecordItem::Pair(k, v) => {
                        walk_expr(k, ws, body, prefix_len, source, violations)?;
                        walk_expr(v, ws, body, prefix_len, source, violations)?;
                    }
                    nu::RecordItem::Spread(_, e) => {
                        walk_expr(e, ws, body, prefix_len, source, violations)?;
                    }
                }
            }
        }
        // Range: walk from / next / to bounds when present.
        nu::Expr::Range(r) => {
            if let Some(e) = &r.from {
                walk_expr(e, ws, body, prefix_len, source, violations)?;
            }
            if let Some(e) = &r.next {
                walk_expr(e, ws, body, prefix_len, source, violations)?;
            }
            if let Some(e) = &r.to {
                walk_expr(e, ws, body, prefix_len, source, violations)?;
            }
        }
        // Match block: each arm is (pattern, body). Walk the body
        // always; walk the pattern's literal-match subexpressions via
        // walk_pattern so a path inside `match x { "/foo/bar" => ... }`
        // surfaces.
        nu::Expr::MatchBlock(arms) => {
            for (pat, arm_body) in arms {
                walk_pattern(&pat.pattern, ws, body, prefix_len, source, violations)?;
                walk_expr(arm_body, ws, body, prefix_len, source, violations)?;
            }
        }
        // Attribute block: walk every attribute's wrapped expression
        // plus the attribute block's item.
        nu::Expr::AttributeBlock(ab) => {
            for attr in &ab.attributes {
                walk_expr(&attr.expr, ws, body, prefix_len, source, violations)?;
            }
            walk_expr(&ab.item, ws, body, prefix_len, source, violations)?;
        }
        // Glob interpolation: same shape as StringInterpolation -- a
        // Vec<Expression> whose literal-String parts can carry path
        // shape.
        nu::Expr::GlobInterpolation(parts, _) => {
            for part in parts {
                walk_expr(part, ws, body, prefix_len, source, violations)?;
            }
        }
        // Keyword-wrapped expression: recurse on the inner.
        nu::Expr::Keyword(kw) => {
            walk_expr(&kw.expr, ws, body, prefix_len, source, violations)?;
        }
        // All remaining variants carry no walkable Expression with a
        // literal a path-rule could match (Var, VarDecl, Int, Float,
        // Bool, Binary, Operator, Nothing, Garbage, Signature,
        // ImportPattern, Overlay, CellPath, DateTime, ValueWithUnit).
        // Skip silently.
        _ => {}
    }
    ControlFlow::Continue(())
}

/// Walk a match-arm `Pattern`, recursing into any literal expressions
/// the pattern contains. Most pattern forms (Variable, IgnoreValue,
/// Garbage, etc.) carry no walkable expression; `Pattern::Expression`
/// wraps an Expression we should walk; record/list/or patterns nest
/// further MatchPatterns whose `.pattern` field we recurse on.
fn walk_pattern(
    pat: &nu::Pattern,
    ws: &nu::StateWorkingSet,
    body: &str,
    prefix_len: usize,
    source: Option<&WhereSource>,
    violations: &mut Vec<LintViolation>,
) -> ControlFlow<()> {
    match pat {
        nu::Pattern::Expression(e) => {
            walk_expr(e, ws, body, prefix_len, source, violations)?;
        }
        nu::Pattern::Record(items) => {
            for (_key, sub) in items {
                walk_pattern(&sub.pattern, ws, body, prefix_len, source, violations)?;
            }
        }
        nu::Pattern::List(subs) | nu::Pattern::Or(subs) => {
            for sub in subs {
                walk_pattern(&sub.pattern, ws, body, prefix_len, source, violations)?;
            }
        }
        // Value, Variable, Rest, IgnoreRest, IgnoreValue, Garbage:
        // no walkable expression.
        _ => {}
    }
    ControlFlow::Continue(())
}

// ============================================================================
// Rules
// ============================================================================

/// Apply the hardcoded-path rule to a literal `s` originating at byte
/// offset `span_start` within the wrapped source. Push a
/// `HardcodedVariable` violation (or the `More` sentinel) if the rule
/// fires. Returns `Break` to short-circuit further walking once the
/// cap fires.
fn check_path(
    s: &str,
    span_start: usize,
    body: &str,
    prefix_len: usize,
    source: Option<&WhereSource>,
    violations: &mut Vec<LintViolation>,
) -> ControlFlow<()> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return ControlFlow::Continue(());
    }
    // URLs share path-shape but are NOT filesystem paths.
    if trimmed.contains("://") {
        return ControlFlow::Continue(());
    }
    // System paths whose location IS the contract pass uniformly.
    for prefix in ALLOWLIST_PATH_PREFIXES {
        if trimmed.starts_with(prefix) {
            return ControlFlow::Continue(());
        }
    }
    let flag = trimmed.starts_with('/')
        || trimmed.starts_with("~/")
        || trimmed.starts_with("./")
        || trimmed.starts_with("../");
    if flag {
        let body_offset = span_start.saturating_sub(prefix_len);
        let (line, col) = span_to_line_col(body, body_offset);
        let candidate = LintViolation::hardcoded_variable(Where {
            position: [line, col],
            source: source.cloned(),
        });
        return push_violation(violations, candidate);
    }
    ControlFlow::Continue(())
}

/// Apply the denied-external rule to the `head` Expression of an
/// `ExternalCall`. Push a `DeniedCommand` violation (or the `More`
/// sentinel) if the head is a literal whose BASENAME (the final
/// `/`-separated segment, so `^/usr/bin/awk` reduces to `awk`)
/// appears in `DENYLIST_EXTERNALS`; skip silently if the head is a
/// variable / cell-path / anything non-literal.
fn check_external_head(
    head: &nu::Expression,
    body: &str,
    prefix_len: usize,
    source: Option<&WhereSource>,
    violations: &mut Vec<LintViolation>,
) -> ControlFlow<()> {
    let name = match &head.expr {
        nu::Expr::GlobPattern(s, _)
        | nu::Expr::String(s)
        | nu::Expr::RawString(s)
        | nu::Expr::Filepath(s, _)
        | nu::Expr::Directory(s, _) => s.as_str(),
        _ => return ControlFlow::Continue(()),
    };
    let basename = name.rsplit('/').next().unwrap_or(name);
    if DENYLIST_EXTERNALS.contains(&basename) {
        let body_offset = head.span.start.saturating_sub(prefix_len);
        let (line, col) = span_to_line_col(body, body_offset);
        let candidate = LintViolation::denied_command(Where {
            position: [line, col],
            source: source.cloned(),
        });
        return push_violation(violations, candidate);
    }
    ControlFlow::Continue(())
}

// ============================================================================
// Inline tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> ParseEngine {
        ParseEngine::new_full()
    }

    fn lint(body: &str) -> Vec<LintViolation> {
        lint_body(&engine(), "record<noop: int>", body, None)
    }

    fn lint_args(args_schema: &str, body: &str) -> Vec<LintViolation> {
        lint_body(&engine(), &format!("record<{args_schema}>"), body, None)
    }

    fn kinds(v: &[LintViolation]) -> Vec<&'static str> {
        v.iter()
            .map(|x| match x {
                LintViolation::HardcodedVariable { .. } => "hardcoded_variable",
                LintViolation::DeniedCommand { .. } => "denied_command",
                LintViolation::SummaryLength { .. } => "summary_length",
                LintViolation::More => "more",
            })
            .collect()
    }

    #[test]
    fn passes_clean_record_return() {
        let v = lint("{ out: ($args.noop + 1) }");
        assert!(v.is_empty(), "expected no violations, got {v:?}");
    }

    #[test]
    fn flags_quoted_abs_path() {
        let v = lint("cd \"/home/box/proj/x\"; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn flags_bare_abs_path() {
        let v = lint("cd /home/box/proj/x; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn passes_allowlist_etc() {
        assert!(lint("cd /etc/foo; { out: 0 }").is_empty());
        assert!(lint("cd \"/etc/foo\"; { out: 0 }").is_empty());
    }

    #[test]
    fn passes_allowlist_dev_proc() {
        assert!(lint("cd /dev/null; { out: 0 }").is_empty());
        assert!(lint("cd \"/proc/self\"; { out: 0 }").is_empty());
    }

    #[test]
    fn flags_tilde_path() {
        let v = lint("cd ~/proj/x; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn flags_relative_path() {
        let v = lint("cd ./relative/x; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn passes_var_path() {
        let v = lint_args("path: string", "cd $args.path; { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn passes_url() {
        let v = lint("http get \"https://api.example.com\"; { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn flags_denied_external() {
        let v = lint("^awk '{print $1}'; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["denied_command"]);
    }

    #[test]
    fn passes_allowed_external() {
        let v = lint("^git status; { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn passes_var_headed_external() {
        let v = lint_args("cmd: string", "^$args.cmd status; { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn passes_regex_op_match() {
        let v = lint("if (\"abc\" =~ '/foo/bar/') { { out: 1 } } else { { out: 0 } }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn passes_regex_op_nomatch() {
        let v = lint("if (\"abc\" !~ '/foo/bar/') { { out: 1 } } else { { out: 0 } }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn passes_regex_named_flag_str_replace() {
        let v = lint("let x = (\"abc\" | str replace --regex '/x/y/' ''); { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn passes_regex_named_flag_parse() {
        let v = lint("let x = (\"a/b/c\" | parse --regex '(?<g>.+/.+)'); { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn passes_source_path() {
        // `source <path>` takes a parse-time-const path as positional[0]
        // (a `source $args.p` is not_a_constant), so flagging it is a
        // false positive (the_user 2026-06-14). source-not-found is a
        // recoverable parse error - the source Call still reaches the
        // walker - so this exercises the receiver-skip, not a parse bail.
        let v = lint("source \"/home/box/lib/util.nu\"; { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn passes_use_and_overlay_paths() {
        // use / overlay use: a parse-time-const path. On module-not-found
        // (as here) they fall back to a plain Call whose positional[0]
        // path the receiver-skip exempts; a resolved module is an
        // Expr::ImportPattern/Overlay the catch-all skips. Either way the
        // path is not flagged (the_user 2026-06-14).
        let u = lint("use ./helpers/util.nu; { out: 0 }");
        assert!(u.is_empty(), "got {u:?}");
        let o = lint("overlay use ./helpers/util.nu; { out: 0 }");
        assert!(o.is_empty(), "got {o:?}");
    }

    #[test]
    fn passes_bare_glob() {
        let v = lint("ls *.md; { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn passes_division() {
        let v = lint("let x = (1 / 2); { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn flags_nested_def_body() {
        let v = lint("def helper [a] { cd \"/x/y\"; 1 }; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn flags_interpolation_literal_parts() {
        let v = lint("cd $\"/home/($env.USER)/proj/x\"; { out: 0 }");
        assert_eq!(v.len(), 2, "got {v:?}");
        assert!(
            v.iter()
                .all(|x| matches!(x, LintViolation::HardcodedVariable { .. }))
        );
    }

    #[test]
    fn cap_at_three_with_more_sentinel() {
        // Four+ violations -- walker caps at 3 typed + appends More.
        let body = "\
^awk 'x'
cd \"/a/b\"
^grep foo
cd ~/y
{ out: 0 }";
        let v = lint(body);
        assert_eq!(v.len(), 4, "got {v:?}");
        assert_eq!(
            kinds(&v),
            vec![
                "denied_command",
                "hardcoded_variable",
                "denied_command",
                "more",
            ]
        );
    }

    #[test]
    fn under_cap_no_more_sentinel() {
        // Three violations -- right at the cap, no More.
        let body = "\
^awk 'x'
cd \"/a/b\"
^grep foo
{ out: 0 }";
        let v = lint(body);
        assert_eq!(v.len(), 3, "got {v:?}");
        assert!(!matches!(v.last(), Some(LintViolation::More)));
    }

    #[test]
    fn empty_body_passes() {
        let v = lint("");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn record_field_paths_flagged() {
        let v = lint("{ p: \"/home/box/proj\", out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn list_items_walked() {
        // Two paths in a list: both flagged (under cap).
        let v = lint("let xs = [\"/a/b\" \"/c/d\"]; { out: 0 }");
        assert_eq!(v.len(), 2, "got {v:?}");
    }

    #[test]
    fn line_col_within_body() {
        let v = lint("cd \"/a/b\"; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        match &v[0] {
            LintViolation::HardcodedVariable {
                position: [line, col],
                ..
            } => {
                assert_eq!(*line, 1);
                assert_eq!(*col, 4);
            }
            other => panic!("expected HardcodedVariable; got {other:?}"),
        }
    }

    #[test]
    fn flags_single_segment_abs_path() {
        let v = lint("cd \"/tmp\"; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn flags_single_segment_abs_path_bare() {
        let v = lint("cd /tmp; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn allowlist_still_passes_under_strict_rule() {
        let with_slash = lint("cd /etc/; { out: 0 }");
        assert!(with_slash.is_empty(), "got {with_slash:?}");
        let without_slash = lint("cd /etc; { out: 0 }");
        assert_eq!(without_slash.len(), 1, "got {without_slash:?}");
    }

    #[test]
    fn flags_abs_path_external_head_denylist() {
        // ^/usr/bin/awk fires both DeniedCommand (basename = awk) AND
        // HardcodedVariable (absolute path). Both surface independently.
        let v = lint("^/usr/bin/awk 'x'; { out: 0 }");
        let denylist_count = v
            .iter()
            .filter(|x| matches!(x, LintViolation::DeniedCommand { .. }))
            .count();
        let path_count = v
            .iter()
            .filter(|x| matches!(x, LintViolation::HardcodedVariable { .. }))
            .count();
        assert!(denylist_count >= 1, "no denylist fired; got {v:?}");
        assert!(path_count >= 1, "no path fired; got {v:?}");
    }

    #[test]
    fn flags_replacement_in_str_replace_regex() {
        let v =
            lint("let x = (\"abc\" | str replace --regex '/x/' '/replacement/path'); { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn walks_table_cell_paths() {
        let v = lint("[[c1 c2]; [\"/a/b\" 1] [2 \"/c/d\"]]; { out: 0 }");
        assert_eq!(v.len(), 2, "got {v:?}");
        assert!(
            v.iter()
                .all(|x| matches!(x, LintViolation::HardcodedVariable { .. }))
        );
    }

    #[test]
    fn walks_match_arm_body_paths() {
        let v = lint("match $args.noop { 0 => { cd \"/a/b\" } _ => { 0 } }; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn walks_where_row_condition_paths() {
        let v = lint("[{p: \"x\"}] | where p == \"/a/b\"; { out: 0 }");
        assert!(
            v.iter()
                .any(|x| matches!(x, LintViolation::HardcodedVariable { .. })),
            "got {v:?}",
        );
    }

    #[test]
    fn walks_range_bounds() {
        let v = lint("let r = (cd \"/a/b\"; 1)..5; { out: 0 }");
        assert!(
            v.iter()
                .any(|x| matches!(x, LintViolation::HardcodedVariable { .. })),
            "got {v:?}",
        );
    }

    #[test]
    fn passes_allowlist_sys() {
        assert!(lint("cd /sys/class/net; { out: 0 }").is_empty());
        assert!(lint("cd \"/sys/class/net\"; { out: 0 }").is_empty());
    }

    #[test]
    fn flags_run_path() {
        let v = lint("cd /run/foo; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(kinds(&v), vec!["hardcoded_variable"]);
    }

    #[test]
    fn passes_regex_named_flag_split_list() {
        let v = lint("let xs = ([\"a/b\" \"c\"] | split list --regex '/'); { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn flags_split_list_without_regex() {
        let v = lint("let xs = ([\"x\"] | split list \"/a/b\"); { out: 0 }");
        assert!(
            v.iter()
                .any(|x| matches!(x, LintViolation::HardcodedVariable { .. })),
            "got {v:?}",
        );
    }
}
