# Eye: file inspection call targets (grammar:eye).
#
# tree wraps the `grimoire eye tree` plugin command as a call target; the plugin
# owns the rendering, so nu_plugin_grimoire must be deployed for it to run. (The
# markdown query `grimoire eye md find` is plugin-only, with no call wrapper.)
export module tree
export use tree
