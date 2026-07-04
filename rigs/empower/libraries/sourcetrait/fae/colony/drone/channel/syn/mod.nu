use rig/sourcetrait/fae/colony/drone/channel/common


# Announce a packet the fae sent to a colony drone (FAE DRONE <name> SYN).
#
# The packet <fae_nom>_<tx_id>.md must already be in the drone's input dir (the
# queen's drone-open colony_channel_input_dir). Ensures the fae's drone-packet dir
# exists, then appends "FAE DRONE <name> SYN <packet>" to the colony inbox (the
# queen relays the line to the drone). response_to_rx_id optional; when set, adds
# "RE <colony_nom>_<rx_id>.md" referencing a drone packet the fae received. Errors
# if the colony is not online, the drone channel is not open, or a referenced packet
# is missing. Void return.
export def main [args: record<ai_id: string, drone_name: string, tx_id: int, response_to_rx_id: oneof<int, nothing>>]: nothing -> nothing {
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
    let drone_in = (common drone_input_dir $queen_ai_id $queen_session_nom $args.drone_name)
    if not ($drone_in | path exists) {
        error make { msg: $"drone channel is not open: ($drone_in)" }
    }
    let packet = $"($fae_session_nom)_($args.tx_id).md"
    let packet_path = ($drone_in | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"packet does not exist: ($packet_path)" }
    }
    mkdir (common fae_drone_dir $args.ai_id $fae_session_nom $args.drone_name)
    let line = if $args.response_to_rx_id == null {
        $"FAE DRONE ($args.drone_name) SYN ($packet)"
    } else {
        let re_packet = $"($queen_session_nom)_($args.response_to_rx_id).md"
        let re_path = (common fae_drone_dir $args.ai_id $fae_session_nom $args.drone_name | path join $re_packet)
        if not ($re_path | path exists) {
            error make { msg: $"response_to packet does not exist: ($re_path)" }
        }
        $"FAE DRONE ($args.drone_name) SYN ($packet) RE ($re_packet)"
    }
    $"($line)(char nl)" | save --append $c_inbox
}
