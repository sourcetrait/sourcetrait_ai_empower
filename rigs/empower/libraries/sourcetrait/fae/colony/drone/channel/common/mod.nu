# Shared derivation for fae-side per-drone channel
# (sourcetrait/fae:colony/drone/channel).
#
# The fae sends to an individual colony drone via the queen relay: the request
# packet goes to the drone's input dir <colony_shm>/channel/colony/drone/<name>, and
# the FAE DRONE control line goes to the colony inbox
# <colony_shm>/channel/colony/inbox.txt (the queen monitors it and relays the line to
# the drone teammate). The drone's responses arrive as COLONY DRONE lines on the
# fae's inbox (sourcetrait/fae:colony/channel), with packets in the fae's
# drone-packet dir <fae_shm>/channel/colony/drone/<name>. Send-only.

export def queen_ai_id [ai_id: string]: nothing -> string {
    $"ant_($ai_id)"
}

export def session_nom [identity: string]: nothing -> oneof<string, nothing> {
    let ctx = ($env.XDG_CACHE_HOME | path join "sourcetrait" "empower" "claudeline" $identity "context" "latest.yaml")
    if ($ctx | path exists) {
        open --raw $ctx | decode | from yaml | get -o session_nom
    } else {
        null
    }
}

# the colony's inbox file (the fae writes its FAE DRONE lines here; the queen
# monitors it and relays them to the drone).
export def colony_inbox [queen_ai_id: string, queen_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $queen_ai_id $queen_session_nom "channel" "colony" "inbox.txt"
}

# the drone's input dir (the fae writes drone-bound packets here; the drone reads them).
export def drone_input_dir [queen_ai_id: string, queen_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $queen_ai_id $queen_session_nom "channel" "colony" "drone" $drone_name
}

# the fae's drone-packet dir (the fae reads the drone's response packets here).
export def fae_drone_dir [ai_id: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $ai_id $fae_session_nom "channel" "colony" "drone" $drone_name
}
