use crate::*;

// ============================================================================
// Kinds + violation
// ============================================================================

/// What: which lint class fired. The snake_case label returned by
/// `as_str` powers the agent-facing `lint::<class>` token in
/// rendered violations.
///
/// Why: a discrete enum (not a stringly-keyed kind) keeps the walker's
/// match-arms exhaustive against the lint repertoire; adding a new
/// class is one variant + one arm in the snake-form table + the rule
/// that fires it.
///
/// Where: stored on every `LintViolation` produced by `lint_body`; the
/// only consumer is `LintViolation::render` (and the inline tests).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum LintKind {
    HardcodedVariable,
    BlacklistedCommand,
}

impl LintKind {
    /// Snake-form label used after the `lint::` prefix in rendered
    /// violations. Must stay stable -- agents and skill docs key off
    /// these strings.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::HardcodedVariable => "hardcoded_variable",
            Self::BlacklistedCommand => "blacklisted_command",
        }
    }
}

/// What: a single agent-facing lint violation. Carries the kind, the
/// 1-based `(line, col)` of the offending literal within the body
/// source, and an optional `source` tag that disambiguates which body
/// the violation belongs to when aggregating across helpers or files.
///
/// Why: rendered shape is `lint::<class> [<line>:<col>]` (no source)
/// or `lint::<class> [<line>:<col>] <source>` (with source). The
/// bracket-first invariant keeps the line parse-stable; the trailing
/// source is free-form (e.g. `fn double`, `mod math/double.nu`) so
/// kind-tagged identifiers can grow without breaking parsers.
///
/// Where: returned in `Vec<LintViolation>` from `lint_body`. Consumed
/// by `format_lint_violations` and by `server::tool::NuSh::run`'s
/// error mapping path (`ErrorData::invalid_params`).
#[derive(Debug, Clone)]
pub(crate) struct LintViolation {
    pub(crate) kind: LintKind,
    pub(crate) line: usize,
    pub(crate) col: usize,
    pub(crate) source: Option<String>,
}

impl LintViolation {
    /// Render the violation as a single agent-facing line.
    pub(crate) fn render(&self) -> String {
        match &self.source {
            Some(s) => format!(
                "lint::{} [{}:{}] {}",
                self.kind.as_str(),
                self.line,
                self.col,
                s,
            ),
            None => format!("lint::{} [{}:{}]", self.kind.as_str(), self.line, self.col),
        }
    }
}

// ============================================================================
// Rule tables
// ============================================================================

/// Path prefixes whose location is contract-fixed; the agent has no
/// alternative and the lint should pass these uniformly.
const ALLOWLIST_PATH_PREFIXES: &[&str] = &[
    "/dev/",
    "/etc/",
    "/proc/",
];

/// External commands the agent should not invoke. The fix is to use
/// idiomatic nushell or lift the work to typed args/helpers.
const BLACKLIST_EXTERNALS: &[&str] = &[
    "awk",
    "bash",
    "cp",
    "date",
    "echo",
    "find",
    "grep",
    "jq",
    "ls",
    "nu",
    "rg",
    "rm",
    "sed",
    "sh",
    "sort",
    "which",
];

/// Internal-decl names that accept a `--regex` named flag. Presence of
/// the flag on a call to one of these decls gates skip of the path-rule
/// on every positional `String`/`RawString` arg in that call (those are
/// regex pattern or replacement strings, not paths).
const REGEX_RECEIVERS: &[&str] = &[
    "find",
    "parse",
    "split column",
    "split row",
    "str replace",
];

// ============================================================================
// Entry point
// ============================================================================

/// What: lint `body` against the hardcoded-path and blacklisted-external
/// rules and return aggregated violations. Wraps the body in
/// `def __lint_body [args: record<args_schema>] { <body> }`, parses via
/// the supplied full-shell `ParseEngine`, locates the def's body block,
/// and walks it.
///
/// Why: handler-side lint runs BEFORE template synthesis so violations
/// surface as `-32602 invalid_params` instead of as worker-side parse
/// errors; aggregating (rather than first-wins) gives the agent the
/// full picture in one shot, matching the `error`-fence pedagogy in
/// the nu-scripting skill.
///
/// Where: called by `server::tool::NuSh::run` immediately before
/// `template::build_run_source`. Slice 5.2 will broaden the call sites
/// (interact + define_function + import_library + reimport_library);
/// slice 5.3 will iterate over `RunParams.functions` for helper bodies.
pub(crate) fn lint_body(
    parse_engine: &ParseEngine,
    args_schema: &str,
    body: &str,
    source: Option<&str>,
) -> Vec<LintViolation> {
    let (wrapped, prefix_len) = wrap_as_def_body(body, args_schema);
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
    walk_block(body_block, &ws, body, prefix_len, source, &mut violations);
    violations
}

/// Render aggregated violations as one line each, newline-joined, with
/// no header or trailer. Empty input yields the empty string.
pub(crate) fn format_lint_violations(v: &[LintViolation]) -> String {
    v.iter()
        .map(LintViolation::render)
        .collect::<Vec<_>>()
        .join("\n")
}

/// What: lint a pre-parsed body `Block` against the same rules as
/// `lint_body` but without re-parsing. `body_source` is the source text
/// the block was parsed from; `prefix_len` is the byte offset that
/// translates from the parsed source's spans to `body_source`-relative
/// positions (set to the wrap prefix len, 0 when no wrap was used).
///
/// Why: the library validator (slice 5.2) already parses each function
/// file in a `module __v_<stem> { ... }` wrapper; reusing that parse
/// avoids a second pass per file. Same walker, same rules, just a
/// different entry point.
///
/// Where: called by `library::validate_function_file_ast` after the
/// structural shape passes, with `block` = main's body block, source =
/// `Some("mod <rel_path>")`.
pub(crate) fn lint_block(
    block: &nu::Block,
    ws: &nu::StateWorkingSet,
    body_source: &str,
    prefix_len: usize,
    source: Option<&str>,
) -> Vec<LintViolation> {
    let mut violations = Vec::new();
    walk_block(block, ws, body_source, prefix_len, source, &mut violations);
    violations
}

// ============================================================================
// Internal walkers
// ============================================================================

/// Locate the body `BlockId` of the synthetic `def __lint_body [...] { BODY }`
/// after a successful wrap-and-parse. Returns `None` if the parse left no
/// recognizable `def` call at the top level.
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

fn walk_block(
    block: &nu::Block,
    ws: &nu::StateWorkingSet,
    body: &str,
    prefix_len: usize,
    source: Option<&str>,
    violations: &mut Vec<LintViolation>,
) {
    for p in &block.pipelines {
        for elem in &p.elements {
            walk_expr(&elem.expr, ws, body, prefix_len, source, violations);
        }
    }
}

fn walk_expr(
    e: &nu::Expression,
    ws: &nu::StateWorkingSet,
    body: &str,
    prefix_len: usize,
    source: Option<&str>,
    violations: &mut Vec<LintViolation>,
) {
    match &e.expr {
        // Value-shape variants the parser classified as path-likely: the
        // path-rule fires uniformly on the carried literal.
        nu::Expr::Directory(s, _)
        | nu::Expr::Filepath(s, _)
        | nu::Expr::GlobPattern(s, _) => {
            check_path(s, e.span.start, body, prefix_len, source, violations);
        }
        // String / RawString in any position: lint-everything-stringy.
        nu::Expr::String(s) | nu::Expr::RawString(s) => {
            check_path(s, e.span.start, body, prefix_len, source, violations);
        }
        // String interpolation: lint each literal part; recurse into
        // dynamic parts.
        nu::Expr::StringInterpolation(parts) => {
            for part in parts {
                walk_expr(part, ws, body, prefix_len, source, violations);
            }
        }
        // Internal call: walk every argument. The regex-receiver skip
        // applies tightly: only positional[0] of String/RawString type
        // gets the skip in a REGEX_RECEIVERS call with `--regex` present
        // (slice 5.5 tightening; previously skipped ALL positional Strings
        // which over-skipped the replacement arg of `str replace --regex`).
        nu::Expr::Call(call) => {
            let decl = ws.get_decl(call.decl_id);
            let name = decl.name();
            let regex_skip = REGEX_RECEIVERS.contains(&name)
                && call.arguments.iter().any(|a| {
                    matches!(a, nu::Argument::Named((n, _, _)) if n.item == "regex")
                });
            let mut positional_idx = 0usize;
            for arg in &call.arguments {
                match arg {
                    nu::Argument::Positional(ae) => {
                        let skip_this = regex_skip
                            && positional_idx == 0
                            && matches!(
                                ae.expr,
                                nu::Expr::String(_) | nu::Expr::RawString(_),
                            );
                        positional_idx += 1;
                        if skip_this {
                            continue;
                        }
                        walk_expr(ae, ws, body, prefix_len, source, violations);
                    }
                    nu::Argument::Named((_n, _s, value)) => {
                        if let Some(ae) = value {
                            walk_expr(ae, ws, body, prefix_len, source, violations);
                        }
                    }
                    nu::Argument::Unknown(ae) | nu::Argument::Spread(ae) => {
                        walk_expr(ae, ws, body, prefix_len, source, violations);
                    }
                }
            }
        }
        // External call: blacklist on head (basename-aware so an
        // absolute-path head like `^/usr/bin/awk` doesn't bypass) +
        // walk head through path-rule + recurse into ext args.
        nu::Expr::ExternalCall(head, ext_args) => {
            check_external_head(head, body, prefix_len, source, violations);
            walk_expr(head, ws, body, prefix_len, source, violations);
            for arg in ext_args.iter() {
                let inner = match arg {
                    nu::ExternalArgument::Regular(e) | nu::ExternalArgument::Spread(e) => e,
                };
                walk_expr(inner, ws, body, prefix_len, source, violations);
            }
        }
        // $args.path-style access: walk the head only (path tail members
        // are cell-path keys, not lintable literals).
        nu::Expr::FullCellPath(fcp) => {
            walk_expr(&fcp.head, ws, body, prefix_len, source, violations);
        }
        // Binary op: walk lhs always; walk rhs only when op is not regex.
        nu::Expr::BinaryOp(lhs, op, rhs) => {
            walk_expr(lhs, ws, body, prefix_len, source, violations);
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
                walk_expr(rhs, ws, body, prefix_len, source, violations);
            }
        }
        // Block-like Exprs: descend into the resolved block's pipelines.
        // RowCondition (slice 5.5) is also a BlockId.
        nu::Expr::Block(id)
        | nu::Expr::Closure(id)
        | nu::Expr::Subexpression(id)
        | nu::Expr::RowCondition(id) => {
            let b = ws.get_block(*id);
            walk_block(b, ws, body, prefix_len, source, violations);
        }
        // Unary not: recurse on inner.
        nu::Expr::UnaryNot(inner) => {
            walk_expr(inner, ws, body, prefix_len, source, violations);
        }
        // Collect (slice 5.5): wraps an inner Expression with a var
        // binding; the inner is where any literal lives.
        nu::Expr::Collect(_, inner) => {
            walk_expr(inner, ws, body, prefix_len, source, violations);
        }
        // List literal: recurse on every item.
        nu::Expr::List(items) => {
            for item in items {
                match item {
                    nu::ListItem::Item(ae) | nu::ListItem::Spread(_, ae) => {
                        walk_expr(ae, ws, body, prefix_len, source, violations);
                    }
                }
            }
        }
        // Table literal (slice 5.5): walk columns + every row's cells.
        nu::Expr::Table(t) => {
            for col in t.columns.iter() {
                walk_expr(col, ws, body, prefix_len, source, violations);
            }
            for row in t.rows.iter() {
                for cell in row.iter() {
                    walk_expr(cell, ws, body, prefix_len, source, violations);
                }
            }
        }
        // Record literal: every Pair has both a key and a value Expression
        // to walk; Spread has one inner Expression.
        nu::Expr::Record(items) => {
            for item in items {
                match item {
                    nu::RecordItem::Pair(k, v) => {
                        walk_expr(k, ws, body, prefix_len, source, violations);
                        walk_expr(v, ws, body, prefix_len, source, violations);
                    }
                    nu::RecordItem::Spread(_, e) => {
                        walk_expr(e, ws, body, prefix_len, source, violations);
                    }
                }
            }
        }
        // Range (slice 5.5): walk from / next / to bounds when present.
        nu::Expr::Range(r) => {
            if let Some(e) = &r.from {
                walk_expr(e, ws, body, prefix_len, source, violations);
            }
            if let Some(e) = &r.next {
                walk_expr(e, ws, body, prefix_len, source, violations);
            }
            if let Some(e) = &r.to {
                walk_expr(e, ws, body, prefix_len, source, violations);
            }
        }
        // Match block (slice 5.5): each arm is (pattern, body). Walk
        // the body always; walk the pattern's literal-match subexpressions
        // via walk_pattern so a path inside `match x { "/foo/bar" => ... }`
        // surfaces.
        nu::Expr::MatchBlock(arms) => {
            for (pat, arm_body) in arms {
                walk_pattern(&pat.pattern, ws, body, prefix_len, source, violations);
                walk_expr(arm_body, ws, body, prefix_len, source, violations);
            }
        }
        // Attribute block (slice 5.5): walk every attribute's wrapped
        // expression plus the attribute block's item.
        nu::Expr::AttributeBlock(ab) => {
            for attr in &ab.attributes {
                walk_expr(&attr.expr, ws, body, prefix_len, source, violations);
            }
            walk_expr(&ab.item, ws, body, prefix_len, source, violations);
        }
        // Glob interpolation (slice 5.5): same shape as StringInterpolation
        // -- a Vec<Expression> whose literal-String parts can carry path
        // shape.
        nu::Expr::GlobInterpolation(parts, _) => {
            for part in parts {
                walk_expr(part, ws, body, prefix_len, source, violations);
            }
        }
        // Keyword-wrapped expression (some parse-time forms): recurse on
        // the inner Expression.
        nu::Expr::Keyword(kw) => {
            walk_expr(&kw.expr, ws, body, prefix_len, source, violations);
        }
        // All remaining variants carry no walkable Expression with a
        // literal a path-rule could match (Var, VarDecl, Int, Float,
        // Bool, Binary, Operator, Nothing, Garbage, Signature,
        // ImportPattern, Overlay, CellPath, DateTime, ValueWithUnit).
        // Skip silently.
        _ => {}
    }
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
    source: Option<&str>,
    violations: &mut Vec<LintViolation>,
) {
    match pat {
        nu::Pattern::Expression(e) => {
            walk_expr(e, ws, body, prefix_len, source, violations);
        }
        nu::Pattern::Record(items) => {
            for (_key, sub) in items {
                walk_pattern(&sub.pattern, ws, body, prefix_len, source, violations);
            }
        }
        nu::Pattern::List(subs) | nu::Pattern::Or(subs) => {
            for sub in subs {
                walk_pattern(&sub.pattern, ws, body, prefix_len, source, violations);
            }
        }
        // Value, Variable, Rest, IgnoreRest, IgnoreValue, Garbage: no
        // walkable expression.
        _ => {}
    }
}

// ============================================================================
// Rules
// ============================================================================

/// Apply the hardcoded-path rule to a literal `s` originating at byte
/// offset `span_start` within the wrapped source. Push a
/// `HardcodedVariable` violation if the rule fires.
fn check_path(
    s: &str,
    span_start: usize,
    body: &str,
    prefix_len: usize,
    source: Option<&str>,
    violations: &mut Vec<LintViolation>,
) {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return;
    }
    // URLs share path-shape but are NOT filesystem paths.
    if trimmed.contains("://") {
        return;
    }
    // System paths whose location IS the contract pass uniformly.
    for prefix in ALLOWLIST_PATH_PREFIXES {
        if trimmed.starts_with(prefix) {
            return;
        }
    }
    // Slice 5.5 tightening: drop the "requires a second `/`" check.
    // Strict-uniform per `[[empower-correctness-priority]]` rev 2 says
    // `/tmp` should flag exactly as much as `/home/box/proj` does --
    // the legacy second-`/` carve-out is the same shape as the
    // pedagogy-framing slop pattern the_user already cataloged.
    let flag = trimmed.starts_with('/')
        || trimmed.starts_with("~/")
        || trimmed.starts_with("./")
        || trimmed.starts_with("../");
    if flag {
        let body_offset = span_start.saturating_sub(prefix_len);
        let (line, col) = span_to_line_col(body, body_offset);
        violations.push(LintViolation {
            kind: LintKind::HardcodedVariable,
            line,
            col,
            source: source.map(str::to_string),
        });
    }
}

/// Apply the blacklisted-external rule to the `head` Expression of an
/// `ExternalCall`. Push a `BlacklistedCommand` violation if the head
/// is a literal whose BASENAME (the final `/`-separated segment, so
/// `^/usr/bin/awk` reduces to `awk`) appears in `BLACKLIST_EXTERNALS`;
/// skip silently if the head is a variable / cell-path / anything
/// non-literal (those are lifted external heads, the desired pattern).
///
/// Slice 5.5: basename normalization closes the absolute-path bypass
/// surfaced by the slice 5.4 audit (`^/usr/bin/awk` previously slipped
/// the blacklist because the literal name match required the bare
/// command). Path-rule still fires for the absolute head independently
/// via `walk_expr`'s ExternalCall arm.
fn check_external_head(
    head: &nu::Expression,
    body: &str,
    prefix_len: usize,
    source: Option<&str>,
    violations: &mut Vec<LintViolation>,
) {
    let name = match &head.expr {
        nu::Expr::GlobPattern(s, _)
        | nu::Expr::String(s)
        | nu::Expr::RawString(s)
        | nu::Expr::Filepath(s, _)
        | nu::Expr::Directory(s, _) => s.as_str(),
        _ => return,
    };
    let basename = name.rsplit('/').next().unwrap_or(name);
    if BLACKLIST_EXTERNALS.contains(&basename) {
        let body_offset = head.span.start.saturating_sub(prefix_len);
        let (line, col) = span_to_line_col(body, body_offset);
        violations.push(LintViolation {
            kind: LintKind::BlacklistedCommand,
            line,
            col,
            source: source.map(str::to_string),
        });
    }
}

// ============================================================================
// Inline tests -- direct unit coverage of `lint_body` against the slice
// 5.1 case matrix. Integration coverage via `tests/body_lint.rs` exercises
// the rmcp handler wire-up end-to-end.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> ParseEngine {
        ParseEngine::new_full()
    }

    fn lint(body: &str) -> Vec<LintViolation> {
        lint_body(&engine(), "noop: int", body, None)
    }

    fn lint_args(args_schema: &str, body: &str) -> Vec<LintViolation> {
        lint_body(&engine(), args_schema, body, None)
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
        assert_eq!(v[0].kind, LintKind::HardcodedVariable);
    }

    #[test]
    fn flags_bare_abs_path() {
        let v = lint("cd /home/box/proj/x; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(v[0].kind, LintKind::HardcodedVariable);
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
        assert_eq!(v[0].kind, LintKind::HardcodedVariable);
    }

    #[test]
    fn flags_relative_path() {
        let v = lint("cd ./relative/x; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(v[0].kind, LintKind::HardcodedVariable);
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
    fn flags_blacklisted_external() {
        let v = lint("^awk '{print $1}'; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(v[0].kind, LintKind::BlacklistedCommand);
    }

    #[test]
    fn passes_allowed_external() {
        let v = lint("^git status; { out: 0 }");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn passes_var_headed_external() {
        let v = lint_args(
            "cmd: string",
            "^$args.cmd status; { out: 0 }",
        );
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
        // Inner def's body contains a hardcoded path; walker must descend.
        let v = lint("def helper [a] { cd \"/x/y\"; 1 }; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(v[0].kind, LintKind::HardcodedVariable);
    }

    #[test]
    fn flags_interpolation_literal_parts() {
        // Interpolation has literal parts /home/ and /proj which both
        // match the path-rule. Strict-uniform per
        // empower-correctness-priority -- flag both.
        let v = lint("cd $\"/home/($env.USER)/proj/x\"; { out: 0 }");
        assert_eq!(v.len(), 2, "got {v:?}");
        assert!(v.iter().all(|x| x.kind == LintKind::HardcodedVariable));
    }

    #[test]
    fn aggregates_multiple_violations() {
        let body = "\
^awk 'x'
cd \"/a/b\"
^grep foo
cd ~/y
{ out: 0 }";
        let v = lint(body);
        assert_eq!(v.len(), 4, "got {v:?}");
        // First violation -> ^awk on line 1
        assert_eq!(v[0].kind, LintKind::BlacklistedCommand);
        assert_eq!(v[0].line, 1);
        // Second -> cd "/a/b" on line 2
        assert_eq!(v[1].kind, LintKind::HardcodedVariable);
        assert_eq!(v[1].line, 2);
        // Third -> ^grep on line 3
        assert_eq!(v[2].kind, LintKind::BlacklistedCommand);
        assert_eq!(v[2].line, 3);
        // Fourth -> cd ~/y on line 4
        assert_eq!(v[3].kind, LintKind::HardcodedVariable);
        assert_eq!(v[3].line, 4);
    }

    #[test]
    fn render_no_source() {
        let v = LintViolation {
            kind: LintKind::HardcodedVariable,
            line: 3,
            col: 7,
            source: None,
        };
        assert_eq!(v.render(), "lint::hardcoded_variable [3:7]");
    }

    #[test]
    fn render_with_source() {
        let v = LintViolation {
            kind: LintKind::BlacklistedCommand,
            line: 12,
            col: 1,
            source: Some("fn double".to_string()),
        };
        assert_eq!(v.render(), "lint::blacklisted_command [12:1] fn double");
    }

    #[test]
    fn format_aggregates_with_newlines() {
        let vs = vec![
            LintViolation {
                kind: LintKind::HardcodedVariable,
                line: 1,
                col: 4,
                source: None,
            },
            LintViolation {
                kind: LintKind::BlacklistedCommand,
                line: 3,
                col: 1,
                source: None,
            },
        ];
        let s = format_lint_violations(&vs);
        assert_eq!(
            s,
            "lint::hardcoded_variable [1:4]\nlint::blacklisted_command [3:1]",
        );
    }

    #[test]
    fn empty_body_passes() {
        let v = lint("");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn record_field_paths_flagged() {
        // Record values get walked; the path inside the value triggers.
        let v = lint("{ p: \"/home/box/proj\", out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(v[0].kind, LintKind::HardcodedVariable);
    }

    #[test]
    fn list_items_walked() {
        let v = lint("let xs = [\"/a/b\" \"/c/d\"]; { out: 0 }");
        assert_eq!(v.len(), 2, "got {v:?}");
    }

    #[test]
    fn line_col_within_body() {
        // First line is the cd, span_start should land at line 1 col 4
        // (cd<space>) for the Directory value.
        let v = lint("cd \"/a/b\"; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(v[0].line, 1);
        assert_eq!(v[0].col, 4);
    }

    // ----- slice 5.5: stricter rules + widened walker ------------------

    #[test]
    fn flags_single_segment_abs_path() {
        // Slice 5.5: `/tmp`, `/x` previously slipped because the
        // path-rule required a second `/`. Strict-uniform per the
        // pedagogy correction now flags them too.
        let v = lint("cd \"/tmp\"; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(v[0].kind, LintKind::HardcodedVariable);
    }

    #[test]
    fn flags_single_segment_abs_path_bare() {
        let v = lint("cd /tmp; { out: 0 }");
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(v[0].kind, LintKind::HardcodedVariable);
    }

    #[test]
    fn allowlist_still_passes_under_strict_rule() {
        // The new strict rule must NOT regress the allowlist: `/etc`
        // doesn't match the allowlist prefix `/etc/` literally but it
        // also doesn't appear in real-world hardcoded-path usage in a
        // way the the_user wants flagged. Document the current behavior:
        // bare `/etc` (no trailing slash) DOES flag because allowlist
        // requires the trailing slash. Trailing-slash form `/etc/`
        // passes.
        let with_slash = lint("cd /etc/; { out: 0 }");
        assert!(with_slash.is_empty(), "got {with_slash:?}");
        let without_slash = lint("cd /etc; { out: 0 }");
        // Currently flagged because allowlist prefixes include the
        // trailing /. This is a coherent design call (allowlist is
        // about the directory contents, not the root entry).
        assert_eq!(without_slash.len(), 1, "got {without_slash:?}");
    }

    #[test]
    fn flags_abs_path_external_head_blacklist() {
        // Slice 5.5: `^/usr/bin/awk` should fire BlacklistedCommand
        // via basename normalization. The path-rule ALSO fires
        // independently because the head is an absolute path.
        let v = lint("^/usr/bin/awk 'x'; { out: 0 }");
        let blacklist_count = v
            .iter()
            .filter(|x| x.kind == LintKind::BlacklistedCommand)
            .count();
        let path_count = v
            .iter()
            .filter(|x| x.kind == LintKind::HardcodedVariable)
            .count();
        assert!(blacklist_count >= 1, "no blacklist fired; got {v:?}");
        assert!(path_count >= 1, "no path fired; got {v:?}");
    }

    #[test]
    fn flags_replacement_in_str_replace_regex() {
        // Slice 5.5 tightening: the regex skip only covers positional[0]
        // of REGEX_RECEIVERS, so a hardcoded replacement at positional[1]
        // surfaces. The pattern at positional[0] continues to skip.
        let v = lint(
            "let x = (\"abc\" | str replace --regex '/x/' '/replacement/path'); { out: 0 }",
        );
        // The pattern '/x/' is skipped; the replacement '/replacement/path'
        // is flagged.
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(v[0].kind, LintKind::HardcodedVariable);
    }

    #[test]
    fn walks_table_cell_paths() {
        let v = lint("[[c1 c2]; [\"/a/b\" 1] [2 \"/c/d\"]]; { out: 0 }");
        // Two table cells contain hardcoded paths.
        assert_eq!(v.len(), 2, "got {v:?}");
        assert!(v.iter().all(|x| x.kind == LintKind::HardcodedVariable));
    }

    #[test]
    fn walks_match_arm_body_paths() {
        let v = lint(
            "match $args.noop { 0 => { cd \"/a/b\" } _ => { 0 } }; { out: 0 }",
        );
        // The match arm's body contains a hardcoded path.
        assert_eq!(v.len(), 1, "got {v:?}");
        assert_eq!(v[0].kind, LintKind::HardcodedVariable);
    }

    #[test]
    fn walks_where_row_condition_paths() {
        // `where p == "/a/b"` lowers to a RowCondition block; the body
        // block contains a hardcoded path in the comparison's rhs.
        let v = lint(
            "[{p: \"x\"}] | where p == \"/a/b\"; { out: 0 }",
        );
        assert!(
            v.iter().any(|x| x.kind == LintKind::HardcodedVariable),
            "got {v:?}",
        );
    }

    #[test]
    fn walks_range_bounds() {
        // Subexpression at a Range bound: the walker must descend
        // through Range -> Subexpression's Block -> Call("cd") ->
        // Directory("/a/b").
        let v = lint("let r = (cd \"/a/b\"; 1)..5; { out: 0 }");
        assert!(
            v.iter().any(|x| x.kind == LintKind::HardcodedVariable),
            "got {v:?}",
        );
    }
}
