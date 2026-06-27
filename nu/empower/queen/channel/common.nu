# Shared derivation for the queen-side channel tools (empower:queen/channel).
#
# All channel paths derive from the bonded fae's identity plus the well-known
# claudeline + shm layout, so the call-targets take only the fae id (and a
# tx/rx id). The queen's own ai_identity is ant_<fae>; each side's current
# session_nom is read from its claudeline context/latest.yaml. File-mailbox
# layout: the queen's inbox (fae writes, queen reads) is
# <shm>/ai/ant_<fae>/<queen_session_nom>/queen/channel/fae/bond; the fae's inbox
# (queen writes, fae reads) is <shm>/ai/<fae>/<fae_session_nom>/channel/ant/bond/
# queen. input.txt is the control file in each, input/ holds the packets.

# the queen's ai_identity, derived from the bonded fae's identity.
export def queen-identity [fae: string]: nothing -> string {
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

# the queen's own inbox dir (fae writes, queen reads): control file is
# <dir>/input.txt, packets are under <dir>/input.
export def queen-input-base [queen_identity: string, queen_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $queen_identity $queen_session_nom "queen" "channel" "fae" "bond"
}

# the fae's inbox dir (queen writes, fae reads): control file is <dir>/input.txt,
# packets are under <dir>/input.
export def fae-output-base [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "ant" "bond" "queen"
}
