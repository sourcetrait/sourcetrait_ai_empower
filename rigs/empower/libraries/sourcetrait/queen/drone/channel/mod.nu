# Queen-invoked ops on a drone's bonded-fae channel (sourcetrait/queen:drone/channel).
#
# The queen manages its drone teammates' channels: `open` sets one up before
# launch, `close` is the manual teardown. The drone-invoked side of the same
# channel (ready/done/syn/ack) lives in sourcetrait/drone:channel; `close`
# performs the same operation a persisted drone runs itself via
# sourcetrait/drone:channel:done.
export module open
export module close
