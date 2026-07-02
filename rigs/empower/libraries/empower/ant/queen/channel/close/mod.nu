
# Take the queen's bonded-fae channel offline (COLONY OFFLINE).
#
# Appends "COLONY OFFLINE" to the colony outbox (the fae's inbox) only if it
# exists. Errors if the colony has no live session. Void return.
export def main [args: record<fae: string>]: nothing -> nothing {
    let colony_identity = (empower ant queen channel common colony_identity $args.fae)
    let colony_session_nom = (empower ant queen channel common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (empower ant queen channel common session_nom $args.fae)
    if $fae_session_nom != null {
        let outbox = (empower ant queen channel common colony_outbox $args.fae $fae_session_nom)
        if ($outbox | path exists) {
            $"COLONY OFFLINE(char nl)" | save --append $outbox
        }
    }
}