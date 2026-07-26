# commit.rs

## fn commit
Takes the rig's WRITE lock, since a commit wipes and rebuilds the canonical subtree and a
concurrent call reading the index would see it mid-flight.

The engine comes from `lint_engine.current()` rather than a stored handle, so the validator
picks up a plugin registered since the host started. A validator that never refreshed would
reject rig source that RUNS.
