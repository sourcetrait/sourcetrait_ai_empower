use ./common.nu

# Bring the fae's bonded-colony channel online and report the queen's state.
#
# Creates the fae's colony inbox - the control file channel/ant/colony/input.txt
# (which the fae monitors) plus its packet dir - and appends FAE ONLINE to it, then
# (passive) to the queen's control file only if it already exists. Returns the
# fae's colony input paths (channel_input_file is the file to monitor), and
# queen_online: null when the queen has no live session, else its session_nom plus
# the queen's control file (channel_output_file) and packet dir (channel_output_dir).
# Errors if the fae itself has no live session.
export def main [args: record<ai_identity: string>]: nothing -> record<channel_input_file: string, channel_input_dir: string, queen_online: oneof<nothing, record<session_nom: string, channel_output_file: string, channel_output_dir: string>>> {
    let fae_session_nom = (common session_nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let in_file = (common fae_colony_control_file $args.ai_identity $fae_session_nom)
    let in_dir = (common fae_colony_packet_dir $args.ai_identity $fae_session_nom)
    mkdir $in_dir
    $"FAE ONLINE ($fae_session_nom)(char nl)" | save --append $in_file

    let queen_identity = (common queen_identity $args.ai_identity)
    let queen_session_nom = (common session_nom $queen_identity)
    let queen_online = if $queen_session_nom == null {
        null
    } else {
        let q_base = (common queen_inbox_base $queen_identity $queen_session_nom)
        let q_file = ($q_base | path join "input.txt")
        let q_dir = ($q_base | path join "input")
        if ($q_file | path exists) {
            $"FAE ONLINE ($fae_session_nom)(char nl)" | save --append $q_file
        }
        { session_nom: $queen_session_nom, channel_output_file: $q_file, channel_output_dir: $q_dir }
    }
    { channel_input_file: $in_file, channel_input_dir: $in_dir, queen_online: $queen_online }
}
