# Shared derivation for the drone-side channel tools (empower:ant/drone/channel).
#
# A drone is a queen-launched teammate of the ant colony; it shares the colony's
# ai_identity (ant_<fae>) and session_nom. The queen invokes open/ready/close to
# manage the drone's channel; the drone invokes syn/ack for its own outbound. The
# drone has no inbox file and does not monitor - the queen relays inbound lines to
# it. The drone reads fae-sent packets from its input dir
# <colony_shm>/channel/colony/drone/<name>. The drone writes its COLONY DRONE lines
# to the colony outbox <fae_shm>/channel/colony/inbox.txt (the fae's inbox) and its
# packets to its output dir <fae_shm>/channel/colony/drone/<name>.

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

# the drone's packet input dir (the fae writes drone-bound packets here; the drone
# reads them).
export def drone_input_dir [colony_identity: string, colony_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_identity $colony_session_nom "channel" "colony" "drone" $drone_name
}

# the colony's outbox file (the drone writes its COLONY DRONE lines here) - this is
# the fae's inbox, which the fae monitors.
export def colony_outbox [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "colony" "inbox.txt"
}

# the drone's packet output dir on the fae side (the drone writes its packets to
# the fae here).
export def drone_output_dir [fae: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "colony" "drone" $drone_name
}
