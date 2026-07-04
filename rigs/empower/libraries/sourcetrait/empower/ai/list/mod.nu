use rig/sourcetrait/empower/pid


# The live AI sessions, for direct agent use
#
# Each row is a running, pid-confirmed claude session: kind, ai_id,
# session_nom, pid. See empower:pid:list_ai for the validity rule.
export def main [args: nothing]: nothing -> record<sessions: table<kind: string, ai_id: string, session_nom: string, pid: int>> {
    pid list_ai null
}
