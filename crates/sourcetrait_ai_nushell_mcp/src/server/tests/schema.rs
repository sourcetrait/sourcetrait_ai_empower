//! Unit tests for `crate::server::schema` (the item-21 structured-schema
//! grammar matrix, both directions + the live migration shapes).

use crate::*;

fn obj(s: &str) -> mcp::JsonObject {
    match serde_json::from_str::<json::Value>(s).unwrap() {
        json::Value::Object(m) => m,
        _ => panic!("not an object"),
    }
}

// ---- args_schema_to_nu (json -> nu) ----

#[test]
fn args_void() {
    assert_eq!(args_schema_to_nu(&obj("{}")).unwrap(), "nothing");
}

#[test]
fn args_simple_record() {
    assert_eq!(
        args_schema_to_nu(&obj(r#"{"a":"int","b":"string"}"#)).unwrap(),
        "record<a: int, b: string>"
    );
}

#[test]
fn args_nested_record_list_table_oneof() {
    let got = args_schema_to_nu(&obj(
        r#"{"r":{"a":"string","b":["int"]},"t":[{"c":"int"}],"u":{"oneof<>":["int",null]}}"#,
    ))
    .unwrap();
    assert_eq!(
        got,
        "record<r: record<a: string, b: list<int>>, t: table<c: int>, u: oneof<int, nothing>>"
    );
}

#[test]
fn args_list_of_oneof_with_record_member() {
    let got = args_schema_to_nu(&obj(r#"{"xs":[{"oneof<>":[{"a":"int"},"string"]}]}"#)).unwrap();
    assert_eq!(got, "record<xs: list<oneof<record<a: int>, string>>>");
}

#[test]
fn args_all_scalars_incl_cell_path() {
    let got = args_schema_to_nu(&obj(
        r#"{"c":"cell-path","d":"directory","n":"number","p":"path"}"#,
    ))
    .unwrap();
    assert_eq!(
        got,
        "record<c: cell-path, d: directory, n: number, p: path>"
    );
}

// ---- denials ----

#[test]
fn deny_any() {
    assert!(args_schema_to_nu(&obj(r#"{"x":"any"}"#)).is_err());
}

#[test]
fn deny_nested_empty_record() {
    assert!(args_schema_to_nu(&obj(r#"{"x":{}}"#)).is_err());
}

#[test]
fn deny_empty_array() {
    assert!(args_schema_to_nu(&obj(r#"{"x":[]}"#)).is_err());
}

#[test]
fn deny_table_of_empty_record() {
    assert!(args_schema_to_nu(&obj(r#"{"x":[{}]}"#)).is_err());
}

#[test]
fn deny_unknown_scalar() {
    assert!(args_schema_to_nu(&obj(r#"{"x":"bogus"}"#)).is_err());
}

#[test]
fn deny_multi_element_array() {
    assert!(args_schema_to_nu(&obj(r#"{"x":["int","string"]}"#)).is_err());
}

#[test]
fn deny_empty_oneof() {
    assert!(args_schema_to_nu(&obj(r#"{"x":{"oneof<>":[]}}"#)).is_err());
}

// ---- nu_to_args_schema (nu -> json) ----

#[test]
fn emit_void() {
    assert_eq!(nu_to_args_schema("nothing").unwrap(), obj("{}"));
}

#[test]
fn emit_nested() {
    let got = nu_to_args_schema(
        "record<t: record<y: string, n: list<int>>, u: table<a: int>, v: oneof<int, nothing>>",
    )
    .unwrap();
    assert_eq!(
        got,
        obj(r#"{"t":{"y":"string","n":["int"]},"u":[{"a":"int"}],"v":{"oneof<>":["int",null]}}"#)
    );
}

#[test]
fn emit_rejects_top_level_record_empty() {
    assert!(nu_to_args_schema("record<>").is_err());
}

#[test]
fn emit_rejects_bare_list() {
    assert!(nu_to_args_schema("record<x: list>").is_err());
}

// ---- round-trips ----

#[test]
fn roundtrip_json_nu_json() {
    for s in [
        "{}",
        r#"{"x":"int"}"#,
        r#"{"t":{"y":"string","n":["int"]}}"#,
        r#"{"xs":[{"oneof<>":[{"a":"int"},"string"]}]}"#,
        r#"{"tab":[{"set":"string","count":"int"}]}"#,
    ] {
        let nu = args_schema_to_nu(&obj(s)).unwrap();
        let back = nu_to_args_schema(&nu).unwrap();
        assert_eq!(back, obj(s), "roundtrip failed for {s}");
    }
}

#[test]
fn result_void_and_record() {
    assert_eq!(result_schema_to_nu(&obj("{}")).unwrap(), "nothing");
    assert_eq!(
        result_schema_to_nu(&obj(r#"{"out":"int"}"#)).unwrap(),
        "record<out: int>"
    );
    assert_eq!(nu_to_result_schema("nothing").unwrap(), obj("{}"));
}

// ---- nu -> json -> nu round-trips (the emit direction) ----

#[test]
fn roundtrip_nu_json_nu() {
    // Sorted field names so the BTreeMap-sorted render is string-identical to
    // the input.
    for nu in [
        "nothing",
        "record<a: int>",
        "record<a: int, b: string>",
        "record<r: record<x: cell-path, y: list<string>>>",
        "record<t: table<col: int, name: string>>",
        "record<u: oneof<int, nothing>>",
        "record<xs: list<oneof<int, string>>>",
        "record<n: oneof<record<a: int>, string>>",
    ] {
        let json = nu_to_args_schema(nu).unwrap();
        let back = args_schema_to_nu(&json).unwrap();
        assert_eq!(back, nu, "nu roundtrip failed for {nu}");
    }
}

// ---- emit normalizations + nested composites ----

#[test]
fn emit_list_of_record_normalizes_to_table_json() {
    // list<record<...>> and table<...> share the [{...}] JSON form: the
    // settled grammar maps [{record}] canonically to a table.
    assert_eq!(
        nu_to_args_schema("record<x: list<record<a: int>>>").unwrap(),
        obj(r#"{"x":[{"a":"int"}]}"#)
    );
    assert_eq!(
        nu_to_args_schema("record<x: table<a: int>>").unwrap(),
        obj(r#"{"x":[{"a":"int"}]}"#)
    );
}

#[test]
fn emit_nested_list_and_list_of_table() {
    assert_eq!(
        nu_to_args_schema("record<x: list<list<int>>>").unwrap(),
        obj(r#"{"x":[["int"]]}"#)
    );
    assert_eq!(
        nu_to_args_schema("record<x: list<table<a: int>>>").unwrap(),
        obj(r#"{"x":[[{"a":"int"}]]}"#)
    );
}

#[test]
fn emit_oneof_with_composite_members() {
    assert_eq!(
        nu_to_args_schema("record<u: oneof<record<a: int>, table<b: string>, nothing>>",).unwrap(),
        obj(r#"{"u":{"oneof<>":[{"a":"int"},[{"b":"string"}],null]}}"#)
    );
}

// ---- emit denials (the nu parse side) ----

#[test]
fn emit_rejects_bare_table_and_record() {
    assert!(nu_to_args_schema("record<x: table>").is_err());
    assert!(nu_to_args_schema("record<x: record>").is_err());
}

#[test]
fn emit_rejects_nested_empty_record_and_table() {
    assert!(nu_to_args_schema("record<x: record<>>").is_err());
    assert!(nu_to_args_schema("record<x: table<>>").is_err());
}

#[test]
fn emit_rejects_any_and_unknown() {
    assert!(nu_to_args_schema("record<x: any>").is_err());
    assert!(nu_to_args_schema("record<x: frobnicate>").is_err());
}

#[test]
fn emit_rejects_top_level_bare_forms() {
    assert!(nu_to_args_schema("int").is_err());
    assert!(nu_to_args_schema("list<int>").is_err());
    assert!(nu_to_args_schema("oneof<int, string>").is_err());
}

// ---- json-side denials not covered above ----

#[test]
fn deny_oneof_extra_key() {
    assert!(args_schema_to_nu(&obj(r#"{"x":{"oneof<>":["int"],"k":"string"}}"#)).is_err());
}

#[test]
fn deny_oneof_value_not_array() {
    assert!(args_schema_to_nu(&obj(r#"{"x":{"oneof<>":"int"}}"#)).is_err());
}

#[test]
fn deny_bool_and_number_nodes() {
    assert!(args_schema_to_nu(&obj(r#"{"x":true}"#)).is_err());
    assert!(args_schema_to_nu(&obj(r#"{"x":3}"#)).is_err());
}

// ---- all 14 scalars, both directions ----

#[test]
fn all_scalars_roundtrip_both_ways() {
    let names = [
        "int",
        "float",
        "string",
        "bool",
        "datetime",
        "duration",
        "filesize",
        "binary",
        "range",
        "number",
        "glob",
        "cell-path",
        "path",
        "directory",
    ];
    for n in names {
        let json = obj(&format!(r#"{{"f":"{n}"}}"#));
        let nu = format!("record<f: {n}>");
        assert_eq!(args_schema_to_nu(&json).unwrap(), nu, "j2n {n}");
        assert_eq!(nu_to_args_schema(&nu).unwrap(), json, "n2j {n}");
    }
}

// ---- the live migration shapes (regression locks) ----

#[test]
fn roundtrip_symbols_result_shape() {
    let s = r#"{"suspect_count":"int","suspects":[{"files":["string"],"id":"string"}]}"#;
    let nu = result_schema_to_nu(&obj(s)).unwrap();
    assert_eq!(
        nu,
        "record<suspect_count: int, suspects: table<files: list<string>, id: string>>"
    );
    assert_eq!(nu_to_result_schema(&nu).unwrap(), obj(s));
}

#[test]
fn roundtrip_measure_result_shape() {
    let s = r#"{"failed_picks":"int","per_group":[{"count":"int","group":"string","kp_chars":"int"}],"per_set":[{"count":"int","kp_chars":"int","set":"string"}],"total_kp_chars":"int","valid_picks":"int"}"#;
    let nu = result_schema_to_nu(&obj(s)).unwrap();
    assert_eq!(nu_to_result_schema(&nu).unwrap(), obj(s));
}

#[test]
fn roundtrip_integrity_nested_record_shape() {
    let s = r#"{"counts":{"indexed":"int","memories":"int"},"p1_frontmatter":{"name_mismatch":[{"file":"string","name":"string","slug":"string"}]},"p5_sizes":{"memory_md_bytes":"int","over_cap":"bool"}}"#;
    let nu = result_schema_to_nu(&obj(s)).unwrap();
    assert_eq!(nu_to_result_schema(&nu).unwrap(), obj(s));
}

#[test]
fn args_void_and_validate_result_shape() {
    // memories' migrated shape: void args + a list<string> result.
    assert_eq!(args_schema_to_nu(&obj("{}")).unwrap(), "nothing");
    let s = r#"{"failed_details":[{"bytes":"int","pattern":"string","reason":"string"}],"failed_patterns":["string"]}"#;
    let nu = result_schema_to_nu(&obj(s)).unwrap();
    assert_eq!(nu_to_result_schema(&nu).unwrap(), obj(s));
}
