use ./common.nu

# Set up a drone's bonded-fae channel (queen-invoked, before launch).
#
# Creates the drone's own inbox dir (drone/<name>/channel/input, where the fae
# writes packets to the drone) and the fae's per-drone receive inbox
# (channel/ant/drone/<name>/input + input.txt, where the drone writes back and the
# fae monitors). Returns channel_input_dir (the drone's inbox) and
# channel_output_dir (the fae's per-drone packet dir the drone sends to). The queen
# hands channel_output_dir to the drone as a session variable. Errors if the colony
# or the bonded fae has no live session.
export def main [args: record<fae: string, drone_name: string>]: nothing -> record<channel_input_dir: string, channel_output_dir: string> {
    let colony_identity = (common colony_identity $args.fae)
    let colony_session_nom = (common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (common session_nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let in_dir = (common drone_inbox_base $colony_identity $colony_session_nom $args.drone_name | path join "input")
    mkdir $in_dir
    let out_dir = (common fae_drone_packet_dir $args.fae $fae_session_nom $args.drone_name)
    mkdir $out_dir
    let fae_control = (common fae_drone_control_file $args.fae $fae_session_nom $args.drone_name)
    touch $fae_control
    { channel_input_dir: $in_dir, channel_output_dir: $out_dir }
}
