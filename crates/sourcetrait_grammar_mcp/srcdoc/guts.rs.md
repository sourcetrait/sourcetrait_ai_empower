# guts.rs

The test-only-`pub` convention's landing site. `TestServer` wraps a `NuSh` plus a
tokio runtime and drives the REAL tool handlers in-process, returning each tool's
envelope as a serde_json Value - the same shape the wire carries - so the
integration tests exercise the handlers without spawning the binary.

THE PROCESS-GLOBAL CONSTRAINT is the thing to hold before writing a test here.
`CONFIG` is a `OnceLock` and `BASE_DIRS` a `LazyLock` that reads XDG at first
access, so every `TestServer` in ONE test binary shares ONE namespace: one id and
namespace, one XDG root, one rigs git repo. Per-FILE isolation is therefore free
(each `tests/*.rs` is its own process) while WITHIN a file tests must use unique
rig names and must not rely on a pristine namespace. A test that genuinely needs
an empty namespace belongs in its own binary or in the system crate.

`--test-threads=1` follows from the same constraint - a shared namespace and one
git repo race concurrently.

## fn sweep_dead_test_namespaces
Each test BINARY gets its own `grammar_inproc_<pid>` root holding a full rigs git
repo and a keypair, and a test harness returns from `main` with no hook to hang
teardown on: statics never run `Drop`, and there is no atexit here. Left alone
these accumulate one per run FOREVER - a real sweep found 128 of them, roughly
20 MB.

So each run sweeps the DEAD ones on the way IN, which bounds the litter to at most
one namespace per currently-running test binary instead of one per run ever. A
leftover whose pid is gone from /proc cannot be in use by anyone, and that is the
whole liveness test.

Linux-gated like the rest of the /proc work, and elsewhere it is a NO-OP rather
than a guess: without a liveness check the sweep could delete a live concurrent
binary's namespace. Best-effort throughout, since a failed sweep must never fail a
test. Pid reuse only defers a removal by one round.

## fn ensure_test_config
The `unsafe` block is sound because the `Once` serializes it AND it runs before
the first `BASE_DIRS` read - `BASE_DIRS` is only touched inside
`ensure_substrate` and the cache paths, all of which are reached through a
`TestServer` constructed after this. That ordering is the entire safety argument,
so a future caller reaching `BASE_DIRS` earlier would break it.

The config is built from `Config::default()` rather than field-by-field so the
harness picks up every embedded default, `[channel]` included, without restating
them; only the id, namespace and work dir are test-specific.

## struct TestServer

### fn new
The runtime is sized like the host's own, and for the same reason: the body lint
and the rig validator parse inline on a worker, and a pathological module graph
overflows the 2 MB default and ABORTS - which in-process would take the TEST
BINARY down rather than just a host.

A fresh `TestServer` per test builds a fresh `NuSh`, which is roughly the spawned
binary's startup cost MINUS the process spawn. That is what makes the in-process
tier fast and cleanly isolated - each test gets its own interact engine on its own
runtime, with no subprocess.

### fn channel_verified
There is deliberately NO `channel_open` on this harness. That one binds a real
socket and presents a CA-issued leaf, so an in-process test would be asserting
the box's certificate installation rather than anything in this crate. It is
exercised live on the test channel instead.

### fn rig_index
Decodes the NUON index into the JSON shape the assertions are written against, so
a test checking `source_path` does not have to know the on-disk format.

## fn clear_config_pins
The pin registry is process-global, so a test that pins has to be able to put it
back for the next test in the same binary. Unlike the namespace on disk, a pin is
NOT isolated by using a unique name.

## fn rig_block
It lives here rather than in a test file because the in-process namespace is
shared per test BINARY: a bare `contains` against the whole signature block can be
satisfied - or falsified - by a rig some other test in the same binary committed.
`rig_call.rs` had exactly that trap waiting, asserting a helper named `util`
absent while `sourcetrait/importable:util` existed.

## fn lint_body
Builds a full-shell `ParseEngine` per call. The plugin-registry read plus the
heavy engine build is precisely why the lint tests are INTEGRATION rather than
unit, and the same is true of the template drivers below.

## fn lint_engine_reuses_until_the_registry_moves
IDENTITY is the only thing that catches this regression. A `LintEngine` that
rebuilt on every `current()` would still behave correctly and would still pass
every behavioural test, while quietly paying a full command-context build on each
lint and each commit.

The other direction - that a CHANGED registry does rebuild - is proven on the live
channel rather than here, because faking it means writing to the user-global
`plugin.msgpackz` that every process on the box shares.
