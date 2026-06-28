# Drone-side comms channel ops for the bonded fae (empower:ant/drone/channel).
#
# A queen-launched drone shares the colony identity (ant_<fae>). The queen
# invokes open/ready/close to manage the drone's channel + relay its presence;
# the drone invokes syn/ack for its own outbound packets. All COLONY DRONE lines
# land in the fae's single shared control file; the drone has no control file of
# its own (the queen relays inbound to it). Call targets: open, ready, syn, ack,
# close; shared derivation in common.nu.
export use ./open.nu
export use ./ready.nu
export use ./syn.nu
export use ./ack.nu
export use ./close.nu
