use ./common.nu

# Bring the fae's per-drone channel online and report the drone's state.
#
# Derives ant_<ai_identity> (the colony) + the fae nom from claudeline context,
# mkdirs the fae's own per-drone inbox (channel/ant/bond/drone/<drone_name> +
# input/), and appends FAE ONLINE to its own control file. The fae is passive: it
# announces ONLINE to the drone's inbox only if that inbox already exists, and
# never creates a remote file. Returns the fae's own input paths, and drone_online:
# null when the colony has no live session or the named drone has not opened its
# channel, else the colony session_nom plus the drone's packet dir to send to it.
# Errors if the fae itself has no live session.
export def main [args: record<ai_identity: string, drone_name: string>]: nothing -> record<channel_input_file: string, channel_input_dir: string, drone_online: oneof<nothing, record<session_nom: string, channel_output_dir: string>>> {
    let fae_session_nom = (common session-nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let in_base = (common fae-input-base $args.ai_identity $fae_session_nom $args.drone_name)
    let in_dir = ($in_base | path join "input")
    let in_file = ($in_base | path join "input.txt")
    mkdir $in_dir
    $"FAE ONLINE ($fae_session_nom)(char nl)" | save --append $in_file

    let colony_identity = (common colony-identity $args.ai_identity)
    let colony_session_nom = (common session-nom $colony_identity)
    let drone_online = if $colony_session_nom == null {
        null
    } else {
        let out_base = (common drone-output-base $colony_identity $colony_session_nom $args.drone_name)
        let out_file = ($out_base | path join "input.txt")
        let out_dir = ($out_base | path join "input")
        if ($out_file | path exists) {
            $"FAE ONLINE ($fae_session_nom)(char nl)" | save --append $out_file
            { session_nom: $colony_session_nom, channel_output_dir: $out_dir }
        } else {
            null
        }
    }
    { channel_input_file: $in_file, channel_input_dir: $in_dir, drone_online: $drone_online }
}
