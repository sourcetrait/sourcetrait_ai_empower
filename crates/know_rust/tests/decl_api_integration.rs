//! Integration tests for the declaration-driven API channel (the
//! hard+soft item model): pub-reachable module-level fns mint
//! pair-shaped keys with zero usage; pub-in-private-module and
//! doc(hidden) decls do not; canonical spelling follows the shortest
//! public binding with the hard parent riding as a pair alias; and
//! zero-score minted keys stay UNRENDERED until a score source
//! exists (the consumer-weight term).

use know_rust::*;
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;

fn write_tree(base: &Path, files: &HashMap<&str, String>) {
    for (rel, content) in files {
        let p = base.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parent");
        }
        std::fs::write(&p, content).expect("write file");
    }
}

#[test]
fn decl_channel_mints_reachable_pairs_with_aliases() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"lib\",\"app\"]\n"),
        ),
        (
            "lib/Cargo.toml",
            String::from(
                "[package]\nname=\"lib\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            // mpsc shape: the decl lives in a PRIVATE inner mod; the
            // public spelling exists only via the pub use lift.
            // p::ghost_fn is pub-in-private-mod (not API);
            // hidden_fn is doc(hidden); rootfn is root-level API;
            // crate_fn is pub(crate) (not the public face);
            // main is a language entry point (never minted).
            "lib/src/lib.rs",
            String::from(
                "pub mod m {\n    mod inner { pub fn channel() {} }\n    pub use inner::channel;\n}\nmod p { pub fn ghost_fn() {} }\npub mod h { #[doc(hidden)] pub fn hidden_fn() {} }\npub fn rootfn() {}\npub fn main() {}\npub(crate) fn crate_fn() {}\npub mod util;\npub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\n",
            ),
        ),
        (
            "lib/src/util.rs",
            String::from("pub fn helper2() {}\n"),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from(
                "use lib::Core;\npub fn a() -> Core { Core::new() }\npub fn b() -> Core { Core::new() }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let calibration = Calibration::default();
    characterize(root, &out, &calibration).expect("characterize succeeds");
    let fp: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("fingerprint.json")).expect("read fingerprint"),
    )
    .expect("parse fingerprint");
    let facts: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("facts.json")).expect("read facts"),
    )
    .expect("parse facts");
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics");

    // Canonical spelling = the shortest public binding (m::channel,
    // not inner::channel), with zero counts and pub attribution.
    let ch = pm
        .get("implementation_functions:m::channel")
        .expect("m::channel minted; impl-fn keys: {:?}");
    assert_eq!(
        ch.get("defining_crate").and_then(|v| v.as_str()),
        Some("lib")
    );
    assert_eq!(ch.get("intra_count").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(ch.get("is_pub").and_then(|v| v.as_bool()), Some(true));
    assert!(
        !pm.contains_key("implementation_functions:inner::channel"),
        "the hard spelling must not mint a second key"
    );

    // The hard parent rides as a pair alias.
    let aliases: Vec<&str> = facts
        .pointer("/pair_aliases/m::channel")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    assert!(
        aliases.contains(&"inner"),
        "hard parent `inner` aliases m::channel; got {aliases:?}"
    );

    // Reachability + face gates.
    assert!(
        !pm.keys().any(|k| k.contains("ghost_fn")),
        "pub fn in a PRIVATE mod is not API; keys: {:?}",
        pm.keys().filter(|k| k.contains("ghost")).collect::<Vec<_>>()
    );
    assert!(
        !pm.keys().any(|k| k.contains("hidden_fn")),
        "doc(hidden) is disqualified"
    );
    assert!(
        !pm.keys().any(|k| k.contains("crate_fn")),
        "pub(crate) is not the public face"
    );
    assert!(
        !pm.keys().any(|k| k.ends_with("::main")),
        "`fn main` is a language entry point - the decl channel never mints it; keys: {:?}",
        pm.keys().filter(|k| k.contains("main")).collect::<Vec<_>>()
    );

    // Root-level fn keys under the crate binding; file-mod fn under
    // its file-derived module.
    assert!(
        pm.contains_key("implementation_functions:lib::rootfn"),
        "root-level pub fn -> lib::rootfn; impl-fn keys: {:?}",
        pm.keys()
            .filter(|k| k.starts_with("implementation_functions:"))
            .collect::<Vec<_>>()
    );
    assert!(
        pm.contains_key("implementation_functions:util::helper2"),
        "file-mod pub fn -> util::helper2"
    );

    // Zero-score minted keys stay unrendered until the weight term.
    let templates = Templates::new(None);
    emit(root, &out, &calibration, &templates, None).expect("emit succeeds");
    let orient =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");
    assert!(
        !orient.contains("`implementation_functions:m::channel`"),
        "zero-score decl pick must not render without a weight"
    );

    // With a weight blob the pick earns a public-by-consumption score,
    // renders in 5.2, and a consumer demanding the HARD spelling
    // (lib::inner::channel) is served through the pair alias.
    let mut pairs = std::collections::BTreeMap::new();
    pairs.insert(
        "m::channel".to_string(),
        WeightCell {
            consumers: 1,
            sites: 5,
        },
    );
    let mut targets = std::collections::BTreeMap::new();
    targets.insert(
        "lib".to_string(),
        TargetWeights {
            sources: Vec::new(),
            names: std::collections::BTreeMap::new(),
            pairs,
        },
    );
    let blob = WeightBlob { targets };
    emit(root, &out, &calibration, &templates, Some(&blob)).expect("weighted emit succeeds");
    let orient =
        std::fs::read_to_string(out.join("orientation.md")).expect("read weighted orientation");
    let s52: String = orient
        .lines()
        .skip_while(|l| !l.starts_with("### 5.2"))
        .take_while(|l| !l.starts_with("### 5.3"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        s52.contains("`implementation_functions:m::channel`"),
        "weighted decl pick renders in the PUBLIC set; 5.2:\n{s52}"
    );

    let ctmp = TempDir::new().expect("consumer tempdir");
    let cfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../t/lib\"}\n",
            ),
        ),
        (
            // The HARD spelling: full path through the private inner
            // module. Served only via the m::channel pair's alias.
            "src/lib.rs",
            String::from("pub fn go() { lib::m::inner::channel(); }\n"),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(ctmp.path(), &cfiles);
    let trace_out = ctmp.path().join("trace.json");
    measure_demand(ctmp.path(), &out, Some(&trace_out))
        .expect("hard-spelling demand served via the pair alias");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&trace_out).expect("read trace"),
    )
    .expect("parse trace");
    assert_eq!(
        report.pointer("/summary/miss_count").and_then(|v| v.as_u64()),
        Some(0),
        "no name misses"
    );
    assert_eq!(
        report.pointer("/summary/pair_miss_count").and_then(|v| v.as_u64()),
        Some(0),
        "the inner::channel pair lands exact via the alias tier"
    );
}
