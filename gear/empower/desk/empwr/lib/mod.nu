# The empwr desk gear's lib - the principal/host-side relay toolkit. Installed as
# the `empwr` desk gear on the host (a nu lib on NU_LIB_DIRS); wire from config.nu
# with `use empwr *` (the `*` flattens the submodules -> `ai relay from|to|up`;
# drop it for `empwr ai relay ...`). The module exports `ai`; it is NOT named
# `empowered` (an older label, since dropped).

export module ai
