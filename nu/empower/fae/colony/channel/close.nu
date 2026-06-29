use ./common.nu

# Take the fae's bonded-colony channel offline (FAE OFFLINE).
#
# Appends "FAE OFFLINE" to the fae's own colony control file always, and to the
# queen's control file only if it exists. Errors if the fae itself has no live
# session. Void return.
export def main [args: record<ai_identity: string>]: nothing -> nothing {
    let fae_session_nom = (common session_nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let in_file = (common fae_colony_control_file $args.ai_identity $fae_session_nom)
    $"FAE OFFLINE(char nl)" | save --append $in_file

    let queen_identity = (common queen_identity $args.ai_identity)
    let queen_session_nom = (common session_nom $queen_identity)
    if $queen_session_nom != null {
        let q_file = (common queen_inbox_base $queen_identity $queen_session_nom | path join "input.txt")
        if ($q_file | path exists) {
            $"FAE OFFLINE(char nl)" | save --append $q_file
        }
    }
}
