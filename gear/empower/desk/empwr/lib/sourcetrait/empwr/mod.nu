# The empwr desk gear's library - the principal/host-side toolkit. Installed to
# the desk gear home; wire from config.nu with
# `use gear/desk/sourcetrait/empwr *` (the `*` flattens the submodules ->
# `ai relay from|to|up`, `mcp info|call|...`; drop it for `empwr ai relay ...`).
#
# `ai` is desk-side relaying (the desk/box perspective and flows differ, so it
# stays a desk module); `mcp` is re-exported from the shared common library
# (gear/common/sourcetrait/lib_empwr).

export module ai
export use gear/common/sourcetrait/lib_empwr mcp
