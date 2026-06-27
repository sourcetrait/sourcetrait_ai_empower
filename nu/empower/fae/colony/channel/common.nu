# Shared derivation for the fae-side colony channel tools (empower:fae/colony/channel).
#
# Mirror of the queen side from the fae's perspective: every path derives from the
# fae's own ai_identity plus the well-known claudeline + shm layout. The bonded
# colony's ai_identity is ant_<ai_identity>; each side's current session_nom is
# read from its claudeline context/latest.yaml. File-mailbox layout: the fae's
# inbox (queen writes, fae reads) is
# <shm>/ai/<ai_identity>/<fae_session_nom>/channel/ant/bond/queen; the queen's
# inbox (fae writes, queen reads) is
# <shm>/ai/ant_<ai_identity>/<queen_session_nom>/queen/channel/fae/bond. input.txt
# is the control file in each, input/ holds the packets.

# the bonded colony's ai_identity, derived from the fae's identity.
export def queen-identity [ai_identity: string]: nothing -> string {
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

# the fae's own inbox dir (queen writes, fae reads): control file is
# <dir>/input.txt, packets are under <dir>/input.
export def fae-input-base [ai_identity: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_identity $fae_session_nom "channel" "ant" "bond" "queen"
}

# the queen's inbox dir (fae writes, queen reads): control file is
# <dir>/input.txt, packets are under <dir>/input.
export def queen-output-base [queen_identity: string, queen_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $queen_identity $queen_session_nom "queen" "channel" "fae" "bond"
}
