# Agent-facing AI session queries for the empower platform (sourcetrait/empower:ai).
#
# Thin call targets over sourcetrait/empower:pid for direct agent use: list the
# live AI sessions (ai:list) or test whether an ai_id is running (ai:is_running).
# The comms open_channel calls consult the pid layer directly, not these wrappers.

export module list
export use list
export module is_running
export use is_running
