use ./common.nu

# Acknowledge a packet the fae received from the bonded colony's queen (FAE ACK).
#
# The received packet <queen_nom>_<rx_id>.txt is in the fae's colony packet dir.
# Appends "FAE ACK <packet>" to the queen's control file. Errors if the queen is
# not online or the received packet does not exist. Void return.
export def main [args: record<ai_identity: string, rx_id: string>]: nothing -> nothing {
    let fae_session_nom = (common session_nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let queen_identity = (common queen_identity $args.ai_identity)
    let queen_session_nom = (common session_nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_identity) is not online" }
    }
    let q_file = (common queen_inbox_base $queen_identity $queen_session_nom | path join "input.txt")
    if not ($q_file | path exists) {
        error make { msg: $"queen control channel does not exist: ($q_file)" }
    }
    let packet = $"($queen_session_nom)_($args.rx_id).txt"
    let packet_path = (common fae_colony_packet_dir $args.ai_identity $fae_session_nom | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"FAE ACK ($packet)(char nl)" | save --append $q_file
}
