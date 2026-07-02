
# Acknowledge a packet the fae received from the bonded colony's queen (FAE ACK).
#
# The received packet <queen_nom>_<rx_id>.md is in the fae's queen-packet dir.
# Appends "FAE ACK <packet>" to the colony inbox. Errors if the queen is not online
# or the received packet does not exist. Void return.
export def main [args: record<ai_identity: string, rx_id: int>]: nothing -> nothing {
    let fae_session_nom = (empower fae colony channel common session_nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let queen_identity = (empower fae colony channel common queen_identity $args.ai_identity)
    let queen_session_nom = (empower fae colony channel common session_nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_identity) is not online" }
    }
    let c_inbox = (empower fae colony channel common colony_inbox $queen_identity $queen_session_nom)
    if not ($c_inbox | path exists) {
        error make { msg: $"colony inbox does not exist: ($c_inbox)" }
    }
    let packet = $"($queen_session_nom)_($args.rx_id).md"
    let packet_path = (empower fae colony channel common queen_packet_dir $args.ai_identity $fae_session_nom | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"FAE ACK ($packet)(char nl)" | save --append $c_inbox
}