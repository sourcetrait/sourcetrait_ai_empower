# Shared derivation for the fae-side per-drone channel (empower:fae/colony/drone/channel).
#
# The fae talks to an individual colony drone directly. The fae writes FAE DRONE
# lines to the drone's own inbox <colony_shm>/drone/<name>/channel/input.txt and
# packets to <colony_shm>/drone/<name>/channel/input. The drone's responses arrive
# on the fae's per-drone inbox <fae_shm>/channel/ant/drone/<name>/input.txt
# (control, monitored) and <fae_shm>/channel/ant/drone/<name>/input (packets).

# the bonded colony's ai_identity, derived from the fae's own identity.
export def queen_identity [ai_identity: string]: nothing -> string {
    $"ant_($ai_identity)"
}

# an entity's current session_nom from its claudeline context/latest.yaml, or
# null when it has no context file (no live session).
export def session_nom [identity: string]: nothing -> oneof<string, nothing> {
    let ctx = ($env.XDG_CACHE_HOME | path join "sourcetrait" "empower" "claudeline" $identity "context" "latest.yaml")
    if ($ctx | path exists) {
        open --raw $ctx | decode | from yaml | get -i session_nom
    } else {
        null
    }
}

# the drone's own inbox base (fae writes, drone reads): control <base>/input.txt,
# packets <base>/input.
export def drone_inbox_base [colony_identity: string, colony_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_identity $colony_session_nom "drone" $drone_name "channel"
}

# the fae's per-drone inbox control file (the drone writes COLONY DRONE lines
# here; the fae monitors it).
export def fae_drone_control_file [ai_identity: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "ant" "drone" $drone_name "input.txt"
}

# the fae's per-drone packet dir (the fae reads drone packets here).
export def fae_drone_packet_dir [ai_identity: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "ant" "drone" $drone_name "input"
}
