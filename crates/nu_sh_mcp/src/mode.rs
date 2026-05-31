/// Worker execution mode. Set once per worker process at spawn time
/// (the host passes `--mode <stateless|stateful>` on the worker
/// binary's command line); the worker reads it into `WarmBase::mode`
/// and `eval_source` branches on it.
///
/// - `Stateless`: clone `WarmBase::engine_state` per call; eval on the
///   clone; drop the clone after sending the response. The worker's
///   warm `engine_state` is never mutated; nothing persists across
///   calls. Matches `run()`'s semantics paired with the do-block
///   scoping in `build_run_source`.
///
/// - `Stateful`: eval directly against `WarmBase::engine_state`;
///   `merge_delta` for parser side effects + `merge_env` from the
///   call's Stack into engine_state at the end. Top-level defs from
///   the source persist across calls, as do env mutations and `cd`.
///   Matches `interact()`'s semantics paired with the unwrapped
///   template in `build_interact_source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Stateless,
    Stateful,
}
