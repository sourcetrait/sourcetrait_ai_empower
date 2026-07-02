
# Take the fae's bonded-colony channel offline (FAE OFFLINE).
#
# Appends "FAE OFFLINE" to the colony inbox only if it exists. Errors if the fae has
# no live session. Void return.
export def main [args: record<ai_identity: string>]: nothing -> nothing {
    let fae_session_nom = (empower fae colony channel common session_nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let queen_identity = (empower fae colony channel common queen_identity $args.ai_identity)
    let queen_session_nom = (empower fae colony channel common session_nom $queen_identity)
    if $queen_session_nom != null {
        let c_inbox = (empower fae colony channel common colony_inbox $queen_identity $queen_session_nom)
        if ($c_inbox | path exists) {
            $"FAE OFFLINE(char nl)" | save --append $c_inbox
        }
    }
}