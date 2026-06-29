use ./common.nu

# Announce a drone is ready to the bonded fae (COLONY DRONE <name> READY).
#
# Queen-invoked once a persistent drone reports READY after its bootstrap. Appends
# "COLONY DRONE <drone_name> READY" to the fae's per-drone control file, so the fae
# knows the drone is reachable and may start sending it requests. Errors if the
# colony or the bonded fae has no live session, or the fae per-drone control file
# is absent. Void return.
export def main [args: record<fae: string, drone_name: string>]: nothing -> nothing {
    let colony_identity = (common colony_identity $args.fae)
    let colony_session_nom = (common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (common session_nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let fae_control = (common fae_drone_control_file $args.fae $fae_session_nom $args.drone_name)
    if not ($fae_control | path exists) {
        error make { msg: $"fae per-drone channel does not exist: ($fae_control)" }
    }
    $"COLONY DRONE ($args.drone_name) READY(char nl)" | save --append $fae_control
}
