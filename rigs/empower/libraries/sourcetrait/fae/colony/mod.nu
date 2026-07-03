# Fae-side tooling addressing the bonded colony.
#
# start_drone / stop_drone request the queen to spawn / stop a drone. The comms
# colony's queen lives under `channel`; per-drone send lives under `drone`. See
# sourcetrait/fae:colony/channel and sourcetrait/fae:colony/drone/channel.
export module start_drone
export module stop_drone
export module channel
export module drone
