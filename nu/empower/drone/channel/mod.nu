# Drone-side comms channel ops for the bonded fae (empower:drone/channel).
#
# Mirror of empower:fae/colony/channel from a queen-launched drone's side:
# DRONE ONLINE/SYN/ACK/OFFLINE over the file-mailbox. The drone has no claudeline
# session of its own, so its session_nom (the colony's) + drone_name are passed
# (queen-supplied); only the bonded fae's nom is context-read. Call targets:
# open, syn, ack, close; shared derivation in common.nu.
export use ./open.nu
export use ./syn.nu
export use ./ack.nu
export use ./close.nu
