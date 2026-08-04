# Drone role library (sourcetrait/drone): a queen-launched colony teammate.
#
# The drone-invoked side of its bonded-fae channel lives under `comm`
# (ready/done/syn/ack); the queen-invoked open_channel/close_channel live in
# sourcetrait/queen:drone/comm. Shared derivations in `common` (wrapping
# sourcetrait/ant).
export module comm
export module common
