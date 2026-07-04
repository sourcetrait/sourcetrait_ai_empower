use rig/sourcetrait/empower/pid/common

# Live, pid-confirmed AI sessions: a running claude matched to its status yaml.
#
# Only fully-valid live rows: a claude process whose cwd-derived ai_id has a
# readable claudeline status yaml carrying a session_nom and a pid equal to the live
# process. kind is "claude" (the only kind now, named from the claudeline producer).
export def main [args: nothing]: nothing -> record<sessions: table<kind: string, ai_id: string, session_nom: string, pid: int>> {
    { sessions: (common live_sessions) }
}
