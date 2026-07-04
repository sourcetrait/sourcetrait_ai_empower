use rig/sourcetrait/fae/colony/channel/common
use rig/sourcetrait/empower/pid


# Request the bonded colony's queen to start a drone (fae-side).
#
# Write the drone's prompt to a normal shm file first (NOT a packet), then pass its
# path as drone_prompt_shm (relative to XDGX_SHM_DIR) plus a caller-chosen
# channel_tx_id. This call does the packet writing: it writes the start-request
# packet (drone_name, persist, and the prompt's canonical path) to the queen's input
# dir as <fae_nom>_<channel_tx_id>.md, announces it to the colony inbox as a FAE SYN,
# and provisions the fae's drone-packet receive dir so the drone can respond. Returns
# the start packet's canonical path and the fae's drone-receive dir. Errors if the
# fae or the colony has no live session, or the prompt shm file is missing.
export def main [args: record<ai_id: string, channel_tx_id: int, drone_name: string, persist: bool, drone_prompt_shm: string>]: nothing -> record<packet_path: string, drone_receive_dir: string> {
    let fae_session_nom = (common session_nom $args.ai_id)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_id)" }
    }
    let queen_ai_id = (common queen_ai_id $args.ai_id)
    let queen_session_nom = ((pid list_ai null).sessions | where ai_id == $queen_ai_id | get -o 0.session_nom)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_ai_id) is not online" }
    }
    let prompt_path = ($env.XDGX_SHM_DIR | path join $args.drone_prompt_shm)
    if not ($prompt_path | path exists) {
        error make { msg: $"drone prompt shm file does not exist: ($prompt_path)" }
    }
    let c_inbox = (common colony_inbox $queen_ai_id $queen_session_nom)
    if not ($c_inbox | path exists) {
        error make { msg: $"colony inbox does not exist: ($c_inbox)" }
    }
    # provision the fae's per-drone receive dir (the drone writes its packets here).
    let drone_receive_dir = ($env.XDGX_SHM_DIR | path join "ai" $args.ai_id $fae_session_nom "channel" "colony" "drone" $args.drone_name)
    mkdir $drone_receive_dir
    # write the start-request packet to the queen's input dir.
    let packet = $"($fae_session_nom)_($args.channel_tx_id).md"
    let queen_in = (common queen_input_dir $queen_ai_id $queen_session_nom)
    mkdir $queen_in
    let packet_path = ($queen_in | path join $packet)
    let body = ([
        "# Fae start_drone request"
        ""
        $"- drone_name: ($args.drone_name)"
        $"- persist: ($args.persist)"
        $"- prompt_path: ($prompt_path)"
    ] | str join (char nl))
    $body | save -f $packet_path
    # announce the start request to the colony inbox (the queen reads it + spawns).
    $"FAE SYN ($packet)(char nl)" | save --append $c_inbox
    { packet_path: $packet_path, drone_receive_dir: $drone_receive_dir }
}
