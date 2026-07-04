use rig/sourcetrait/drone/common


# Acknowledge a fae packet a drone received (COLONY DRONE <name> ACK).
#
# Drone-invoked. The received packet <fae_nom>_<rx_id>.md is in the drone's input
# dir. Appends "COLONY DRONE <name> ACK <packet>" to the colony outbox (the fae's
# inbox). Errors if the fae is not online or the received packet is missing. Void return.
export def main [args: record<fae: string, drone_name: string, rx_id: int>]: nothing -> nothing {
    let colony_ai_id = (common colony_ai_id $args.fae)
    let colony_session_nom = (common session_nom $colony_ai_id)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_ai_id)" }
    }
    let fae_session_nom = (common session_nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let outbox = (common colony_outbox $args.fae $fae_session_nom)
    if not ($outbox | path exists) {
        error make { msg: $"colony outbox (fae inbox) does not exist: ($outbox)" }
    }
    let packet = $"($fae_session_nom)_($args.rx_id).md"
    let packet_path = (common drone_input_dir $colony_ai_id $colony_session_nom $args.drone_name | path join $packet)
    if not ($packet_path | path exists) {
        error make { msg: $"received packet does not exist: ($packet_path)" }
    }
    $"COLONY DRONE ($args.drone_name) ACK ($packet)(char nl)" | save --append $outbox
}
