
# Whether an ai_identity has a live (running, pid-confirmed) claude session.
#
# For direct agent use; the comms channel:open calls consult empower:pid:list_ai
# directly rather than this wrapper.
export def main [args: record<ai_identity: string>]: nothing -> record<running: bool> {
    { running: ((empower pid list_ai null).sessions | where ai_identity == $args.ai_identity | is-not-empty) }
}
