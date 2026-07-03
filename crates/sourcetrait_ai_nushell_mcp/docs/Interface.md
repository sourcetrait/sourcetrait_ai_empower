# MCP Interface

## Server

One binary, one variant surface, selected at runtime by trusted operator
config (the `.mcp.json` server entry's `args`, or the invoking command
line):

```
nushell_mcp [--id <string>] [--namespace <string>] [--deny <csv>]
```

- `--id` (default: `$USER`) - the agent identity owning the state store.
- `--namespace` (default: `default`) - the state namespace within the
  id's store.
- `--deny` - comma-separated tools to withhold from the surface. The
  deniable set: `run, rerun, interact, call, learn, new, commit,
  library`; the core four (`info`, `inspect`, `processes`, `kill`)
  always register. A denied tool is ABSENT from tools/list (never
  registered); calling it anyway fails at the protocol layer. There is
  no implication between tokens - denying `run` does not deny `rerun`;
  list both when both are meant. An unknown token fails startup.

The values are trusted config, not validated input: a malformed id or
namespace surfaces as the natural downstream error (improper
configuration).

Every store is fully private per `(id, namespace)`:

```
$XDG_DATA_HOME/sourcetrait/nushell_mcp/<id>/<namespace>/{keypair,libraries}
$XDG_CACHE_HOME/sourcetrait/nushell_mcp/<id>/<namespace>/{runs,interacts,calls,closures}
```

Workers receive the coordinate as spawn env, so bodies and committed
call-targets read `$env.NUSHELL_MCP_ID` / `$env.NUSHELL_MCP_NAMESPACE`
ambiently; `info()` reports the same pair.

Example `.mcp.json` entries (one binary, two channels):

```json
{
  "mcpServers": {
    "nushell": {
      "type": "stdio",
      "command": "/path/to/nushell_mcp",
      "args": ["--id", "emptwo"]
    },
    "nushell_mcp_test": {
      "type": "stdio",
      "command": "/path/to/build/nushell_mcp",
      "args": ["--id", "emptwo", "--namespace", "test"]
    }
  }
}
```

## One-shot CLI

`nushell_mcp cli <tool> ...` runs ONE tool in-process against the
configured store and prints the envelope as bare compact JSON on
stdout - one line, machine format, no color (`| from json` and
captured output are byte-clean) - no agent, no MCP client. The cli is
a wrapper's substrate, not a human display surface. Exit codes: 0
success, 1 error envelope, 2 unparseable input. Record-shaped INPUTS
are single-quoted NUON strings; an omitted args value is the empty
record.

```nu
nushell_mcp cli info
nushell_mcp --id emptwo cli inspect sourcetrait/empower:pid:list_ai
nushell_mcp cli call sourcetrait/geo:shape:area '{width: 3.0, height: 4.0}'
nushell_mcp cli library new sourcetrait/mylib ~/src/mylib
nushell_mcp cli commit sourcetrait/mylib
nushell_mcp cli run --args-schema '{x: int}' --args '{x: 5}' --result-schema '{out: int}' '{ out: ($args.x + 1) }'
```

All 12 tools are mirrored. Caveats: `interact` is single-shot (session
state dies with the process); `processes` / `kill` are process-scoped
(a one-shot invocation shows none); `--deny` does not apply (it gates
agent registration, not the operator surface). Writing into a store a
live agent host is using is the operator's own risk - git's index lock
keeps the library repo itself safe, but an in-flight call can
transiently fail.

## Overview
- [`run()`](#run) Evaluate a typed nushell source-code body on a stateless worker.
- [`interact()`](#interact) Evaluate a typed nushell source-code body on a persistent stateful worker.
- [`call()`](#call) Invoke a committed library function with typed args.
- [`rerun()`](#rerun) Re-evaluate a cached `run()` body with fresh args.
- [`processes()`](#processes) List in-flight MCP tool usage.
- [`kill()`](#kill) Cancel an in-flight usage by its nonce.
- [`info()`](#info) Versions, plugins, and libraries summary.
- [`inspect()`](#inspect) Detailed documentation of a specific callable library, module, function.
- [`new()`](#new) Scaffold modules / functions (by namepath) into existing libraries.
- [`commit()`](#commit) Commit the agent's library source-code to the MCP's repository for live use.
- [`library()`](#library) Library administration: new, install, check, uninstall.
- [`learn()`](#learn) Generate the latest `/nu` SKILL.md.


## `run()`
*Evaluate a typed nushell source-code body on a stateless worker.*

### arguments

Schema (partial):
```json
{
  "properties": {
    "args_schema":   { "type": "object", "additionalProperties": true },
    "result_schema": { "type": "object", "additionalProperties": true },
    "args":          { "type": "object", "additionalProperties": true },
    "body":          { "type": "string" },
    "timeout_ms":    { "type": ["integer", "null"], "format": "uint64", "minimum": 0 }
  },
  "required": ["args_schema", "result_schema", "args", "body"]
}
```

### Example

Nu equivalent:
```nu
# @returns record<count: int, first: record<name: string, ratio: float>>
def run [args: record<eldest: bool, tabs: table<name: string, age: int, ratio: float>>] {
    let chosen = if $args.eldest {
        $args.tabs | sort-by age | last
    } else {
        $args.tabs | first
    }

    {
        count: ($args.tabs | length),
        first: {
            name: $chosen.name,
            ratio: $chosen.ratio
        }
    }
}
```

MCP (partial):
```json
{
  "arguments": {
    "args_schema": { "eldest": "bool", "tabs": [{ "name": "string", "age": "int", "ratio": "float" }] },
    "result_schema": { "count": "int", "first": { "name": "string", "ratio": "float" } },
    "args": {
      "eldest": true,
      "tabs": [
        {"name": "cindy", "age": 29, "ratio": 0.42},
        {"name": "bob",   "age": 32, "ratio": 0.77}
      ]
    },
    "body": "let chosen = if $args.eldest {\n    $args.tabs | sort-by age | last\n} else {\n    $args.tabs | first\n}\n\n{\n    count: ($args.tabs | length),\n    first: {\n        name: $chosen.name,\n        ratio: $chosen.ratio\n    }\n}"
  }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "result": {
        "count": 2,
        "first": { "name": "bob", "ratio": 0.77 }
      },
      "nonce": "8jPtjMSfRVh",
      "rerun_id": "isaBZMrHho3"
    },
    "content": []
  }
}
```

## `interact()`
*Evaluate a typed nushell source-code body on a persistent stateful worker.*

### arguments

Schema (partial):
```json
{
  "properties": {
    "args_schema":   { "type": "object", "additionalProperties": true },
    "result_schema": { "type": "object", "additionalProperties": true },
    "args":          { "type": "object", "additionalProperties": true },
    "body":          { "type": "string" },
    "timeout_ms":    { "type": ["integer", "null"], "format": "uint64", "minimum": 0 }
  },
  "required": ["args_schema", "result_schema", "args", "body"]
}
```

### Example

Nu equivalent:
```nu
# @returns record<seeded: int>
def interact [args: record<value: int>] {
    $env.COUNTER = $args.value
    { seeded: $args.value }
}
```

MCP (partial):
```json
{
  "arguments": {
    "args_schema": { "value": "int" },
    "result_schema": { "seeded": "int" },
    "args": { "value": 10 },
    "body": "$env.COUNTER = $args.value\n{ seeded: $args.value }"
  }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "result": { "seeded": 10 },
      "nonce": "Xy12abc34"
    },
    "content": []
  }
}
```

## `call()`
*Invoke a committed library function with typed args.*

`namepath` is the function coordinate `library:module/path:function` (a
callable always lives in a module - there are no root functions). Discover
live targets + their schemas with [`info()`](#info) / [`inspect()`](#inspect).

### arguments

Schema (partial):
```json
{
  "properties": {
    "namepath":   { "type": "string" },
    "args":       { "type": "object", "additionalProperties": true },
    "timeout_ms": { "type": ["integer", "null"], "format": "uint64", "minimum": 0 }
  },
  "required": ["namepath", "args"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": {
    "namepath": "geo:shape:area",
    "args": { "width": 3.0, "height": 4.0 }
  }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "result": { "area": 12.0 },
      "nonce": "Hk39pXqRT2v"
    },
    "content": []
  }
}
```

## `rerun()`
*Re-evaluate a cached `run()` body with fresh args.*

The cached schemas + body are reused; only `args` changes per call.

`rerun_id` is the base62 id returned in a prior `run()` envelope.

### arguments

Schema (partial):
```json
{
  "properties": {
    "rerun_id":   { "type": "string" },
    "args":       { "type": "object", "additionalProperties": true },
    "timeout_ms": { "type": ["integer", "null"], "format": "uint64", "minimum": 0 }
  },
  "required": ["rerun_id", "args"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": {
    "rerun_id": "isaBZMrHho3",
    "args": { "x": 50 }
  }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "result": { "y": 100 },
      "nonce": "Tn4PdJG8mUq"
    },
    "content": []
  }
}
```

## `processes()`
*List in-flight MCP tool usage.*


### arguments

Schema (partial):
```json
{
  "properties": {},
  "required": []
}
```

### Example

MCP (partial):
```json
{
  "arguments": {}
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "processes": [
        {
          "nonce": "Bv7Ck3wMt2x",
          "tool": "run",
          "started_at": 1717250000000,
          "args": { "x": 21 }
        }
      ]
    },
    "content": []
  }
}
```

## `kill()`
*Cancel an in-flight usage by its nonce.*

SIGKILLs the worker holding the call. No payload; silently succeeds if
the nonce is unknown or already completed (race-safe).

### arguments

Schema (partial):
```json
{
  "properties": {
    "nonce": { "type": "string" }
  },
  "required": ["nonce"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "nonce": "Bv7Ck3wMt2x" }
}
```

Output (partial):
```json
{
  "result": { "content": [] }
}
```

## `info()`
*Versions, plugins, and libraries summary.*

### arguments

Schema (partial):
```json
{
  "properties": {},
  "required": []
}
```

### Example

MCP (partial):
```json
{
  "arguments": {}
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "name": "nushell_mcp",
      "version": "0.0.46",
      "nu_version": "0.113.1",
      "id": "emptwo",
      "namespace": "default",
      "plugins": [ ["polars", "0.112.2"], ["inc", null] ],
      "libraries": [
        {
          "name": "geo",
          "path": "/abs/path/to/source/geo",
          "summary": "planar geometry helpers",
          "modules": [
            {
              "name": "shape",
              "summary": "",
              "submodules": [],
              "functions": [
                {
                  "name": "area",
                  "summary": "result is in the inputs' unit, squared",
                  "args_schema": { "width": "float", "height": "float" },
                  "result_schema": { "area": "float" }
                }
              ]
            }
          ],
          "functions": []
        }
      ]
    },
    "content": []
  }
}
```

## `inspect()`
*Detailed documentation of a specific callable library, module, function.*

`namepath` is `library`, `library:module/path`, or
`library:module/path:function` - inspect any of the three arities.

### arguments

Schema (partial):
```json
{
  "properties": {
    "namepath": { "type": "string" }
  },
  "required": ["namepath"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "namepath": "geo:shape:area" }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "library": "geo",
      "module_path": "shape",
      "name": "area",
      "summary": "result is in the inputs' unit, squared",
      "args_schema": { "width": "float", "height": "float" },
      "result_schema": { "area": "float" },
      "details": "Planar rectangle only; negative inputs are a type-clean error."
    },
    "content": []
  }
}
```

## `new()`
*Scaffold modules / functions (by namepath) into existing libraries.*

Batch-scaffolds module (`library:module/path`) or function
(`library:module/path:function`) skeletons into ALREADY-ESTABLISHED
libraries (establish one with [`library()`](#library) `new`); the namepaths
may span multiple libraries. Additive - refuses to scaffold over an
existing leaf. Edit the files, then [`commit()`](#commit).

### arguments

Schema (partial):
```json
{
  "properties": {
    "namepaths": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["namepaths"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": {
    "namepaths": ["geo:shape:area", "geo:shape:perimeter"]
  }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "created": [
        "/abs/path/to/source/geo/shape",
        "/abs/path/to/source/geo/shape/area.nu",
        "/abs/path/to/source/geo/shape/perimeter.nu"
      ]
    },
    "content": []
  }
}
```

## `commit()`
*Commit the agent's library source-code to the MCP's repository for live use.*

### arguments

Schema (partial):
```json
{
  "properties": {
    "library": { "type": "string" }
  },
  "required": ["library"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "library": "geo" }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "added": ["geo/shape/area.nu", "geo/shape/mod.nu"],
      "modified": ["geo/mod.nu"],
      "removed": []
    },
    "content": []
  }
}
```

## `library()`
*Library administration: new, install, check, uninstall.*

The admin tool over a whole library. `action` is one of:
- `new` - establish a fresh, empty library at `source_dir` + register it.
- `install` - bring a complete/shipped source into the MCP (establish +
  first commit, atomic: a validation failure registers nothing).
- `check` - validate the in-source tree (the library's `cargo test`); no
  mutation. Errors block a commit; warnings are advisory.
- `uninstall` - remove the library from the MCP. The agent `source_dir` is
  never touched. Idempotent: an absent library is success.

`source_dir` is the universal "are you sure" cross-check on every action:
for a registered library it must equal the recorded source path; for
`new` / `install` it is the path recorded.

### arguments

Schema (partial):
```json
{
  "properties": {
    "action":     { "type": "string" },
    "library":    { "type": "string" },
    "source_dir": { "type": "string" }
  },
  "required": ["action", "library", "source_dir"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "action": "check", "library": "geo", "source_dir": "/abs/path/to/source/geo" }
}
```

### Output

Each action returns its own record under `summary` (`uninstall` returns no
summary). `new` -> `{created}`; `install` -> `{added, modified, removed}`;
`check` -> the validation report `{ok, errors, warnings}` (errors block a
commit, warnings advise; each is a list of diagnostics):
```json
{
  "result": {
    "structuredContent": {
      "summary": {
        "ok": true,
        "errors": [],
        "warnings": [
          { "kind": "lint::summary_length", "source": { "path": "geo/mod.nu", "position": [1, 1] }, "message": "doc summary line exceeds 80 characters" }
        ]
      }
    },
    "content": []
  }
}
```

## `learn()`
*Generate the latest `/nu` SKILL.md.*

Renders the embedded template to `<harness_dir>/skills/nu/SKILL.md`,
stamped with the server version. Returns `{written_path, bytes,
version}`. Regenerate whenever `info().version` differs from the skill's
stamp.

### arguments

Schema (partial):
```json
{
  "properties": {
    "harness_dir": { "type": "string" }
  },
  "required": ["harness_dir"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "harness_dir": "/abs/path/to/harness" }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "written_path": "/abs/path/to/harness/skills/nu/SKILL.md",
      "bytes": 30000,
      "version": "0.0.46"
    },
    "content": []
  }
}
```
