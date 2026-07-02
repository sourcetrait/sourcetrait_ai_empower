# Fae-side tooling addressing the bonded colony.
#
# start_drone / stop_drone request the queen to spawn / stop a drone. The comms
# colony's queen lives under `channel`; per-drone send lives under `drone`. See
# empower:fae/colony/channel and empower:fae/colony/drone/channel.
export module start_drone
export module stop_drone
export module channel
export module drone
