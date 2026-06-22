# empowered - the user's host-side toolkit (shipped via the empower project).
# Named `empowered` (not `empower`) so it can't collide with the `empower` nu
# library if the container ever loads this toolkit too (testing/controlling a
# sub-container).
#
# Wire from config.nu (replaces the old `source me.nu; use me *`):
#   const empowered = ($nu.default-config-dir | path join empowered mod.nu)
#   use $empowered *
# The `*` brings submodules into scope flat -> `ai relay from|to|up` (you never
# see the `empowered` name). Drop the `*` for `empowered ai relay ...`.

export module ai
