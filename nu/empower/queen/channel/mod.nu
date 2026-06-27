# Queen-side comms channel ops for the bonded fae (empower:queen/channel).
#
# SYN/ACK/ONLINE/OFFLINE over the file-mailbox, with every path derived from the
# bonded fae's identity via the well-known claudeline + shm layout - the caller
# passes only the fae id and a tx/rx id, never a path. Call targets: open, syn,
# ack, close; shared derivation lives in common.nu.
export use ./open.nu
export use ./syn.nu
export use ./ack.nu
export use ./close.nu
