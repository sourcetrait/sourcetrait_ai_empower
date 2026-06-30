# Shared derivation for the fae-side colony channel tools (empower:fae/colony/channel).
#
# The fae talks to its bonded colony's queen. The fae's inbox
# <fae_shm>/channel/colony/inbox.txt is the fae's own inbox - it monitors it; every
# COLONY (queen) and COLONY DRONE (drone) line lands there. The fae reads queen
# packets from its queen-packet dir <fae_shm>/channel/colony/queen. The fae writes
# its FAE lines to the colony inbox <colony_shm>/channel/colony/inbox.txt and its
# queen-bound packets to the queen's input dir <colony_shm>/channel/colony/queen.

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

# the fae's inbox file (the fae monitors it; the colony writes COLONY and COLONY
# DRONE lines here).
export def fae_inbox [ai_identity: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "colony" "inbox.txt"
}

# the fae's queen-packet dir (the fae reads queen packets here; the queen writes them).
export def queen_packet_dir [ai_identity: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "colony" "queen"
}

# the colony's inbox file (the fae writes its FAE lines here; the queen monitors it).
export def colony_inbox [queen_identity: string, queen_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $queen_identity $queen_session_nom "channel" "colony" "inbox.txt"
}

# the queen's input dir (the fae writes queen-bound packets here; the queen reads them).
export def queen_input_dir [queen_identity: string, queen_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $queen_identity $queen_session_nom "channel" "colony" "queen"
}
