# Fae/colony comms channel ops: file-mailbox SYN/ACK over /dev/shm.
#
# Control-file line protocol for the ant/colony channel - SYN announces a
# packet the peer should read, ACK acknowledges one received. Currently: ack,
# syn.
export use ./ack.nu
export use ./syn.nu
