# interact.rs

## fn interact
Documented and tested SEPARATELY from run end to end, never deduplicated with it, even though
they share `RunParams`. The two differ in substrate and template, and the surfaces are read by
an agent choosing between them.

No body cache is written here, and that is deliberate rather than an omission: a stateful
session body is not a replay target - re-firing it against a different session state would not
mean the same thing.
