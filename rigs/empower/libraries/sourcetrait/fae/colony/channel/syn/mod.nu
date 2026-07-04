use rig/sourcetrait/fae/colony/channel/common


# Announce a packet the fae sent to the bonded colony's queen (FAE SYN).
#
# The packet <fae_nom>_<tx_id>.md must already be in the queen's input dir (where
# the fae writes queen-bound packets). Appends "FAE SYN <packet>" to the colony
# inbox. response_to_rx_id optional; when set, adds "RE <queen_nom>_<rx_id>.md"
# referencing a queen packet the fae received (in the fae's queen-packet dir).
# Errors if the queen is not online, the sent packet is missing, or the referenced
# packet is missing. Void return.
export def main [args: record<ai_id: string, tx_id: int, response_to_rx_id: oneof<int, nothing>>]: nothing -> nothing {
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
    let packet = $"($fae_session_nom)_($args.tx_id).md"
    let packet_path = (common queen_input_dir $queen_ai_id $queen_session_nom | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"packet does not exist: ($packet_path)" }
    }
    let line = if $args.response_to_rx_id == null {
        $"FAE SYN ($packet)"
    } else {
        let re_packet = $"($queen_session_nom)_($args.response_to_rx_id).md"
        let re_path = (common queen_packet_dir $args.ai_id $fae_session_nom | path join $re_packet)
        if not ($re_path | path exists) {
            error make { msg: $"response_to packet does not exist: ($re_path)" }
        }
        $"FAE SYN ($packet) RE ($re_packet)"
    }
    $"($line)(char nl)" | save --append $c_inbox
}
