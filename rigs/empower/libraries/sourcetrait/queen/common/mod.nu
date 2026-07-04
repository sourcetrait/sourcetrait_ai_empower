# Shared derivation for the queen role's tools (sourcetrait/queen:common).
#
# The queen (the ant colony's leader) talks to the bonded fae over the colony
# channel and manages its drones' channels. The colony machinery shared with
# the drone role wraps sourcetrait/ant; the queen-specific packet dirs live
# here. Paths mirror the queen role config (queen.yaml). The colony's single
# inbox <colony_shm>/channel/colony/inbox.txt is the queen's inbox - it
# monitors it; the fae writes its FAE and FAE DRONE lines there (the queen
# relays drone lines to the drone teammates). The queen writes its COLONY
# lines to the colony's outbox <fae_shm>/channel/colony/inbox.txt, which is
# the fae's inbox (the fae monitors it). Packets: the fae writes queen-bound
# packets to the queen's input dir <colony_shm>/channel/colony/queen; the
# queen writes its packets to the fae at its output dir
# <fae_shm>/channel/colony/queen. colony_shm = <shm>/ai/ant_<fae>/<colony_nom>,
# fae_shm = <shm>/ai/<fae>/<fae_nom>.

use rig/sourcetrait/ant/channel

# the colony's ai_id, derived from the bonded fae's identity.
export def colony_ai_id [fae: string]: nothing -> string {
    channel colony_ai_id $fae
}

# an entity's current session_nom from its claudeline context/latest.yaml, or
# null when it has no context file (no live session).
export def session_nom [identity: string]: nothing -> oneof<string, nothing> {
    channel session_nom $identity
}

# the colony's inbox file (the queen monitors it; the fae writes FAE and FAE
# DRONE lines here).
export def colony_inbox [colony_ai_id: string, colony_session_nom: string]: nothing -> string {
    channel colony_inbox $colony_ai_id $colony_session_nom
}

# the colony's outbox file (the queen writes its COLONY lines here) - this is
# the fae's inbox, which the fae monitors.
export def colony_outbox [fae: string, fae_session_nom: string]: nothing -> string {
    channel colony_outbox $fae $fae_session_nom
}

# a drone's packet input dir (the fae writes drone-bound packets here; the
# drone reads them).
export def drone_input_dir [colony_ai_id: string, colony_session_nom: string, drone_name: string]: nothing -> string {
    channel drone_input_dir $colony_ai_id $colony_session_nom $drone_name
}

# a drone's packet output dir on the fae side (the drone writes its packets
# to the fae here).
export def drone_output_dir [fae: string, fae_session_nom: string, drone_name: string]: nothing -> string {
    channel drone_output_dir $fae $fae_session_nom $drone_name
}

# announce a drone lifecycle status on both inboxes (the colony inbox always,
# the fae inbox if the fae is online and it exists).
export def announce_drone [
    fae: string,
    drone_name: string,
    colony_ai_id: string,
    colony_session_nom: string,
    fae_session_nom: oneof<string, nothing>,
    status: string,
]: nothing -> nothing {
    channel announce_drone $fae $drone_name $colony_ai_id $colony_session_nom $fae_session_nom $status
}

# the queen's packet input dir (the fae writes queen-bound packets here; the
# queen reads them).
export def queen_input_dir [colony_ai_id: string, colony_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $colony_ai_id $colony_session_nom "channel" "colony" "queen"
}

# the queen's packet output dir on the fae side (the queen writes its packets
# to the fae here).
export def queen_output_dir [fae: string, fae_session_nom: string]: nothing -> string {
    $env.XDGX_SHM_DIR | path join "ai" $fae $fae_session_nom "channel" "colony" "queen"
}
