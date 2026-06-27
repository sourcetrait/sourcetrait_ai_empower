use ./common.nu

# Take the queen's bonded-fae channel offline (COLONY OFFLINE).
#
# Appends "COLONY OFFLINE" to the queen's own control file always, and to the
# fae's control file only if it already exists (passive - never creates a remote
# file). Errors if the queen itself has no live session. Void return.
export def main [args: record<fae: string>]: nothing -> nothing {
    let queen_identity = (common queen-identity $args.fae)
    let queen_session_nom = (common session-nom $queen_identity)
    if $queen_session_nom == null {
        error make { msg: $"queen has no live session: no context for ($queen_identity)" }
    }
    let in_file = (common queen-input-base $queen_identity $queen_session_nom | path join "input.txt")
    $"COLONY OFFLINE(char nl)" | save --append $in_file

    let fae_session_nom = (common session-nom $args.fae)
    if $fae_session_nom != null {
        let out_file = (common fae-output-base $args.fae $fae_session_nom | path join "input.txt")
        if ($out_file | path exists) {
            $"COLONY OFFLINE(char nl)" | save --append $out_file
        }
    }
}
