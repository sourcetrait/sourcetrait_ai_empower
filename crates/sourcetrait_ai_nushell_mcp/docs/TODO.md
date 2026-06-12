# TODO: nu_sh_mcp

## json schemas

investigate possibility passing `args_schema` and `result_schema` as structured
json.

there would need to be a json-to-schema conversion functionality created.

the representation would have to be obvious or it may not be worth the effort.
the hard part: record vs table vs list 

the following conversion schema establishes a reserved "union" keyword and
makes parsing an json arrays `[]` require three branches of logic:
1. homogenous scalar list: bare `["scalar"]`
2. heterogenous list: single object with "union" field and array `[{ "union": [..]}]`
3. table: single object that is without a "union" field: `[{ ... }]`

### scalar
```json
"field_int":      "int",
"field_float":    "float",
"field_string":   "string",
"field_bool":     "bool",
"field_nothing":  "nothing",
"field_datetime": "datetime",
"field_duration": "duration",
"field_filesize": "filesize",
"field_binary":   "binary",
"field_range":    "range",
"field_path":     "path",
"field_directory": "directory",
"field_number": "number",
"field_glob":     "glob",
"field_cellpath": "cell-path"
```

### record
```json
"field": {
  "record_field_1": "int"
}
```
### table 
```json
"field": [{
  "table_field_1": "int"
}]
```
### list
```json
"field_1": ["int"],
"field_2": [
  { "union": [
    { "record1_field_1": "int" },
    { "record2_field_1": "string" }
  ]}
]
```
