
# Request the bonded colony's queen to stop a drone (fae-side).
#
# The simpler sibling of start_drone: no prompt shm, just the instruction to the
# queen to stop the named drone. Writes a hard-coded stop-request packet
# (<fae_nom>_<tx_id>.md) to the queen's input dir and announces it as a FAE SYN to
# the colony inbox. Returns the stop packet's canonical path. Errors if the fae or
# the colony has no live session.
export def main [args: record<ai_identity: string, drone_name: string, tx_id: int>]: nothing -> record<packet_path: string> {
    let fae_session_nom = (empower fae colony channel common session_nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let queen_identity = (empower fae colony channel common queen_identity $args.ai_identity)
    let queen_session_nom = ((empower pid list_ai null).sessions | where ai_identity == $queen_identity | get -i 0.session_nom)
    if $queen_session_nom == null {
        error make { msg: $"bonded colony ($queen_identity) is not online" }
    }
    let c_inbox = (empower fae colony channel common colony_inbox $queen_identity $queen_session_nom)
    if not ($c_inbox | path exists) {
        error make { msg: $"colony inbox does not exist: ($c_inbox)" }
    }
    let packet = $"($fae_session_nom)_($args.tx_id).md"
    let queen_in = (empower fae colony channel common queen_input_dir $queen_identity $queen_session_nom)
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