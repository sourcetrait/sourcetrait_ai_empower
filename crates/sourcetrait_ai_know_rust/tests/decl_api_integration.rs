//! Integration tests for the declaration-driven API channel (the
//! hard+soft item model): pub-reachable module-level fns mint
//! pair-shaped keys with zero usage; pub-in-private-module and
//! doc(hidden) decls do not; canonical spelling follows the shortest
//! public binding with the hard parent riding as a pair alias; and
//! zero-score minted keys stay UNRENDERED until a score source
//! exists (the consumer-weight term).

use sourcetrait_ai_know_rust::*;
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
    emit(root, &out, &calibration, &templates, None, "author").expect("emit succeeds");
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
    emit(root, &out, &calibration, &templates, Some(&blob), "author").expect("weighted emit succeeds");
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

#[test]
fn decl_channel_mints_consts_and_types() {
    // The broadened decl surface: module-level pub consts/statics
    // mint globals pairs; zero-usage pub-reachable type decls mint
    // bare structure:/traits: keys - including the bevy `pub type
    // Write` shape (private mod + `pub use <mod>::*;` glob lift) and
    // the glob-lifted fn shape. All weighted-render-only.
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
            // palette: pub mod consts (RED minted; SECRET doc(hidden);
            // PRIV not pub; LABEL static minted). internal: private
            // mod (GHOST unreachable). detail: the bevy Write shape -
            // private mod + glob re-export lifts Lifted/Carried2 (and
            // the fn glob_fn). Quiet2/Bare2/Unused: zero-usage
            // top-level type decls. Veiled: doc(hidden). hidden_mod:
            // unreachable type.
            "lib/src/lib.rs",
            String::from(
                "pub mod palette {\n    pub const RED: u8 = 1;\n    #[doc(hidden)] pub const SECRET: u8 = 2;\n    const PRIV: u8 = 3;\n    pub static LABEL: &str = \"x\";\n}\nmod internal { pub const GHOST: u8 = 4; }\nmod detail {\n    pub type Lifted<T> = Option<T>;\n    pub struct Carried2;\n    pub fn glob_fn() {}\n    pub mod nested { pub type DeepLift = u16; }\n    mod sealed { pub struct NoLift; }\n}\npub use detail::*;\npub mod eng { mod st { pub const DEEP_CONST: u8 = 9; } pub use st::*; }\npub use eng::*;\npub mod eng2 { mod st2 { pub const LEAF_CONST: u8 = 7; } pub use st2::*; }\npub use eng2::LEAF_CONST;\nmacro_rules! cfg_wrap { ($($t:tt)*) => { $($t)* }; }\nmod cw2 { cfg_wrap!{ pub fn copy2() {} } }\ncfg_wrap!{ pub use cw2::copy2; }\npub mod io2 {\n    mod util3 {\n        mod cpy { cfg_wrap!{ pub async fn copy3() {} } }\n        cfg_wrap!{ pub use cpy::copy3; }\n    }\n    cfg_wrap!{ pub use util3::copy3; }\n}\npub mod fs2 {\n    mod cpy { cfg_wrap!{ pub fn copy3() {} } }\n    cfg_wrap!{ pub use self::cpy::copy3; }\n}\npub type Unused = u8;\npub struct Quiet2;\npub trait Bare2 {}\n#[doc(hidden)] pub struct Veiled;\nmod hidden_mod { pub struct Ghost2; }\npub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\n",
            ),
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
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics");

    // Consts leg: pub-reachable module-level consts/statics mint
    // globals pairs with zero counts. DEEP_CONST is the
    // glob-through-glob shape (nushell's NU_VARIABLE_ID:
    // `pub use st::*;` into eng, `pub use eng::*;` to the root) -
    // the splice closure chains, and the shortest chain (the root)
    // is canonical.
    for key in [
        "globals:palette::RED",
        "globals:palette::LABEL",
        "globals:lib::DEEP_CONST",
        // The nushell NU_VARIABLE_ID shape: a root LEAF re-export
        // whose parent names the item's VIRTUAL (glob-spliced)
        // location, not its hard parent.
        "globals:lib::LEAF_CONST",
    ] {
        let m = pm.get(key).unwrap_or_else(|| {
            panic!(
                "{key} minted; globals keys: {:?}",
                pm.keys().filter(|k| k.starts_with("globals:")).collect::<Vec<_>>()
            )
        });
        assert_eq!(m.get("intra_count").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(m.get("is_pub").and_then(|v| v.as_bool()), Some(true));
    }
    assert!(
        !pm.keys().any(|k| k.contains("SECRET") || k.contains("PRIV") || k.contains("GHOST")),
        "hidden / non-pub / unreachable consts never mint; globals keys: {:?}",
        pm.keys().filter(|k| k.starts_with("globals:")).collect::<Vec<_>>()
    );

    // Types leg: zero-usage pub-reachable decls mint bare keys -
    // including the glob-lifted private-mod class (bevy Write).
    for key in [
        "structure:Lifted",
        "structure:Carried2",
        // The bevy Write topology: a pub mod riding INSIDE the
        // glob-lifted private mod (system::lifetimeless::Write);
        // the glob splice carries the pub suffix.
        "structure:DeepLift",
        "structure:Unused",
        "structure:Quiet2",
        "traits:Bare2",
    ] {
        let m = pm.get(key).unwrap_or_else(|| {
            panic!(
                "{key} minted; structure/trait keys: {:?}",
                pm.keys()
                    .filter(|k| k.starts_with("structure:") || k.starts_with("traits:"))
                    .collect::<Vec<_>>()
            )
        });
        assert_eq!(m.get("intra_count").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(m.get("is_pub").and_then(|v| v.as_bool()), Some(true));
    }
    assert!(
        !pm.contains_key("structure:Veiled") && !pm.contains_key("structure:Ghost2"),
        "doc(hidden) + unreachable types never mint"
    );
    assert!(
        !pm.contains_key("structure:NoLift"),
        "a PRIVATE mod below the glob hop stays unreachable"
    );
    // The glob lift covers fns uniformly (the helix `pub use imp::*`
    // class): glob_fn keys under the crate root binding.
    assert!(
        pm.contains_key("implementation_functions:lib::glob_fn"),
        "glob-lifted fn mints under the root binding; impl-fn keys: {:?}",
        pm.keys()
            .filter(|k| k.starts_with("implementation_functions:"))
            .collect::<Vec<_>>()
    );
    // The tokio io::copy class (unit 7a): a macro-token fn at the
    // invocation's module chain, lifted by a macro-token `pub use`
    // - both walker-invisible before the stream-top-level rule.
    assert!(
        pm.contains_key("implementation_functions:lib::copy2"),
        "macro-token decl + macro-token pub use mint the pair; impl-fn keys: {:?}",
        pm.keys()
            .filter(|k| k.starts_with("implementation_functions:"))
            .collect::<Vec<_>>()
    );
    // The full tokio io-util composition: `pub async fn` (qualifier
    // back-walk), bound at a PRIVATE site (`pub use cpy::copy3;` in
    // private util3), hopped public by `pub use util3::copy3;` at
    // pub io2 - re-exports pierce privacy; the final hop's site
    // decides.
    assert!(
        pm.contains_key("implementation_functions:io2::copy3"),
        "binding-through-binding mints at the public hop; impl-fn keys: {:?}",
        pm.keys()
            .filter(|k| k.starts_with("implementation_functions:"))
            .collect::<Vec<_>>()
    );
    // The fs::copy/io::copy split: SAME fn name in two subtrees with
    // same-named private parent mods - site-relative bindings attach
    // only under their own site, so each decl keys in its own tree
    // (no conflation).
    assert!(
        pm.contains_key("implementation_functions:fs2::copy3"),
        "the sibling subtree's same-named fn keys separately; impl-fn keys: {:?}",
        pm.keys()
            .filter(|k| k.starts_with("implementation_functions:"))
            .collect::<Vec<_>>()
    );

    // Weighted-render-only: nothing renders without a blob...
    let templates = Templates::new(None);
    emit(root, &out, &calibration, &templates, None, "author").expect("plain emit");
    let plain = std::fs::read_to_string(out.join("orientation.md")).expect("read plain");
    for pick in ["globals:palette::RED", "structure:Lifted"] {
        assert!(
            !plain.contains(&format!("`{}`", pick)),
            "zero-score decl pick must not render without a weight: {pick}"
        );
    }

    // ...and demand-backed picks render in 5.2 with a blob (pair
    // demand for the const; NAME demand for the type).
    let mut pairs = std::collections::BTreeMap::new();
    pairs.insert(
        "palette::RED".to_string(),
        WeightCell {
            consumers: 1,
            sites: 4,
        },
    );
    let mut names = std::collections::BTreeMap::new();
    names.insert(
        "Lifted".to_string(),
        WeightCell {
            consumers: 1,
            sites: 3,
        },
    );
    // Import-shaped const demand records a NAME, not a pair; the
    // zero-usage globals key must consult the name cell.
    names.insert(
        "DEEP_CONST".to_string(),
        WeightCell {
            consumers: 1,
            sites: 2,
        },
    );
    let mut targets = std::collections::BTreeMap::new();
    targets.insert(
        "lib".to_string(),
        TargetWeights {
            sources: Vec::new(),
            names,
            pairs,
        },
    );
    let blob = WeightBlob { targets };
    emit(root, &out, &calibration, &templates, Some(&blob), "author")
        .expect("weighted emit succeeds");
    let weighted =
        std::fs::read_to_string(out.join("orientation.md")).expect("read weighted");
    let s52: String = weighted
        .lines()
        .skip_while(|l| !l.starts_with("### 5.2"))
        .take_while(|l| !l.starts_with("### 5.3"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        s52.contains("`globals:palette::RED`"),
        "weighted const pick renders in PUBLIC (the decl-site seed feeds the instance); 5.2:\n{s52}"
    );
    assert!(
        s52.contains("`structure:Lifted`"),
        "weighted glob-lifted type pick renders in PUBLIC; 5.2:\n{s52}"
    );
    assert!(
        s52.contains("`globals:lib::DEEP_CONST`"),
        "name-cell demand renders the chained-glob const at its root canonical; 5.2:\n{s52}"
    );

    // Serving: a consumer demanding the const through its module
    // path and the type by name reads zero misses.
    let ctmp = TempDir::new().expect("consumer tempdir");
    let cfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../t/lib\"}\n",
            ),
        ),
        (
            "src/lib.rs",
            String::from(
                "use lib::palette::RED;\nuse lib::{Lifted, DEEP_CONST};\npub fn go(_l: Lifted<u8>) -> u8 { RED + DEEP_CONST }\npub fn take() { let _ = lib::palette::LABEL; }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(ctmp.path(), &cfiles);
    let trace_out = ctmp.path().join("trace.json");
    measure_demand(ctmp.path(), &out, Some(&trace_out))
        .expect("const + type demands served by the decl picks");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&trace_out).expect("read trace"),
    )
    .expect("parse trace");
    assert_eq!(
        report.pointer("/summary/miss_count").and_then(|v| v.as_u64()),
        Some(0),
        "no name misses; report: {report}"
    );
    assert_eq!(
        report.pointer("/summary/pair_miss_count").and_then(|v| v.as_u64()),
        Some(0),
        "no pair misses"
    );
}
