# Fae-side comms channel to the colony's queen (sourcetrait/fae:colony/channel).
#
# The fae drives FAE SYN/ACK/ONLINE/OFFLINE to the colony inbox, and receives all
# COLONY (queen) and COLONY DRONE (drone) lines on its own inbox, which it monitors.
# Every path derives from the fae's own ai_id via the claudeline + shm layout.
# Call targets: open, syn, ack, close; shared derivation in common. Per-drone
# send lives under `drone` (sourcetrait/fae:colony/drone/channel).
export module open
export module syn
export module ack
export module close
export module common
