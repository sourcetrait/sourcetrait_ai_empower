use ./common.nu

# Acknowledge a packet the queen received from the bonded fae (COLONY ACK).
#
# The received packet is <fae_session_nom>_<rx_id>.txt in the queen's own packet
# dir (the class prefix is implicit in the path). Appends "COLONY ACK <packet>"
# to the fae's control file. Errors if the bonded fae is not online (its inbox is
# absent) or the received packet does not exist. Void return.
export def main [args: record<fae: string, rx_id: string>]: nothing -> nothing {
    let queen_identity = (common queen-identity $args.fae)
    let queen_session_nom = (common session-nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"queen has no live session: no context for ($queen_identity)" }
    }
    let fae_session_nom = (common session-nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let out_file = (common fae-output-base $args.fae $fae_session_nom | path join "input.txt")
    if not ($out_file | path exists) {
        error make { msg: $"fae output channel does not exist: ($out_file)" }
    }
    let packet = $"($fae_session_nom)_($args.rx_id).txt"
    let packet_path = (common queen-input-base $queen_identity $queen_session_nom | path join "input" $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"COLONY ACK ($packet)(char nl)" | save --append $out_file
}
