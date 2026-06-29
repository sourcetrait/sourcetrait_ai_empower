# Fae-side tooling addressing the bonded colony.
#
# start_drone requests the queen to spawn a drone. The comms channel to the
# colony's queen lives under `channel`; per-drone send lives under `drone`. See
# empower:fae/colony/channel and empower:fae/colony/drone/channel.
export use ./start_drone.nu
export module channel
export module drone
