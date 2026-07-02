# Drone-side comms channel ops for the bonded fae (empower:ant/drone/channel).
#
# A queen-launched drone shares the colony identity (ant_<fae>). The queen invokes
# open/close to manage the drone's channel; the drone invokes ready/done/syn/ack for
# its lifecycle and outbound. The drone has no inbox and does not monitor (the queen
# relays inbound to it). Lifecycle lines (COLONY DRONE <name> ONLINE/OFFLINE) go to
# both the colony inbox and the fae inbox; other lines go to the colony outbox (the
# fae's inbox). Call targets: open, ready, done, syn, ack, close; shared derivation
# in common.
export module open
export module ready
export module done
export module syn
export module ack
export module close
export module common
