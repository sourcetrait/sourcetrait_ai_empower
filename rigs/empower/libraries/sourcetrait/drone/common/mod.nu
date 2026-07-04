# Shared derivation for the drone role's tools (sourcetrait/drone:common).
#
# A drone is a queen-launched teammate of the ant colony; it shares the
# colony's ai_id (ant_<fae>) and session_nom. The queen invokes
# sourcetrait/queen:drone/channel:{open,close} to manage the drone's channel;
# the drone invokes channel:{ready,done,syn,ack} for its lifecycle and
# outbound. The drone has no inbox file and does not monitor - the queen
# relays inbound lines to it. The drone reads fae-sent packets from its input
# dir <colony_shm>/channel/colony/drone/<name>. The drone writes its COLONY
# DRONE lines to the colony outbox <fae_shm>/channel/colony/inbox.txt (the
# fae's inbox) and its packets to its output dir
# <fae_shm>/channel/colony/drone/<name>. The colony machinery shared with the
# queen role wraps sourcetrait/ant.

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

# the colony's outbox file (the drone writes its COLONY DRONE lines here) -
# this is the fae's inbox, which the fae monitors.
export def colony_outbox [fae: string, fae_session_nom: string]: nothing -> string {
    channel colony_outbox $fae $fae_session_nom
}

# the drone's packet input dir (the fae writes drone-bound packets here; the
# drone reads them).
export def drone_input_dir [colony_ai_id: string, colony_session_nom: string, drone_name: string]: nothing -> string {
    channel drone_input_dir $colony_ai_id $colony_session_nom $drone_name
}

# the drone's packet output dir on the fae side (the drone writes its packets
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
