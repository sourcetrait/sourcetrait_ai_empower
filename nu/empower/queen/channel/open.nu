use ./common.nu

# Bring the queen's bonded-fae channel online and report the fae's state.
#
# Derives ant_<fae> + both session_noms from claudeline context, mkdirs the
# queen's own inbox (channel/fae/bond + input/), and appends COLONY ONLINE to her
# own control file. The queen is passive: it announces ONLINE to the fae's inbox
# only if that inbox already exists, and never creates a remote file. Returns the
# queen's own input paths, and fae_online: null when the fae has no live session,
# else its session_nom plus the output packet dir to send to it. Errors if the
# queen itself has no live session.
export def main [args: record<fae: string>]: nothing -> record<channel_input_file: string, channel_input_dir: string, fae_online: oneof<nothing, record<session_nom: string, channel_output_dir: string>>> {
    let queen_identity = (common queen-identity $args.fae)
    let queen_session_nom = (common session-nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"queen has no live session: no context for ($queen_identity)" }
    }
    let in_base = (common queen-input-base $queen_identity $queen_session_nom)
    let in_dir = ($in_base | path join "input")
    let in_file = ($in_base | path join "input.txt")
    mkdir $in_dir
    $"COLONY ONLINE ($queen_session_nom)(char nl)" | save --append $in_file

    let fae_session_nom = (common session-nom $args.fae)
    let fae_online = if $fae_session_nom == null {
        null
    } else {
        let out_base = (common fae-output-base $args.fae $fae_session_nom)
        let out_file = ($out_base | path join "input.txt")
        let out_dir = ($out_base | path join "input")
        if ($out_file | path exists) {
            $"COLONY ONLINE ($queen_session_nom)(char nl)" | save --append $out_file
        }
        { session_nom: $fae_session_nom, channel_output_dir: $out_dir }
    }
    { channel_input_file: $in_file, channel_input_dir: $in_dir, fae_online: $fae_online }
}
