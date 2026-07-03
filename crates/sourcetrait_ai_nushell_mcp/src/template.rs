use crate::*;
use indoc::formatdoc;

/// What: convert the agent-supplied args (`mcp::JsonObject`) into a
/// NUON record-literal string ready for substitution into a nu
/// source template.
///
/// Why: NUON is nushell's native typed-data literal format and is
/// what the worker's parser will consume from the substituted source;
/// converting in Rust avoids round-tripping a JSON string through
/// nushell's own JSON parser at eval time AND aligns with the
/// `mem:sourcetrait-rust-conventions` "NUON is the user-visible wire
/// format for typed Values" convention. JSON happens to be a valid
/// nu record literal for JSON-shaped inputs, but ad-hoc reuse of the
/// JSON serialization is a worse fit for the convention and the
/// performance is worse (string parse work the parser doesn't need
/// to do).
///
/// Where: called (via `args_literal`) by `build_run_source` (the
/// gate-chain `__to_run` call site) and `build_interact_source` (the
/// `let args` RHS) once per template emission.
fn args_to_nuon(args: &mcp::JsonObject) -> String {
    let value = json_object_to_nu_value(args);
    let engine_state = nu::EngineState::new();
    nu::to_nuon(&engine_state, &value, nu::ToNuonConfig::default())
        .unwrap_or_else(|_| "{}".to_string())
}

/// Recursively convert a `serde_json::Map` (the `mcp::JsonObject`
/// shape) into a `nu_protocol::Value::Record`.
fn json_object_to_nu_value(map: &mcp::JsonObject) -> nu::Value {
    let span = nu::Span::unknown();
    let mut record = nu::Record::new();
    for (k, v) in map {
        record.insert(k.clone(), json_value_to_nu_value(v));
    }
    nu::Value::record(record, span)
}

/// Recursively convert a `serde_json::Value` into a
/// `nu_protocol::Value`. Numbers preserve the int/float distinction;
/// non-finite or out-of-i64-range numbers degrade to a string
/// preserving the original token (rare for agent-supplied JSON).
fn json_value_to_nu_value(v: &serde_json::Value) -> nu::Value {
    let span = nu::Span::unknown();
    match v {
        serde_json::Value::Null => nu::Value::nothing(span),
        serde_json::Value::Bool(b) => nu::Value::bool(*b, span),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                nu::Value::int(i, span)
            } else if let Some(f) = n.as_f64() {
                nu::Value::float(f, span)
            } else {
                nu::Value::string(n.to_string(), span)
            }
        }
        serde_json::Value::String(s) => nu::Value::string(s.clone(), span),
        serde_json::Value::Array(arr) => {
            let items: Vec<nu::Value> = arr.iter().map(json_value_to_nu_value).collect();
            nu::Value::list(items, span)
        }
        serde_json::Value::Object(map) => {
            let mut record = nu::Record::new();
            for (k, v) in map {
                record.insert(k.clone(), json_value_to_nu_value(v));
            }
            nu::Value::record(record, span)
        }
    }
}

/// What: the value substituted at the `__run` call site (and the
/// `build_interact_source` `let args` RHS). A void positional (`nothing`)
/// with empty args binds the bare `null`; otherwise the args record as
/// NUON. A void positional with NON-empty args emits the record NUON
/// against a `nothing` parameter and fails the typecheck -- strict void
/// (item 21).
///
/// Where: called by `build_run_source` + `build_interact_source` once per
/// template emission.
fn args_literal(args_type: &str, args: &mcp::JsonObject) -> String {
    if args_type == "nothing" && args.is_empty() {
        "null".to_string()
    } else {
        args_to_nuon(args)
    }
}

/// What: builds the nushell source the stateless worker evals for a
/// `run()` call -- a `do { ... }` block with one infix-signatured def
/// `__run [args: A]: nothing -> R` carrying the agent body, invoked as
/// `__run ARGS_LITERAL`. ARGS_LITERAL is `args_literal` (NUON record, or
/// bare `null` for void args).
///
/// Why: the positional `[args: A]` runtime-enforces the args (missing /
/// wrong-typed rejected, even on any-typed values; verified). The
/// `: nothing -> R` output type documents the result and parse-checks a
/// statically-typed body's result (raises `OutputMismatch`); a dynamic /
/// any-typed result is unchecked and records are open (extra fields pass)
/// -- the accepted best-effort-result scope; strict extras are the
/// deferred record/table seal. The outer `do { ... }` is load-bearing:
/// defs inside it don't persist in the worker's EngineState across calls,
/// the stateless contract.
///
/// Where: called by `server::tool::NuSh::run` and by `rerun` (which
/// rebuilds RunParams from cache and reuses this builder); the rendered
/// source ships through `WorkerHandle::send_request` to a pool worker.
pub(crate) fn build_run_source(
    args_type: &str,
    result_type: &str,
    args: &mcp::JsonObject,
    body: &str,
    nonce: &str,
) -> String {
    let lit = args_literal(args_type, args);
    // $env.NONCE carries this call's nonce ambiently into the body + any
    // helper it invokes (env reads inherit down the call tree). Set inside the
    // `do {}`, before __run, so it is visible to __run yet does not escape:
    // the do-block scopes env, and the stateless per-call clone is dropped
    // after the reply anyway -- it ceases to exist when the call completes.
    formatdoc!(
        r#"
        do {{
            $env.NONCE = "{nonce}"
            def __run [args: {at}]: nothing -> {rt} {{
                {body}
            }}
            __run {lit}
        }}
    "#,
        nonce = nonce,
        at = args_type,
        rt = result_type,
        lit = lit,
        body = body,
    )
}

/// What: builds the nushell source the stateless worker evals for a
/// `call()` -- `use <library>` (the compound `<author>/<name>`) loads the committed library from
/// the author-parented store, then the call is driven module-qualified along its
/// namepath (module_path slashes -> spaces): `<library> <mod...> <name> LIT`.
/// LIT is the args record as NUON, or bare `null` for empty / void args.
/// Invoking the call's dir-module by name runs its `export def main`. (`use
/// a/b` imports the module named `b`, the path leaf, so the drive stays
/// `<library> ...`.)
///
/// Why: the committed call-target is a `<name>/mod.nu` dir-module holding a
/// single infix-signatured `export def main [args: A]: nothing -> R`. Its body
/// self-refs sibling modules by the AUTHORED path -- `use <author>/<library>/<mod>`
/// at the file top, then `<mod> <fn>` -- which the parse-time const `$NU_LIB_DIRS`
/// (the author-parented store root) resolves at the file's own parse, so loading
/// the whole library here makes every target and its self-refs resolvable.
/// `main`'s positional runtime-enforces the args and its output type parse-checks
/// a static result; nushell runs a module's `main` when the module name is
/// invoked, so the module-qualified path is the whole drive.
///
/// Where: called by `server::tool::NuSh::call`; the rendered source ships
/// through a pool worker exactly like run() / rerun().
pub(crate) fn build_call_source(
    library: &str,
    module_path: &str,
    name: &str,
    args: &mcp::JsonObject,
    nonce: &str,
) -> String {
    let lit = if args.is_empty() {
        "null".to_string()
    } else {
        args_to_nuon(args)
    };
    // `library` is the compound `<author>/<name>`; `use <author>/<name>` imports
    // the module named by the LAST path segment, so the drive starts there.
    let library_name = library.rsplit('/').next().unwrap_or(library);
    // Namepath -> the module-qualified invocation, space-separated. A function
    // always has a parent module (no root functions), so module_path is
    // non-empty in practice; the empty branch is defensive.
    let call_path = if module_path.is_empty() {
        format!("{library_name} {name}")
    } else {
        format!("{library_name} {} {name}", module_path.replace('/', " "))
    };
    // $env.NONCE carries this call's nonce ambiently into the committed `main`
    // + any helper it invokes (env reads inherit down). Set at top-level before
    // the target; the stateless per-call clone is dropped after the reply, so
    // it ceases to exist when the call completes.
    formatdoc!(
        r#"
        $env.NONCE = "{nonce}"
        use {library}
        {call_path} {lit}
    "#,
        nonce = nonce,
        library = library,
        call_path = call_path,
        lit = lit,
    )
}

/// What: builds the nushell source the stateful worker evals for an
/// `interact()` call -- a `( ... )` subexpression holding one
/// infix-signatured `def --env __interact [args: A]: nothing -> R`
/// carrying the agent body, invoked as `__interact LIT`. LIT is
/// `args_literal` (NUON record, or bare `null` for void args).
///
/// Why: interact() advertises stateful semantics -- `$env` mutations
/// and `cd` in the body persist across calls (the worker's Stateful
/// branch calls `merge_env` after eval). `def --env` carries the body's
/// env/cd OUT to the caller's scope; wrapping the call in `()` (not
/// `do {}`, which would scope env away) lets that reach eval-top where
/// merge_env reads it (verified by an A/B cross-call probe). The
/// positional `[args: A]` runtime-enforces the args -- the prior
/// typed-let enforced NOTHING, the security fix; the `: nothing -> R`
/// output type parse-checks a statically-typed result. Unlike the old
/// top-level-body form, agent defs in BODY are LOCAL to `__interact`
/// and no longer persist -- that persistence was an accidental
/// byproduct, never a contract. The `;` after the def is required
/// inside `()`.
///
/// Where: called by `server::tool::NuSh::interact` to produce the
/// source string shipped to the stateful worker. Pairs with
/// `Mode::Stateful` in `worker::request_loop::eval_source`.
pub(crate) fn build_interact_source(
    args_type: &str,
    result_type: &str,
    args: &mcp::JsonObject,
    body: &str,
    nonce: &str,
) -> String {
    let lit = args_literal(args_type, args);
    // $env.NONCE carries this call's nonce ambiently into the body + helpers.
    // Set as the FIRST line of the `def --env` body (NOT at eval-top): a --env
    // def does not inherit a stack-only var set in the enclosing `()`, but it
    // does see a var its own body assigns, which then reaches helpers it calls.
    // The call stays DIRECT (`__interact LIT`, never wrapped in `let (...)`,
    // which would trap the body's --env writes in a nested scope and break
    // persistence). The assignment escapes via --env, so -- UNLIKE the
    // stateless paths -- the stateful worker strips $env.NONCE from the stack
    // before `merge_env` (see worker/request_loop.rs eval_source): NONCE never
    // persists into the next interact() call, while the body's own $env writes
    // still do.
    formatdoc!(
        r#"
        (
            def --env __interact [args: {at}]: nothing -> {rt} {{
                $env.NONCE = "{nonce}"
                {body}
            }} ;
            __interact {lit}
        )
    "#,
        nonce = nonce,
        at = args_type,
        rt = result_type,
        lit = lit,
        body = body,
    )
}
