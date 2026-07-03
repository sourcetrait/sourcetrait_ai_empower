# Fae-side per-drone channel (sourcetrait/fae:colony/drone/channel).
#
# Send-only: the fae writes its request packet to the drone's input dir and the FAE
# DRONE control line to the colony inbox (the queen relays it to the drone). Drone
# responses arrive as COLONY DRONE lines on the fae's inbox
# (sourcetrait/fae:colony/channel), packets in the fae's drone-packet dir. Call
# targets: syn, ack; shared derivation in common.
export module syn
export module ack
export module common
