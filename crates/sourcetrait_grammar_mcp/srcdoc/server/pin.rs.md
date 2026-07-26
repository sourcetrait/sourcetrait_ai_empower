# pin.rs

`CONFIG` is a `OnceLock` set at startup and never written again, so anything
adjustable at runtime needs its own live copy. The channel's spam thresholds are the
existing precedent for that shape; this is the same idea applied to the
`[supervisor]` warning lines, with one addition - a pin is bound to the LIFETIME OF A
PROCESS rather than left to be unset by hand.

THE MOTIVATING CASE is a training run: pin `vram_warn_headroom_mib` to 0 for the
run's own pid. The gate fires at `total - headroom`, so zero headroom puts the line at
the card's full capacity and usage never reaches it. When the training process exits
the pin lapses on its own, which is the whole point - nothing has to remember to put
the threshold back.

## enum PinnableKey
An enum rather than a string key, because the pinnable set is CLOSED by design and
this makes it closed in the type system too: a new pinnable setting cannot be added
without the compiler asking what it means everywhere.

`[channel]` is deliberately absent. `config_channel` already mutates those, and a
value with two mutators is a value whose effective setting depends on which surface
you happen to ask.

### fn field
This is what the file layer's validators take, because they compose the table prefix
themselves. Passing the already-dotted `name()` to one produced
`supervisor.supervisor.cpu_warn_fraction` in a live error message - caught by the
production smoke test rather than by the unit test, which asserted only that the value
was refused and never read the text back.

### fn name
Composed from `field()` rather than written out a second time, so the two spellings of
one setting cannot drift apart.

### fn from_name
Matched against `name()` rather than against a third list of literals, for the same
reason `name()` composes from `field()`: one spelling, one place.

## struct ConfigPin

### field start_time
THE PID-REUSE GUARD. A pid alone is not an identity on a long-lived host - the kernel
recycles them - so the pair (pid, start_time) is what actually names the process. On
every check the stored start time is compared against the live one, and a mismatch
means the pin's process is gone and something else now holds its number.

## static PINS
A map keyed by the SETTING is what makes last-write-wins fall out rather than be
enforced: a second pin on the same key replaces the first, so there is never a set of
competing pins to arbitrate between.

## fn validate
SHARED with the file layer rather than restated, so a pin cannot reach a state a
config load would have refused. `fraction_field` went `pub(crate)` for exactly this.

Note the asymmetry in what each key accepts: zero HEADROOM is legal and is the
motivating case, while a zero FRACTION is refused. The bound differs because the
quantity does.

## fn pin
The pid is proven live FIRST. A pin against an already-dead process is refused rather
than created and reaped a tick later, because those two outcomes look identical to the
caller a second afterwards and only one of them is honest about what happened.

## fn reap_pins
Called on the watchdog tick, which already scans /proc, AND again before any read of
the effective config - so a caller can never observe a pin held by a process that has
already exited, whatever the tick happens to be doing.

## fn effective_supervisor
EVERY reader goes through this rather than `config().supervisor`, which is what keeps a
pin from being something the watchdog honors while `get_config` reports the old value.

The unreachable arm is ignored rather than panicking: `validate` is the only
constructor and it pairs the shape to the key, so a future mispairing stays a wrong
reading instead of a dead host.

## fn clear_pins
Test-only in PURPOSE but not `cfg(test)`, because the integration tests are separate
crates that see only `pub` items and reach it through `guts`.

## fn corrupt_start_time_for_test
This is what a pid reuse looks like from the reaper's side - the pid still resolves,
but the process wearing it is not the one that took the pin - and it is not otherwise
reachable in a test: forcing a real reuse means exhausting the pid space.
