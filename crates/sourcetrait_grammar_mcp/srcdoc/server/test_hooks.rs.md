# test_hooks.rs

Never in a shipped build. These exist because the two conditions the lifecycle
system tests care about cannot be triggered from a nu body at all: every builtin
polls Signals, and nu cannot panic Rust.

## struct TestHangDecl
A bare blocking sleep never reaches a Signals check point, so a cancel or a timeout
cannot stop it - which is exactly the accepted hung-engine-thread residual, made
deterministic. It is what lets the whole chain be exercised end to end: timeout,
cancel, the thread outliving its cancel, watchdog confirmation past the grace
window, and the responder's `emergency.nuonl` line.

The sleep is far longer than any test; the spawning test process reaps the thread on
exit.

## struct TestPanicDecl
Exercises the `catch_unwind` guard: the host survives and the eval errors, and on
the interact lane the engine resets.

## fn register_test_hooks
Called by `build_base` for BOTH modes, so a hook is reachable on the stateless pool
and the interact lane alike.
