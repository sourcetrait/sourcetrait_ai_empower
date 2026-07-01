
# The live AI sessions, for direct agent use
#
# Each row is a running, pid-confirmed claude session: kind, ai_identity,
# session_nom, pid. See empower:pid:list_ai for the validity rule.
export def main [args: nothing]: nothing -> record<sessions: table<kind: string, ai_identity: string, session_nom: string, pid: int>> {
    empower pid list_ai null
}
