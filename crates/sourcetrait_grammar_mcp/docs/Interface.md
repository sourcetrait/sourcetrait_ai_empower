# MCP Interface

## Server

One binary, one variant surface, selected at runtime by trusted operator
config (the `.mcp.json` server entry's `args`, or the invoking command
line):

```
grammar_mcp [--id <string>] [--namespace <string>] [--workdir <path>] [--deny <csv>]
```

- `--id` (default: `$USER`) - the agent identity owning the state store.
- `--namespace` (default: `default`) - the state namespace within the
  id's store.
- `--workdir` (default: `<home>/proj/equip/<id>`) - the agent's working
  directory, exported to every eval body as `$env.EQUIP_WORK_DIR`. A
  leading `~` / `~/` expands against the home dir; any other value
  passes through literally. The default serves the bare user-cli case;
  agent harness entries always pass it explicitly.
- `--deny` - comma-separated tools to withhold from the surface. The
  deniable set: `run, rerun, interact, call, learn, new, commit,
  library`; the core four (`info`, `inspect`, `processes`, `kill`)
  always register. A denied tool is ABSENT from tools/list (never
  registered); calling it anyway fails at the protocol layer. There is
  no implication between tokens - denying `run` does not deny `rerun`;
  list both when both are meant. An unknown token fails startup.

The values are trusted config, not validated input: a malformed id,
namespace, or workdir surfaces as the natural downstream error (improper
configuration); workdir is tilde-expanded but never existence-checked.

Every store is fully private per `(id, namespace)`:

```
$XDG_DATA_HOME/sourcetrait/grammar/<id>/<namespace>/{keypair,libraries}
$XDG_CACHE_HOME/sourcetrait/grammar/<id>/<namespace>/{runs,interacts,calls}
```

Eval bodies and committed call-targets read `$env.EQUIP_ID` /
`$env.EQUIP_NAMESPACE` / `$env.EQUIP_WORK_DIR` ambiently (the host seeds them
from its config); `info()` reports the same values.

Example `.mcp.json` entries (one binary, two channels):

```json
{
  "mcpServers": {
    "nushell": {
      "type": "stdio",
      "command": "/path/to/grammar_mcp",
      "args": ["--id", "emptwo", "--workdir", "~/ai/emptwo"]
    },
    "grammar_test": {
      "type": "stdio",
      "command": "/path/to/build/grammar_mcp",
      "args": ["--id", "emptwo", "--namespace", "test", "--workdir", "~/ai/emptwo"]
    }
  }
}
```

## One-shot CLI

`grammar_mcp cli <tool> ...` runs ONE tool in-process against the
configured store and prints the envelope as bare compact JSON on
stdout - one line, machine format, no color (`| from json` and
captured output are byte-clean) - no agent, no MCP client. The cli is
a wrapper's substrate, not a human display surface. Exit codes: 0
success, 1 error envelope, 2 unparseable input. Record-shaped INPUTS
are single-quoted NUON strings; an omitted args value is the empty
record.

```nu
grammar_mcp cli info
grammar_mcp --id emptwo cli inspect sourcetrait/grammar:pid:list_ai
grammar_mcp cli call sourcetrait/geo:shape:area '{width: 3.0, height: 4.0}'
grammar_mcp cli library new sourcetrait/mylib ~/src/mylib
grammar_mcp cli commit sourcetrait/mylib
grammar_mcp cli run --args-schema '{x: int}' --args '{x: 5}' --result-schema '{out: int}' '{ out: ($args.x + 1) }'
```

All 12 tools are mirrored. Caveats: `interact` is single-shot (session
state dies with the process); `processes` / `kill` are process-scoped
(a one-shot invocation shows none); `--deny` does not apply (it gates
agent registration, not the operator surface). Writing into a store a
live agent host is using is the operator's own risk - git's index lock
keeps the library repo itself safe, but an in-flight call can
transiently fail.

## Overview
- [`run()`](#run) Evaluate a typed nushell source-code body on a stateless thread.
- [`interact()`](#interact) Evaluate a typed nushell source-code body on a persistent stateful thread.
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
*Evaluate a typed nushell source-code body on a stateless thread.*

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
      "nonce": "8jPtjMSfRVh"
    },
    "content": []
  }
}
```

## `interact()`
*Evaluate a typed nushell source-code body on a persistent stateful thread.*

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

`namepath` is the function coordinate
`<author>/<library>:module/path:function` (a callable always lives in a
module - there are no root functions; the library is always the compound
`<author>/<name>`). Discover live targets + their schemas with
[`info()`](#info) / [`inspect()`](#inspect).

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
    "namepath": "acme/geo:shape:area",
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

`nonce` is the base62 nonce a prior `run()` returned - the nonce IS the
re-evaluation handle, and a rerun's own nonce is itself one. A run that
timed out is still rerunnable: its body was cached before dispatch, so
`rerun(nonce, args, timeout_ms: bigger)` recovers it.

### arguments

Schema (partial):
```json
{
  "properties": {
    "nonce":      { "type": "string" },
    "args":       { "type": "object", "additionalProperties": true },
    "timeout_ms": { "type": ["integer", "null"], "format": "uint64", "minimum": 0 }
  },
  "required": ["nonce", "args"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": {
    "nonce": "isaBZMrHho3",
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

Triggers a cooperative cancel of the in-flight call (nushell bails at its next
check point) and reaps its external process tree - there is no worker to
SIGKILL. No payload; silently succeeds if the nonce is unknown or already
completed (race-safe).

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
      "name": "grammar",
      "version": "0.0.0-88",
      "nu_version": "0.114.1",
      "id": "emptwo",
      "namespace": "default",
      "work_dir": "/home/user/ai/emptwo",
      "plugins": [ ["polars", "0.112.2"], ["inc", null] ],
      "libraries": [
        {
          "name": "acme/geo",
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

`namepath` is `<author>/<library>`, `<author>/<library>:module/path`, or
`<author>/<library>:module/path:function` - inspect any of the three arities.

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
  "arguments": { "namepath": "acme/geo:shape:area" }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "library": "acme/geo",
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

Batch-scaffolds module (`<author>/<library>:module/path`) or function
(`<author>/<library>:module/path:function`) skeletons into ALREADY-ESTABLISHED
libraries (establish one with [`library()`](#library) `new`); the namepaths
may span multiple libraries. A function scaffolds as a dir-module holding a
single `main` (`<name>/mod.nu`), wired into its parent by both
`export module <name>` and `export use <name>`. Additive - refuses to
scaffold over an existing leaf. Edit the files, then [`commit()`](#commit).

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
    "namepaths": ["acme/geo:shape:area", "acme/geo:shape:perimeter"]
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
        "/abs/path/to/source/geo/shape/area/mod.nu",
        "/abs/path/to/source/geo/shape/perimeter/mod.nu"
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
  "arguments": { "library": "acme/geo" }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "added": ["rig/acme/geo/shape/area/mod.nu", "rig/acme/geo/shape/mod.nu"],
      "modified": ["rig/acme/geo/mod.nu"],
      "removed": []
    },
    "content": []
  }
}
```

## `library()`
*Library administration: new, install, check, uninstall.*

The admin tool over a whole library. `action` is one of:
`library` is always the compound `<author>/<name>`; a bare name is rejected.

- `new` - establish a fresh, empty library at `source_dir` + register it.
- `install` - bring a complete/shipped source into the MCP (establish +
  first commit, atomic: a validation failure registers nothing).
- `check` - run the library's validation pass over the in-source tree; no
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
  "arguments": { "action": "check", "library": "acme/geo", "source_dir": "/abs/path/to/source/geo" }
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
          { "kind": "lint::summary_length", "source": { "path": "acme/geo/mod.nu", "position": [1, 1] }, "message": "doc summary line exceeds 80 characters" }
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
      "version": "0.0.0-88"
    },
    "content": []
  }
}
```
