use ./common.nu

# Announce a packet the fae sent to a colony drone (FAE DRONE <name> SYN).
#
# The packet <fae_nom>_<tx_id>.txt must already be in the drone's own inbox (the
# queen's drone-open channel_output_dir = the drone's channel_input_dir). Ensures
# the fae's per-drone receive dir exists, then appends "FAE DRONE <name> SYN
# <packet>" to the QUEEN's control file - the queen relays the line to the drone.
# response_to_rx_id optional; when set, adds "RE <colony_nom>_<rx_id>.txt"
# referencing a drone packet the fae received. Errors if the colony is not online,
# the drone channel is not open, or a referenced packet is missing. Void return.
export def main [args: record<ai_identity: string, drone_name: string, tx_id: string, response_to_rx_id: oneof<string, nothing>>]: nothing -> nothing {
    let fae_session_nom = (common session-nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let queen_identity = (common queen-identity $args.ai_identity)
    let queen_session_nom = (common session-nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_identity) is not online" }
    }
    let q_file = (common queen-control-file $queen_identity $queen_session_nom)
    if not ($q_file | path exists) {
        error make { msg: $"queen control channel does not exist: ($q_file)" }
    }
    let drone_in = (common drone-input-dir $queen_identity $queen_session_nom $args.drone_name)
    if not ($drone_in | path exists) {
        error make { msg: $"drone channel is not open: ($drone_in)" }
    }
    let packet = $"($fae_session_nom)_($args.tx_id).txt"
    let packet_path = ($drone_in | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"packet does not exist: ($packet_path)" }
    }
    mkdir (common fae-drone-packet-dir $args.ai_identity $fae_session_nom $args.drone_name)
    let line = if $args.response_to_rx_id == null {
        $"FAE DRONE ($args.drone_name) SYN ($packet)"
    } else {
        let re_packet = $"($queen_session_nom)_($args.response_to_rx_id).txt"
        let re_path = (common fae-drone-packet-dir $args.ai_identity $fae_session_nom $args.drone_name | path join $re_packet)
        if not ($re_path | path exists) {
            error make { msg: $"response_to packet does not exist: ($re_path)" }
        }
        $"FAE DRONE ($args.drone_name) SYN ($packet) RE ($re_packet)"
    }
    $"($line)(char nl)" | save --append $q_file
}
