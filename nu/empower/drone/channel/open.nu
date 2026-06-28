use ./common.nu

# Bring the drone's bonded-fae channel online and report the fae's state.
#
# Derives the colony session_nom from ant_<fae>'s context (the drone shares the
# colony identity) + the bonded fae nom from the fae's context. mkdirs the drone's
# own inbox (channel/fae/bond + input/), appends DRONE ONLINE to its own control
# file, and (passive) to the fae's per-drone inbox only if it already exists -
# never creates a remote file. Returns the drone's own input paths, and
# fae_online: null when the fae has no live session, else its session_nom plus the
# output packet dir to send to it. Errors if the colony itself has no live session.
export def main [args: record<fae: string, drone_name: string>]: nothing -> record<channel_input_file: string, channel_input_dir: string, fae_online: oneof<nothing, record<session_nom: string, channel_output_dir: string>>> {
    let colony_identity = (common colony-identity $args.fae)
    let colony_session_nom = (common session-nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let in_base = (common drone-input-base $colony_identity $colony_session_nom $args.drone_name)
    let in_dir = ($in_base | path join "input")
    let in_file = ($in_base | path join "input.txt")
    mkdir $in_dir
    $"DRONE ONLINE ($colony_session_nom)(char nl)" | save --append $in_file

    let fae_session_nom = (common session-nom $args.fae)
    let fae_online = if $fae_session_nom == null {
        null
    } else {
        let out_base = (common fae-output-base $args.fae $fae_session_nom $args.drone_name)
        let out_file = ($out_base | path join "input.txt")
        let out_dir = ($out_base | path join "input")
        if ($out_file | path exists) {
            $"DRONE ONLINE ($colony_session_nom)(char nl)" | save --append $out_file
        }
        { session_nom: $fae_session_nom, channel_output_dir: $out_dir }
    }
    { channel_input_file: $in_file, channel_input_dir: $in_dir, fae_online: $fae_online }
}
