//! Integration test for workspace-adopted roots resolution (slice 1
//! of the workspace-adopted unit): the re-exported foreign surface
//! resolves to lock-pinned roots with form classification; std /
//! lang / workspace / type-rooted / example-site re-exports never
//! adopt.

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
fn adopted_roots_resolve_forms_and_exclusions() {
    // A fake CARGO_HOME registry carries the extmath checkout so the
    // glob root enumerates its pub surface end-to-end (the adopted
    // crate's own re-export graph decides what `::*` exposes).
    let fake_home = TempDir::new().expect("cargo home tempdir");
    let checkout = fake_home
        .path()
        .join("registry")
        .join("src")
        .join("index.test-0000")
        .join("extmath-0.30.10");
    std::fs::create_dir_all(checkout.join("src")).expect("mkdir checkout");
    std::fs::write(
        checkout.join("Cargo.toml"),
        "[package]\nname=\"extmath\"\nversion=\"0.30.10\"\nedition=\"2021\"\n",
    )
    .expect("write checkout manifest");
    std::fs::write(
        checkout.join("src").join("lib.rs"),
        "pub struct Vec9;\n\
         pub const EPS: f32 = 0.1;\n\
         pub mod swizzles { pub trait Vec3Swizzles {} }\n\
         mod detail { pub struct Lifted; }\n\
         pub use detail::Lifted;\n\
         mod hidden { pub struct NoSee; }\n",
    )
    .expect("write checkout lib");
    // SAFETY: single mutation before any reader; tests in this file
    // run in one process and no other test consults CARGO_HOME.
    unsafe { std::env::set_var("CARGO_HOME", fake_home.path()) };

    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"lib\",\"app\"]\n"),
        ),
        (
            // A minimal lock pinning the adopted packages.
            "Cargo.lock",
            String::from(
                "version = 4\n\n[[package]]\nname = \"extmath\"\nversion = \"0.30.10\"\n\n\
                 [[package]]\nname = \"extfut\"\nversion = \"0.3.31\"\n\n\
                 [[package]]\nname = \"lib\"\nversion = \"0.0.1\"\n",
            ),
        ),
        (
            "lib/Cargo.toml",
            String::from(
                "[package]\nname=\"lib\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "lib/src/lib.rs",
            String::from(
                "pub use extmath::*;\n\
                 pub use extmath::swizzles::Vec3Swizzles;\n\
                 pub use extfut;\n\
                 pub use std::time::Duration;\n\
                 pub use crate::detail::Inner;\n\
                 pub enum Align { Left, Right }\n\
                 pub use Align::*;\n\
                 pub mod detail { pub struct Inner; }\n\
                 pub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\n",
            ),
        ),
        (
            // Example-file re-exports never adopt.
            "lib/examples/demo.rs",
            String::from("pub use exdemo::Thing;\nfn main() {}\n"),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from("use lib::Core;\npub fn go() -> Core { Core::new() }\n"),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let calibration = Calibration::default();
    characterize(root, &out, &calibration).expect("characterize succeeds");

    let adopted: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("know_rust_adopted.json"))
            .expect("adopted artifact written"),
    )
    .expect("parse adopted json");
    let arr = adopted.as_array().expect("array of roots");
    let by_root: HashMap<&str, &serde_json::Value> = arr
        .iter()
        .filter_map(|r| r.get("root").and_then(|v| v.as_str()).map(|n| (n, r)))
        .collect();

    // Glob + leaf adoption, lock-pinned.
    let extmath = by_root.get("extmath").expect("extmath adopts");
    assert_eq!(
        extmath.get("version").and_then(|v| v.as_str()),
        Some("0.30.10"),
        "lock pins the version"
    );
    let globs: Vec<&str> = extmath
        .get("globs")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    assert!(
        globs.contains(&""),
        "root-level glob recorded; got {globs:?}"
    );
    let leaves = extmath.get("leaves").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    assert!(
        leaves.iter().any(|p| {
            p.as_array()
                .map(|ab| ab.first().and_then(|x| x.as_str()) == Some("Vec3Swizzles"))
                .unwrap_or(false)
        }),
        "leaf adoption records the source item; got {leaves:?}"
    );

    // Namespace adoption.
    let extfut = by_root.get("extfut").expect("extfut adopts");
    assert_eq!(
        extfut.get("namespace").and_then(|v| v.as_bool()),
        Some(true),
        "single-segment re-export is namespace adoption"
    );

    // Exclusions: std / lang-rooted / type-rooted / example-site.
    for never in ["std", "crate", "Align", "exdemo"] {
        assert!(
            !by_root.contains_key(never),
            "{never} must not adopt; roots: {:?}",
            by_root.keys().collect::<Vec<_>>()
        );
    }

    // Enumeration: the glob root's checkout resolved through the
    // fake registry and its pub surface enumerated - root decls,
    // module decls, and the leaf-lifted item from a PRIVATE module;
    // the unreachable hidden::NoSee stays out.
    assert!(
        extmath
            .get("checkout")
            .and_then(|v| v.as_str())
            .map(|c| c.contains("extmath-0.30.10"))
            .unwrap_or(false),
        "checkout resolves through the fake registry; got {:?}",
        extmath.get("checkout")
    );
    let surface: Vec<(String, Vec<String>)> = extmath
        .get("surface")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    let name = s.get("name").and_then(|v| v.as_str())?.to_string();
                    let chain = s
                        .get("chain")
                        .and_then(|v| v.as_array())
                        .map(|c| {
                            c.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    Some((name, chain))
                })
                .collect()
        })
        .unwrap_or_default();
    let names: Vec<&str> = surface.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.contains(&"Vec9") && names.contains(&"EPS") && names.contains(&"Vec3Swizzles"),
        "surface carries root + module decls; got {names:?}"
    );
    assert!(
        surface
            .iter()
            .any(|(n, c)| n == "Lifted" && c.is_empty()),
        "leaf-lifted item from a private module surfaces at the ROOT chain; got {surface:?}"
    );
    assert!(
        !names.contains(&"NoSee"),
        "unreachable private-module decl stays out; got {names:?}"
    );

    // Eligibility: the glob-adopted root-surface items MINT
    // first-class keys with adopted provenance; the workspace's own
    // Core stays workspace-origin.
    let fp: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".orientation").join("fingerprint.json"))
            .expect("read fingerprint"),
    )
    .expect("parse fingerprint");
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics");
    let vec9 = pm.get("structure:Vec9").expect("adopted struct mints");
    assert_eq!(
        vec9.get("adopted").and_then(|v| v.as_str()),
        Some("extmath@0.30.10"),
        "adopted provenance rides the key; got {vec9}"
    );
    assert_eq!(
        vec9.get("defining_crate").and_then(|v| v.as_str()),
        Some("extmath"),
        "defining identity is the adopted crate"
    );
    assert!(
        pm.contains_key("globals:extmath::EPS"),
        "adopted const mints a globals pair under the root outer"
    );
    assert!(
        pm.get("structure:Core")
            .and_then(|m| m.get("adopted"))
            .map(|v| v.is_null())
            .unwrap_or(true),
        "workspace-origin keys carry no adopted mark"
    );
}
