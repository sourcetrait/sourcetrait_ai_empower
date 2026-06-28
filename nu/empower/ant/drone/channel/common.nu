# Shared derivation for the drone-side channel tools (empower:ant/drone/channel).
#
# A drone is a queen-launched teammate of the ant colony; it shares the colony's
# ai_identity (ant_<fae>) and session_nom. The queen invokes open/ready/close to
# manage the drone's channel; the drone invokes syn/ack for its own outbound. The
# drone's own inbox (fae writes packets, drone reads) is
# <shm>/ai/ant_<fae>/<colony_nom>/drone/<name>/channel/input - PACKETS ONLY, no
# control file (the queen relays inbound to the drone). The drone sends to the
# fae's SINGLE shared control file <shm>/ai/<fae>/<fae_nom>/channel/ant/input.txt
# (COLONY DRONE lines), with its packets in the per-sender dir
# <shm>/ai/<fae>/<fae_nom>/channel/ant/drone/<name>/input.

export def colony-identity [fae: string]: nothing -> string {
    $"ant_($fae)"
}

export def session-nom [identity: string]: nothing -> oneof<string, nothing> {
    let ctx = ($env.XDG_CACHE_HOME | path join "sourcetrait" "empower" "claudeline" $identity "context" "latest.yaml")
    if ($ctx | path exists) {
        open --raw $ctx | decode | from yaml | get -i session_nom
    } else {
        null
    }
}

# the drone's own inbox base (fae writes packets, drone reads): packets
# <base>/input. No control file - the queen relays inbound lines to the drone.
export def drone-input-base [colony_identity: string, colony_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_identity $colony_session_nom "drone" $drone_name "channel"
}

# the fae's single shared inbox control file - every COLONY line lands here.
export def fae-control-file [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "input.txt"
}

# the fae's per-sender packet dir for this drone (drone writes packets here).
export def fae-drone-packet-dir [fae: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "drone" $drone_name "input"
}
