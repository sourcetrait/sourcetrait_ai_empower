# MCP Interface

## Server

One binary, one variant surface, selected at runtime by trusted operator
config (the `.mcp.json` server entry's `args`, or the invoking command
line):

```
grammar_mcp [--id <string>] [--namespace <string>] [--workdir <path>] [--deny <csv>]
```

- `--id` (default: `$USER`) - the agent identity owning the namespace.
- `--namespace` (default: `default`) - the namespace within the
  id's identity.
- `--workdir` (default: `<home>/proj/equip/<id>`) - the agent's working
  directory, exported to every eval body as `$env.EQUIP_WORK_DIR`. A
  leading `~` / `~/` expands against the home dir; any other value
  passes through literally. The default serves the bare user-cli case;
  agent harness entries always pass it explicitly.
- `--deny` - comma-separated tools to withhold from the surface. The
  deniable set: `run, rerun, interact, call, learn, new, commit,
  rig`; the core four (`info`, `inspect`, `processes`, `kill`)
  always register. A denied tool is ABSENT from tools/list (never
  registered); calling it anyway fails at the protocol layer. There is
  no implication between tokens - denying `run` does not deny `rerun`;
  list both when both are meant. An unknown token fails startup.

The values are trusted config, not validated input: a malformed id,
namespace, or workdir surfaces as the natural downstream error (improper
configuration); workdir is tilde-expanded but never existence-checked.

Every namespace is fully private per `(id, namespace)`:

```
$XDG_DATA_HOME/sourcetrait/grammar/<id>/<namespace>/{keypair,rigs}
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
configured namespace and prints the envelope as bare compact JSON on
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
grammar_mcp cli rig new sourcetrait/myrig ~/src/myrig
grammar_mcp cli commit sourcetrait/myrig
grammar_mcp cli run --args-schema '{x: int}' --args '{x: 5}' --result-schema '{out: int}' '{ out: ($args.x + 1) }'
```

All 12 tools are mirrored. Caveats: `interact` is single-shot (session
state dies with the process); `processes` / `kill` are process-scoped
(a one-shot invocation shows none); `--deny` does not apply (it gates
agent registration, not the operator surface). Writing into a namespace a
live agent host is using is the operator's own risk - git's index lock
keeps the rig repo itself safe, but an in-flight call can
transiently fail.

## Overview
- [`run()`](#run) Evaluate a typed nushell source-code body on a stateless thread.
- [`interact()`](#interact) Evaluate a typed nushell source-code body on a persistent stateful thread.
- [`call()`](#call) Invoke a committed rig function with typed args.
- [`rerun()`](#rerun) Re-evaluate a cached `run()` body with fresh args.
- [`processes()`](#processes) List in-flight MCP tool usage.
- [`kill()`](#kill) Cancel an in-flight usage by its nonce.
- [`info()`](#info) Versions, plugins, and every rig's callable signatures.
- [`inspect()`](#inspect) Detailed documentation for rigs, modules, and calls.
- [`new()`](#new) Scaffold modules / functions (by namepath) into existing rigs.
- [`commit()`](#commit) Commit the agent's rig source-code to the MCP's repository for live use.
- [`rig()`](#rig) Rig administration: new, install, check, uninstall.
- [`learn()`](#learn) Generate the latest `/nu` SKILL.md.
- [`channel_open()`](#channel_open) Open the host's packet channel and return the endpoint to watch.
- [`channel_verified()`](#channel_verified) Confirm the channel/Open packet was seen; ends the verify window.
- [`channel_close()`](#channel_close) Close the host's packet channel.
- [`config_channel()`](#config_channel) Read or adjust the channel's send-rate thresholds at runtime.
- [`purviews()`](#purviews) List every configured purview, and what is currently in view.
- [`purview()`](#purview) Set the current view to these purviews.
- [`purview_configure()`](#purview_configure) Set a purview's namepath patterns; an empty list deletes it.
- [`purview_extend()`](#purview_extend) Bring more purviews into the current view.


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
*Invoke a committed rig function with typed args.*

`namepath` is the function namepath
`<author>/<rig>:module/path:function` (a callable always lives in a
module - there are no root functions; the rig is always the compound
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
*Versions, plugins, and every rig's callable signatures.*

`signatures` shows what the CURRENT purview puts in view, not necessarily the
whole namespace (see [Purview scoping](#purview-scoping)).

### arguments

Schema (partial):
```json
{
  "properties": {
    "purviews": { "type": "array", "items": { "type": "string" } }
  },
  "required": []
}
```

`purviews` renders as if those purviews were in view, WITHOUT changing what the
session actually has in view. Omit it (or pass `[]`) for the current view - the
everyday call. Naming purviews is the SUBAGENT BLINDERS case: hand a subagent a
narrower surface at bootstrap without giving up your own.

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
      "signatures": "acme\n geo # planar geometry helpers\n  shape\n   plane\n    area <width:float,height:float> <area:float> # result is in the inputs' unit, squared\n",
      "purview": [ ["default", ["*"]] ]
    },
    "content": []
  }
}
```

`purview` is what is in view, each id beside the namepath patterns it resolves to, in
the order they came into view.

`signatures` is ONE indented text block. Rendered, that value reads:

```txt
acme
 geo # planar geometry helpers
  shape
   plane
    area <width:float,height:float> <area:float> # result is in the inputs' unit, squared
```

STRUCTURE IS THE INDENTATION - one space per level - and NOTHING IS STATED THAT
CAN BE INFERRED. There are no separator characters in the block; a reader
recovers each line's kind from its POSITION and its SHAPE:

| line | kind |
|---|---|
| depth 0 | an author; carries no summary |
| depth 1 | a rig - always the two levels `<author>/<name>` |
| deeper, no signature groups | a module |
| deeper, WITH `<args> <result>` | a call |

To build a namepath: join the author to the rig with `/`, then `:`, then the
module segments with `/`, then `:` before the call.

```txt
acme   geo   shape   plane   area   ->   acme/geo:shape/plane:area
```

A CALL IS RECOGNIZABLE ON ITS OWN, because it always carries BOTH signature
groups - a void renders `<>`, never nothing at all. That is what tells a reader
where the module path ends, so a module holding both submodules and calls needs
no marking of any kind:

```txt
 myrig
  m
   here <x:int> <out:int>
   deep
    down <y:int> <out:int>
```

`here` is `myrig:m:here`; `down` is `myrig:m/deep:down`.

A one-line summary follows as ` # ...`, omitted ENTIRELY when the line is
undocumented. Within a level, calls come before submodules and each group sorts
by name; rigs sort by author, then name.

A signature group is the nu type grammar with two changes, both only at the top
level: the `record<...>` wrapper is written `<...>`, and there is no space after
a comma. A VOID renders `<>`, so a call taking nothing and returning nothing
reads `ping <> <>`. Nested types keep their full spelling -
`table<name:string,where:directory>`, `oneof<int,nothing>`, `list<string>`.

The block is FILTERED to the current purview. A rig appears when its own
namepath is in view or when anything inside it is, so a purview naming one module
still shows the author and rig lines above it - the block always spells a
namepath.

## `inspect()`
*Detailed documentation for rigs, modules, and calls.*

`namepath` is either an EXACT namepath - `<author>/<rig>`,
`<author>/<rig>:module/path`, or `<author>/<rig>:module/path:function`,
any of the three arities - or a PATTERN, which is what a trailing hierarchy
character makes it:

| pattern | selects |
|---|---|
| `<author>/` | everything that author published |
| `<author>/<rig>:` | everything in that rig |
| `<author>/<rig>:module/path/` | that module and everything below it |
| `<author>/<rig>:module/path:` | the calls in that module, no deeper |
| `*` | the whole namespace |
| `.` | the current purview, resolved to its namepath patterns |

`/` DESCENDS and `:` selects the level BELOW, which is what each separator
already means in an exact namepath, so the two module forms differ deliberately.
A pattern matching nothing renders an EMPTY block rather than erroring.

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
      "doc": {
        "src": "/abs/path/to/namespace/rigs/rig/acme/geo/shape/area/mod.nu",
        "signature": "acme/geo:shape:area <width:float,height:float> <area:float> # result is in the inputs' unit, squared",
        "details": "Planar rectangle only; negative inputs are a type-clean error."
      }
    },
    "content": []
  }
}
```

The envelope is EXACTLY one field, `doc` - the namepath is not reprinted,
because you supplied it. Which of three shapes arrives follows from the arity
you asked for:

| namepath | `doc` |
|---|---|
| `<author>/<rig>` | `{srcdir, summary, details}` |
| `<author>/<rig>:module/path` | `{src, summary, details}` |
| `<author>/<rig>:module/path:function` | `{src, signature, details}` |
| any PATTERN | `{signatures}` |

`srcdir` and `src` point into the COMMITTED CANONICAL tree - a rig's
directory, and a module's or a call's `mod.nu` - not the authored source.

A CALL CARRIES NO `summary`: the summary is part of the signature, exactly as in
the `info()` block. The signature here is the STANDALONE form, printing the FULL
NAMEPATH where the block prints only the leaf name, because nothing around it
supplies the hierarchy. Everything after that first token is identical between
the two.

A PATTERN returns `{signatures}` - the same indented block `info()` renders,
rooted at the pattern instead of at the whole namespace and produced by the same
renderer, so the two cannot drift. The ANCESTOR lines above the root are still
printed, because the block's grammar IS its indentation: without them a subtree
is one you cannot turn back into a namepath.

MCP (partial):
```json
{
  "arguments": { "namepath": "acme/geo:shape/" }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "doc": {
        "signatures": "acme\n geo # planar geometry helpers\n  shape\n   plane\n    area <width:float,height:float> <area:float> # result is in the inputs' unit, squared\n"
      }
    },
    "content": []
  }
}
```

## `new()`
*Scaffold modules / functions (by namepath) into existing rigs.*

Batch-scaffolds module (`<author>/<rig>:module/path`) or function
(`<author>/<rig>:module/path:function`) skeletons into ALREADY-ESTABLISHED
rigs (establish one with [`rig()`](#rig) `new`); the namepaths
may span multiple rigs. A function scaffolds as a dir-module holding a
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
*Commit the agent's rig source-code to the MCP's repository for live use.*

### arguments

Schema (partial):
```json
{
  "properties": {
    "rig": { "type": "string" }
  },
  "required": ["rig"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "rig": "acme/geo" }
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

## `rig()`
*Rig administration: new, install, check, uninstall.*

The admin tool over a whole rig. `action` is one of:
`rig` is always the compound `<author>/<name>`; a bare name is rejected.

- `new` - establish a fresh, empty rig at `source_dir` + register it.
- `install` - bring a complete/shipped source into the MCP (establish +
  first commit, atomic: a validation failure registers nothing).
- `check` - run the rig's validation pass over the in-source tree; no
  mutation. Errors block a commit; warnings are advisory.
- `uninstall` - remove the rig from the MCP. The agent `source_dir` is
  never touched. Idempotent: an absent rig is success.

`source_dir` is the universal "are you sure" cross-check on every action:
for a registered rig it must equal the recorded source path; for
`new` / `install` it is the path recorded.

### arguments

Schema (partial):
```json
{
  "properties": {
    "action":     { "type": "string" },
    "rig":    { "type": "string" },
    "source_dir": { "type": "string" }
  },
  "required": ["action", "rig", "source_dir"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "action": "check", "rig": "acme/geo", "source_dir": "/abs/path/to/source/geo" }
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

## The channel

A single loopback WSS stream carrying structured packets from inside the host to
the agent's Monitor.

IT IS A NOTIFICATION LANE, NOT A SERIALIZATION LANE. A packet says "state changed,
here is a small summary"; the agent fetches the actual data itself. Bulk belongs in
`attached` (below), in a file, or in `grimm dbg` when it is only a trace.

Channels are OPTIONAL and LAZY: the hub binds on the first `channel_open()`, so a
session that never opens one pays nothing for it.

### The handshake, in order

1. `channel_open()` -> `{status, wss, inbox}`.
2. Point a Monitor at `wss`. The FIRST connection claims the channel; another is
   refused with close `1013 channel already claimed`.
3. The host sends an `mcp/channel/Open` packet on connect. SEEING it is the proof -
   there is no separate ack channel.
4. `channel_verified()`. Until then, emitting is forbidden and an attempt tears the
   channel down. The window is 5 minutes; on expiry the host closes with `1008`.

### The packet

```
record<
  id: string,                    # host-stamped message id
  from: string,                  # origin: `mcp` for the host, `thread/<nonce>` for an eval
  model: string,                 # the shape the event carries
  event: oneof<record, table>,   # the state summary; ALWAYS present
  attached?: string              # inbox filename - the NAME, never the content
>
```

`mcp/` IS RESERVED for host-originated models (`mcp/channel/Open`,
`mcp/channel/spam/*`, `mcp/supervisor/*`), so a model path can be checked for
provenance mechanically. `grimm channel_send` refuses a model under that prefix.

Packets are NUON, one per frame, newline-escaped. The usable frame maximum is
1048575 bytes.

### Close codes

| code | meaning |
|---|---|
| `1000 channel closed` | `channel_close()` - planned |
| `1001 host shutting down` | the host exited (stdin closed, or TERM/INT/HUP) |
| `1008 ...` | the peer failed the handshake |
| `1013 channel already claimed` | a second claimant was refused |
| bare `1006` | the host DIED - the only code the host never sends |

## `channel_open()`
*Open the host's packet channel and return the endpoint to watch.*

Starts the hub if none is running, otherwise reports the running one. `status` is
`new` or `existing`; on `existing` with a live peer the host re-sends
`mcp/channel/Open`, so a peer that has gone away surfaces here as
`channel::peer_gone`. A successful `existing` therefore means the channel is still
claimed by a live peer. The verify window is armed on both paths.

### arguments

No parameters.

### Example

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "status": "new",
      "wss": "wss://127.0.0.1:34623",
      "inbox": "/dev/shm/box/mcp/1JOAPgBt0PN/inbox"
    },
    "content": []
  }
}
```

## `channel_verified()`
*Confirm the channel/Open packet was seen; ends the verify window.*

The authentication step: the same agent that drives the MCP over stdio proves it
owns the claiming connection. REQUIRES a claim - start the Monitor first, or it
fails `channel::not_claimed`. No-return; success carries no payload.

### arguments

No parameters.

## `channel_close()`
*Close the host's packet channel.*

Closes with `1000 channel closed`. Idempotent - closing an already-closed channel
succeeds. No-return.

### arguments

No parameters.

## `config_channel()`
*Read or adjust the channel's send-rate thresholds at runtime.*

A PARTIAL update: supply only what should move, and the policy now in force is
returned, so a caller never has to assume its own change took. Passing no fields
reads the current policy. Thresholds only - the port and cert directory cannot
change under a live hub.

Windows are INTEGER SECONDS: MCP arguments cross as JSON, which cannot carry a nu
`duration`.

### arguments

Schema (partial):
```json
{
  "properties": {
    "spam_warn_window_secs":  { "type": ["integer", "null"], "format": "uint64" },
    "spam_warn_rate":         { "type": ["integer", "null"], "format": "uint32" },
    "spam_error_window_secs": { "type": ["integer", "null"], "format": "uint64" },
    "spam_error_rate":        { "type": ["integer", "null"], "format": "uint32" }
  }
}
```

### Example

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "spam_warn_window_secs": 10,
      "spam_warn_rate": 10,
      "spam_error_window_secs": 10,
      "spam_error_rate": 15
    },
    "content": []
  }
}
```

## Purview scoping

A PURVIEW is a named, scoped view of the callable namespace. Its values are
namepath PATTERNS (the same grammar `inspect()` takes), exact namepaths, or
REFERENCES to other purviews - so `sourcetrait/`, `acme/geo:`,
`acme/geo:shape/`, `acme/geo:shape:area` and `@other` are all legal values.

It exists to keep `info()` SMALL. On a namespace carrying many rigs the
signature block is the agent's startup read, and most of it is irrelevant to the
task at hand.

IT IS NOT ACCESS CONTROL. A purview filters what an agent KNOWS, never what it
may call: `call()` reaches any registered call regardless of what is in view. Do
not build permissions on it.

IDs are arbitrary path-like labels - slash-separated snake components, always
bare relative (`default`, `iter/almost`, `john/cindy/mary`), never a leading `/`
or `./`. They are unrelated to namepaths and to filesystem paths; `iter/almost`
may or may not refer to anything called almost.

Three built-ins:

| id | meaning |
|---|---|
| `default` | what a fresh session has in view; CONFIGURABLE |
| `.` | what is in view right now; DERIVED, never stored |
| `*` | everything |

THERE IS NO UNCONFIGURED DEFAULT. Startup writes `default` as `['*']` whenever
it has no row, so a fresh namespace sees everything and every reader may assume
the row exists. `purview_configure("default", [])` therefore RESETS it to `['*']`
rather than deleting it - `default` has no not-existing state.

Configuration persists per NAMESPACE, in the namespace's meta directory
(`.meta/purviews.nuon`). The CURRENT view is SESSION-resident: it belongs to the
host process, starts at `default`, and does not survive a restart. Configuration
does.

### References

`@<purview_id>` includes whatever that purview puts in view. That is composition
at the CONFIGURATION level - persisted and shared - as distinct from
`purview_extend()`, which composes at the SESSION level and dies with the host.

`@` is unambiguous: no author, rig, module or call may begin with one.

- `@.` and `@*` are REFUSED. Both are derived rather than stored, so they name
  no row and can reference nothing.
- REFERENCES EXPAND ONLY WHEN FILTERING. Every report - `purviews()`,
  `info()`'s `purview` - shows the values as written. Expansion happens where the
  signature block is actually built, so what you read back is the configuration
  rather than a derived view of it.
- CYCLES FLATTEN rather than lock up. A purview already visited on a walk
  contributes nothing the second time, so `a -> @b -> @a` yields the union of
  both and `a -> @a` yields a's own patterns. Writing a cycle is legal; it just
  cannot buy anything on the revisit.
- A reference to a purview with no row DANGLES and is pruned like any other
  dangling namepath pattern.

### Lifecycle

- `rig(install)` ADDS the new rig's `<author>/<name>:` pattern to `default`,
  appending beside its existing `['*']` so nothing leaves view. Narrowing
  everything down to one rig as the price of installing it would be a surprising
  trade.
- `rig(uninstall)` removes that pattern everywhere, and DANGLING values - ones no
  registered rig or purview can satisfy - are pruned whenever the table is
  written. A purview pruned down to nothing is DELETED, since an empty value list
  is already the delete operation.

## `purviews()`
*List every configured purview, and what is currently in view.*

### arguments

No parameters.

### Example

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "purviews": [ ["default", ["*"]], ["iter/geo", ["acme/geo:", "@shared"]] ],
      "current": [ "default" ]
    },
    "content": []
  }
}
```

`purviews` is the persisted table, verbatim - `@shared` is NOT expanded. `current`
is the purview ids in view, the keys alone, since `purviews` already says what
each resolves to.

## `purview()`
*Set the current view to these purviews.*

REPLACES what is in view. An EMPTY list means `default`, which is what makes a
separate reset tool unnecessary. Each id may carry the `@` alias form, so a
reference copied out of a configuration works here unchanged. An unknown id is
REFUSED rather than silently narrowing the view to something you did not ask for.

### arguments

Schema (partial):
```json
{
  "properties": {
    "purviews": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["purviews"]
}
```

### Example

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "revealed": "acme\n geo # planar geometry helpers\n  shape\n   area <width:float,height:float> <area:float>\n"
    },
    "content": []
  }
}
```

`revealed` is `info()`'s `signatures` for what this call brought INTO view, or
null when it brought nothing. Null does NOT mean nothing changed: narrowing the
view reveals nothing new, and so does naming a purview whose rigs were already
visible under another id. Call `info()` for what you are looking at now.

## `purview_configure()`
*Set a purview's namepath patterns; an empty list deletes it.*

Creates the purview if absent, REPLACES its values if present, DELETES it when
the list is empty - except `default`, which resets to `['*']`. The derived `.`
and `*` cannot be configured.

### arguments

Schema (partial):
```json
{
  "properties": {
    "purview":   { "type": "string" },
    "namepaths": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["purview", "namepaths"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": {
    "purview": "iter/geo",
    "namepaths": ["acme/geo:", "@shared"]
  }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "signatures": "acme\n geo # planar geometry helpers\n  shape\n   area <width:float,height:float> <area:float>\n"
    },
    "content": []
  }
}
```

`signatures` is EVERYTHING this purview reveals - the whole set with references
expanded, never a delta - or null when the call deleted it. Full rather than
incremental because the caller needs to CHECK what it just wrote, and a delta
says nothing at all about a purview the session is not looking through.

## `purview_extend()`
*Bring more purviews into the current view.*

ADDITIVE: nothing already in view is disturbed. An unknown id is REFUSED rather
than silently contributing nothing, because a typo would otherwise look exactly
like a purview that is genuinely empty.

### arguments

Schema (partial):
```json
{
  "properties": {
    "purviews": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["purviews"]
}
```

### Example

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "revealed": "acme\n geo # planar geometry helpers\n  shape\n   area <width:float,height:float> <area:float>\n",
      "current": [ "default", "iter/geo" ]
    },
    "content": []
  }
}
```

`revealed` is what CAME INTO view, not the whole new view, and it is measured
over what is VISIBLE rather than over the value strings - so a purview naming a
rig already in view under a different id reveals nothing and returns null.
`current` is the purview ids in view now.

## The embedded API (`grimm *`)

Commands that exist ONLY inside a run / call / interact body. They are ordinary
two-word nushell subcommands, so a body needs no `use`.

- `grimm dbg <data>` - append a record or table to `debug.nuonl` in this call's own
  nonce log dir, one NUON line per call. Structured tracing that never touches fd 1
  and does not have to fit in the result envelope.
- `grimm channel_send <model> <event> [attached]` -> the message id. `event` is the
  state summary that goes on the wire; `attached` is bulk the host writes to the
  inbox and NAMES on the wire. The two slots are what keep data off the channel.

`data` / `event` / `attached` are each `oneof<record, table>`.

SENDING TOO FAST IS THE ONE THING THE HOST STOPS, because it implies a bug rather
than load: crossing the soft threshold warns once and keeps operating, and crossing
the hard one refuses the send AND stops the offender - including a detached `job`
that catches the error and loops. A closed or unverified channel refuses and writes
nothing.
