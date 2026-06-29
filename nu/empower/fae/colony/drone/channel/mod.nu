# Fae-side per-drone channel (empower:fae/colony/drone/channel).
#
# Send-only: the fae writes its request packet to the drone's input dir and the FAE
# DRONE control line to the colony inbox (the queen relays it to the drone). Drone
# responses arrive as COLONY DRONE lines on the fae's inbox
# (empower:fae/colony/channel), packets in the fae's drone-packet dir. Call targets:
# syn, ack; shared derivation in common.nu.
export use ./syn.nu
export use ./ack.nu
