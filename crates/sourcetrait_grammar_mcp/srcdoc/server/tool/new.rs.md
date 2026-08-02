# new.rs

## fn scaffold
The handler is named `scaffold` while the TOOL is named `new`, because `new` is not usable as a
method name here - hence the explicit `name = "new"` on the attribute.

A BARE RIG NAMEPATH IS REJECTED with a message pointing at `rig(new)`. This tool scaffolds
modules and functions into already-established rigs; establishing a rig is `rig(new)`'s job, so
an author reaching for it that way needs the other tool named rather than a generic arity
complaint.

THE BATCH IS ALL-OR-NOTHING ON PRE-EXISTENCE. Every target rig is locked in canonical sorted,
deduped order first - sorted because a consistent lock order is what prevents a deadlock
between two concurrent batches - then EVERY leaf is checked for pre-existence across the whole
batch BEFORE any is created. Otherwise a batch could half-apply and leave the author to work
out which half.

The scaffolding itself is not transactional beyond that check: a filesystem failure mid-batch
leaves what it already wrote.
