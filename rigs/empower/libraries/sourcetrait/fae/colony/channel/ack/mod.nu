use rig/sourcetrait/fae/colony/channel/common


# Acknowledge a packet the fae received from the bonded colony's queen (FAE ACK).
#
# The received packet <queen_nom>_<rx_id>.md is in the fae's queen-packet dir.
# Appends "FAE ACK <packet>" to the colony inbox. Errors if the queen is not online
# or the received packet does not exist. Void return.
export def main [args: record<ai_id: string, rx_id: int>]: nothing -> nothing {
    let fae_session_nom = (common session_nom $args.ai_id)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_id)" }
    }
    let queen_ai_id = (common queen_ai_id $args.ai_id)
    let queen_session_nom = (common session_nom $queen_ai_id)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_ai_id) is not online" }
    }
    let c_inbox = (common colony_inbox $queen_ai_id $queen_session_nom)
    if not ($c_inbox | path exists) {
        error make { msg: $"colony inbox does not exist: ($c_inbox)" }
    }
    let packet = $"($queen_session_nom)_($args.rx_id).md"
    let packet_path = (common queen_packet_dir $args.ai_id $fae_session_nom | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"FAE ACK ($packet)(char nl)" | save --append $c_inbox
}
