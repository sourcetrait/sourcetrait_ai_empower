use ./common.nu

# Announce a packet the queen sent to the bonded fae (COLONY SYN).
#
# The packet <colony_nom>_<tx_id>.txt must already be in the fae's colony packet
# dir (open's channel_output_dir). Appends "COLONY SYN <packet>" to the fae's
# colony control file. response_to_rx_id is optional (null = plain SYN); when set
# it is the rx_id of a fae packet the queen received, adding "RE <fae_nom>_<rx_id>.txt"
# (correlation, not an ack). Errors if the fae is not online, the sent packet is
# missing, or the referenced received packet is missing in the queen's own packet
# dir. Void return.
export def main [args: record<fae: string, tx_id: string, response_to_rx_id: oneof<string, nothing>>]: nothing -> nothing {
    let colony_identity = (common colony_identity $args.fae)
    let colony_session_nom = (common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (common session_nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let fae_control = (common fae_colony_control_file $args.fae $fae_session_nom)
    if not ($fae_control | path exists) {
        error make { msg: $"fae colony channel does not exist: ($fae_control)" }
    }
    let packet = $"($colony_session_nom)_($args.tx_id).txt"
    let packet_path = (common fae_colony_packet_dir $args.fae $fae_session_nom | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"packet does not exist: ($packet_path)" }
    }
    let line = if $args.response_to_rx_id == null {
        $"COLONY SYN ($packet)"
    } else {
        let re_packet = $"($fae_session_nom)_($args.response_to_rx_id).txt"
        let re_path = (common queen_inbox_base $colony_identity $colony_session_nom | path join "input" $re_packet)
        if not ($re_path | path exists) {
            error make { msg: $"response_to packet does not exist: ($re_path)" }
        }
        $"COLONY SYN ($packet) RE ($re_packet)"
    }
    $"($line)(char nl)" | save --append $fae_control
}
