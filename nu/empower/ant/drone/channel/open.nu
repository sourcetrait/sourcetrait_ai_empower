use ./common.nu
use ../../../pid/list_ai.nu

# Set up a drone's bonded-fae channel (queen-invoked, before launch).
#
# Creates the drone's packet input dir on the colony side (where the fae writes
# drone-bound packets; the drone reads them). Returns the drone's
# colony_channel_input_dir, colony_channel_output_dir (the drone's packet output dir
# on the fae side - the fae owns and creates it), and colony_channel_outbox (the
# fae's inbox the drone writes its COLONY DRONE lines to). The queen passes these to
# the drone as session variables. Errors if the colony or the bonded fae has no live
# session.
export def main [args: record<fae: string, drone_name: string>]: nothing -> record<colony_channel_input_dir: string, colony_channel_output_dir: string, colony_channel_outbox: string> {
    let colony_identity = (common colony_identity $args.fae)
    let colony_session_nom = (common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = ((list_ai null).sessions | where ai_identity == $args.fae | get -i 0.session_nom)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let in_dir = (common drone_input_dir $colony_identity $colony_session_nom $args.drone_name)
    mkdir $in_dir
    let out_dir = (common drone_output_dir $args.fae $fae_session_nom $args.drone_name)
    let outbox = (common colony_outbox $args.fae $fae_session_nom)
    { colony_channel_input_dir: $in_dir, colony_channel_output_dir: $out_dir, colony_channel_outbox: $outbox }
}
