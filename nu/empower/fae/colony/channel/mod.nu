# Fae-side comms channel to the colony's queen (empower:fae/colony/channel).
#
# The fae drives FAE SYN/ACK/ONLINE/OFFLINE to the queen's inbox and receives
# COLONY lines from the queen on its colony inbox control file
# (channel/ant/colony/input.txt), which the fae monitors itself. Every path derives
# from the fae's own ai_identity via the claudeline + shm layout. Call targets:
# open, syn, ack, close; shared derivation in common.nu. Per-drone send lives under
# `drone` (empower:fae/colony/drone/channel).
export use ./open.nu
export use ./syn.nu
export use ./ack.nu
export use ./close.nu
