# engine.rs

## fn base_context
THE shared constructor, and the sharing is the point: `build_base` and the rig
validator's `ParseEngine::new_full` both call it, so the two cannot drift about
what command set a rig may legally call. If the two diverged - the validator
missing a layer the eval engine has - a rig calling `from grimoire liquid` or
`str snake-case` would fail commit while running fine, and the diagnostic would
be an `ExtraPositional` naming the builtin head the parser mis-bound, nothing in
it suggesting a missing engine layer. The shared constructor rather than two
parallel ones removes that failure mode by construction.

`is_mcp = true` is load-bearing and easy to read as decoration: it routes an
external command's stdin to null. The host's own stdin IS the JSON-RPC channel,
so without it an external could steal the protocol stream.

`is_interactive = false` has a second effect worth knowing about, well away from
this file: nushell gates its foreground-process-group handling on it, so
externals spawn with no setpgid and there is no process group to kill. That is
why the teardown walks /proc instead.
