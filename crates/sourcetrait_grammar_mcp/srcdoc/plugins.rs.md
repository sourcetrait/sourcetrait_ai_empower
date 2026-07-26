# plugins.rs

One module serves both the eval engines' plugin LOADING and the host's plugin
ENUMERATION, deliberately, so the two views of the registry cannot drift.

## struct PluginInfo
A tuple struct, so it serializes as the positional `[name, version]` pair rather
than a two-key object. Both slots are inferable from position, and an `Invalid`
registry item yields a `null` second slot rather than dropping it - the pair
keeps its arity whatever the plugin reported.

## fn registry_path

## fn read_registry
Silent-skip is the discipline here, not laziness: a missing or unreadable
registry has to leave every non-plugin body fully usable, so a failure at any
step returns None and the engine simply carries no plugin decls.

## fn registry_mtime
MTIME RATHER THAN CONTENT, because it is the one signal that catches BOTH ways
the registry can move: a `plugin add/rm` driven through interact(), and an
external edit from the user's own shell. Intercepting the commands would have
caught only the first, and the second is what a user editing their own registry
does.

The Executor snapshots it when it builds the stateless base and re-stats it once
per dispatch. That is affordable only because a stat is roughly
sub-microsecond - the check is on the hot path, and it fires a rebuild only on an
actual change.

## fn load_plugin_decls
Registers DECLS ONLY - no plugin process spawns here, because nushell spawns
those lazily on first invocation. That is what makes the same call cheap enough
for the rig validator to make too: it pays the registry read and nothing else.

The failed-plugin count gets one summary line to stderr rather than a per-plugin
report, since `load_plugin_file` has already printed each individual error by the
time it returns.

## fn list_registered_plugins
`version` is present only when the plugin author chained `.with_version` when
building their plugin, so a `null` here says nothing about whether the plugin
works. The versions this reports are each plugin's own self-reported crate
version and are NOT an engine-compatibility signal - a plugin reporting an older
number can be perfectly current against the pinned engine.
