# Fae-side tooling addressing the bonded colony.
#
# start_drone / stop_drone request the queen to spawn / stop a drone. The comms
# colony's queen lives under `channel`; per-drone send lives under `drone`. See
# empower:fae/colony/channel and empower:fae/colony/drone/channel.
export use ./start_drone.nu
export use ./stop_drone.nu
export module channel
export module drone
