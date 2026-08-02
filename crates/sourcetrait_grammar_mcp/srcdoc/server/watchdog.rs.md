# watchdog.rs

ONE background tokio task. Eval runs on dedicated blocking threads OFF the runtime, so
the watchdog stays schedulable even under TOTAL eval saturation - the emergency path
cannot be hung by what it reports.

CLASSIFY-FIRST: it never kills or recovers. A legitimate heavy transform looks identical
to a runaway, and that false positive is the one to avoid.

PLATFORM-GATED throughout: the /proc readers are Linux-only and return None elsewhere,
and nvidia-smi is RUNTIME-probed, so a GPU-less box simply skips it. The parsers are pure
and unit-tested over fixture strings; the readers are thin wrappers, which is what makes
that split testable at all.

## struct HungWatch
Registered on the TIMEOUT branch of either dispatch path, and on a `kill(nonce)` of a
live entry. The timeout path has already triggered cancel and tree-killed the external
tree by then, so this registration is the additional "watch this for a hang" step rather
than the response itself.

`finished` is the deterministic discriminator: the eval thread carries a `FinishGuard`
that flips it on DROP, so a normal return OR a caught panic both flip it, and only a true
hang leaves it false.

## fn register_hung
Idempotent per nonce, because a kill and the later timeout both fire for one eval and the
FIRST stamp is the honest one. Poison-tolerant, like every long-lived lock here.

## const HUNG_GRACE_MS
Because tree-kill fires at timeout, an eval merely BLOCKED on an external is unblocked -
its thread returns, `finished` flips, and it is pruned. Only a pure-Rust hang survives to
confirmation, which is exactly the distinction the grace exists to draw.

## const LEVEL_SUSTAIN
RESOURCE LEVELS ARE THE AGENT'S CALL, NOT THE HOST'S. Heavy load is something the agent
decides how to react to, and it needs ONE warning over a long period to make that
decision - not a stream of them.

A momentary excursion is not an event, so a level must hold for a full minute. Any dip
below the line restarts this clock.

## const LEVEL_REWARN
Ten minutes is the minimum that would matter; dozens of minutes is the intent. Thirty is chosen
because AN INFERENCE PROJECT RUNS 30 TO 60 MINUTES and holds resources high for all of
it - which is NORMAL work, not a fault. At the ten-minute floor such a run would warn six
times about a condition the agent already knows about and chose; at thirty it warns about
twice, which is enough to notice a genuinely sustained level without talking over the
work.

The long floor is also what removes the need for a separate CLEAR threshold: a value
hovering AT the line cannot produce an event per oscillation.

## fn total_ram_kb
A FRACTION OF THE MACHINE rather than an absolute figure, so the same default means the
same thing on a box with different memory instead of silently ageing.

## const DISK_SAMPLE_EVERY
Five minutes rather than the two-second tick, because `df` is slow and can stall on a
wedged mount.

## fn parse_df
The mount point is everything after the FIFTH column, since a mount path may contain
spaces while the five numeric-ish columns before it may not.

## struct DiskWatch
THE BASELINE DECIDES WHAT WE WATCH. A filesystem already at or above the line when the
host starts is a PRE-EXISTING CONDITION, not something to report: this box carries a
4 KiB `/run/nvidia-ctk-hook...` pseudo-mount sitting at 100% by design, and a naive
threshold rule would warn about it forever. Only those below the line at startup are
enrolled; they warn if they later cross it.

Worth knowing what this watch happens to cover: `/dev/shm` is only 62.5 MiB here and is
where the channel inbox writes attachments, so the attachment lane's own failure mode is
inside this gate.

## fn sample_disk
On the BLOCKING pool specifically, because a wedged mount must not take the async runtime
with it.

## struct LevelGate
Pure over its own state and takes an INJECTED `now`, like `scan_hung`, so it is
unit-tested without sleeping.

## const CLK_TCK
A non-100 kernel skews only the LOGGED percentage, never any action - nothing here reacts
to the number, so the approximation is bounded to the report.

## fn sampling_online
A PRODUCTION host samples whether or not anyone is listening: the log is the durable
record, and sampling while idle is intended. Under `--test` it FOLLOWS the channel
instead, and follows rather than latches, so closing and re-opening a channel takes
sampling down and brings it back.

The waste this removes is MEASURED, not theorised. A second host on the box - a
`grammar_test` host connected with its channel closed - shells `nvidia-smi` every 16
seconds and logs VramWarnings about a card it has no stake in, none deliverable (the
announce is verification-gated while the log is not). Across a long training burn both
namespaces logged the same 57 readings with timestamps within 18 ms, because the watchdog
is spawned per host and `sample_vram` reads the whole card. `--test` is precisely the flag
that says this host is the second one.

## fn scan_hung
Pure over the registry so it is unit-testable with a synthetic map, and it returns the
stateless count separately because that is what the derived Critical is measured against.

## fn parse_cpu_ticks
Fields counted AFTER the last ')' since the comm field can contain spaces and parens, so
utime (field 14) is index 11 and stime (field 15) is index 12.

## fn spawn_watchdog
HOUSEKEEPING RUNS UNGATED, above the sampling gate. Harvesting adopted zombies and lapsed
pins is owed whether or not anyone is watching levels; skipping it would trade a little
wasted sampling for a real leak. Neither emits an Emergency.

`OrphanReaper::reap` lives on this tick for the same reason everything else does: the tick
runs off the eval threads, so a /proc scan cannot be starved by eval saturation.

CONTINUITY STATE IS RESET while offline - `prev_cpu`, the four gates and the
`DiskWatch` - so resuming re-establishes rather than inherits. Without that the first
sample back would report a CPU average spanning the whole offline gap, and a gate could
fire on a sustain window covering a period nobody measured. A reading we never observed is
worse than no reading.

EDGE-TRIGGERED: the hung and Critical conditions are emitted once when they first arise,
via a rising-edge diff of the active-key set between samples, so the log captures distinct
EVENTS rather than a per-second time series of one ongoing condition. The resource levels
reach the same outcome by a different route, through their own gates.
