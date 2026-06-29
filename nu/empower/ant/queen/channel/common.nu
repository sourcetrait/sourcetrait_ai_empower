# Shared derivation for the queen-side channel tools (empower:ant/queen/channel).
#
# The queen (the ant colony's leader) talks to the bonded fae. Paths mirror the
# ant harness config (queen.yaml). The queen's own inbox (fae writes, queen reads)
# is <colony_shm>/queen/channel (input.txt control, input/ packets). The queen
# writes COLONY lines to the fae's colony inbox
# <fae_shm>/channel/ant/colony/input.txt and packets to
# <fae_shm>/channel/ant/colony/input. colony_shm = <shm>/ai/ant_<fae>/<colony_nom>,
# fae_shm = <shm>/ai/<fae>/<fae_nom>.

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

# the queen's own inbox base (fae writes, queen reads): control <base>/input.txt,
# packets <base>/input.
export def queen_inbox_base [colony_identity: string, colony_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_identity $colony_session_nom "queen" "channel"
}

# the fae's colony inbox control file (the queen writes COLONY lines here; the fae
# monitors it).
export def fae_colony_control_file [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "colony" "input.txt"
}

# the fae's colony packet dir (the queen writes packets here).
export def fae_colony_packet_dir [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "colony" "input"
}
