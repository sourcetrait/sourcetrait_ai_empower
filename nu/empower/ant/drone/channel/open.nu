use ./common.nu

# Set up a drone's bonded-fae channel (queen-invoked, before launch).
#
# Creates the drone's own packet inbox (drone/<name>/channel/input) - the drone
# has no control file, since the queen relays inbound lines to it. Returns
# channel_input_dir (the drone's packet inbox) and channel_output_dir (the fae's
# per-drone packet dir the drone sends to - NOT created here; the fae owns it).
# The queen hands channel_output_dir to the drone as a session variable. Errors
# if the colony or the bonded fae has no live session.
export def main [args: record<fae: string, drone_name: string>]: nothing -> record<channel_input_dir: string, channel_output_dir: string> {
    let colony_identity = (common colony-identity $args.fae)
    let colony_session_nom = (common session-nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let fae_session_nom = (common session-nom $args.fae)
    if $fae_session_nom == null {
        error make { msg: $"bonded fae ($args.fae) is not online" }
    }
    let in_dir = (common drone-input-base $colony_identity $colony_session_nom $args.drone_name | path join "input")
    mkdir $in_dir
    let out_dir = (common fae-drone-packet-dir $args.fae $fae_session_nom $args.drone_name)
    { channel_input_dir: $in_dir, channel_output_dir: $out_dir }
}
