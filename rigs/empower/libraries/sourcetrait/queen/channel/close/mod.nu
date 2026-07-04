use rig/sourcetrait/queen/common


# Take the queen's bonded-fae channel offline (COLONY OFFLINE).
#
# Appends "COLONY OFFLINE" to the colony outbox (the fae's inbox) only if it
# exists. Errors if the colony has no live session. Void return.
export def main [args: record<fae: string>]: nothing -> nothing {
    let colony_ai_id = (common colony_ai_id $args.fae)
    let colony_session_nom = (common session_nom $colony_ai_id)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_ai_id)" }
    }
    let fae_session_nom = (common session_nom $args.fae)
    if $fae_session_nom != null {
        let outbox = (common colony_outbox $args.fae $fae_session_nom)
        if ($outbox | path exists) {
            $"COLONY OFFLINE(char nl)" | save --append $outbox
        }
    }
}
