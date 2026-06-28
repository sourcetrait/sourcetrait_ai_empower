# Shared derivation for the drone-side channel tools (empower:drone/channel).
#
# A drone is a queen-launched teammate with no claudeline session of its own: its
# session_nom (the colony's) and drone_name are GIVEN by the queen, so the
# call-targets take {fae, session_nom, drone_name} and only the bonded fae's nom
# is context-read. The colony ai_identity is ant_<fae>. File-mailbox layout: the
# drone's inbox (fae writes, drone reads) is
# <shm>/ai/ant_<fae>/<session_nom>/drone/<drone_name>/channel/fae/bond; the fae's
# per-drone inbox (drone writes, fae reads) is
# <shm>/ai/<fae>/<fae_session_nom>/channel/ant/bond/drone/<drone_name>. input.txt
# is the control file in each, input/ holds the packets.

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
export def drone-input-base [fae: string, session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $"ant_($fae)" $session_nom "drone" $drone_name "channel" "fae" "bond"
}

# the fae's per-drone inbox dir (drone writes, fae reads): control file is
# <dir>/input.txt, packets are under <dir>/input.
export def fae-output-base [fae: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "bond" "drone" $drone_name
}
