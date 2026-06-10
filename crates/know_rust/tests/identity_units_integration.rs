//! Integration tests for the identity model (identity = provenance,
//! names = bindings): embedded workspace units discovered through
//! in-repo path-deps, per-crate identity fields (version / lib-name
//! rename / unit tag), unpopulated-unit identity, and the
//! .gitmodules-based submodule provenance.

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
fn embedded_units_carry_identity_and_attribution() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    // Mirrors the libcosmic shape exactly: the ROOT PACKAGE declares
    // the unit path-deps AND `[workspace] exclude` lists the unit
    // dirs. The exclusion is what lets cargo metadata tolerate a
    // manifest-less placeholder dir (excluded dirs' manifests are not
    // loaded at workspace construction; --no-deps never resolves the
    // dep) - exactly why the empty-iced-submodule libcosmic clone
    // characterizes at all.
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"host_root\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nua={path=\"vendored/vws/ua\"}\nghost={path=\"vendored/ghost\"}\ngp1={path=\"vendored/gsub/p1\"}\ngp2={path=\"vendored/gsub/p2\"}\n[workspace]\nmembers=[\"host_lib\",\"app\"]\nexclude=[\"vendored/vws\",\"vendored/ghost\",\"vendored/gsub\"]\n",
            ),
        ),
        ("src/lib.rs", String::from("pub struct RootThing;\n")),
        (
            ".gitmodules",
            String::from(
                "[submodule \"vendored/vws\"]\n\tpath = vendored/vws\n\turl = https://example.com/fork.git\n[submodule \"vendored/gsub\"]\n\tpath = vendored/gsub\n\turl = https://example.com/gsub.git\n",
            ),
        ),
        (
            "host_lib/Cargo.toml",
            String::from(
                "[package]\nname=\"host_lib\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[lib]\nname=\"hcore\"\n[dependencies]\n",
            ),
        ),
        (
            "host_lib/src/lib.rs",
            String::from("pub struct HostThing;\n"),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nhost_lib={path=\"../host_lib\"}\n",
            ),
        ),
        ("app/src/lib.rs", String::from("pub fn run() {}\n")),
        (
            "vendored/vws/Cargo.toml",
            String::from("[workspace]\nmembers=[\"ua\",\"ub\"]\n"),
        ),
        (
            "vendored/vws/ua/Cargo.toml",
            String::from(
                "[package]\nname=\"ua\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[lib]\nname=\"ua_core\"\n[dependencies]\n",
            ),
        ),
        (
            "vendored/vws/ua/src/lib.rs",
            String::from("pub struct UaThing;\npub fn ua_make() -> UaThing { UaThing }\n"),
        ),
        (
            "vendored/vws/ub/Cargo.toml",
            String::from(
                "[package]\nname=\"ub\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "vendored/vws/ub/src/lib.rs",
            String::from("pub struct UbThing;\n"),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);
    // Declared-but-unpopulated units: the empty-submodule placeholder
    // shapes (ghost = plain in-repo; gsub = submodule-declared with
    // TWO deps pointing inside it, which must coalesce to one unit).
    std::fs::create_dir_all(root.join("vendored").join("ghost")).expect("mkdir ghost");
    std::fs::create_dir_all(root.join("vendored").join("gsub")).expect("mkdir gsub");

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

    // Unit crates are first-class with their own identity, never
    // host members.
    for name in ["host_root", "host_lib", "app", "ua", "ub"] {
        assert!(
            fp.pointer(&format!("/per_crate/{}", name)).is_some(),
            "per_crate has {}",
            name
        );
    }
    assert_eq!(
        fp.pointer("/per_crate/ua/unit").and_then(|v| v.as_str()),
        Some("vendored/vws"),
        "unit crate tagged with its unit"
    );
    assert_eq!(
        fp.pointer("/per_crate/ua/dir").and_then(|v| v.as_str()),
        Some("vendored/vws/ua"),
        "unit crate keeps its real dir"
    );
    assert_eq!(
        fp.pointer("/per_crate/host_lib/unit").and_then(|v| v.as_str()),
        Some("."),
        "host crate tagged host unit"
    );

    // Lib renames ride per_crate (bindings, only when truly renamed).
    assert_eq!(
        fp.pointer("/per_crate/host_lib/lib_name").and_then(|v| v.as_str()),
        Some("hcore")
    );
    assert_eq!(
        fp.pointer("/per_crate/ua/lib_name").and_then(|v| v.as_str()),
        Some("ua_core")
    );
    assert!(
        fp.pointer("/per_crate/app/lib_name").is_none(),
        "no rename -> no lib_name field"
    );
    assert_eq!(
        fp.pointer("/per_crate/ua/version").and_then(|v| v.as_str()),
        Some("0.0.1")
    );

    // Unit table: host + populated submodule unit + unpopulated
    // ghost unit with identity but no members.
    assert_eq!(
        fp.pointer("/workspace_units/./provenance/kind").and_then(|v| v.as_str()),
        Some("host")
    );
    assert_eq!(
        fp.pointer("/workspace_units/vendored~1vws/provenance/kind")
            .and_then(|v| v.as_str()),
        Some("submodule"),
        "gitmodules-backed unit carries submodule provenance"
    );
    assert_eq!(
        fp.pointer("/workspace_units/vendored~1vws/provenance/url")
            .and_then(|v| v.as_str()),
        Some("https://example.com/fork.git")
    );
    let vws_members: Vec<&str> = fp
        .pointer("/workspace_units/vendored~1vws/members")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(vws_members, vec!["ua", "ub"]);
    assert_eq!(
        fp.pointer("/workspace_units/vendored~1ghost/populated")
            .and_then(|v| v.as_bool()),
        Some(false),
        "declared-but-absent unit still recorded with identity"
    );
    assert_eq!(
        fp.pointer("/workspace_units/vendored~1ghost/provenance/kind")
            .and_then(|v| v.as_str()),
        Some("in_repo")
    );

    // Coalescing: two deps into one declared submodule yield ONE
    // unit at the submodule root, not one per dep dir.
    assert_eq!(
        fp.pointer("/workspace_units/vendored~1gsub/provenance/kind")
            .and_then(|v| v.as_str()),
        Some("submodule")
    );
    assert_eq!(
        fp.pointer("/workspace_units/vendored~1gsub/provenance/url")
            .and_then(|v| v.as_str()),
        Some("https://example.com/gsub.git")
    );
    assert!(
        fp.pointer("/workspace_units/vendored~1gsub~1p1").is_none()
            && fp.pointer("/workspace_units/vendored~1gsub~1p2").is_none(),
        "per-dep dirs under a submodule must not fragment into units"
    );

    // Roots: host + populated unit only; mode escalates regional.
    let roots: Vec<&str> = fp
        .get("workspace_roots")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    assert!(roots.contains(&".") && roots.contains(&"vendored/vws"));
    assert!(!roots.contains(&"vendored/ghost"), "unpopulated unit is not a scanned root");
    assert_eq!(
        fp.pointer("/selection/mode").and_then(|v| v.as_str()),
        Some("regional")
    );

    // Attribution proof: the unit's items land on the unit's crates.
    let ua_thing = facts
        .get("types")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter().any(|t| {
                t.get("name").and_then(|v| v.as_str()) == Some("UaThing")
                    && t.get("crate").and_then(|v| v.as_str()) == Some("ua")
            })
        })
        .unwrap_or(false);
    assert!(ua_thing, "UaThing attributes to unit crate ua, not the host");

    // S1 renders the unit table with provenance: the non-conflation
    // surface reaches the consuming agent structurally.
    let templates = Templates::new(None);
    emit(root, &out, &calibration, &templates, None, "author").expect("emit succeeds");
    let orient =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");
    assert!(
        orient.contains("Workspace units (identity = provenance"),
        "units block renders"
    );
    assert!(
        orient.contains("- `.` - host workspace (3 members)"),
        "host line renders; got:\n{}",
        orient.lines().filter(|l| l.contains("unit") || l.starts_with("- `")).take(12).collect::<Vec<_>>().join("\n")
    );
    assert!(
        orient.contains(
            "- `vendored/vws` - vendored submodule https://example.com/fork.git @ ? (2 members scanned)"
        ),
        "populated submodule unit line renders (rev unknown outside a git repo)"
    );
    assert!(
        orient.contains("- `vendored/ghost` - in-repo vendored source (not populated; not scanned)"),
        "unpopulated in-repo unit line renders"
    );
}
