use crate::*;
use crate::server::rig::{
    IndexFunction, IndexModule, RigIndex, index_from_nuon, index_to_nuon,
};

fn obj(s: &str) -> mcp::JsonObject {
    match serde_json::from_str::<json::Value>(s).unwrap() {
        json::Value::Object(m) => m,
        _ => panic!("not an object"),
    }
}

/// The index carries agent-authored SCHEMAS, so its round-trip has to survive the
/// whole grammar rather than the easy scalar case: an empty (void) schema, an OPEN
/// record, a table, a list, a nested record, and a `oneof<>` whose KEY contains angle
/// brackets and whose member is a bare null. Each of those is a place where a NUON
/// render could quietly change the value's type on the way back.
#[test]
fn the_index_round_trips_through_nuon_across_the_schema_grammar() {
    let args = r#"{"n":"int","p":"path","fill":{},"u":{"oneof<>":["int",null]}}"#;
    let result = r#"{"rows":[{"name":"string","tags":["string"]}],"nested":{"a":"bool"}}"#;
    let index = RigIndex {
        source_path: PathBuf::from("/home/box/proj/thing"),
        functions: Vec::new(),
        modules: vec![IndexModule {
            name: "m".to_string(),
            functions: vec![
                IndexFunction {
                    name: "void_fn".to_string(),
                    args_schema: obj("{}"),
                    result_schema: obj("{}"),
                },
                IndexFunction {
                    name: "wide".to_string(),
                    args_schema: obj(args),
                    result_schema: obj(result),
                },
            ],
            modules: vec![IndexModule {
                name: "deep".to_string(),
                functions: Vec::new(),
                modules: Vec::new(),
            }],
        }],
    };

    let nuon = index_to_nuon(&index).expect("render");
    assert!(
        !nuon.contains('\n'),
        "the render is compact, so an index is one line; got {nuon}",
    );

    let back = index_from_nuon(&nuon).expect("parse");
    assert_eq!(back.source_path, index.source_path, "the source path is the namespace's link to the authored tree");
    assert!(back.functions.is_empty(), "no root functions, and an empty list must stay empty");

    let m = &back.modules[0];
    assert_eq!(m.name, "m");
    assert_eq!(m.modules[0].name, "deep", "module nesting survives");
    assert_eq!(
        m.functions[0].args_schema,
        obj("{}"),
        "a void function's empty schema must come back an empty record, not vanish",
    );
    assert_eq!(
        m.functions[1].args_schema,
        obj(args),
        "the open record, and the oneof's bracketed key + bare null member, must survive",
    );
    assert_eq!(
        m.functions[1].result_schema,
        obj(result),
        "the table, its list column and the nested record must survive",
    );
}

/// A malformed index is a decode ERROR, never a silently empty rig - detection
/// keys on this file, so "parsed to nothing" and "not a rig" must not look alike.
#[test]
fn a_malformed_index_fails_to_parse() {
    assert!(index_from_nuon("{not: valid").is_err(), "truncated NUON");
    assert!(index_from_nuon("[1 2 3]").is_err(), "a list is not an index record");
}
