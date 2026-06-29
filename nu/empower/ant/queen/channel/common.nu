# Shared derivation for the queen-side channel tools (empower:ant/queen/channel).
#
# The queen (the ant colony's leader) talks to the bonded fae over the colony
# channel. Paths mirror the queen harness config (queen.yaml). The colony's single
# inbox <colony_shm>/channel/colony/inbox.txt is the queen's inbox - it monitors it;
# the fae writes its FAE and FAE DRONE lines there (the queen relays drone lines to
# the drone teammates). The queen writes its COLONY lines to the colony's outbox
# <fae_shm>/channel/colony/inbox.txt, which is the fae's inbox (the fae monitors it).
# Packets: the fae writes queen-bound packets to the queen's input dir
# <colony_shm>/channel/colony/queen; the queen writes its packets to the fae at its
# output dir <fae_shm>/channel/colony/queen. colony_shm = <shm>/ai/ant_<fae>/<colony_nom>,
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

# the colony's inbox file (the queen monitors it; the fae writes FAE and FAE DRONE
# lines here).
export def colony_inbox [colony_identity: string, colony_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_identity $colony_session_nom "channel" "colony" "inbox.txt"
}

# the queen's packet input dir (the fae writes queen-bound packets here; the queen
# reads them).
export def queen_input_dir [colony_identity: string, colony_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_identity $colony_session_nom "channel" "colony" "queen"
}

# the colony's outbox file (the queen writes its COLONY lines here) - this is the
# fae's inbox, which the fae monitors.
export def colony_outbox [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "colony" "inbox.txt"
}

# the queen's packet output dir on the fae side (the queen writes its packets to
# the fae here).
export def queen_output_dir [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "colony" "queen"
}
