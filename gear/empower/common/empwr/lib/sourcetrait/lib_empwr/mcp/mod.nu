# nushell_mcp administration from the user's shell (`empwr mcp ...`).
#
# Wraps `nushell_mcp [--id ..] [--namespace ..] cli <tool> ...` and parses the
# machine JSON the cli emits when piped into real nu values: data-bearing
# results come back as records/tables, exec-shaped results unwrap, and an
# error envelope becomes `error make` carrying the diagnostics. This module is
# the cli's ONLY consumer - the user reaches nushell_mcp through `empwr mcp`,
# and the agent drives the MCP tools directly, never the cli.
#
# An empty --id / --namespace (the default) means the binary's own default
# state coordinate; --timeout-ms 0 (the default) means the binary's default.

# run one cli invocation and parse its envelope; error make on a failure.
def mcp_cli [id: string, namespace: string, cli_args: list<string>]: nothing -> any {
    mut argv: list<string> = []
    if $id != "" { $argv = ($argv | append ["--id" $id]) }
    if $namespace != "" { $argv = ($argv | append ["--namespace" $namespace]) }
    let argv = ($argv | append "cli" | append $cli_args)
    let outcome = (^nushell_mcp ...$argv | complete)
    let raw = ($outcome.stdout | str trim)
    let parsed: any = (if ($raw | is-empty) { null } else { $raw | from json })
    if $outcome.exit_code != 0 {
        error make { msg: $"nushell_mcp cli failed: (mcp_error_detail $parsed $outcome.stderr)" }
    }
    $parsed
}

# render an error envelope's diagnostics; fall back to stderr.
def mcp_error_detail [parsed: any, stderr: string]: nothing -> string {
    let envelope_errors: any = (if ($parsed | describe | str starts-with "record") {
        $parsed | get -o error.errors
    } else {
        null
    })
    if ($envelope_errors != null) and (($envelope_errors | length) > 0) {
        $envelope_errors | each {|row| $"($row.kind): ($row.message)" } | str join "; "
    } else if not ($stderr | str trim | is-empty) {
        $stderr | str trim
    } else {
        "no diagnostics"
    }
}

# append --timeout-ms to a cli argv when a non-default timeout was given.
def with_timeout [cli_args: list<string>, timeout_ms: int]: nothing -> list<string> {
    if $timeout_ms > 0 {
        $cli_args | append ["--timeout-ms" ($timeout_ms | into string)]
    } else {
        $cli_args
    }
}

# Versions, plugins, and libraries summary.
export def info [--id: string = "", --namespace: string = ""]: nothing -> record {
    mcp_cli $id $namespace ["info"]
}

# Documentation of one node by namepath (library, library:module/path, or
# library:module/path:function).
export def inspect [namepath: string, --id: string = "", --namespace: string = ""]: nothing -> record {
    mcp_cli $id $namespace ["inspect" $namepath]
}

# Invoke a committed library function; returns the result value itself.
export def call [namepath: string, args: record = {}, --id: string = "", --namespace: string = "", --timeout-ms: int = 0]: nothing -> any {
    let cli_args = (with_timeout ["call" $namepath ($args | to nuon)] $timeout_ms)
    (mcp_cli $id $namespace $cli_args) | get result
}

# Evaluate a source-code body on a stateless worker; returns the full
# {result, nonce, rerun_id} envelope (rerun_id feeds `mcp rerun`).
export def run [body: string, --args-schema: record = {}, --result-schema: record = {}, --args: record = {}, --id: string = "", --namespace: string = "", --timeout-ms: int = 0]: nothing -> record {
    let cli_args = (with_timeout [
        "run" $body
        "--args-schema" ($args_schema | to nuon)
        "--result-schema" ($result_schema | to nuon)
        "--args" ($args | to nuon)
    ] $timeout_ms)
    mcp_cli $id $namespace $cli_args
}

# Evaluate a body on a stateful worker. Single-shot: the session state dies
# with the invocation; returns the {result, nonce} envelope.
export def interact [body: string, --args-schema: record = {}, --result-schema: record = {}, --args: record = {}, --id: string = "", --namespace: string = "", --timeout-ms: int = 0]: nothing -> record {
    let cli_args = (with_timeout [
        "interact" $body
        "--args-schema" ($args_schema | to nuon)
        "--result-schema" ($result_schema | to nuon)
        "--args" ($args | to nuon)
    ] $timeout_ms)
    mcp_cli $id $namespace $cli_args
}

# Re-evaluate a cached run() body with fresh args; returns the result value.
export def rerun [rerun_id: string, args: record = {}, --id: string = "", --namespace: string = "", --timeout-ms: int = 0]: nothing -> any {
    let cli_args = (with_timeout ["rerun" $rerun_id ($args | to nuon)] $timeout_ms)
    (mcp_cli $id $namespace $cli_args) | get result
}

# List in-flight usage. Process-scoped: a one-shot invocation shows none.
export def processes [--id: string = "", --namespace: string = ""]: nothing -> table {
    (mcp_cli $id $namespace ["processes"]) | get processes
}

# Cancel an in-flight usage by nonce. Process-scoped.
export def kill [nonce: string, --id: string = "", --namespace: string = ""]: nothing -> nothing {
    mcp_cli $id $namespace ["kill" $nonce] | ignore
}

# Generate the /nu skill at <harness_dir>/skills/nu/SKILL.md.
export def learn [harness_dir: path, --id: string = "", --namespace: string = ""]: nothing -> record {
    mcp_cli $id $namespace ["learn" ($harness_dir | into string)]
}

# Scaffold module / function skeletons by namepath into established libraries.
export def new [...namepaths: string, --id: string = "", --namespace: string = ""]: nothing -> record {
    mcp_cli $id $namespace (["new"] | append $namepaths)
}

# Validate + promote a library's source tree into the store.
export def commit [library: string, --id: string = "", --namespace: string = ""]: nothing -> record {
    mcp_cli $id $namespace ["commit" $library]
}

# Library administration: new, install, check, uninstall.
export def library [action: string, library: string, source_dir: path, --id: string = "", --namespace: string = ""]: nothing -> any {
    mcp_cli $id $namespace ["library" $action $library ($source_dir | into string)]
}
