# MCP Interface

## Overview
- [`run()`](#run) Evaluate a typed nushell source-code body on a stateless worker.
- [`interact()`](#interact) Evaluate a typed nushell source-code body on a persistent stateful worker.
- [`call()`](#call) Invoke a committed library function with typed args.
- [`rerun()`](#rerun) Re-evaluate a cached `run()` body with fresh args.
- [`processes()`](#processes) List in-flight MCP tool usage.
- [`kill()`](#kill) Cancel an in-flight usage by its nonce.
- [`info()`](#info) Versions, plugins, and libraries summary.
- [`inspect()`](#inspect) Detailed documentation of a specific callable library, module, function.
- [`new()`](#new) Scaffold a callable library / module / function into the agent's source-code repository.
- [`commit()`](#commit) Commit the agent's library source-code to the MCP's repository for live use.
- [`delete()`](#delete) Delete a library.
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
*Scaffold a callable library / module / function into the agent's source-code repository.*


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

## `delete()`
*Delete a library.*

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
