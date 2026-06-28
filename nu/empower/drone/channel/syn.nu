use ./common.nu

# Announce a packet the drone sent to the bonded fae (DRONE SYN).
#
# The packet <colony_session_nom>_<tx_id>.txt must already be written into the
# fae's per-drone packet dir (open's channel_output_dir). Appends "DRONE SYN
# <packet>" to the fae's per-drone control file.
#
# response_to_rx_id is OPTIONAL - pass null for a plain SYN. When set, it is the
# rx_id of a fae packet the drone previously received, and the line gains a
# correlation suffix: "DRONE SYN <packet> RE <fae_session_nom>_<rx_id>.txt",
# tagging this packet as the data the fae requested. NOT an acknowledgement
# (acknowledge receipt separately with ack).
#
# Errors if the colony / bonded fae is not online, the sent packet does not
# exist, or (when response_to_rx_id is set) the referenced received packet does
# not exist in the drone's own packet dir.
export def main [args: record<fae: string, drone_name: string, tx_id: string, response_to_rx_id: oneof<string, nothing>>]: nothing -> nothing {
    let colony_identity = (common colony-identity $args.fae)
    let colony_session_nom = (common session-nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (common session-nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let out_base = (common fae-output-base $args.fae $fae_session_nom $args.drone_name)
    let out_file = ($out_base | path join "input.txt")
    if not ($out_file | path exists) {
        error make { msg: $"fae output channel does not exist: ($out_file)" }
    }
    let packet = $"($colony_session_nom)_($args.tx_id).txt"
    let packet_path = ($out_base | path join "input" $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"packet does not exist: ($packet_path)" }
    }
    let line = if $args.response_to_rx_id == null {
        $"DRONE SYN ($packet)"
    } else {
        let re_packet = $"($fae_session_nom)_($args.response_to_rx_id).txt"
        let re_path = (common drone-input-base $colony_identity $colony_session_nom $args.drone_name | path join "input" $re_packet)
        if not ($re_path | path exists) {
            error make { msg: $"response_to packet does not exist: ($re_path)" }
        }
        $"DRONE SYN ($packet) RE ($re_packet)"
    }
    $"($line)(char nl)" | save --append $out_file
}
