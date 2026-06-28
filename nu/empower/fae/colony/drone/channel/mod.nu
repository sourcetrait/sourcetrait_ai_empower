# Fae-side per-drone send channel, via the queen (empower:fae/colony/drone/channel).
#
# Send-only: the fae writes its request packet straight to the drone's own inbox
# and the FAE DRONE control line to the queen's inbox (the queen relays it to the
# drone). Drone responses arrive on the fae's single colony inbox
# (empower:fae/colony/channel) as COLONY DRONE lines. Call targets: syn, ack;
# shared derivation in common.nu.
export use ./syn.nu
export use ./ack.nu
