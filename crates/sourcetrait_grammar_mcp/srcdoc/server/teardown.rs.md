# teardown.rs

nushell runs externals HEADLESS (`is_interactive = false` takes the plain
`command.spawn` branch, no setpgid), so there is no process group to killpg and a
`ThreadJob` tracks only the DIRECT child pid. A naive SIGKILL of that child ORPHANS its
grandchild.

The reap that works, verified against nushell rev 0df4ca2: `PR_SET_CHILD_SUBREAPER` on
the host at startup, so orphans reparent to us and stay on the /proc ppid chain; plus a
/proc descendant walk from each tracked child BEFORE the kill, which makes it
deterministic; plus SIGKILL on each. `Jobs::kill_and_remove` alone reaches only the
tracked direct child.

All of it is Linux-gated. A non-target build degrades to a direct-child kill with no
/proc walk, which keeps the shipped crate honest by construction rather than by
assumption - we target Linux, and saying so in `cfg` is more honest than pretending
otherwise.

## fn make_tracker
Setting the returned job as an engine's `current_job.background_thread_job` is what makes
nushell register the pid of every external that eval spawns into an Arc-shared set, which
the kill and timeout paths then read.

The Mail sender is a THROWAWAY on purpose: pid tracking is sender-independent, and a
foreground eval never emits job Mail.

Its `Signals` share the eval's cancel flag, so once cancel fires no further pid is
registered.

## const REAP_GRACE_SECS
THE GRACE IS THE SAFETY MECHANISM, not a tuning knob.

A blanket `waitpid(-1, WNOHANG)` would be the obvious implementation and is WRONG here:
it races the legitimate waiters - `Command::output()` inside `run_git`, and nushell's own
external handling - and can steal a child's exit status, so their wait fails ECHILD and a
working `run_git` returns a spurious error.

Those waiters reap within MILLISECONDS of the child exiting, so a zombie still sitting
there seconds later provably belongs to nobody. Age is what separates "abandoned" from
"someone is mid-wait", without needing a registry of every pid the process is currently
awaiting.

## fn parse_state_ppid
Fields are counted after the LAST ')' because the comm field is unquoted and can itself
contain spaces and parens. After it, index 0 is state (field 3) and index 1 is ppid.

## fn parse_start_time
Exists for the PID-REUSE GUARD on config pins. A pid alone cannot say whether the process
holding a pin is still the one that took it - pids are recycled, and a long-lived host
will outlive plenty of them. The start time makes the pair (pid, start_time) an identity:
the kernel will not hand out the same pid with the same start tick, so a pin whose stored
start time no longer matches belongs to a process that has already gone.

Same last-')' counting as above, so field 22 is index 19.

## struct OrphanReaper
The subreaper is taken for its ATTACHMENT property, but the same syscall imposes a DUTY:
we are now those orphans' parent, and a parent must `wait()` them. An adopted child that
exits with no waiter otherwise stays a zombie for the host's LIFETIME, holding a pid and a
process-table slot and polluting the very /proc sweeps our own orphan discipline depends
on.

The reliable producer is `git commit`, which detaches its own auto-maintenance
(`gc --auto`) - one leaked zombie per commit, and the rig lifecycle commits on every
`rig new`, `commit` and `uninstall`. It is not git-specific: any external that daemonizes
a child lands here identically.

THE RATE IS WORSE THAN ONE PER COMMIT IMPLIES, because it compounds through nested
process trees. Measured: a host at 6 zombies, and one
`cargo test -p sourcetrait_grammar_mcp` run took it to 166 - the integration suite does
dozens of in-process rig commits, and each test binary then exits, orphaning its gc
children onto the nearest subreaper ancestor, which is the host running the eval. The
same sample carried `grep` and `awk` zombies from an agent-side Monitor's tail pipeline,
putting the not-git-specific claim in evidence rather than in principle.

SERVE-PATH ONLY. A one-shot CLI process exits promptly, at which point its orphans
reparent to init and are reaped there.

### fn reap
Runs on the watchdog tick, which is cheap enough - a /proc scan of stat files, and usually
nothing to reap. `WNOHANG` so a surprise never blocks the watchdog, and a pid someone else
already reaped returns ECHILD, which is exactly the no-op wanted.

## fn install_child_subreaper
Best-effort and called once at host startup. The subreaper only buys RACE-SAFETY on
kill-time orphans; the /proc descendant walk is the actual mechanism.

## fn kill_plugin_subprocesses
Unblocks a plugin-hung eval, which nothing else can: a hung plugin ignores the cancel
`Signals` and is not a tracked external, so killing its subprocess is what closes the
plugin IPC pipe and makes the eval's plugin read return an error. Plugins respawn lazily
on next use.

BROAD BY NATURE - nushell shares plugin subprocesses across evals - so it fires only on
an explicit `kill(nonce)` and on the shutdown sweep, NEVER on an automatic timeout, where
the supervisor scopes that escalation instead.

## fn descendants_of
Walked BEFORE the kill, which is what makes the set deterministic rather than a race
against reparenting.
