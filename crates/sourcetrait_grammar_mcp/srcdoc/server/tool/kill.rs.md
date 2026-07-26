# kill.rs

## fn kill
COOPERATIVE CANCEL: flipping the eval's interrupt `Signals` makes it bail at nushell's next
check point and free its permit. A hung thread - a pure-Rust loop that never polls - cannot be
reached, which is the accepted residual; its external children are reaped by the teardown
regardless.

An unknown or already-finished nonce is a RACE-SAFE no-op, because the dispatch's cleanup guard
removes the entry.

THE ESCALATION HAPPENS OUTSIDE THE REGISTRY LOCK, deliberately: both the /proc walk and the
SIGKILL are blocking, and holding the async registry mutex across them would stall every other
tool.

`kill_plugin_subprocesses` fires HERE but never on an automatic timeout. It is broad - nushell
shares plugin subprocesses across evals - so it is reserved for a call the agent EXPLICITLY
killed, where unblocking a plugin-hung eval is worth restarting other evals' plugins.

The watchdog snapshot is taken while the entry is still live, since `kill` returns before the
dispatch future does and the entry would otherwise be gone by the time a hang could be
confirmed.
