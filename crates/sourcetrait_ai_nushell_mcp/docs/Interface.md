# MCP Interface

## Overview
- [`run()`](#run) Evaluate a typed nushell body on a stateless worker.
- [`interact()`](#interact) Evaluate a typed nushell body on a persistent stateful worker.
- [`call()`](#call) Invoke a registered library function with typed args.
- [`rerun()`](#rerun) Re-evaluate a cached `run()` body with fresh args.
- [`register_library()`](#register_library) Register an empty library namespace.
- [`unregister_library()`](#unregister_library) Drop a registered library and all its functions.
- [`define_function()`](#define_function) Add (or replace) a function in a registered library.
- [`undefine_function()`](#undefine_function) Remove a function from a library.
- [`import_library()`](#import_library) Import a pre-authored library tree from a client path.
- [`reimport_library()`](#reimport_library) Re-import a library from its saved source path.
- [`processes()`](#processes) Snapshot every in-flight tool call on the host.
- [`kill()`](#kill) Cancel an in-flight call by its nonce.
- [`info()`](#info) Name, version, nu version, nu plugins, and the registered library hierarchy.

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

## `run()`
*Evaluate a typed nushell body on a stateless worker.*

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
*Evaluate a typed nushell body on a persistent stateful worker.*

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
*Invoke a registered library function with typed args.*

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
    "library": "math",
    "module_path": "ops",
    "name": "double",
    "args": { "x": 21 }
  }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "result": { "out": 42 },
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

## `register_library()`
*Register an empty library namespace.*

Subsequent `define_function()` calls populate it.

The local mirror at `path` is created if absent.

### arguments

Schema (partial):
```json
{
  "properties": {
    "name": { "type": "string" },
    "path": { "type": "string" }
  },
  "required": ["name", "path"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": {
    "name": "math",
    "path": "/your/local/math"
  }
}
```

Output (partial):
```json
{
  "result": { "content": [] }
}
```

## `unregister_library()`
*Drop a registered library and all its functions.*

Does not touch the agent's local mirror.

### arguments

Schema (partial):
```json
{
  "properties": {
    "name": { "type": "string" }
  },
  "required": ["name"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "name": "math" }
}
```

Output (partial):
```json
{
  "result": { "content": [] }
}
```

## `define_function()`
*Add (or replace) a function in a registered library.*

### arguments

Schema (partial):
```json
{
  "properties": {
    "library":       { "type": "string" },
    "module_path":   { "type": "string" },
    "name":          { "type": "string" },
    "args_schema":   { "type": "object", "additionalProperties": true },
    "result_schema": { "type": "object", "additionalProperties": true },
    "body":          { "type": "string" }
  },
  "required": ["library", "module_path", "name", "args_schema", "result_schema", "body"]
}
```

### Example

Nu equivalent (the function body the agent ships):
```nu
# @returns record<out: int>
def double [args: record<x: int>] {
    { out: ($args.x * 2) }
}
```

MCP (partial):
```json
{
  "arguments": {
    "library": "math",
    "module_path": "ops",
    "name": "double",
    "args_schema": { "x": "int" },
    "result_schema": { "out": "int" },
    "body": "{ out: ($args.x * 2) }"
  }
}
```

Output (partial):
```json
{
  "result": { "content": [] }
}
```

## `undefine_function()`
*Remove a function from a library.*

### arguments

Schema (partial):
```json
{
  "properties": {
    "library":     { "type": "string" },
    "module_path": { "type": "string" },
    "name":        { "type": "string" }
  },
  "required": ["library", "module_path", "name"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": {
    "library": "math",
    "module_path": "ops",
    "name": "double"
  }
}
```

Output (partial):
```json
{
  "result": { "content": [] }
}
```

## `import_library()`
*Import a pre-authored library tree from a client path.*

## Requirements
Each function file must have exactly:
- `export def main [args: record<...>]`
- `export def resolve [args: record<...>] { $args }`

Each `mod.nu` may only re-export children.

### arguments

Schema (partial):
```json
{
  "properties": {
    "name": { "type": "string" },
    "path": { "type": "string" }
  },
  "required": ["name", "path"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": {
    "name": "utils",
    "path": "/your/local/utils"
  }
}
```

Output (partial):
```json
{
  "result": { "content": [] }
}
```

## `reimport_library()`
*Re-import a library from its saved source path.*

### arguments

Schema (partial):
```json
{
  "properties": {
    "name": { "type": "string" }
  },
  "required": ["name"]
}
```

### Example

MCP (partial):
```json
{
  "arguments": { "name": "utils" }
}
```

Output (partial):
```json
{
  "result": { "content": [] }
}
```

## `processes()`
*Snapshot every in-flight tool call on the host.*

Pair with [`kill()`](#kill) to cancel a specific call.

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

SIGKILLs the worker holding the call.

Silently returns success if unknown.

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
*Name, version, nu version, nu plugins, and the registered library hierarchy.*

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
      "version": "0.0.40",
      "nu_version": "0.113.1",
      "plugins": [
        { "name": "query", "version": "0.112.2" },
        { "name": "polars" }
      ],
      "libraries": [
        {
          "name": "math",
          "path": "/your/local/math",
          "modules": [
            {
              "name": "ops",
              "submodules": [],
              "functions": [
                {
                  "name": "double",
                  "args_schema": { "x": "int" },
                  "result_schema": { "out": "int" }
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
