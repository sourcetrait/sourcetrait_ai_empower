use crate::*;


pub(crate) const LINT_VIOLATION_CAP: usize = 3;


const ALLOWLIST_PATH_PREFIXES: &[&str] = &["/dev/", "/etc/", "/proc/", "/sys/"];

const DENYLIST_EXTERNALS: &[&str] = &[
    "awk", "bash", "cp", "date", "echo", "find", "grep", "jq", "ls", "nu", "rg", "rm", "sed", "sh",
    "sort", "which",
];

const REGEX_RECEIVERS: &[&str] = &[
    "find",
    "idx search",
    "parse",
    "split column",
    "split list",
    "split row",
    "str replace",
];

const PARSE_PATH_RECEIVERS: &[&str] = &["use", "overlay use", "source", "source-env"];


const MSG_HARDCODED_VARIABLE: &str = "hardcoded path literal; lift it to args";
const MSG_DENIED_COMMAND: &str = "denied external command; use a nushell builtin";


pub(crate) fn lint_body(parse_engine: &ParseEngine, args_type: &str, body: &str) -> Vec<Diagnostic> {
    let (wrapped, prefix_len) = wrap_as_def_body(body, args_type);
    let engine_state = parse_engine.engine_state();
    let mut ws = nu::StateWorkingSet::new(engine_state);
    let outer = nu::parse(&mut ws, Some("body.nu"), wrapped.as_bytes(), false);

    let body_block_id = match find_def_body_id(&outer, &ws) {
        Some(id) => id,
        None => return Vec::new(),
    };

    let mut diagnostics = Vec::new();
    let body_block = ws.get_block(body_block_id);
    let _ = walk_block(body_block, &ws, body, prefix_len, &mut diagnostics);
    diagnostics
}


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
        nu::Expr::Directory(s, _) | nu::Expr::Filepath(s, _) | nu::Expr::GlobPattern(s, _) => {
            check_path(s, e.span.start, body, prefix_len, diagnostics)?;
        }
        nu::Expr::String(s) | nu::Expr::RawString(s) => {
            check_path(s, e.span.start, body, prefix_len, diagnostics)?;
        }
        nu::Expr::StringInterpolation(parts) => {
            for part in parts {
                walk_expr(part, ws, body, prefix_len, diagnostics)?;
            }
        }
        nu::Expr::Call(call) => {
            let decl = ws.get_decl(call.decl_id);
            let name = decl.name();
            let regex_skip = REGEX_RECEIVERS.contains(&name)
                && call
                    .arguments
                    .iter()
                    .any(|a| matches!(a, nu::Argument::Named((n, _, _)) if n.item == "regex"));
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
        nu::Expr::FullCellPath(fcp) => {
            walk_expr(&fcp.head, ws, body, prefix_len, diagnostics)?;
        }
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
        nu::Expr::Block(id)
        | nu::Expr::Closure(id)
        | nu::Expr::Subexpression(id)
        | nu::Expr::RowCondition(id) => {
            let b = ws.get_block(*id);
            walk_block(b, ws, body, prefix_len, diagnostics)?;
        }
        nu::Expr::UnaryNot(inner) => {
            walk_expr(inner, ws, body, prefix_len, diagnostics)?;
        }
        nu::Expr::Collect(_, inner) => {
            walk_expr(inner, ws, body, prefix_len, diagnostics)?;
        }
        nu::Expr::List(items) => {
            for item in items {
                match item {
                    nu::ListItem::Item(ae) | nu::ListItem::Spread(_, ae) => {
                        walk_expr(ae, ws, body, prefix_len, diagnostics)?;
                    }
                }
            }
        }
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
        nu::Expr::MatchBlock(arms) => {
            for (pat, arm_body) in arms {
                walk_pattern(&pat.pattern, ws, body, prefix_len, diagnostics)?;
                walk_expr(arm_body, ws, body, prefix_len, diagnostics)?;
            }
        }
        nu::Expr::AttributeBlock(ab) => {
            for attr in &ab.attributes {
                walk_expr(&attr.expr, ws, body, prefix_len, diagnostics)?;
            }
            walk_expr(&ab.item, ws, body, prefix_len, diagnostics)?;
        }
        nu::Expr::GlobInterpolation(parts, _) => {
            for part in parts {
                walk_expr(part, ws, body, prefix_len, diagnostics)?;
            }
        }
        nu::Expr::Keyword(kw) => {
            walk_expr(&kw.expr, ws, body, prefix_len, diagnostics)?;
        }
        _ => {}
    }
    ControlFlow::Continue(())
}

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
        _ => {}
    }
    ControlFlow::Continue(())
}


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
    if trimmed.contains("://") {
        return ControlFlow::Continue(());
    }
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
