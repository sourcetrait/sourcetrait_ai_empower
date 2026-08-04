
export def main [fill: record<agent: record<kind: string>, ai: record<identity: string, email: string>, user: record<handle: string, full_name: string, informal: string>>]: nothing -> record<mv: table<from: path, to: path>> {
    let instructions_filename = match $fill.agent.kind {
        "claude" => "CLAUDE.md"
        _ => "AGENTS.md"
    }

    {
        mv: [ { from: "INSTRUCTIONS.md", to: $instructions_filename } ]
    }
}