use rig/sourcetrait/queen/common


# Announce a packet the queen sent to the bonded fae (COLONY SYN).
#
# The packet <colony_nom>_<tx_id>.md must already be in the queen's output dir on
# the fae side (queen.colony_channel_output_dir). Appends "COLONY SYN <packet>" to
# the colony outbox (the fae's inbox). response_to_rx_id optional; when set, adds
# "RE <fae_nom>_<rx_id>.md" referencing a fae packet the queen received (in the
# queen's input dir). Errors if the fae is not online, the sent packet is missing,
# or the referenced packet is missing. Void return.
export def main [args: record<fae: string, tx_id: int, response_to_rx_id: oneof<int, nothing>>]: nothing -> nothing {
    let colony_ai_id = (common colony_ai_id $args.fae)
    let colony_session_nom = (common session_nom $colony_ai_id)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_ai_id)" }
    }
    let fae_session_nom = (common session_nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let outbox = (common colony_outbox $args.fae $fae_session_nom)
    if not ($outbox | path exists) {
        error make { msg: $"colony outbox (fae inbox) does not exist: ($outbox)" }
    }
    let packet = $"($colony_session_nom)_($args.tx_id).md"
    let packet_path = (common queen_output_dir $args.fae $fae_session_nom | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"packet does not exist: ($packet_path)" }
    }
    let line = if $args.response_to_rx_id == null {
        $"COLONY SYN ($packet)"
    } else {
        let re_packet = $"($fae_session_nom)_($args.response_to_rx_id).md"
        let re_path = (common queen_input_dir $colony_ai_id $colony_session_nom | path join $re_packet)
        if not ($re_path | path exists) {
            error make { msg: $"response_to packet does not exist: ($re_path)" }
        }
        $"COLONY SYN ($packet) RE ($re_packet)"
    }
    $"($line)(char nl)" | save --append $outbox
}
