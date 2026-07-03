use sourcetrait/drone/common


# Announce a persisted drone is online and ready to accept instructions.
#
# Drone-invoked, once a persisted drone has finished its bootstrap - the signal
# that matters for a persisted drone is that it can accept instructions. Appends
# COLONY DRONE <name> ONLINE to both inboxes (the colony inbox always, the fae inbox
# if it exists). A one-shot drone does not call this; the queen's
# sourcetrait/queen:drone/channel:open announces ONLINE early. Errors if the colony
# has no live session. Void return.
export def main [args: record<fae: string, drone_name: string>]: nothing -> nothing {
    let colony_identity = (common colony_identity $args.fae)
    let colony_session_nom = (common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (common session_nom $args.fae)
    common announce_drone $args.fae $args.drone_name $colony_identity $colony_session_nom $fae_session_nom "ONLINE"
}
