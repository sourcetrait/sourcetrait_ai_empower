use ./common.nu

# Acknowledge a drone packet the fae received (FAE DRONE <name> ACK).
#
# The received packet <colony_nom>_<rx_id>.txt is in the fae's per-drone packet
# dir. Appends "FAE DRONE <name> ACK <packet>" to the QUEEN's control file - the
# queen relays it to the drone. Errors if the colony is not online or the received
# packet does not exist. Void return.
export def main [args: record<ai_identity: string, drone_name: string, rx_id: string>]: nothing -> nothing {
    let fae_session_nom = (common session-nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let queen_identity = (common queen-identity $args.ai_identity)
    let queen_session_nom = (common session-nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_identity) is not online" }
    }
    let q_file = (common queen-control-file $queen_identity $queen_session_nom)
    if not ($q_file | path exists) {
        error make { msg: $"queen control channel does not exist: ($q_file)" }
    }
    let packet = $"($queen_session_nom)_($args.rx_id).txt"
    let packet_path = (common fae-drone-packet-dir $args.ai_identity $fae_session_nom $args.drone_name | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"FAE DRONE ($args.drone_name) ACK ($packet)(char nl)" | save --append $q_file
}
