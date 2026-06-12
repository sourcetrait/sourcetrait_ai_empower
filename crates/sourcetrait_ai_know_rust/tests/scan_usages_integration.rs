use sourcetrait_ai_know_rust::*;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn scan_usages_emits_facts_on_fixture() {
    let fixture = fixture_root();
    let out_dir = TempDir::new().expect("create tempdir");

    scan_usages(fixture.as_path(), out_dir.path()).expect("scan_usages succeeds");

    let usages_path = out_dir.path().join("know_rust_usages.json");
    let json = std::fs::read_to_string(&usages_path).expect("read know_rust_usages.json");
    let facts: UsageFacts = serde_json::from_str(&json).expect("parse UsageFacts");

    assert!(facts.files_scanned > 0, "files_scanned > 0");
    assert!(
        !facts.ast_fn_sig_usages.is_empty(),
        "at least one fn sig usage"
    );
    assert!(
        !facts.ast_field_usages.is_empty(),
        "at least one field usage"
    );

    let widget_name_field = facts.ast_field_usages.iter().find(|f| {
        f.container == "Widget" && f.field_name == "name" && f.ident == "String"
    });
    assert!(
        widget_name_field.is_some(),
        "fixture's Widget.name: String field usage surfaces"
    );

    let build_widget_params: Vec<&FnSigUsage> = facts
        .ast_fn_sig_usages
        .iter()
        .filter(|u| u.fn_name == "build_widget")
        .collect();
    assert!(
        !build_widget_params.is_empty(),
        "fixture's build_widget fn sig usages surface"
    );
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mini_workspace")
}
