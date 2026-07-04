use rig/sourcetrait/queen/common
use rig/sourcetrait/empower/pid


# Bring the queen's bonded-fae channel online and report the fae's state.
#
# Ensures the queen's own receive infrastructure exists (the colony inbox file the
# queen monitors + the queen packet input dir), appends COLONY ONLINE to the colony
# outbox (the fae's inbox) only if it already exists (passive), and reports the
# fae's state. Returns the queen's own colony inbox + input dir, and fae_online:
# null when the fae has no live session, else its session_nom plus the colony outbox
# (the fae's inbox) and the queen's output dir on the fae side. Errors if the colony
# has no live session.
export def main [args: record<fae: string>]: nothing -> record<colony_inbox: string, queen_input_dir: string, fae_online: oneof<nothing, record<session_nom: string, colony_outbox: string, queen_output_dir: string>>> {
    let colony_ai_id = (common colony_ai_id $args.fae)
    let colony_session_nom = (common session_nom $colony_ai_id)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_ai_id)" }
    }
    let inbox = (common colony_inbox $colony_ai_id $colony_session_nom)
    let in_dir = (common queen_input_dir $colony_ai_id $colony_session_nom)
    mkdir $in_dir
    touch $inbox

    let fae_session_nom = ((pid list_ai null).sessions | where ai_id == $args.fae | get -o 0.session_nom)
    let fae_online = if $fae_session_nom == null {
        null
    } else {
        let outbox = (common colony_outbox $args.fae $fae_session_nom)
        let out_dir = (common queen_output_dir $args.fae $fae_session_nom)
        if ($outbox | path exists) {
            $"COLONY ONLINE ($colony_session_nom)(char nl)" | save --append $outbox
        }
        { session_nom: $fae_session_nom, colony_outbox: $outbox, queen_output_dir: $out_dir }
    }
    { colony_inbox: $inbox, queen_input_dir: $in_dir, fae_online: $fae_online }
}
