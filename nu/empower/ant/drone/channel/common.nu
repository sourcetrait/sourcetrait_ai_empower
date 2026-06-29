# Shared derivation for the drone-side channel tools (empower:ant/drone/channel).
#
# A drone is a queen-launched teammate of the ant colony; it shares the colony's
# ai_identity (ant_<fae>) and session_nom. The queen invokes open/ready/close to
# manage the drone's channel; the drone invokes syn/ack for its own outbound. The
# drone's own inbox (fae writes, drone reads) is
# <colony_shm>/drone/<name>/channel (input.txt control, input/ packets). The drone
# writes COLONY DRONE lines to the fae's per-drone inbox
# <fae_shm>/channel/ant/drone/<name>/input.txt and packets to
# <fae_shm>/channel/ant/drone/<name>/input.

# the colony's ai_identity, derived from the bonded fae's identity.
export def colony_identity [fae: string]: nothing -> string {
    $"ant_($fae)"
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
export def fae_drone_control_file [fae: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "drone" $drone_name "input.txt"
}

# the fae's per-drone packet dir (the drone writes packets here).
export def fae_drone_packet_dir [fae: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "drone" $drone_name "input"
}
