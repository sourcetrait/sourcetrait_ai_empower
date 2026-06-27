use ./common.nu

# Acknowledge a packet the fae received from the bonded colony (FAE ACK).
#
# The received packet is <queen_session_nom>_<rx_id>.txt in the fae's own packet
# dir (the class prefix is implicit in the path). Appends "FAE ACK <packet>" to
# the queen's control file. Errors if the colony is not online (its inbox is
# absent) or the received packet does not exist. Void return.
export def main [args: record<ai_identity: string, rx_id: string>]: nothing -> nothing {
    let fae_session_nom = (common session-nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let queen_identity = (common queen-identity $args.ai_identity)
    let queen_session_nom = (common session-nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_identity) is not online" }
    }
    let out_file = (common queen-output-base $queen_identity $queen_session_nom | path join "input.txt")
    if not ($out_file | path exists) {
        error make { msg: $"colony output channel does not exist: ($out_file)" }
    }
    let packet = $"($queen_session_nom)_($args.rx_id).txt"
    let packet_path = (common fae-input-base $args.ai_identity $fae_session_nom | path join "input" $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"FAE ACK ($packet)(char nl)" | save --append $out_file
}
