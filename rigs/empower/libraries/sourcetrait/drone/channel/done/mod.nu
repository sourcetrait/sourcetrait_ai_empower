use rig/sourcetrait/drone/common


# Announce a drone is offline and communications are closed (drone self-teardown).
#
# Drone-invoked - a drone tearing itself down (a one-shot when finished, or a
# persisted drone ending). Appends COLONY DRONE <name> OFFLINE to both inboxes (the
# colony inbox always, the fae inbox if it exists). The same operation as the
# queen's sourcetrait/queen:drone/channel:close - either the drone (here) or the
# queen tears the drone down. Errors if the colony has no live session. Void return.
export def main [args: record<fae: string, drone_name: string>]: nothing -> nothing {
    let colony_ai_id = (common colony_ai_id $args.fae)
    let colony_session_nom = (common session_nom $colony_ai_id)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_ai_id)" }
    }
    let fae_session_nom = (common session_nom $args.fae)
    common announce_drone $args.fae $args.drone_name $colony_ai_id $colony_session_nom $fae_session_nom "OFFLINE"
}
