# Shared derivation for the fae-side colony channel tools (empower:fae/colony/channel).
#
# The fae talks to its bonded colony's queen. The fae's own inbox is a SINGLE
# shared control file <shm>/ai/<ai_identity>/<fae_nom>/channel/ant/input.txt (the
# fae monitors it; every COLONY line from the queen AND its drones lands here),
# with the queen's packets in the per-sender dir
# <shm>/ai/<ai_identity>/<fae_nom>/channel/ant/queen/input. The fae sends FAE
# lines to the queen's own inbox <shm>/ai/ant_<ai_identity>/<colony_nom>/queen/
# channel (input.txt control + input/ packets).

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

# the fae's single shared inbox control file - the fae monitors this; all COLONY
# lines (queen and drones) land here.
export def fae-control-file [ai_identity: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "ant" "input.txt"
}

# the fae's per-sender packet dir for the queen (fae reads queen packets here).
export def fae-queen-packet-dir [ai_identity: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "ant" "queen" "input"
}

# the queen's own inbox base (fae writes, queen reads): control <base>/input.txt,
# packets <base>/input.
export def queen-input-base [queen_identity: string, queen_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $queen_identity $queen_session_nom "queen" "channel"
}
