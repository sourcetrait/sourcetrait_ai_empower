use sourcetrait/queen/common


# Take a drone offline and close communications (COLONY DRONE <name> OFFLINE).
#
# Queen-invoked - the manual teardown interface; the same operation a persisted
# drone runs itself via sourcetrait/drone:channel:done. Appends COLONY DRONE
# <name> OFFLINE to both inboxes (the colony inbox always, the fae inbox if it
# exists). Errors if the colony has no live session. Void return.
export def main [args: record<fae: string, drone_name: string>]: nothing -> nothing {
    let colony_identity = (common colony_identity $args.fae)
    let colony_session_nom = (common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (common session_nom $args.fae)
    common announce_drone $args.fae $args.drone_name $colony_identity $colony_session_nom $fae_session_nom "OFFLINE"
}
