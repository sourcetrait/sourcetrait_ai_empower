# Drone-invoked comms channel ops for the bonded fae (sourcetrait/drone:channel).
#
# A queen-launched drone shares the colony identity (ant_<fae>). The drone
# invokes ready/done/syn/ack for its lifecycle and outbound; the queen manages
# the channel itself via sourcetrait/queen:drone/channel:{open,close}. The
# drone has no inbox and does not monitor (the queen relays inbound to it).
# Lifecycle lines (COLONY DRONE <name> ONLINE/OFFLINE) go to both the colony
# inbox and the fae inbox; other lines go to the colony outbox (the fae's
# inbox). Shared derivation in sourcetrait/drone:common.
export module ready
export module done
export module syn
export module ack
