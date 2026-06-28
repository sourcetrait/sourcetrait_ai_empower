use ./common.nu

# Acknowledge a packet the fae received from a bonded drone (FAE ACK).
#
# The received packet <colony_session_nom>_<rx_id>.txt is in the fae's own per-drone
# packet dir (the class prefix is implicit in the path). Appends "FAE ACK <packet>"
# to the drone's control file. Errors if the bonded drone is not online (its inbox
# is absent) or the received packet does not exist. Void return.
export def main [args: record<ai_identity: string, drone_name: string, rx_id: string>]: nothing -> nothing {
    let fae_session_nom = (common session-nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let colony_identity = (common colony-identity $args.ai_identity)
    let colony_session_nom = (common session-nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"bonded colony ($colony_identity) is not online" }
    }
    let out_file = (common drone-output-base $colony_identity $colony_session_nom $args.drone_name | path join "input.txt")
    if not ($out_file | path exists) {
        error make { msg: $"drone output channel does not exist: ($out_file)" }
    }
    let packet = $"($colony_session_nom)_($args.rx_id).txt"
    let packet_path = (common fae-input-base $args.ai_identity $fae_session_nom $args.drone_name | path join "input" $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"FAE ACK ($packet)(char nl)" | save --append $out_file
}
