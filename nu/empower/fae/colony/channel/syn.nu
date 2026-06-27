use ./common.nu

# Announce a packet the fae sent to the bonded colony (FAE SYN).
#
# Writes "FAE SYN <packet>" to the queen's control file, where <packet> is
# <fae_session_nom>_<tx_id>.txt. The fae must have already written that packet
# into the queen's packet dir (open's channel_output_dir); the class prefix is
# implicit in the path, so the filename carries none.
#
# response_to_rx_id is OPTIONAL - pass null for a plain SYN. When set, it is the
# rx_id of a colony packet the fae previously received, and the line gains a
# correlation suffix: "FAE SYN <packet> RE <queen_session_nom>_<rx_id>.txt",
# tagging this packet as the data the colony requested in that earlier packet.
# This is NOT an acknowledgement (acknowledge receipt separately with ack) - it
# just lets the colony match this followup to its request.
#
# Errors if the bonded colony is not online (its inbox is absent), the sent
# packet does not exist, or (when response_to_rx_id is set) the referenced
# received packet does not exist in the fae's own packet dir. Void return.
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
    let out_base = (common queen-output-base $queen_identity $queen_session_nom)
    let out_file = ($out_base | path join "input.txt")
    if not ($out_file | path exists) {
        error make { msg: $"colony output channel does not exist: ($out_file)" }
    }
    let packet = $"($fae_session_nom)_($args.tx_id).txt"
    let packet_path = ($out_base | path join "input" $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"packet does not exist: ($packet_path)" }
    }
    let line = if $args.response_to_rx_id == null {
        $"FAE SYN ($packet)"
    } else {
        let re_packet = $"($queen_session_nom)_($args.response_to_rx_id).txt"
        let re_path = (common fae-input-base $args.ai_identity $fae_session_nom | path join "input" $re_packet)
        if not ($re_path | path exists) {
            error make { msg: $"response_to packet does not exist: ($re_path)" }
        }
        $"FAE SYN ($packet) RE ($re_packet)"
    }
    $"($line)(char nl)" | save --append $out_file
}
