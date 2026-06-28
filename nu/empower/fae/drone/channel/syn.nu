use ./common.nu

# Announce a packet the fae sent to a bonded drone (FAE SYN).
#
# Writes "FAE SYN <packet>" to the drone's control file, where <packet> is
# <fae_session_nom>_<tx_id>.txt. The fae must have already written that packet into
# the drone's packet dir (open's channel_output_dir); the class prefix is implicit
# in the path, so the filename carries none.
#
# response_to_rx_id is OPTIONAL - pass null for a plain SYN. When set, it is the
# rx_id of a drone packet the fae previously received, and the line gains a
# correlation suffix: "FAE SYN <packet> RE <colony_session_nom>_<rx_id>.txt",
# tagging this packet as the data the drone requested. NOT an acknowledgement
# (acknowledge receipt separately with ack).
#
# Errors if the bonded drone is not online (its inbox is absent), the sent packet
# does not exist, or (when response_to_rx_id is set) the referenced received packet
# does not exist in the fae's own packet dir. Void return.
export def main [args: record<ai_identity: string, drone_name: string, tx_id: string, response_to_rx_id: oneof<string, nothing>>]: nothing -> nothing {
    let fae_session_nom = (common session-nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let colony_identity = (common colony-identity $args.ai_identity)
    let colony_session_nom = (common session-nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"bonded colony ($colony_identity) is not online" }
    }
    let out_base = (common drone-output-base $colony_identity $colony_session_nom $args.drone_name)
    let out_file = ($out_base | path join "input.txt")
    if not ($out_file | path exists) {
        error make { msg: $"drone output channel does not exist: ($out_file)" }
    }
    let packet = $"($fae_session_nom)_($args.tx_id).txt"
    let packet_path = ($out_base | path join "input" $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"packet does not exist: ($packet_path)" }
    }
    let line = if $args.response_to_rx_id == null {
        $"FAE SYN ($packet)"
    } else {
        let re_packet = $"($colony_session_nom)_($args.response_to_rx_id).txt"
        let re_path = (common fae-input-base $args.ai_identity $fae_session_nom $args.drone_name | path join "input" $re_packet)
        if not ($re_path | path exists) {
            error make { msg: $"response_to packet does not exist: ($re_path)" }
        }
        $"FAE SYN ($packet) RE ($re_packet)"
    }
    $"($line)(char nl)" | save --append $out_file
}
