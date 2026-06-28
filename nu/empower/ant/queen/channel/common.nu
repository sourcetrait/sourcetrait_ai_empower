# Shared derivation for the queen-side channel tools (empower:ant/queen/channel).
#
# The queen (the ant colony's leader) talks to the bonded fae. Paths derive from
# the bonded fae's identity plus the queen's own (the colony, ant_<fae>) via the
# claudeline + shm layout; call-targets take the fae id (+ a tx/rx id). The
# queen's own inbox (fae writes, queen reads) is
# <shm>/ai/ant_<fae>/<colony_nom>/queen/channel (input.txt control, input/
# packets). The queen sends to the fae's SINGLE shared control file
# <shm>/ai/<fae>/<fae_nom>/channel/ant/input.txt (all COLONY lines), with its
# packets in the per-sender dir <shm>/ai/<fae>/<fae_nom>/channel/ant/queen/input.

# the colony's ai_identity, derived from the bonded fae's identity.
export def colony-identity [fae: string]: nothing -> string {
    $"ant_($fae)"
}

# an entity's current session_nom from its claudeline context/latest.yaml, or
# null when it has no context file (no live session).
export def session-nom [identity: string]: nothing -> oneof<string, nothing> {
    let ctx = ($env.XDG_CACHE_HOME | path join "sourcetrait" "empower" "claudeline" $identity "context" "latest.yaml")
    if ($ctx | path exists) {
        open --raw $ctx | decode | from yaml | get -i session_nom
    } else {
        null
    }
}

# the queen's own inbox base (fae writes, queen reads): control <base>/input.txt,
# packets <base>/input.
export def queen-input-base [colony_identity: string, colony_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_identity $colony_session_nom "queen" "channel"
}

# the fae's single shared inbox control file - every COLONY line lands here.
export def fae-control-file [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "input.txt"
}

# the fae's per-sender packet dir for the queen (queen writes packets here).
export def fae-queen-packet-dir [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "queen" "input"
}
