
# Bring the fae's bonded-colony channel online and report the queen's state.
#
# Ensures the fae's own receive infrastructure exists (the fae inbox file the fae
# monitors + the fae's queen-packet receive dir), appends FAE ONLINE to the colony
# inbox only if it already exists (passive), and reports the queen's state. Returns
# the fae's own inbox + queen-packet dir, and queen_online: null when the queen has
# no live session, else its session_nom plus the colony inbox and the queen's input
# dir (where the fae sends). Errors if the fae has no live session.
export def main [args: record<ai_identity: string>]: nothing -> record<fae_inbox: string, queen_packet_dir: string, queen_online: oneof<nothing, record<session_nom: string, colony_inbox: string, queen_input_dir: string>>> {
    let fae_session_nom = (empower fae colony channel common session_nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let inbox = (empower fae colony channel common fae_inbox $args.ai_identity $fae_session_nom)
    let q_pkt = (empower fae colony channel common queen_packet_dir $args.ai_identity $fae_session_nom)
    mkdir $q_pkt
    touch $inbox

    let queen_identity = (empower fae colony channel common queen_identity $args.ai_identity)
    let queen_session_nom = ((empower pid list_ai null).sessions | where ai_identity == $queen_identity | get -i 0.session_nom)
    let queen_online = if $queen_session_nom == null {
        null
    } else {
        let c_inbox = (empower fae colony channel common colony_inbox $queen_identity $queen_session_nom)
        let q_in = (empower fae colony channel common queen_input_dir $queen_identity $queen_session_nom)
        if ($c_inbox | path exists) {
            $"FAE ONLINE ($fae_session_nom)(char nl)" | save --append $c_inbox
        }
        { session_nom: $queen_session_nom, colony_inbox: $c_inbox, queen_input_dir: $q_in }
    }
    { fae_inbox: $inbox, queen_packet_dir: $q_pkt, queen_online: $queen_online }
}