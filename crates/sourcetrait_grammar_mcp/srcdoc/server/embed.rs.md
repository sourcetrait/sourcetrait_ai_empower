# embed.rs

In-process nushell evaluation, the host-side eval engine. The stateless base is built
once and held by the `Executor`, which hands each eval a pre-built clone onto a
dedicated 64 MB blocking thread; the stateful interact engine is a single long-lived
thread. Cancellation rides a per-eval `Signals` flag the dispatch layer registers in
the resource registry.

## struct FinishGuard
The discriminator between "slow, or bailed on cancel" and "actually hung", and it works
because DROP covers both endings: a normal return and a caught panic both drop the
guard, so both flip the flag. Only a thread stuck where it never returns leaves it
false, which is exactly the condition the watchdog is looking for.

## enum EvalFailure
Everything an eval reports is prose bound for `thread::returned_error`, with ONE
exception: a module-resolution cycle is a CONDITION an agent can route on, so it reaches
the envelope as its own kind rather than as a string that happens to read a certain way.

`From<String>` is what keeps every ordinary `?` site in this file untouched by that
distinction.

## fn build_base
The `shadow_host_fatal_decls` call MUST follow the shell context that defines the real
`exit` / `exec` / `panic`, because name resolution takes the last-registered decl - the
shadow only wins if it is registered second.

`generate_nu_constant` must follow the plugin load, or `$nu.plugin-path` is empty.

No `setsid()`: the host is a child of the MCP client and cannot detach its own
controlling terminal. The TLS crypto provider is installed in `NuSh::new`, not here,
since nushell reads its own OnceLock once per process.

## fn seed_env
The whole OS env goes in because externals need `$env.PATH`.

`NU_LIB_DIRS` is EXCLUDED so the box's own value cannot override the parse-time const
seeded below - that const is the sole controlled lib path, and letting the environment
win would let a body reach outside the namespace.

The EQUIP trio is set directly from CONFIG rather than inherited: eval is in-process
with no spawn-env to carry them, and they are not in the host's own environment, so the
host seeds them itself.

## fn merge_env_no_chdir
NOT nushell's own `merge_env`, and this is the single most consequential deviation in
the file. `merge_env` ends by calling `std::env::set_current_dir` with the stack's
`$env.PWD` (nu-protocol engine_state.rs, 0.114.1 rev 0df4ca2) - correct for a REPL that
owns its process, wrong here. Eval is IN-PROCESS, so that call would move the whole
HOST's working directory whenever an interact body ran `cd`, and it would stay moved for
the process lifetime, pinning that directory open.

The env-overlay drain mirrors nushell's exactly. The config half goes through the public
`set_config`, which carries the same plugin-GC propagation `merge_env` does inline and
fires it only when the GC config actually changed; the private
`update_plugin_gc_configs` is not reachable, which is why `set_config` rather than a
bare field assignment is the correct substitute.

Dropping the chdir also drops `merge_env`'s only error path, which is why this is
infallible.

Worth knowing even though it does not apply here: nushell resolves an external child's
cwd from `$env.PWD` rather than from the process, so externals behave identically either
way. The difference is visible only to code reading the real process cwd.

## fn eval_in_process
`stdout_file` and `capture_all` COMPOSE - they are different layers, not last-wins.
`stdout_file` reassigns the external CHILD's actual fd, while `capture_all` collects the
pipeline value; so an external mid-body writes to the per-nonce log while the terminal
expression's value is what comes back. Verified with N concurrent evals showing zero
cross-talk and nothing on the host's fd 1.

`out_dest` is a PER-STACK field, which is what makes that concurrency-safe by
construction: applying it to an external `try_clone()`s the File into the child's
`Stdio` per spawn. The only path to the host's real fd 1 is a bare `Stack::new()` with
no redirect at all.

`register_nuapi` must precede the parse, because command names resolve at parse time.

`files_before` is taken BEFORE the parse so the cycle check counts only what THIS parse
registered - the interact lane's working set accumulates across calls and would
eventually look like a cycle on its own.

THE CYCLE CHECK COMES FIRST, before reporting parse errors, because a resolution cycle's
own parse errors are unstable in both variant and payload and useless in every form.

`PipelineData::Empty` as the input pairs with `is_mcp = true`, which routes external
stdin to null.

The persist branch strips `$env.NONCE` before merging: it is a per-call ambient, not
session state, so the body's own env writes survive and NONCE does not.

`nu::JsonValue::from_value` rather than `serde_json::to_value` is load-bearing - the
latter serializes nushell's internal tagged representation with spans, not the friendly
JSON `to json` emits.

## const EVAL_STACK_SIZE
nu def-recursion is guarded by `$env.config.recursion_limit`, so the only native
overflow left is pathological PARSER nesting: roughly 1000-deep clears at 8 MB and
roughly 10000-deep at 64 MB. 64 MB clears any realistic body. The residual - deeper
nesting still - aborts the whole host, and that is the locked accepted contract rather
than an open problem.

## fn eval_stateless
THE PERMIT RIDES IN THE THREAD. A hung, uncancellable eval therefore keeps its bounded
slot occupied - visible and counted - rather than leaking a thread while freeing the
slot for another hang.

Cancellation rides `cancel`, which becomes this eval's `Signals`, so a kill or a timeout
triggers it and the eval bails at nushell's next check point, dropping its clone and
releasing the permit. A HUNG THREAD - a pure-Rust hot loop that never polls Signals -
cannot be reached at all, because you cannot SIGKILL a thread; it runs to its natural
end. External children are reaped separately.

The engine clone drops when the thread returns, which is what reclaims its memory.

## struct InteractEngine
The serial thread IS the lane, so a hung interact eval deadlocks it entirely. That is
the case reset-on-panic does NOT cover - it handles panics, not a pure-Rust hang - and
recovery there is still an MCP restart.

### fn spawn
RESET-ON-PANIC is broader than it looks. A caught panic may have poisoned the shared
`env_jobs` lock or left the persistent engine inconsistent, so the jobs table is reset
past any poison and the engine rebuilt fresh. Session env, cd and defs are lost; the
host and the lane survive.

The host owning that `Arc<Mutex<Jobs>>` is what makes the poison HEALABLE - a field left
buried in an immutable base could not be swapped.

If the thread fails to spawn, which is effectively impossible, `rx` drops with the
closure and every `eval()` surfaces the closed channel as an error rather than hanging.

The tracker is overwritten fresh each eval, so a cancel reaches only the current call's
external children.
