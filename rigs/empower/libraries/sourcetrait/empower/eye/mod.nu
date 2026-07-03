# Eye: file inspection call targets (empower:eye).
#
# tree wraps the `empowered eye tree` plugin command as a call target; the plugin
# owns the rendering, so nu_plugin_empowered must be deployed for it to run. (The
# markdown query `empowered eye md find` is plugin-only, with no call wrapper.)
export module tree
