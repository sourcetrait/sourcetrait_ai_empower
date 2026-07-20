# The grmr desk gear's library - the principal/host-side toolkit. Installed to
# the desk gear home; wire from config.nu with `use gear/desk/sourcetrait/grmr`
# and drive PREFIXED: `grmr ai relay ...`, `grmr mcp info ...`. The prefix is
# the intended usage - we avoid flattening into the global namespace (the `*`
# import form exists but is not used).
#
# `ai` is desk-side relaying (the desk/box perspective and flows differ, so it
# stays a desk module); `mcp` is re-exported from the shared common library
# (gear/common/sourcetrait/lib_grmr).

export module ai
export use gear/common/sourcetrait/lib_grmr mcp
