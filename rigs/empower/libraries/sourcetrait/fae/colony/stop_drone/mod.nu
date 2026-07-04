use rig/sourcetrait/fae/colony/channel/common
use rig/sourcetrait/empower/pid


# Request the bonded colony's queen to stop a drone (fae-side).
#
# The simpler sibling of start_drone: no prompt shm, just the instruction to the
# queen to stop the named drone. Writes a hard-coded stop-request packet
# (<fae_nom>_<tx_id>.md) to the queen's input dir and announces it as a FAE SYN to
# the colony inbox. Returns the stop packet's canonical path. Errors if the fae or
# the colony has no live session.
export def main [args: record<ai_id: string, drone_name: string, tx_id: int>]: nothing -> record<packet_path: string> {
    let fae_session_nom = (common session_nom $args.ai_id)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_id)" }
    }
    let queen_ai_id = (common queen_ai_id $args.ai_id)
    let queen_session_nom = ((pid list_ai null).sessions | where ai_id == $queen_ai_id | get -o 0.session_nom)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_ai_id) is not online" }
    }
    let c_inbox = (common colony_inbox $queen_ai_id $queen_session_nom)
    if not ($c_inbox | path exists) {
        error make { msg: $"colony inbox does not exist: ($c_inbox)" }
    }
    let packet = $"($fae_session_nom)_($args.tx_id).md"
    let queen_in = (common queen_input_dir $queen_ai_id $queen_session_nom)
    mkdir $queen_in
    let packet_path = ($queen_in | path join $packet)
    let body = ([
        "# Fae stop_drone request"
        ""
        $"- drone_name: ($args.drone_name)"
    ] | str join (char nl))
    $body | save -f $packet_path
    $"FAE SYN ($packet)(char nl)" | save --append $c_inbox
    { packet_path: $packet_path }
}
