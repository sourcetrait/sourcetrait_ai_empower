# Shared derivation for the fae-side per-drone send channel (empower:fae/colony/drone/channel).
#
# The fae sends to an individual colony drone THROUGH the queen: the request
# packet goes straight to the drone's own inbox
# <shm>/ai/ant_<ai_identity>/<colony_nom>/drone/<name>/channel/input, and the FAE
# DRONE control line goes to the QUEEN's control file
# <shm>/ai/ant_<ai_identity>/<colony_nom>/queen/channel/input.txt (the queen
# relays it to the drone). The drone's responses arrive on the fae's per-drone
# packet dir <shm>/ai/<ai_identity>/<fae_nom>/channel/ant/drone/<name>/input,
# announced as COLONY DRONE lines on the fae's single shared inbox. Send-only:
# receive is the one colony inbox (empower:fae/colony/channel).

export def queen-identity [ai_identity: string]: nothing -> string {
    $"ant_($ai_identity)"
}

export def session-nom [identity: string]: nothing -> oneof<string, nothing> {
    let ctx = ($env.XDG_CACHE_HOME | path join "sourcetrait" "empower" "claudeline" $identity "context" "latest.yaml")
    if ($ctx | path exists) {
        open --raw $ctx | decode | from yaml | get -i session_nom
    } else {
        null
    }
}

# the queen's control file (the FAE DRONE line goes here; the queen relays it).
export def queen-control-file [queen_identity: string, queen_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $queen_identity $queen_session_nom "queen" "channel" "input.txt"
}

# the drone's own packet inbox (the fae writes its request packet here).
export def drone-input-dir [queen_identity: string, queen_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $queen_identity $queen_session_nom "drone" $drone_name "channel" "input"
}

# the fae's per-drone packet dir (the drone's responses land here; fae reads).
export def fae-drone-packet-dir [ai_identity: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "ant" "drone" $drone_name "input"
}
