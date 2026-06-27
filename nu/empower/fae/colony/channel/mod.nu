# Fae-side comms channel ops for the bonded colony (empower:fae/colony/channel).
#
# Mirror of empower:queen/channel from the fae's side: SYN/ACK/ONLINE/OFFLINE over
# the file-mailbox, every path derived from the fae's own ai_identity via the
# well-known claudeline + shm layout - the caller passes only its ai_identity and
# a tx/rx id, never a path. Call targets: open, syn, ack, close; shared derivation
# lives in common.nu.
export use ./open.nu
export use ./syn.nu
export use ./ack.nu
export use ./close.nu
