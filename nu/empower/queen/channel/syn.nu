use ./common.nu

# Announce a packet the queen sent to the bonded fae (COLONY SYN).
#
# Writes "COLONY SYN <packet>" to the fae's control file, where <packet> is
# <queen_session_nom>_<tx_id>.txt. The queen must have already written that
# packet into the fae's packet dir (open's channel_output_dir); the class prefix
# is implicit in the path, so the filename carries none.
#
# response_to_rx_id is OPTIONAL - pass null for a plain SYN. When set, it is the
# rx_id of a fae packet the queen previously received, and the line gains a
# correlation suffix: "COLONY SYN <packet> RE <fae_session_nom>_<rx_id>.txt",
# tagging this packet as the data the fae requested in that earlier packet. This
# is NOT an acknowledgement (acknowledge receipt separately with ack) - it just
# lets the fae match this followup to its request.
#
# Errors if the bonded fae is not online (its inbox is absent), the sent packet
# does not exist, or (when response_to_rx_id is set) the referenced received
# packet does not exist in the queen's own packet dir. Void return.
export def main [args: record<fae: string, tx_id: string, response_to_rx_id: oneof<string, nothing>>]: nothing -> nothing {
    let queen_identity = (common queen-identity $args.fae)
    let queen_session_nom = (common session-nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"queen has no live session: no context for ($queen_identity)" }
    }
    let fae_session_nom = (common session-nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let out_base = (common fae-output-base $args.fae $fae_session_nom)
    let out_file = ($out_base | path join "input.txt")
    if not ($out_file | path exists) {
        error make { msg: $"fae output channel does not exist: ($out_file)" }
    }
    let packet = $"($queen_session_nom)_($args.tx_id).txt"
    let packet_path = ($out_base | path join "input" $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"packet does not exist: ($packet_path)" }
    }
    let line = if $args.response_to_rx_id == null {
        $"COLONY SYN ($packet)"
    } else {
        let re_packet = $"($fae_session_nom)_($args.response_to_rx_id).txt"
        let re_path = (common queen-input-base $queen_identity $queen_session_nom | path join "input" $re_packet)
        if not ($re_path | path exists) {
            error make { msg: $"response_to packet does not exist: ($re_path)" }
        }
        $"COLONY SYN ($packet) RE ($re_packet)"
    }
    $"($line)(char nl)" | save --append $out_file
}
