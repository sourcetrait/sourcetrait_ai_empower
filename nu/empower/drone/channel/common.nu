# Shared derivation for the drone-side channel tools (empower:drone/channel).
#
# A drone is a queen-launched teammate that SHARES the colony's ai_identity
# (ant_<fae>), so - exactly like the queen lib - it derives its own session_nom
# from ant_<fae>'s claudeline context; the call-targets take only {fae,
# drone_name}. File-mailbox layout: the drone's inbox (fae writes, drone reads) is
# <shm>/ai/ant_<fae>/<colony_session_nom>/drone/<drone_name>/channel/fae/bond; the
# fae's per-drone inbox (drone writes, fae reads) is
# <shm>/ai/<fae>/<fae_session_nom>/channel/ant/bond/drone/<drone_name>. input.txt
# is the control file in each, input/ holds the packets.

# the colony's ai_identity (shared by the queen and all its drones).
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

# the drone's own inbox dir (fae writes, drone reads): control file is
# <dir>/input.txt, packets are under <dir>/input.
export def drone-input-base [colony_identity: string, colony_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_identity $colony_session_nom "drone" $drone_name "channel" "fae" "bond"
}

# the fae's per-drone inbox dir (drone writes, fae reads): control file is
# <dir>/input.txt, packets are under <dir>/input.
export def fae-output-base [fae: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "bond" "drone" $drone_name
}
