use rig/sourcetrait/empower/pid


# Whether an ai_id has a live (running, pid-confirmed) claude session.
#
# For direct agent use; the comms channel:open calls consult empower:pid:list_ai
# directly rather than this wrapper.
export def main [args: record<ai_id: string>]: nothing -> record<running: bool> {
    { running: ((pid list_ai null).sessions | where ai_id == $args.ai_id | is-not-empty) }
}
