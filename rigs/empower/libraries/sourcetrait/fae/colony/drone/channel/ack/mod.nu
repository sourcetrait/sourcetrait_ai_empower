use rig/sourcetrait/fae/colony/drone/channel/common


# Acknowledge a drone packet the fae received (FAE DRONE <name> ACK).
#
# The received packet <colony_nom>_<rx_id>.md is in the fae's drone-packet dir.
# Appends "FAE DRONE <name> ACK <packet>" to the colony inbox (the queen relays it
# to the drone). Errors if the colony is not online or the received packet does not
# exist. Void return.
export def main [args: record<ai_id: string, drone_name: string, rx_id: int>]: nothing -> nothing {
    let fae_session_nom = (common session_nom $args.ai_id)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_id)" }
    }
    let queen_ai_id = (common queen_ai_id $args.ai_id)
    let queen_session_nom = (common session_nom $queen_ai_id)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_ai_id) is not online" }
    }
    let c_inbox = (common colony_inbox $queen_ai_id $queen_session_nom)
    if not ($c_inbox | path exists) {
        error make { msg: $"colony inbox does not exist: ($c_inbox)" }
    }
    let packet = $"($queen_session_nom)_($args.rx_id).md"
    let packet_path = (common fae_drone_dir $args.ai_id $fae_session_nom $args.drone_name | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"FAE DRONE ($args.drone_name) ACK ($packet)(char nl)" | save --append $c_inbox
}
