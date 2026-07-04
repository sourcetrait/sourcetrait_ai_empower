use rig/sourcetrait/fae/colony/channel/common


# Take the fae's bonded-colony channel offline (FAE OFFLINE).
#
# Appends "FAE OFFLINE" to the colony inbox only if it exists. Errors if the fae has
# no live session. Void return.
export def main [args: record<ai_id: string>]: nothing -> nothing {
    let fae_session_nom = (common session_nom $args.ai_id)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_id)" }
    }
    let queen_ai_id = (common queen_ai_id $args.ai_id)
    let queen_session_nom = (common session_nom $queen_ai_id)
    if $queen_session_nom != null {
        let c_inbox = (common colony_inbox $queen_ai_id $queen_session_nom)
        if ($c_inbox | path exists) {
            $"FAE OFFLINE(char nl)" | save --append $c_inbox
        }
    }
}
