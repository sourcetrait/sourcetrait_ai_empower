# Shared derivation for the fae-side per-drone channel tools (empower:fae/drone/channel).
#
# Mirror of empower:drone/channel from the fae's side: the fae addresses an
# individual queen-launched drone by drone_name. The drone shares the colony's
# ai_identity (ant_<ai_identity>) and the colony session_nom, so the drone's
# mailbox derives from ant_<ai_identity>'s claudeline context; the fae's own
# per-drone mailbox derives from its own ai_identity + session. File-mailbox
# layout: the fae's per-drone inbox (drone writes, fae reads) is
# <shm>/ai/<ai_identity>/<fae_session_nom>/channel/ant/bond/drone/<drone_name>; the
# drone's inbox (fae writes, drone reads) is
# <shm>/ai/ant_<ai_identity>/<colony_session_nom>/drone/<drone_name>/channel/fae/bond.
# input.txt is the control file in each, input/ holds the packets.

# the bonded colony's ai_identity (shared by the queen and all its drones).
export def colony-identity [ai_identity: string]: nothing -> string {
    $"ant_($ai_identity)"
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

# the fae's own per-drone inbox dir (drone writes, fae reads): control file is
# <dir>/input.txt, packets are under <dir>/input.
export def fae-input-base [ai_identity: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "ant" "bond" "drone" $drone_name
}

# the drone's inbox dir (fae writes, drone reads): control file is <dir>/input.txt,
# packets are under <dir>/input.
export def drone-output-base [colony_identity: string, colony_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_identity $colony_session_nom "drone" $drone_name "channel" "fae" "bond"
}
