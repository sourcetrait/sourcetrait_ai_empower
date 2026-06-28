use ./common.nu

# Take the drone's bonded-fae channel offline (DRONE OFFLINE).
#
# Appends "DRONE OFFLINE" to the drone's own control file always, and to the fae's
# per-drone control file only if it already exists (passive - never creates a
# remote file). Errors if the colony itself has no live session.
export def main [args: record<fae: string, drone_name: string>]: nothing -> nothing {
    let colony_identity = (common colony-identity $args.fae)
    let colony_session_nom = (common session-nom $colony_identity)
    if $colony_session_nom == null {
        error make { msg: $"colony has no live session: no context for ($colony_identity)" }
    }
    let in_file = (common drone-input-base $colony_identity $colony_session_nom $args.drone_name | path join "input.txt")
    $"DRONE OFFLINE(char nl)" | save --append $in_file

    let fae_session_nom = (common session-nom $args.fae)
    if $fae_session_nom != null {
        let out_file = (common fae-output-base $args.fae $fae_session_nom $args.drone_name | path join "input.txt")
        if ($out_file | path exists) {
            $"DRONE OFFLINE(char nl)" | save --append $out_file
        }
    }
}
