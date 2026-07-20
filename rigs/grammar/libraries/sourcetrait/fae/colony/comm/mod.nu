# Fae-side comms channel to the colony's queen (sourcetrait/fae:colony/comm).
#
# The fae drives FAE SYN/ACK/ONLINE/OFFLINE to the colony inbox, and receives all
# COLONY (queen) and COLONY DRONE (drone) lines on its own inbox, which it monitors.
# Every path derives from the fae's own ai_id via the claudeline + shm layout.
# Call targets: open_channel, syn, ack, close_channel; shared derivation in
# common. Per-drone send lives under `drone` (sourcetrait/fae:colony/drone/comm).
export module open_channel
export use open_channel
export module syn
export use syn
export module ack
export use ack
export module close_channel
export use close_channel
export module common
