# Drone-side comms channel ops for the bonded fae (empower:ant/drone/channel).
#
# A queen-launched drone shares the colony identity (ant_<fae>). The queen invokes
# open/ready/close to manage the drone's channel; the drone invokes syn/ack for its
# own outbound. The drone has no inbox and does not monitor (the queen relays inbound
# to it). The drone writes its COLONY DRONE lines to the colony outbox (the fae's
# inbox) and its packets to its output dir on the fae side. Call targets: open,
# ready, syn, ack, close; shared derivation in common.nu.
export use ./open.nu
export use ./ready.nu
export use ./syn.nu
export use ./ack.nu
export use ./close.nu
