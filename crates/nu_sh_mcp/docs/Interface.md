# MCP Interface

## Overview
- [`run()`](#run) Evaluate a typed nushell body on a stateless worker.

## `run()`
*Evaluate a typed nushell body on a stateless worker.*

This a 1-3 short prose description.

### arguments

Schema (partial):
```json
{
  "properties": {
    "args_schema":   { "type": "string" },
    "result_schema": { "type": "string" },
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
# @returns record<name: string, ratio: float>
def run [args: record<eldest: bool, tabs: table<name: string, age: int, ratio: float>>] {
    let chosen = if $args.eldest {
        $args.tabs | sort-by age | last
    } else {
        $args.tabs | first
    }

    {
        count: ($tabs | length),
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
    "args_schema": "eldest: bool, tabs: table<name: string, age: int, ratio: float>",
    "result_schema": "first: <record<name: string, ratio: float>>",
    "args": {
      "eldest": true,
      "tabs": [
        {"name": "cindy", "age": 29, "ratio": 0.42},
        {"name": "bob",   "age": 32, "ratio": 0.77}
      ]
    },
    "body": "let chosen = if $args.eldest {\n    $args.tabs | sort-by age | last\n} else {\n    $args.tabs | first\n}\n\n{\n  first: {\n      name: $chosen.name,\n      ratio: $chosen.ratio\n  }\n}"
  }
}
```

Output (partial):
```json
{
  "result": {
    "structuredContent": {
      "result": {
        "first": { "name": "bob", "ratio": 0.77 }
      },
      "nonce": "8jPtjMSfRVh",
      "rerun_id": "isaBZMrHho3"
    },
    "content": [],
    "isError": false
  }
}
```
