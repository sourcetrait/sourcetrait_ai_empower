# Live AI session detection for the empower harness (empower:pid).
#
# Internal helpers wrapped by the agent-facing empower:ai module and used by the
# comms channel:open calls to gate peer-online on a live process. A live session is
# a running claude process confirmed against the pid claudeline records in its
# status yaml; live ps is authoritative (a status yaml alone is only the last write).
export module list_ai
export module common
