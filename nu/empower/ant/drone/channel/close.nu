use ./common.nu

# Take a drone's bonded-fae channel offline (COLONY DRONE <name> OFFLINE).
#
# Queen-invoked at teardown or one-shot completion. Appends "COLONY DRONE <name>
# OFFLINE" to the fae's shared control file if it exists (the drone has no own
# control file). Errors if the colony has no live session. Void return.
export def main [args: record<fae: string, drone_name: string>]: nothing -> nothing {
    let colony_identity = (common colony-identity $args.fae)
    let colony_session_nom = (common session-nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (common session-nom $args.fae)
    if $fae_session_nom != null {
        let fae_control = (common fae-control-file $args.fae $fae_session_nom)
        if ($fae_control | path exists) {
            $"COLONY DRONE ($args.drone_name) OFFLINE(char nl)" | save --append $fae_control
        }
    }
}
