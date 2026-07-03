# Comms machinery shared by the colony roles (sourcetrait/ant:channel).
#
# The derivations the queen and drone roles both need: the colony identity, an
# entity's live session_nom, the colony-side and fae-side inbox files, the
# per-drone packet dirs, and the drone lifecycle announcement. The role
# libraries (sourcetrait/queen, sourcetrait/drone) wrap these in their own
# `common` modules; role call targets import their role common, not this
# module. The fae side (sourcetrait/fae) deliberately keeps its own
# derivations - no fae<->ant dependencies.

# the colony's ai_id, derived from the bonded fae's identity.
export def colony_ai_id [fae: string]: nothing -> string {
    $"ant_($fae)"
}

# an entity's current session_nom from its claudeline context/latest.yaml, or
# null when it has no context file (no live session).
export def session_nom [identity: string]: nothing -> oneof<string, nothing> {
    let ctx = ($env.XDG_CACHE_HOME | path join "sourcetrait" "empower" "claudeline" $identity "context" "latest.yaml")
    if ($ctx | path exists) {
        open --raw $ctx | decode | from yaml | get -o session_nom
    } else {
        null
    }
}

# the colony's inbox file (the queen monitors it; FAE lines and drone
# lifecycle lines land here).
export def colony_inbox [colony_ai_id: string, colony_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_ai_id $colony_session_nom "channel" "colony" "inbox.txt"
}

# the colony's outbox file (COLONY lines are written here) - this is the
# fae's inbox, which the fae monitors.
export def colony_outbox [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "colony" "inbox.txt"
}

# a drone's packet input dir on the colony side (the fae writes drone-bound
# packets here; the drone reads them).
export def drone_input_dir [colony_ai_id: string, colony_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_ai_id $colony_session_nom "channel" "colony" "drone" $drone_name
}

# a drone's packet output dir on the fae side (the drone writes its packets
# to the fae here).
export def drone_output_dir [fae: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "colony" "drone" $drone_name
}

# announce a drone lifecycle status as "COLONY DRONE <name> <status>" on BOTH
# inboxes: the colony inbox (the queen's) and the colony outbox (the fae's
# inbox). Each is written only if it exists; a null fae_session_nom (fae
# offline) skips the fae side.
export def announce_drone [
    fae: string,
    drone_name: string,
    colony_ai_id: string,
    colony_session_nom: string,
    fae_session_nom: oneof<string, nothing>,
    status: string,
]: nothing -> nothing {
    let line = $"COLONY DRONE ($drone_name) ($status)(char nl)"
    let c_inbox = (colony_inbox $colony_ai_id $colony_session_nom)
    if ($c_inbox | path exists) {
        $line | save --append $c_inbox
    }
    if $fae_session_nom != null {
        let outbox = (colony_outbox $fae $fae_session_nom)
        if ($outbox | path exists) {
            $line | save --append $outbox
        }
    }
}
