# The grmr box gear's library - the box-side toolkit. Installed to the box
# gear home; wire from config.nu with `use gear/box/sourcetrait/grmr` and
# drive PREFIXED: `grmr mcp info ...`, `grmr ant new ...`. The prefix is
# the intended usage - we avoid flattening into the global namespace (the
# `*` import form exists but is not used).
#
# `ant` is box-side colony provisioning (ant repositories + colony
# worktrees); `mcp` is re-exported from the shared common library
# (gear/common/sourcetrait/lib_grmr). Everything is a module - there is no
# grmr bin: an external nu process can neither take nor return structured
# values (argv strings in, byte stream out), so the module import IS the
# surface on a nushell system.

export module ant
export use gear/common/sourcetrait/lib_grmr mcp
