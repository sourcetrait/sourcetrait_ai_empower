# Agent-facing AI session queries for the empower harness (empower:ai).
#
# Thin call targets over empower:pid for direct agent use: list the live AI
# sessions (ai:list) or test whether an ai_identity is running (ai:is_running). The
# comms channel:open calls consult the pid layer directly, not these wrappers.

export use list
export use is_running
