use rig/sourcetrait/fae/colony/channel/common
use rig/sourcetrait/empower/pid


# Bring the fae's bonded-colony channel online and report the queen's state.
#
# Ensures the fae's own receive infrastructure exists (the fae inbox file the fae
# monitors + the fae's queen-packet receive dir), appends FAE ONLINE to the colony
# inbox only if it already exists (passive), and reports the queen's state. Returns
# the fae's own inbox + queen-packet dir, and queen_online: null when the queen has
# no live session, else its session_nom plus the colony inbox and the queen's input
# dir (where the fae sends). Errors if the fae has no live session.
export def main [args: record<ai_id: string>]: nothing -> record<fae_inbox: string, queen_packet_dir: string, queen_online: oneof<nothing, record<session_nom: string, colony_inbox: string, queen_input_dir: string>>> {
    let fae_session_nom = (common session_nom $args.ai_id)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_id)" }
    }
    let inbox = (common fae_inbox $args.ai_id $fae_session_nom)
    let q_pkt = (common queen_packet_dir $args.ai_id $fae_session_nom)
    mkdir $q_pkt
    touch $inbox

    let queen_ai_id = (common queen_ai_id $args.ai_id)
    let queen_session_nom = ((pid list_ai null).sessions | where ai_id == $queen_ai_id | get -o 0.session_nom)
    let queen_online = if $queen_session_nom == null {
        null
    } else {
        let c_inbox = (common colony_inbox $queen_ai_id $queen_session_nom)
        let q_in = (common queen_input_dir $queen_ai_id $queen_session_nom)
        if ($c_inbox | path exists) {
            $"FAE ONLINE ($fae_session_nom)(char nl)" | save --append $c_inbox
        }
        { session_nom: $queen_session_nom, colony_inbox: $c_inbox, queen_input_dir: $q_in }
    }
    { fae_inbox: $inbox, queen_packet_dir: $q_pkt, queen_online: $queen_online }
}
