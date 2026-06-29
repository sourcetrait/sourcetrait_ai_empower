use ./common.nu

# Take the queen's bonded-fae channel offline (COLONY OFFLINE).
#
# Appends "COLONY OFFLINE" to the queen's own control file always, and to the
# fae's colony control file only if it exists. Errors if the colony itself has no
# live session. Void return.
export def main [args: record<fae: string>]: nothing -> nothing {
    let colony_identity = (common colony_identity $args.fae)
    let colony_session_nom = (common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let in_file = (common queen_inbox_base $colony_identity $colony_session_nom | path join "input.txt")
    $"COLONY OFFLINE(char nl)" | save --append $in_file

    let fae_session_nom = (common session_nom $args.fae)
    if $fae_session_nom != null {
        let fae_control = (common fae_colony_control_file $args.fae $fae_session_nom)
        if ($fae_control | path exists) {
            $"COLONY OFFLINE(char nl)" | save --append $fae_control
        }
    }
}
