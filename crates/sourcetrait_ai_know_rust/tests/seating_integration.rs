//! Integration tests for the variety-scaled channel seating
//! (the_user ruling, 2026-06-11): the PUBLIC set's per-(group) cells
//! partition into demand-evidence + example-evidence sub-pools with a
//! breadth-driven demand quota, like-vs-like ranking inside each
//! sub-pool (demand: consumers * log2(1+sites)), two-way seat
//! spillover, and pure example seating at zero breadth.

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

/// `lib`: four structs whose `::new` factory calls live ONLY in the
/// example file (the example channel: curated=1, zero usage each)
/// plus zero-usage pub module fns (the decl channel mints `m::*`;
/// demand is their only score source). `app` contributes one
/// trait-impl of `lib::Tm` - an out-of-cell histogram signal (the
/// degenerate-workspace guard skips the whole S5 tier on an empty
/// histogram) that lands in traits/5.3 and leaves the
/// implementation_functions public cell arithmetic untouched.
fn build_fixture(root: &Path) -> std::path::PathBuf {
    let mut lib_src = String::from("pub trait Tm { fn t(&self); }\n");
    for i in 0..4 {
        lib_src.push_str(&format!(
            "pub struct C{i};\nimpl C{i} {{ pub fn new() -> Self {{ C{i} }} }}\n"
        ));
    }
    lib_src.push_str("pub mod m {\n");
    for i in 0..6 {
        lib_src.push_str(&format!("    pub fn f{i}() {{}}\n"));
    }
    lib_src.push_str("    pub fn loud() {}\n    pub fn duo() {}\n}\n");
    let mut demo = String::from("use lib::{C0, C1, C2, C3};\nfn main() {\n");
    for i in 0..4 {
        demo.push_str(&format!("    let _ = C{i}::new();\n"));
    }
    demo.push_str("}\n");
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
        ("lib/src/lib.rs", lib_src),
        ("lib/examples/demo.rs", demo),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from(
                "use lib::Tm;\npub struct A;\nimpl Tm for A { fn t(&self) {} }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);
    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    out
}

/// The mixed-evidence fixture: C0/C1/C2's `::new` calls appear in TWO
/// example files (curated 2 each); C3's in one (curated 1); pub mod m
/// supplies the pure-demand decl keys.
fn build_fixture_two_demos(root: &Path) -> std::path::PathBuf {
    let mut lib_src = String::from("pub trait Tm { fn t(&self); }\n");
    for i in 0..4 {
        lib_src.push_str(&format!(
            "pub struct C{i};\nimpl C{i} {{ pub fn new() -> Self {{ C{i} }} }}\n"
        ));
    }
    lib_src.push_str("pub mod m { pub fn solo() {} }\n");
    let mut demo = String::from("use lib::{C0, C1, C2, C3};\nfn main() {\n");
    for i in 0..4 {
        demo.push_str(&format!("    let _ = C{i}::new();\n"));
    }
    demo.push_str("}\n");
    let demo2 = String::from(
        "use lib::{C0, C1, C2};\nfn main() {\n    let _ = C0::new();\n    let _ = C1::new();\n    let _ = C2::new();\n}\n",
    );
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
        ("lib/src/lib.rs", lib_src),
        ("lib/examples/demo.rs", demo),
        ("lib/examples/demo2.rs", demo2),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from(
                "use lib::Tm;\npub struct A;\nimpl Tm for A { fn t(&self) {} }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);
    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    out
}

/// Tiny deterministic caps: base pinned to floor 1, author profile
/// pinned NEUTRAL (impl_fns public cap = round(1 * 1.0 * 3.5) = 4).
fn tiny_calibration() -> Calibration {
    let mut calibration = Calibration::default();
    calibration.picker.top_n_floor = 1;
    calibration.picker.sloc_divisor = 1_000_000;
    calibration.profile.insert(
        "author".to_string(),
        ProfileConfig {
            set_scale: ProfileSetScale::neutral(),
        },
    );
    calibration
}

fn pairs_blob(pairs: &[(&str, usize, usize)]) -> WeightBlob {
    let mut pmap = std::collections::BTreeMap::new();
    for (pair, consumers, sites) in pairs {
        pmap.insert(
            pair.to_string(),
            WeightCell {
                consumers: *consumers,
                sites: *sites,
            },
        );
    }
    let mut targets = std::collections::BTreeMap::new();
    targets.insert(
        "lib".to_string(),
        TargetWeights {
            sources: Vec::new(),
            names: std::collections::BTreeMap::new(),
            pairs: pmap,
        },
    );
    WeightBlob { targets }
}

fn section_52(orient: &str) -> String {
    orient
        .lines()
        .skip_while(|l| !l.starts_with("### 5.2"))
        .take_while(|l| !l.starts_with("### 5.3"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn quota_partitions_the_cell_and_examples_keep_their_guarantee() {
    // impl_fns public cell: 4 example entries (C0..C3::new, score 1.0
    // each) + 6 demand entries (m::f0..f5, sites 1..6), cap 4. Under
    // the OLD unified ranking the four highest-sites demand entries
    // (scores 3..6) would take the whole cell and every example would
    // drop. Seating with breadth 6 / breadth_ref 12 -> share 0.5 ->
    // q_d = 2, q_e = 2: the top-2 demand entries seat AND the top-2
    // examples keep their guaranteed seats.
    let tmp = TempDir::new().expect("tempdir");
    let out = build_fixture(tmp.path());
    let mut calibration = tiny_calibration();
    calibration.picker.seating.breadth_ref = 12.0;
    characterize(tmp.path(), &out, &calibration).expect("characterize succeeds");
    let templates = Templates::new(None);
    let blob = pairs_blob(&[
        ("m::f0", 1, 1),
        ("m::f1", 1, 2),
        ("m::f2", 1, 3),
        ("m::f3", 1, 4),
        ("m::f4", 1, 5),
        ("m::f5", 1, 6),
    ]);
    emit(tmp.path(), &out, &calibration, &templates, Some(&blob), "author")
        .expect("weighted emit succeeds");
    let s52 = section_52(
        &std::fs::read_to_string(out.join("orientation.md")).expect("read orientation"),
    );

    for seated in ["m::f5", "m::f4", "C0::new", "C1::new"] {
        assert!(
            s52.contains(&format!("`implementation_functions:{seated}`")),
            "{seated} seats; 5.2:\n{s52}"
        );
    }
    for cut in ["m::f3", "m::f2", "m::f1", "m::f0", "C2::new", "C3::new"] {
        assert!(
            !s52.contains(&format!("`implementation_functions:{cut}`")),
            "{cut} must not seat; 5.2:\n{s52}"
        );
    }

    // Zero breadth (no blob): pure example seating - the demand keys
    // carry no score and every example seat returns.
    emit(tmp.path(), &out, &calibration, &templates, None, "author")
        .expect("plain emit succeeds");
    let s52 = section_52(
        &std::fs::read_to_string(out.join("orientation.md")).expect("read orientation"),
    );
    for ex in ["C0::new", "C1::new", "C2::new", "C3::new"] {
        assert!(
            s52.contains(&format!("`implementation_functions:{ex}`")),
            "zero-breadth seating is pure example; 5.2:\n{s52}"
        );
    }
    assert!(
        !s52.contains("`implementation_functions:m::f"),
        "zero-score decl keys stay unrendered without a blob; 5.2:\n{s52}"
    );
}

#[test]
fn unused_quota_seats_demanded_examples_before_undemanded_tail() {
    // The mixed-evidence class (the bevy structure:Hdr shape): a key
    // that is demanded AND example-backed must not lose its voice
    // when quota seats sit unused. Fixture: C0/C1/C2 carry curated=2
    // (two example files), C3 curated=1 BUT demanded (sites 2);
    // m::solo is the only pure-demand entry. breadth 6 / breadth_ref
    // 8 -> share 0.65 -> quota = round(0.65 * 4) = 3; q_d = 1 (solo),
    // unused = 2, natural example seats = 1 (C0 by key-asc tie).
    // The first unused seat goes to the DEMANDED example C3 (despite
    // the lowest example score); the remainder fills by example rank
    // (C1). The undemanded curated-2 C2 is the displaced tail - under
    // a pure example-rank fill it would have seated over C3.
    let tmp = TempDir::new().expect("tempdir");
    let out = build_fixture_two_demos(tmp.path());
    let mut calibration = tiny_calibration();
    calibration.picker.seating.breadth_ref = 8.0;
    characterize(tmp.path(), &out, &calibration).expect("characterize succeeds");
    let templates = Templates::new(None);
    let mut blob = pairs_blob(&[("m::solo", 1, 5), ("C3::new", 1, 2)]);
    // Breadth filler: name cells with no pm key counterpart count
    // toward variety but seat nothing.
    let twn = &mut blob.targets.get_mut("lib").expect("lib target").names;
    for z in ["Zz1", "Zz2", "Zz3", "Zz4"] {
        twn.insert(
            z.to_string(),
            WeightCell {
                consumers: 1,
                sites: 1,
            },
        );
    }
    emit(tmp.path(), &out, &calibration, &templates, Some(&blob), "author")
        .expect("weighted emit succeeds");
    let s52 = section_52(
        &std::fs::read_to_string(out.join("orientation.md")).expect("read orientation"),
    );
    for seated in ["m::solo", "C0::new", "C3::new", "C1::new"] {
        assert!(
            s52.contains(&format!("`implementation_functions:{seated}`")),
            "{seated} seats; 5.2:\n{s52}"
        );
    }
    assert!(
        !s52.contains("`implementation_functions:C2::new`"),
        "the undemanded example tail yields the unused-quota seat to \
         the demanded C3::new; 5.2:\n{s52}"
    );
}

#[test]
fn corroboration_outranks_raw_site_volume_in_the_demand_pool() {
    // One demand seat (breadth 2 / breadth_ref 8 -> share 0.25 ->
    // q_d = round(0.25 * 4) = 1). m::loud has 5x the sites from ONE
    // consumer (rank log2(31) = 4.95); m::duo is corroborated by two
    // (rank 2 * log2(7) = 5.61). The corroborated key takes the seat
    // even though loud's raw display score (30.0) dwarfs everything.
    let tmp = TempDir::new().expect("tempdir");
    let out = build_fixture(tmp.path());
    let mut calibration = tiny_calibration();
    calibration.picker.seating.breadth_ref = 8.0;
    characterize(tmp.path(), &out, &calibration).expect("characterize succeeds");
    let templates = Templates::new(None);
    let blob = pairs_blob(&[("m::loud", 1, 30), ("m::duo", 2, 6)]);
    emit(tmp.path(), &out, &calibration, &templates, Some(&blob), "author")
        .expect("weighted emit succeeds");
    let s52 = section_52(
        &std::fs::read_to_string(out.join("orientation.md")).expect("read orientation"),
    );

    assert!(
        s52.contains("`implementation_functions:m::duo`"),
        "corroborated demand takes the single demand seat; 5.2:\n{s52}"
    );
    assert!(
        !s52.contains("`implementation_functions:m::loud`"),
        "uncorroborated site volume loses the seat; 5.2:\n{s52}"
    );
    for ex in ["C0::new", "C1::new", "C2::new"] {
        assert!(
            s52.contains(&format!("`implementation_functions:{ex}`")),
            "examples keep q_e = 3 seats; 5.2:\n{s52}"
        );
    }
    assert!(
        !s52.contains("`implementation_functions:C3::new`"),
        "the example tail yields exactly one seat to demand; 5.2:\n{s52}"
    );
}
