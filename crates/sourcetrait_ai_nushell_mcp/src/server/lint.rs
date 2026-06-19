use crate::*;

// ============================================================================
// Cap
// ============================================================================

/// Maximum number of typed diagnostics emitted before the walker
/// short-circuits. Truthful-but-silent truncation: the agent sees up to this
/// many agent-fixable rows; the cap bounds the list without a `more` marker
/// (the collapsed envelope reports no internal-cap truncation). Cap is small
/// because lint diagnostics are agent-fixable defects; surfacing more than a
/// handful at once overloads the next-step decision.
pub(crate) const LINT_VIOLATION_CAP: usize = 3;

// ============================================================================
// Rule tables
// ============================================================================

/// Path prefixes whose location is contract-fixed; the agent has no
/// alternative and the lint passes these uniformly. `/run/` is intentionally
/// NOT here -- runtime sockets/state live under `$XDG_RUNTIME_DIR` which the
/// agent should lift to args (the_user 2026-05-31).
const ALLOWLIST_PATH_PREFIXES: &[&str] = &["/dev/", "/etc/", "/proc/", "/sys/"];

/// External commands the agent should not invoke. The fix is to use idiomatic
/// nushell or lift the work to typed args/helpers.
const DENYLIST_EXTERNALS: &[&str] = &[
    "awk", "bash", "cp", "date", "echo", "find", "grep", "jq", "ls", "nu", "rg", "rm", "sed", "sh",
    "sort", "which",
];

/// Internal-decl names that accept a `--regex` named flag. Presence of the flag
/// on a call to one of these decls gates skip of the path-rule on positional[0]
/// (the regex pattern). positional[0] only, so a hardcoded path at later
/// positionals (e.g. the replacement arg of `str replace`) still surfaces.
///
/// Audited against `nu-command/src/` at tag 0.113.1: every Signature with
/// `.switch("regex", ...)` or `.named("regex", ...)`. Plugins not covered.
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
/// `use` / `overlay use` / `source` / `source-env`. The path cannot be lifted
/// to args (a `source $args.p` / `use $args.p` is `not_a_constant`), so
/// flagging it is a false positive (the_user 2026-06-14). A RESOLVED
/// `use`/`overlay use` parses to `Expr::ImportPattern` / `Expr::Overlay`
/// (skipped by the walker's catch-all); on module-NOT-found it falls back to a
/// plain `Expr::Call` with the path as positional[0] - this receiver-skip
/// covers that fallback, and source/source-env always.
const PARSE_PATH_RECEIVERS: &[&str] = &["use", "overlay use", "source", "source-env"];

// ============================================================================
// Diagnostic messages (body lint)
// ============================================================================

const MSG_HARDCODED_VARIABLE: &str = "hardcoded path literal; lift it to args";
const MSG_DENIED_COMMAND: &str = "denied external command; use a nushell builtin";

// ============================================================================
// Entry point
// ============================================================================

/// What: lint `body` against the hardcoded-path and denied-external rules and
/// return up to `LINT_VIOLATION_CAP` error-severity `Diagnostic`s. Wraps the
/// body in `def __lint_body [args: <args_type>] { <body> }`, parses via the
/// supplied full-shell `ParseEngine`, locates the def's body block, and walks
/// it. Each diagnostic carries `source: { path: None, position: [line, col] }`
/// (a body diagnostic has no file).
///
/// Why: handler-side lint runs BEFORE template synthesis so violations surface
/// as a typed `Error::LintViolations` (via `error_to_call_result`) instead of
/// as worker-side parse errors. The cap bounds the list silently (no `more`).
///
/// Where: called by `tool::common::lint_run_params` (used by `NuSh::run` /
/// `NuSh::interact`) immediately before template synthesis.
pub(crate) fn lint_body(parse_engine: &ParseEngine, args_type: &str, body: &str) -> Vec<Diagnostic> {
    let (wrapped, prefix_len) = wrap_as_def_body(body, args_type);
    let engine_state = parse_engine.engine_state();
    let mut ws = nu::StateWorkingSet::new(engine_state);
    let outer = nu::parse(&mut ws, Some("body.nu"), wrapped.as_bytes(), false);

    // If the wrapper parse failed badly enough that the def Block didn't
    // materialize, return no diagnostics -- the downstream worker parse will
    // surface the same parse error in a more appropriate channel. Lint reports
    // rules, not parse errors.
    let body_block_id = match find_def_body_id(&outer, &ws) {
        Some(id) => id,
        None => return Vec::new(),
    };

    let mut diagnostics = Vec::new();
    let body_block = ws.get_block(body_block_id);
    let _ = walk_block(body_block, &ws, body, prefix_len, &mut diagnostics);
    diagnostics
}

// ============================================================================
// Internal walkers
// ============================================================================

/// Locate the body `BlockId` of the synthetic
/// `def __lint_body [...] { BODY }` after a successful wrap-and-parse. Returns
/// `None` if the parse left no recognizable `def` call at the top level.
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

/// Push a diagnostic; signal early-return via `ControlFlow::Break(())` when the
/// cap is hit (silent -- the row is simply not added, no `more` marker). The
/// walker chains `?` on each push to short-circuit recursion.
fn push_diagnostic(diagnostics: &mut Vec<Diagnostic>, candidate: Diagnostic) -> ControlFlow<()> {
    if diagnostics.len() >= LINT_VIOLATION_CAP {
        ControlFlow::Break(())
    } else {
        diagnostics.push(candidate);
        ControlFlow::Continue(())
    }
}

fn walk_block(
    block: &nu::Block,
    ws: &nu::StateWorkingSet,
    body: &str,
    prefix_len: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> ControlFlow<()> {
    for p in &block.pipelines {
        for elem in &p.elements {
            walk_expr(&elem.expr, ws, body, prefix_len, diagnostics)?;
        }
    }
    ControlFlow::Continue(())
}

fn walk_expr(
    e: &nu::Expression,
    ws: &nu::StateWorkingSet,
    body: &str,
    prefix_len: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> ControlFlow<()> {
    match &e.expr {
        // Value-shape variants the parser classified as path-likely: the
        // path-rule fires uniformly on the carried literal.
        nu::Expr::Directory(s, _) | nu::Expr::Filepath(s, _) | nu::Expr::GlobPattern(s, _) => {
            check_path(s, e.span.start, body, prefix_len, diagnostics)?;
        }
        // String / RawString in any position: lint-everything-stringy.
        nu::Expr::String(s) | nu::Expr::RawString(s) => {
            check_path(s, e.span.start, body, prefix_len, diagnostics)?;
        }
        // String interpolation: lint each literal part; recurse into dynamic
        // parts.
        nu::Expr::StringInterpolation(parts) => {
            for part in parts {
                walk_expr(part, ws, body, prefix_len, diagnostics)?;
            }
        }
        // Internal call: walk every argument. The regex-receiver skip applies
        // tightly: only positional[0] of String/RawString type gets the skip in
        // a REGEX_RECEIVERS call with `--regex` present.
        nu::Expr::Call(call) => {
            let decl = ws.get_decl(call.decl_id);
            let name = decl.name();
            let regex_skip = REGEX_RECEIVERS.contains(&name)
                && call
                    .arguments
                    .iter()
                    .any(|a| matches!(a, nu::Argument::Named((n, _, _)) if n.item == "regex"));
            // use/overlay use/source/source-env: positional[0] is a
            // parse-time-const path; exempt it (the_user 2026-06-14).
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
                        walk_expr(ae, ws, body, prefix_len, diagnostics)?;
                    }
                    nu::Argument::Named((_n, _s, value)) => {
                        if let Some(ae) = value {
                            walk_expr(ae, ws, body, prefix_len, diagnostics)?;
                        }
                    }
                    nu::Argument::Unknown(ae) | nu::Argument::Spread(ae) => {
                        walk_expr(ae, ws, body, prefix_len, diagnostics)?;
                    }
                }
            }
        }
        // External call: denylist on head (basename-aware so an absolute-path
        // head like `^/usr/bin/awk` doesn't bypass) + walk head through
        // path-rule + recurse into ext args.
        nu::Expr::ExternalCall(head, ext_args) => {
            check_external_head(head, body, prefix_len, diagnostics)?;
            walk_expr(head, ws, body, prefix_len, diagnostics)?;
            for arg in ext_args.iter() {
                let inner = match arg {
                    nu::ExternalArgument::Regular(e) | nu::ExternalArgument::Spread(e) => e,
                };
                walk_expr(inner, ws, body, prefix_len, diagnostics)?;
            }
        }
        // $args.path-style access: walk the head only (path tail members are
        // cell-path keys, not lintable literals).
        nu::Expr::FullCellPath(fcp) => {
            walk_expr(&fcp.head, ws, body, prefix_len, diagnostics)?;
        }
        // Binary op: walk lhs always; walk rhs only when op is not regex.
        nu::Expr::BinaryOp(lhs, op, rhs) => {
            walk_expr(lhs, ws, body, prefix_len, diagnostics)?;
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
                walk_expr(rhs, ws, body, prefix_len, diagnostics)?;
            }
        }
        // Block-like Exprs: descend into the resolved block's pipelines.
        nu::Expr::Block(id)
        | nu::Expr::Closure(id)
        | nu::Expr::Subexpression(id)
        | nu::Expr::RowCondition(id) => {
            let b = ws.get_block(*id);
            walk_block(b, ws, body, prefix_len, diagnostics)?;
        }
        // Unary not: recurse on inner.
        nu::Expr::UnaryNot(inner) => {
            walk_expr(inner, ws, body, prefix_len, diagnostics)?;
        }
        // Collect: wraps an inner Expression with a var binding; the inner is
        // where any literal lives.
        nu::Expr::Collect(_, inner) => {
            walk_expr(inner, ws, body, prefix_len, diagnostics)?;
        }
        // List literal: recurse on every item.
        nu::Expr::List(items) => {
            for item in items {
                match item {
                    nu::ListItem::Item(ae) | nu::ListItem::Spread(_, ae) => {
                        walk_expr(ae, ws, body, prefix_len, diagnostics)?;
                    }
                }
            }
        }
        // Table literal: walk columns + every row's cells.
        nu::Expr::Table(t) => {
            for col in t.columns.iter() {
                walk_expr(col, ws, body, prefix_len, diagnostics)?;
            }
            for row in t.rows.iter() {
                for cell in row.iter() {
                    walk_expr(cell, ws, body, prefix_len, diagnostics)?;
                }
            }
        }
        // Record literal: every Pair has both a key and a value Expression to
        // walk; Spread has one inner Expression.
        nu::Expr::Record(items) => {
            for item in items {
                match item {
                    nu::RecordItem::Pair(k, v) => {
                        walk_expr(k, ws, body, prefix_len, diagnostics)?;
                        walk_expr(v, ws, body, prefix_len, diagnostics)?;
                    }
                    nu::RecordItem::Spread(_, e) => {
                        walk_expr(e, ws, body, prefix_len, diagnostics)?;
                    }
                }
            }
        }
        // Range: walk from / next / to bounds when present.
        nu::Expr::Range(r) => {
            if let Some(e) = &r.from {
                walk_expr(e, ws, body, prefix_len, diagnostics)?;
            }
            if let Some(e) = &r.next {
                walk_expr(e, ws, body, prefix_len, diagnostics)?;
            }
            if let Some(e) = &r.to {
                walk_expr(e, ws, body, prefix_len, diagnostics)?;
            }
        }
        // Match block: each arm is (pattern, body). Walk the body always; walk
        // the pattern's literal-match subexpressions so a path inside
        // `match x { "/foo/bar" => ... }` surfaces.
        nu::Expr::MatchBlock(arms) => {
            for (pat, arm_body) in arms {
                walk_pattern(&pat.pattern, ws, body, prefix_len, diagnostics)?;
                walk_expr(arm_body, ws, body, prefix_len, diagnostics)?;
            }
        }
        // Attribute block: walk every attribute's wrapped expression plus the
        // attribute block's item.
        nu::Expr::AttributeBlock(ab) => {
            for attr in &ab.attributes {
                walk_expr(&attr.expr, ws, body, prefix_len, diagnostics)?;
            }
            walk_expr(&ab.item, ws, body, prefix_len, diagnostics)?;
        }
        // Glob interpolation: same shape as StringInterpolation -- a
        // Vec<Expression> whose literal-String parts can carry path shape.
        nu::Expr::GlobInterpolation(parts, _) => {
            for part in parts {
                walk_expr(part, ws, body, prefix_len, diagnostics)?;
            }
        }
        // Keyword-wrapped expression: recurse on the inner.
        nu::Expr::Keyword(kw) => {
            walk_expr(&kw.expr, ws, body, prefix_len, diagnostics)?;
        }
        // All remaining variants carry no walkable Expression with a literal a
        // path-rule could match (Var, VarDecl, Int, Float, Bool, Binary,
        // Operator, Nothing, Garbage, Signature, ImportPattern, Overlay,
        // CellPath, DateTime, ValueWithUnit). Skip silently.
        _ => {}
    }
    ControlFlow::Continue(())
}

/// Walk a match-arm `Pattern`, recursing into any literal expressions the
/// pattern contains. Most pattern forms carry no walkable expression;
/// `Pattern::Expression` wraps an Expression we should walk; record/list/or
/// patterns nest further MatchPatterns whose `.pattern` field we recurse on.
fn walk_pattern(
    pat: &nu::Pattern,
    ws: &nu::StateWorkingSet,
    body: &str,
    prefix_len: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> ControlFlow<()> {
    match pat {
        nu::Pattern::Expression(e) => {
            walk_expr(e, ws, body, prefix_len, diagnostics)?;
        }
        nu::Pattern::Record(items) => {
            for (_key, sub) in items {
                walk_pattern(&sub.pattern, ws, body, prefix_len, diagnostics)?;
            }
        }
        nu::Pattern::List(subs) | nu::Pattern::Or(subs) => {
            for sub in subs {
                walk_pattern(&sub.pattern, ws, body, prefix_len, diagnostics)?;
            }
        }
        // Value, Variable, Rest, IgnoreRest, IgnoreValue, Garbage: no walkable
        // expression.
        _ => {}
    }
    ControlFlow::Continue(())
}

// ============================================================================
// Rules
// ============================================================================

/// Apply the hardcoded-path rule to a literal `s` originating at byte offset
/// `span_start` within the wrapped source. Push a `lint::hardcoded_variable`
/// error diagnostic (`source: { path: None, position }`) if the rule fires.
/// Returns `Break` to short-circuit once the cap fires.
fn check_path(
    s: &str,
    span_start: usize,
    body: &str,
    prefix_len: usize,
    diagnostics: &mut Vec<Diagnostic>,
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
        let candidate = Diagnostic::error(
            "lint::hardcoded_variable",
            Some(Source {
                path: None,
                position: [line, col],
            }),
            MSG_HARDCODED_VARIABLE,
        );
        return push_diagnostic(diagnostics, candidate);
    }
    ControlFlow::Continue(())
}

/// Apply the denied-external rule to the `head` Expression of an
/// `ExternalCall`. Push a `lint::denied_command` error diagnostic if the head
/// is a literal whose BASENAME (the final `/`-separated segment, so
/// `^/usr/bin/awk` reduces to `awk`) appears in `DENYLIST_EXTERNALS`; skip
/// silently if the head is a variable / cell-path / anything non-literal.
fn check_external_head(
    head: &nu::Expression,
    body: &str,
    prefix_len: usize,
    diagnostics: &mut Vec<Diagnostic>,
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
        let candidate = Diagnostic::error(
            "lint::denied_command",
            Some(Source {
                path: None,
                position: [line, col],
            }),
            MSG_DENIED_COMMAND,
        );
        return push_diagnostic(diagnostics, candidate);
    }
    ControlFlow::Continue(())
}
