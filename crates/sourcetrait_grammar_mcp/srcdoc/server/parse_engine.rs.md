# parse_engine.rs

## fn set_lib_dirs_const
A parse-time CONST, not `$env`, and that is the whole point. nu-parser's
`find_in_dirs` reads the const var FIRST, falling back to the deprecated
`$env.NU_LIB_DIRS` - so a body cannot override it, and the MCP's own
author-parented namespace stays the sole controlled lib path. The box's own
`NU_LIB_DIRS` is excluded from the seeded env for the same reason.

## struct ParseEngine

### fn new_full
Calls `generate_nu_constant()` after the plugin load, exactly as the eval base
does, so rig source using `$nu.*` parses at commit as it does at serve. Order
matters: before the plugin path is set, `$nu.plugin-path` would be empty.

### fn engine_state
Borrows the base rather than cloning, because the lint's synthetic sources carry no
file references and therefore need no per-file PWD.

### fn engine_state_for_file
The `$env.PWD` it sets is what lets `export use ./<file>.nu` resolve a sibling flat
helper. The self-view goes FIRST in the dirs list so a rig's own rig-prefixed
references resolve against its source during its FIRST commit, before it has been
placed in the namespace.

## struct LintEngine
A `ParseEngine` snapshots the plugin decls at construction, exactly as the stateless
eval base does. A long-lived one therefore DRIFTS the moment a `plugin add` lands:
the eval side picks that change up through `Executor::refresh_base_if_stale`, so a
validator that never refreshed would reject rig source that RUNS - and reject it as
an opaque `ExtraPositional` mis-bind naming nothing about plugins.

This holder removes that asymmetry using the SAME registry-mtime signal the Executor
uses, applied to the other engine. A rebuild is heavier than the Executor's
clone-swap - a whole command context plus a registry read - which is why it fires
only on an actual mtime change rather than per call.

### fn current
ONE method rather than a separate refresh plus getter, so a caller cannot take the
engine and forget the refresh - which is precisely the bug this type exists to
remove. Poison-tolerant, like every other long-lived lock here.

## fn wrap_as_module
Both wrappers return their prefix length because every span the parse reports is
against the WRAPPED source, and a diagnostic has to point into the author's own
file. Subtracting the prefix is what translates one to the other.
