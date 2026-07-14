use rig/sourcetrait/queen/common
use rig/sourcetrait/empower/pid


# Set up a drone's bonded-fae channel (queen-invoked, before launch).
#
# Creates the drone's packet input dir on the colony side (where the fae writes
# drone-bound packets; the drone reads them). Returns the drone's
# colony_channel_input_dir, colony_channel_output_dir (the drone's packet output dir
# on the fae side - the fae owns and creates it), and colony_channel_outbox (the
# fae's inbox the drone writes its COLONY DRONE lines to). The queen passes these to
# the drone as session variables. When persist is false (a one-shot drone, which
# never reports ready) it also announces COLONY DRONE <name> ONLINE on both inboxes -
# for a one-shot the signal that matters is just that it is running. Errors if the
# colony or the bonded fae has no live session.
export def main [args: record<fae: string, drone_name: string, persist: bool>]: nothing -> record<colony_channel_input_dir: string, colony_channel_output_dir: string, colony_channel_outbox: string> {
    let colony_ai_id = (common colony_ai_id $args.fae)
    let colony_session_nom = (common session_nom $colony_ai_id)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_ai_id)" }
    }
    let fae_session_nom = ((pid list_ai null).sessions | where ai_id == $args.fae | get -o 0.session_nom)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let in_dir = (common drone_input_dir $colony_ai_id $colony_session_nom $args.drone_name)
    mkdir $in_dir
    let out_dir = (common drone_output_dir $args.fae $fae_session_nom $args.drone_name)
    let outbox = (common colony_outbox $args.fae $fae_session_nom)
    if not $args.persist {
        common announce_drone $args.fae $args.drone_name $colony_ai_id $colony_session_nom $fae_session_nom "ONLINE"
    }
    { colony_channel_input_dir: $in_dir, colony_channel_output_dir: $out_dir, colony_channel_outbox: $outbox }
}
