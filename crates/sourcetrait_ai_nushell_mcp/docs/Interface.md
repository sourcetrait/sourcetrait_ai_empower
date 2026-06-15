# MCP Interface

## Overview
- [`run()`](#run) Evaluate a typed nushell closure body on a stateless worker.
- [`interact()`](#interact) Evaluate a typed nushell closure body on a persistent stateful worker.
- [`call()`](#call) Invoke a committed library function with typed args.
- [`rerun()`](#rerun) Re-evaluate a cached `run()` body with fresh args.
- [`processes()`](#processes) Snapshot every in-flight tool call on the host.
- [`kill()`](#kill) Cancel an in-flight call by its nonce.
- [`info()`](#info) Versions, plugins, and the live library/module/function hierarchy with summaries + schemas.
- [`inspect()`](#inspect) Full doc (summary + details) + schemas for one node.
- [`new()`](#new) Scaffold a library / module / function into the agent's source tree.
- [`commit()`](#commit) Validate the source tree and upsert it into the signed store.
- [`delete()`](#delete) Guarded drop of a library.
- [`learn()`](#learn) (Re)generate the `/nu` skill to `<harness_dir>/skills/nu/SKILL.md`.

## Schemas

`args_schema` and `result_schema` are structured JSON objects mapping
each field name to its type. The type vocabulary:

- scalar: a type-name string - one of `int`, `float`, `string`, `bool`,
  `datetime`, `duration`, `filesize`, `binary`, `range`, `number`,
  `glob`, `cell-path`, `path`, `directory`.
- record: a nested object, e.g. `{ "point": { "x": "int", "y": "int" } }`.
- table: an array holding one record, e.g.
  `[{ "name": "string", "age": "int" }]`.
- list: an array holding one element type, e.g. `["string"]`.
- oneof (type union): the reserved-key object
  `{ "oneof<>": [ <type>, ... ] }`; the everyday case is value
  nullability, `{ "oneof<>": ["int", null] }`.
- nothing: the JSON `null` literal (only as a `oneof<>` member).
- void: an empty object `{}` as the entire `args_schema` (a no-args
  function) or the entire `result_schema` (a no-value return).

Records and tables are open - extra fields are accepted - and every
declared field is required and is type-checked to its full depth.

## Library authoring

A library is a directory tree the agent edits, promoted into the MCP's
signed canonical store by `commit`. Establish + scaffold with `new`, edit
the files in place, then `commit` to validate + upsert; `delete` drops
it. `call` invokes a committed function; `info` / `inspect` are the live
index of what exists, each node's schemas, and its docs.

### Function files: call / resolve / main

A callable function lives at `<module_path>/<name>.nu` and exports
EXACTLY `call`, `resolve`, `main`:

```nu
export def call [args: record<x: int>] {
    { out: ($args.x * 2) }            # your logic; carries the ARGS schema
}

export def resolve [args: record<out: int>] {
    $args                             # carries the RESULT schema (typecheck)
}

# <summary line, <= 80 chars>
#
# <optional details, any length>
export def main [args: record<x: int>] {
    resolve (call $args)              # AST-locked glue; you own only the doc
}
```

The `args` positional is `record<...>` with real fields, or `nothing`
for a void function; a bare `record<>` is the unfleshed skeleton and is
rejected. `call` and `resolve` are RESERVED - they may appear only as
these exported sentinels, never as any other def / module / directory /
file / parameter / record-key / cell-path-member name. A `.nu` file with
NEITHER sentinel is organizational (free `export def` / `export const`
helpers, not a call-target); `mod.nu` may carry such helpers too.

### The mod.nu cascade

Each directory's `mod.nu` wires its children: `export use ./<name>.nu`
(re-export a sibling function / helper file) and `export module <name>`
(re-export a sibling subdirectory, which has its own `mod.nu`).

### Node documentation + inference reduction

Document a FUNCTION via the comment block directly above its
`export def main`; a MODULE or the LIBRARY via its `mod.nu` LEADING
comment. The first blank comment line splits the block: SUMMARY (<= 80
chars, the only hard rule; surfaced by `info`) then DETAILS (any length;
surfaced by `inspect`).

Write for INFERENCE REDUCTION. The reader already has, for free, the
library name, module path, function name, argument field names and types,
result field names and types, and the summaries of the enclosing library
and modules. Spend the summary on what those do NOT convey - units, side
effects, what counts as "valid", ordering / edge / failure behavior - not
a restatement of the coordinate or the schema. Details carry the rest.

## Recursive globs

A `*` segment matches one level; `**` recurses to all depths. Build the
pattern with `path join` (a leading-`/` literal trips the path lint):

```nu
glob ($args.root | path join "**" "*")
```

The result INCLUDES the root dir itself, and returns files as well as
directories (nu 0.113.1; `**` and `**/*` behave the same). For "every
subdirectory below `<root>`", filter to dirs and drop the root:

```nu
glob ($args.root | path join "**" "*")
| where {|p| (($p | path type) == "dir") and ($p != $args.root) }
```

## `run()`
*Evaluate a typed nushell closure body on a stateless worker.*

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
*Evaluate a typed nushell closure body on a persistent stateful worker.*

Env mutations, `cd`, and top-level `def`s persist across calls; `run()`
state does not leak in. For administrative, stateful sessions; use
`run()` for everything else.

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

Discover the live targets + their schemas with [`info()`](#info) /
[`inspect()`](#inspect); `module_path` is empty for a library-root
function.

### arguments

Schema (partial):
```json
{
  "properties": {
    "library":     { "type": "string" },
    "module_path": { "type": "string" },
    "name":        { "type": "string" },
    "args":        { "type": "object", "additionalProperties": true },
    "timeout_ms":  { "type": ["integer", "null"], "format": "uint64", "minimum": 0 }
  },
  "required": ["library", "module_path", "name", "args"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": {
    "library": "geo",
    "module_path": "shape",
    "name": "area",
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
*Snapshot every in-flight tool call on the host.*

Pair with [`kill()`](#kill) to cancel a specific call. Entries are
`{nonce, tool, started_at, args, ...}`: `rerun` adds `rerun_id`, `call`
adds a flat `path` (`library:module/path:name`). Match against your own
send-set via `args`.

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
*Cancel an in-flight call by its nonce.*

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
*Versions, plugins, and the live library/module/function hierarchy with summaries + schemas.*

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

The `libraries` hierarchy is the live, version-matched `call()` surface:
each library carries `path` (the editable source dir) + a one-line
`summary` + `modules` + root-level `functions`; each module carries
`summary` + `submodules` + `functions`; each function carries its
one-line `summary` + structured `args_schema` + `result_schema` (a
void-args function reads `{}`). A library HAS modules; a module MAY HAVE
submodules. `plugins` are positional `[name, version]` pairs (version is
`null` when the plugin reports none). For a node's full details, call
`inspect()`.

## `inspect()`
*Full doc (summary + details) + schemas for one node.*

Returns the single-node descriptor `{library, module_path, name?,
summary, args_schema?, result_schema?, details}` - `name` and the schemas
are present only for a function; `details` is last. Empty strings when
undocumented. Omit `name` to inspect a module; omit both `name` and
`module_path` for the library root.

### arguments

Schema (partial):
```json
{
  "properties": {
    "library":     { "type": "string" },
    "module_path": { "type": ["string", "null"] },
    "name":        { "type": ["string", "null"] }
  },
  "required": ["library"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "library": "geo", "module_path": "shape", "name": "area" }
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
*Scaffold a library / module / function into the agent's source tree.*

The FIRST call for a `library` establishes it - `source_path` is required
then and immutable after. Add a module with `module_path`; add a
call/resolve/main skeleton with `module_path` + `name`. Additive: it
refuses to scaffold over an existing leaf. Edit the files, then
[`commit()`](#commit). Returns `{source_path, created}`.

### arguments

Schema (partial):
```json
{
  "properties": {
    "library":     { "type": "string" },
    "source_path": { "type": ["string", "null"] },
    "module_path": { "type": ["string", "null"] },
    "name":        { "type": ["string", "null"] }
  },
  "required": ["library"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": {
    "library": "geo",
    "module_path": "shape",
    "name": "area"
  }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "source_path": "/abs/path/to/source/geo",
      "created": ["/abs/path/to/source/geo/shape", "/abs/path/to/source/geo/shape/area.nu"]
    },
    "content": []
  }
}
```

## `commit()`
*Validate the source tree and upsert it into the signed canonical store.*

Re-reads the recorded source_path, validates (structure + the
call/resolve/main contract + reserved terms + the summary-length rule),
and rebuilds the generated index + docs. Idempotent (a no-change resync
returns all-empty). Returns the changed paths grouped by kind; rejects
with `library::violations`.

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

## `delete()`
*Guarded drop of a library.*

Re-pass `source_path` as a sanity check (matched by PLAIN STRING against
the recorded path). Removes the agent source too unless `mcp_only`.
Returns `{removed: [{path, side: mcp|source}]}`.

### arguments

Schema (partial):
```json
{
  "properties": {
    "library":     { "type": "string" },
    "source_path": { "type": "string" },
    "mcp_only":    { "type": "boolean" }
  },
  "required": ["library", "source_path"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "library": "geo", "source_path": "/abs/path/to/source/geo" }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "removed": [
        { "path": "/.../libraries/geo", "side": "mcp" },
        { "path": "/abs/path/to/source/geo", "side": "source" }
      ]
    },
    "content": []
  }
}
```

## `learn()`
*(Re)generate the `/nu` skill.*

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
