# Queen-invoked ops on a drone's bonded-fae channel (sourcetrait/queen:drone/comm).
#
# The queen manages its drone teammates' channels: `open_channel` sets one up
# before launch, `close_channel` is the manual teardown. The drone-invoked side
# of the same channel (ready/done/syn/ack) lives in sourcetrait/drone:comm;
# `close_channel` performs the same operation a persisted drone runs itself via
# sourcetrait/drone:comm:done.
export module open_channel
export use open_channel
export module close_channel
export use close_channel
