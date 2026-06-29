# Drone-side comms channel ops for the bonded fae (empower:ant/drone/channel).
#
# A queen-launched drone shares the colony identity (ant_<fae>). The queen invokes
# open/ready/close to manage the drone's channel; the drone invokes syn/ack for its
# own outbound packets. The drone writes COLONY DRONE lines to the fae's per-drone
# inbox (channel/ant/drone/<name>/input); the fae writes to the drone's own inbox
# (drone/<name>/channel/input). Call targets: open, ready, syn, ack, close; shared
# derivation in common.nu.
export use ./open.nu
export use ./ready.nu
export use ./syn.nu
export use ./ack.nu
export use ./close.nu
