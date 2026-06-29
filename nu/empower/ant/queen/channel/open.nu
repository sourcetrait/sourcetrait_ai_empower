use ./common.nu

# Bring the queen's bonded-fae channel online and report the fae's state.
#
# mkdirs the queen's own inbox (queen/channel + input/), appends COLONY ONLINE to
# its own control file, and (passive) to the fae's colony control file only if it
# already exists. Returns the queen's own input paths, and fae_online: null when
# the fae has no live session, else its session_nom plus the fae's colony control
# file (channel_output_file) and packet dir (channel_output_dir). Errors if the
# colony itself has no live session.
export def main [args: record<fae: string>]: nothing -> record<channel_input_file: string, channel_input_dir: string, fae_online: oneof<nothing, record<session_nom: string, channel_output_file: string, channel_output_dir: string>>> {
    let colony_identity = (common colony_identity $args.fae)
    let colony_session_nom = (common session_nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let in_base = (common queen_inbox_base $colony_identity $colony_session_nom)
    let in_dir = ($in_base | path join "input")
    let in_file = ($in_base | path join "input.txt")
    mkdir $in_dir
    $"COLONY ONLINE ($colony_session_nom)(char nl)" | save --append $in_file

    let fae_session_nom = (common session_nom $args.fae)
    let fae_online = if $fae_session_nom == null {
        null
    } else {
        let fae_control = (common fae_colony_control_file $args.fae $fae_session_nom)
        let fae_packets = (common fae_colony_packet_dir $args.fae $fae_session_nom)
        if ($fae_control | path exists) {
            $"COLONY ONLINE ($colony_session_nom)(char nl)" | save --append $fae_control
        }
        { session_nom: $fae_session_nom, channel_output_file: $fae_control, channel_output_dir: $fae_packets }
    }
    { channel_input_file: $in_file, channel_input_dir: $in_dir, fae_online: $fae_online }
}
