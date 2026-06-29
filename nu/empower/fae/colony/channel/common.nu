# Shared derivation for the fae-side colony channel tools (empower:fae/colony/channel).
#
# The fae talks to its bonded colony's queen. The fae's colony inbox (queen
# writes, fae reads + monitors) is <fae_shm>/channel/ant/colony/input.txt (control)
# and <fae_shm>/channel/ant/colony/input (packets). The fae writes FAE lines to the
# queen's inbox <colony_shm>/queen/channel/input.txt and packets to
# <colony_shm>/queen/channel/input.

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

# the fae's colony inbox control file (the queen writes COLONY lines here; the fae
# monitors it).
export def fae_colony_control_file [ai_identity: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "ant" "colony" "input.txt"
}

# the fae's colony packet dir (the fae reads queen packets here).
export def fae_colony_packet_dir [ai_identity: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "ant" "colony" "input"
}

# the queen's inbox base (fae writes, queen reads): control <base>/input.txt,
# packets <base>/input.
export def queen_inbox_base [queen_identity: string, queen_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $queen_identity $queen_session_nom "queen" "channel"
}
