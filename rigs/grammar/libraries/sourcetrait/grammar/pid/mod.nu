# Live AI session detection for the grammar platform (sourcetrait/grammar:pid).
#
# Internal helpers wrapped by the agent-facing sourcetrait/grammar:ai module and
# used by the comms open_channel calls to gate peer-online on a live process. A
# live session is a running claude process confirmed against the pid claudeline
# records in its status yaml; live ps is authoritative (a status yaml alone is
# only the last write).
export module list_ai
export use list_ai
export module common
