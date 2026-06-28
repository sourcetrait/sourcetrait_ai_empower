use ./common.nu

# Acknowledge a packet the drone received from the bonded fae (DRONE ACK).
#
# The received packet <fae_session_nom>_<rx_id>.txt is in the drone's own packet
# dir. Appends "DRONE ACK <packet>" to the fae's per-drone control file. Errors if
# the colony / bonded fae is not online, or the received packet does not exist.
export def main [args: record<fae: string, drone_name: string, rx_id: string>]: nothing -> nothing {
    let colony_identity = (common colony-identity $args.fae)
    let colony_session_nom = (common session-nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (common session-nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let out_file = (common fae-output-base $args.fae $fae_session_nom $args.drone_name | path join "input.txt")
    if not ($out_file | path exists) {
        error make { msg: $"fae output channel does not exist: ($out_file)" }
    }
    let packet = $"($fae_session_nom)_($args.rx_id).txt"
    let packet_path = (common drone-input-base $colony_identity $colony_session_nom $args.drone_name | path join "input" $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"DRONE ACK ($packet)(char nl)" | save --append $out_file
}
