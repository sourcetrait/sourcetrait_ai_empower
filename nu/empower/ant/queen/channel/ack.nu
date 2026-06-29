use ./common.nu

# Acknowledge a packet the queen received from the bonded fae (COLONY ACK).
#
# The received packet <fae_nom>_<rx_id>.txt is in the queen's own packet dir.
# Appends "COLONY ACK <packet>" to the fae's colony control file. Errors if the
# fae is not online or the received packet does not exist. Void return.
export def main [args: record<fae: string, rx_id: string>]: nothing -> nothing {
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
    let packet = $"($fae_session_nom)_($args.rx_id).txt"
    let packet_path = (common queen_inbox_base $colony_identity $colony_session_nom | path join "input" $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"COLONY ACK ($packet)(char nl)" | save --append $fae_control
}
