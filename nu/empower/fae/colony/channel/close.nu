use ./common.nu

# Take the fae's bonded-colony channel offline (FAE OFFLINE).
#
# Appends "FAE OFFLINE" to the fae's own control file always, and to the queen's
# control file only if it already exists (passive - never creates a remote file).
# Errors if the fae itself has no live session. Void return.
export def main [args: record<ai_identity: string>]: nothing -> nothing {
    let fae_session_nom = (common session-nom $args.ai_identity)
    if $fae_session_nom == null {
        error make { msg: $"fae has no live session: no context for ($args.ai_identity)" }
    }
    let in_file = (common fae-input-base $args.ai_identity $fae_session_nom | path join "input.txt")
    $"FAE OFFLINE(char nl)" | save --append $in_file

    let queen_identity = (common queen-identity $args.ai_identity)
    let queen_session_nom = (common session-nom $queen_identity)
    if $queen_session_nom != null {
        let out_file = (common queen-output-base $queen_identity $queen_session_nom | path join "input.txt")
        if ($out_file | path exists) {
            $"FAE OFFLINE(char nl)" | save --append $out_file
        }
    }
}
