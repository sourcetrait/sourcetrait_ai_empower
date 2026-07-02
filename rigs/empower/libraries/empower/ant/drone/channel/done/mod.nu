
# Announce a drone is offline and communications are closed (drone self-teardown).
#
# Drone-invoked - a drone tearing itself down (a one-shot when finished, or a
# persisted drone ending). Appends COLONY DRONE <name> OFFLINE to both inboxes (the
# colony inbox always, the fae inbox if it exists). The same operation as the queen's
# :close - either the drone (here) or the queen tears the drone down. Errors if the
# colony has no live session. Void return.
export def main [args: record<fae: string, drone_name: string>]: nothing -> nothing {
    let colony_identity = (empower ant drone channel common colony_identity $args.fae)
    let colony_session_nom = (empower ant drone channel common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (empower ant drone channel common session_nom $args.fae)
    empower ant drone channel common announce_drone $args.fae $args.drone_name $colony_identity $colony_session_nom $fae_session_nom "OFFLINE"
}