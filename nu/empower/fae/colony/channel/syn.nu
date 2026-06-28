use ./common.nu

# Announce a packet the fae sent to the bonded colony's queen (FAE SYN).
#
# The packet <fae_nom>_<tx_id>.txt must already be in the queen's packet dir
# (open's channel_output_dir). Appends "FAE SYN <packet>" to the queen's control
# file. response_to_rx_id optional; when set, adds "RE <queen_nom>_<rx_id>.txt"
# referencing a queen packet the fae received (in the fae's queen packet dir).
# Errors if the queen is not online, the sent packet is missing, or the referenced
# received packet is missing. Void return.
export def main [args: record<ai_identity: string, tx_id: string, response_to_rx_id: oneof<string, nothing>>]: nothing -> nothing {
    let fae_session_nom = (common session-nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let queen_identity = (common queen-identity $args.ai_identity)
    let queen_session_nom = (common session-nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_identity) is not online" }
    }
    let q_base = (common queen-input-base $queen_identity $queen_session_nom)
    let q_file = ($q_base | path join "input.txt")
    if not ($q_file | path exists) {
        error make { msg: $"queen control channel does not exist: ($q_file)" }
    }
    let packet = $"($fae_session_nom)_($args.tx_id).txt"
    let packet_path = ($q_base | path join "input" $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"packet does not exist: ($packet_path)" }
    }
    let line = if $args.response_to_rx_id == null {
        $"FAE SYN ($packet)"
    } else {
        let re_packet = $"($queen_session_nom)_($args.response_to_rx_id).txt"
        let re_path = (common fae-queen-packet-dir $args.ai_identity $fae_session_nom | path join $re_packet)
        if not ($re_path | path exists) {
            error make { msg: $"response_to packet does not exist: ($re_path)" }
        }
        $"FAE SYN ($packet) RE ($re_packet)"
    }
    $"($line)(char nl)" | save --append $q_file
}
