# engine.rs

## fn base_context
THE shared constructor, and the sharing is the point: `build_base` and the rig
validator's `ParseEngine::new_full` both call it, so the two cannot drift about
what command set a rig may legally call. That equivalence was a real defect
through 0.0.0-83, when the validator carried only lang plus shell while the eval
engine also carried nu-cmd-extra - so `from grimoire liquid` and
`str snake-case` failed commit while running fine, and the diagnostic was an
`ExtraPositional` naming the builtin head the parser had mis-bound. Nothing in
that error suggests a missing engine layer, which is why the shared constructor
rather than two parallel ones.

`is_mcp = true` is load-bearing and easy to read as decoration: it routes an
external command's stdin to null. The host's own stdin IS the JSON-RPC channel,
so without it an external could steal the protocol stream.

`is_interactive = false` has a second effect worth knowing about, well away from
this file: nushell gates its foreground-process-group handling on it, so
externals spawn with no setpgid and there is no process group to kill. That is
why the teardown walks /proc instead.
