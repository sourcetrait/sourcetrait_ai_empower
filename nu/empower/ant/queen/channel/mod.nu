# Queen-side comms channel ops for the bonded fae (empower:ant/queen/channel).
#
# The queen (ant colony leader) drives SYN/ACK/ONLINE/OFFLINE over the file-
# mailbox: it reads FAE lines from its own inbox and writes COLONY lines to the
# fae's single shared control file. Every path derives from the bonded fae's
# identity + the colony (ant_<fae>) via the claudeline + shm layout. Call
# targets: open, syn, ack, close; shared derivation in common.nu.
export use ./open.nu
export use ./syn.nu
export use ./ack.nu
export use ./close.nu
