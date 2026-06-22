# empower - the user's host-side toolkit (shipped via the empower project).
#
# Wire from config.nu (replaces the old `source me.nu; use me *`):
#   const empower = ($nu.default-config-dir | path join empower mod.nu)
#   use $empower *
# The `*` brings submodules into scope flat -> `ai relay from|to|up`.
# Drop the `*` for namespaced commands instead -> `empower ai relay ...`.

export module ai
