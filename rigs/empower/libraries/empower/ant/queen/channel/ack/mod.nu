
# Acknowledge a packet the queen received from the bonded fae (COLONY ACK).
#
# The received packet <fae_nom>_<rx_id>.md is in the queen's input dir. Appends
# "COLONY ACK <packet>" to the colony outbox (the fae's inbox). Errors if the fae
# is not online or the received packet does not exist. Void return.
export def main [args: record<fae: string, rx_id: int>]: nothing -> nothing {
    let colony_identity = (empower ant queen channel common colony_identity $args.fae)
    let colony_session_nom = (empower ant queen channel common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (empower ant queen channel common session_nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let outbox = (empower ant queen channel common colony_outbox $args.fae $fae_session_nom)
    if not ($outbox | path exists) {
        error make { msg: $"colony outbox (fae inbox) does not exist: ($outbox)" }
    }
    let packet = $"($fae_session_nom)_($args.rx_id).md"
    let packet_path = (empower ant queen channel common queen_input_dir $colony_identity $colony_session_nom | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"COLONY ACK ($packet)(char nl)" | save --append $outbox
}