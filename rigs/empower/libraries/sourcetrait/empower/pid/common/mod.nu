# Shared pid/liveness derivation for the empower:pid helpers.
#
# A live AI session is a running claude process whose working directory maps to an
# ai_id (basename, except a colony worktree .../ant/colony/<fae> -> ant_<fae>)
# with a readable claudeline status yaml whose recorded pid equals the live pid.
# claudeline writes that pid each render; live ps is authoritative for liveness.

# the claude session command prefix; a live session's `command` starts with this.
export const claude_prefix = "/usr/local/bin/claude "

# an ai_id from a process cwd: the basename, except a colony worktree
# (home-relative .../ant/colony/<fae>) maps to ant_<fae>. Mirrors claudeline's
# ai_id so a live process matches the identity its status yaml is scoped to.
export def cwd_to_ai_id [cwd: string]: nothing -> string {
    let cwd_segs = ($cwd | path split | where {|seg| ($seg != "/") and ($seg != "") })
    let home_segs = ($env.HOME | path split | where {|seg| ($seg != "/") and ($seg != "") })
    let home_len = ($home_segs | length)
    let rel = if (($cwd_segs | length) >= $home_len) and (($cwd_segs | first $home_len) == $home_segs) {
        $cwd_segs | skip $home_len
    } else {
        $cwd_segs
    }
    let n = ($rel | length)
    if ($n >= 3) and (($rel | get ($n - 3)) == "ant") and (($rel | get ($n - 2)) == "colony") {
        $"ant_($rel | last)"
    } else {
        $rel | last
    }
}

# the live AI sessions: running claude processes confirmed against their status
# yaml (recorded pid equals the live pid). Only fully-valid rows are returned.
export def live_sessions []: nothing -> table<kind: string, ai_id: string, session_nom: string, pid: int> {
    ps -l
    | where {|proc| $proc.command | str starts-with $claude_prefix }
    | each {|proc|
        let identity = (cwd_to_ai_id $proc.cwd)
        let status = (status_session $identity)
        if ($status != null) and ($status.pid == $proc.pid) {
            { kind: "claude", ai_id: $identity, session_nom: $status.session_nom, pid: $proc.pid }
        } else {
            null
        }
    }
    | compact
}

# a session's recorded {session_nom, pid} from its claudeline status yaml, or null
# when the yaml is absent / unreadable / missing either field.
def status_session [identity: string]: nothing -> oneof<record<session_nom: string, pid: int>, nothing> {
    let yaml = ($env.XDG_CACHE_HOME | path join "sourcetrait" "empower" "claudeline" $identity "status" "latest.yaml")
    if ($yaml | path exists) {
        let parsed = (open --raw $yaml | decode | from yaml)
        let nom = ($parsed | get -o session_nom)
        let pid = ($parsed | get -o pid)
        if ($nom == null) or ($pid == null) {
            null
        } else {
            { session_nom: $nom, pid: $pid }
        }
    } else {
        null
    }
}
