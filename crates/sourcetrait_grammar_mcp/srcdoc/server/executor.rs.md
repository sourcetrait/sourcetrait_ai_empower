# executor.rs

## struct Executor
The ready buffer is a LATENCY PRE-PREPARATION, not clone reuse. Each clone is
SINGLE-USE and drops after its one eval, which is what keeps statelessness intact;
what the buffer buys is that a launching run or call never pays the clone inline.

BASE INVALIDATION IS A CORRECTNESS HINGE, not a nicety. A pre-cloned engine is a
SNAPSHOT of the base's plugin decls and env. The base is a swap point tagged with a
`generation`; a plugin-registry change - a `plugin add/rm` on the interact lane, OR
an external edit from the user's own shell, both seen through the file mtime -
rebuilds the base, bumps the generation, and DROPS the stale ready buffer. So run()
picks up a plugin the interact lane just registered without a restart.

In-flight evals finish on the clone they already hold, dispatched under the old
state, which is fine; only the buffer and future clones pick the change up.

### fn semaphore
The permit rides IN the eval thread and releases when the thread finishes, which is
deliberate: a hung thread keeps its slot OCCUPIED - bounded and visible - rather than
leaking a thread while freeing the slot for another hang.

### fn refill_ready
The clone, which is the cost, happens OUTSIDE the ready lock.

### fn refresh_base_if_stale
Affordable on every dispatch only because the stat is roughly sub-microsecond, and
it rebuilds solely on an actual mtime change.

### fn take_clone
Holds the BASE lock across the pop so the generation read and the fallback clone
stay consistent with a concurrent `refresh_base_if_stale`. Without that the pop
could take a clone whose generation had just been superseded.
