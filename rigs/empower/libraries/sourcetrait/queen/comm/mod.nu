# Queen-side comms channel ops for the bonded fae (sourcetrait/queen:comm).
#
# The queen (ant colony leader) drives COLONY SYN/ACK/ONLINE/OFFLINE to the
# colony outbox (the fae's inbox), and reads FAE lines from the colony inbox
# (which it monitors). Every path derives from the bonded fae's identity + the
# colony (ant_<fae>) via the claudeline + shm layout. Call targets:
# open_channel, syn, ack, close_channel; shared derivation in
# sourcetrait/queen:common.
export module open_channel
export use open_channel
export module syn
export use syn
export module ack
export use ack
export module close_channel
export use close_channel
