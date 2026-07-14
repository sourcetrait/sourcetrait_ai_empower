# Fae-side tooling addressing the bonded colony.
#
# start_drone / stop_drone request the queen to spawn / stop a drone. The comms
# colony's queen lives under `comm`; per-drone send lives under `drone`. See
# sourcetrait/fae:colony/comm and sourcetrait/fae:colony/drone/comm.
export module start_drone
export use start_drone
export module stop_drone
export use stop_drone
export module comm
export module drone
