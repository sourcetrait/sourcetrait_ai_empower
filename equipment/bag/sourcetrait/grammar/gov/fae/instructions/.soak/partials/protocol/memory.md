## protocol: memory

This repository acts as a *durable store* for Claude's live memories; a memory knowledge base backed by Git.

The `fae_memory_dir` directory is where Claude exports live memories to.

On a fresh session start, Claude must *hydrate* repository memories to Claude's live memory dir by copying them from
knowledge base.

Claude should only hydrate memories when there is an empty (fresh session) or stale live dir, or if the live `MEMORY.md` is missing.

If a live memory is newer than the repository's, keep the newer (compare `git` meta).

Keep the two memory sets mirrored.

No memory is considered local-only.

Fold new findings into memory immediately. Do this proactively. No asking is necessary.

Proactively export live memories to the memory knowledge base. This should happen the moment Claude adds or
changes a live memory. Mirror it back and commit (signed). To avoid information loss, do not defer.

If a live memory named `the-user` does not exist, create one and use it to track
ergonomics preferences beyond what the user has already specified in `CLAUDE.md`
files.

The purpose of memory, in general, is to capture:
- Extended understanding of constitutional items (one memory matching one item):
  - info items
  - law items
  - protocol items
- Adhoc understanding of user-directed implied items (one memory matching one item):
  - guideline items
  - implied info items
  - procedure items
  - implied bootstrapping items
- Enumeration and definitions of subtopics (found in iter / docs)
- Broad understanding of the system, harness, duties, and operation

All memories extending an understanding of constitutional items are expected
to be bootstrapped into context via the agent's general implied bootstrap
instructions. This is likewise largely expected of adhoc understanding of
implied items, topic definitions, and broad understanding.

Thus, memory is not intended to be used for topic-specific working knowledge.
That is captured via `{protocol:iter_understood}`. Memories capturing
working understanding instead of that information being captured in the appropriate
`understood` directory will usually not be read-in when working on the project,
leading to regressions.

Memories must always reflect current truth. Historical meta information
must only be reflected elsewhere, typically in journals.

Edits to `MEMORY.md` must fully adhere to `{protocol:ragref_memory}`. The memory
index is purely associative, intended to act as a inline search-engine source.
Prose is forbidden there.