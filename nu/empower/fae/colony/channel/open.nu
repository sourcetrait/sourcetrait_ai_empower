use ./common.nu

# Bring the fae's bonded-colony channel online and report the queen's state.
#
# Derives ant_<ai_identity> + both session_noms from claudeline context, mkdirs
# the fae's own inbox (channel/ant/bond/queen + input/), and appends FAE ONLINE to
# its own control file. The fae is passive: it announces ONLINE to the queen's
# inbox only if that inbox already exists, and never creates a remote file.
# Returns the fae's own input paths, and queen_online: null when the queen has no
# live session, else its session_nom plus the output packet dir to send to it.
# Errors if the fae itself has no live session.
export def main [args: record<ai_identity: string>]: nothing -> record<channel_input_file: string, channel_input_dir: string, queen_online: oneof<nothing, record<session_nom: string, channel_output_dir: string>>> {
    let fae_session_nom = (common session-nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let in_base = (common fae-input-base $args.ai_identity $fae_session_nom)
    let in_dir = ($in_base | path join "input")
    let in_file = ($in_base | path join "input.txt")
    mkdir $in_dir
    $"FAE ONLINE ($fae_session_nom)(char nl)" | save --append $in_file

    let queen_identity = (common queen-identity $args.ai_identity)
    let queen_session_nom = (common session-nom $queen_identity)
    let queen_online = if $queen_session_nom == null {
        null
    } else {
        let out_base = (common queen-output-base $queen_identity $queen_session_nom)
        let out_file = ($out_base | path join "input.txt")
        let out_dir = ($out_base | path join "input")
        if ($out_file | path exists) {
            $"FAE ONLINE ($fae_session_nom)(char nl)" | save --append $out_file
        }
        { session_nom: $queen_session_nom, channel_output_dir: $out_dir }
    }
    { channel_input_file: $in_file, channel_input_dir: $in_dir, queen_online: $queen_online }
}
