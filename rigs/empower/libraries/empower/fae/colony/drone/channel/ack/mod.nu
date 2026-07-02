
# Acknowledge a drone packet the fae received (FAE DRONE <name> ACK).
#
# The received packet <colony_nom>_<rx_id>.md is in the fae's drone-packet dir.
# Appends "FAE DRONE <name> ACK <packet>" to the colony inbox (the queen relays it
# to the drone). Errors if the colony is not online or the received packet does not
# exist. Void return.
export def main [args: record<ai_identity: string, drone_name: string, rx_id: int>]: nothing -> nothing {
    let fae_session_nom = (empower fae colony drone channel common session_nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let queen_identity = (empower fae colony drone channel common queen_identity $args.ai_identity)
    let queen_session_nom = (empower fae colony drone channel common session_nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_identity) is not online" }
    }
    let c_inbox = (empower fae colony drone channel common colony_inbox $queen_identity $queen_session_nom)
    if not ($c_inbox | path exists) {
        error make { msg: $"colony inbox does not exist: ($c_inbox)" }
    }
    let packet = $"($queen_session_nom)_($args.rx_id).md"
    let packet_path = (empower fae colony drone channel common fae_drone_dir $args.ai_identity $fae_session_nom $args.drone_name | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"FAE DRONE ($args.drone_name) ACK ($packet)(char nl)" | save --append $c_inbox
}