# Fae-side per-drone channel (empower:fae/colony/drone/channel).
#
# Send-only outbound: the fae writes its request packet and the FAE DRONE control
# line straight to the drone's own inbox (drone/<name>/channel/input). Drone
# responses arrive on the fae's per-drone inbox (channel/ant/drone/<name>/input.txt),
# monitored as COLONY DRONE lines. Call targets: syn, ack; shared derivation in
# common.nu.
export use ./syn.nu
export use ./ack.nu
