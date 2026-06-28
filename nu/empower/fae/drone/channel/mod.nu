# Fae-side comms channel ops for a bonded drone (empower:fae/drone/channel).
#
# Mirror of empower:drone/channel from the fae's side: the fae addresses an
# individual queen-launched drone by drone_name with SYN/ACK/ONLINE/OFFLINE over the
# file-mailbox, every path derived from the fae's own ai_identity + the drone_name
# via the well-known claudeline + shm layout - the caller passes only its
# ai_identity, a drone_name, and a tx/rx id, never a path. Call targets: open, syn,
# ack, close; shared derivation lives in common.nu.
export use ./open.nu
export use ./syn.nu
export use ./ack.nu
export use ./close.nu
