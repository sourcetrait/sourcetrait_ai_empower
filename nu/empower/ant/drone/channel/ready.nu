use ./common.nu

# Announce a drone is ready to the bonded fae (COLONY DRONE <name> READY).
#
# Queen-invoked once a persistent drone reports READY after its bootstrap. Appends
# "COLONY DRONE <drone_name> READY" to the colony outbox (the fae's inbox). Errors
# if the colony or the bonded fae has no live session, or the colony outbox is
# absent. Void return.
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
    let outbox = (common colony_outbox $args.fae $fae_session_nom)
    if not ($outbox | path exists) {
        error make { msg: $"colony outbox (fae inbox) does not exist: ($outbox)" }
    }
    $"COLONY DRONE ($args.drone_name) READY(char nl)" | save --append $outbox
}
