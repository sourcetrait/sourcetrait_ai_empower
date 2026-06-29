# Queen-side comms channel ops for the bonded fae (empower:ant/queen/channel).
#
# The queen (ant colony leader) drives COLONY SYN/ACK/ONLINE/OFFLINE to the colony
# outbox (the fae's inbox), and reads FAE lines from the colony inbox (which it
# monitors). Every path derives from the bonded fae's identity + the colony
# (ant_<fae>) via the claudeline + shm layout. Call targets: open, syn, ack, close;
# shared derivation in common.nu.
export use ./open.nu
export use ./syn.nu
export use ./ack.nu
export use ./close.nu
