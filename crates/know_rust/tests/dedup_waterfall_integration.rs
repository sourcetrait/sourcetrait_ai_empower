//! Integration test for R8 slice 7: dedup waterfall, no data loss.
//!
//! the_user: dedup provides UNIQUE data, it never LOSES data.
//! Subtraction runs against RENDERED (capped) sets in precedence
//! order 5.1 > 5.2 > 5.3; a candidate cut by one set's cap falls to
//! its next qualifying set with that set's own score; only cap
//! competition may drop a pick. The fixture forces the architecture
//! cap below the qualified-candidate count and asserts (a) the cut
//! candidates render in 5.2, (b) every candidate appears exactly
//! once across 5.1-5.4.

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

fn section_picks(orient: &str, start: &str, end: &str) -> Vec<String> {
    orient
        .lines()
        .skip_while(|l| !l.starts_with(start))
        .take_while(|l| !l.starts_with(end))
        .filter(|l| l.starts_with("- `"))
        .filter_map(|l| {
            let rest = &l[3..];
            rest.find('`').map(|i| rest[..i].to_string())
        })
        .collect()
}

#[test]
fn arch_cap_cut_candidates_fall_to_public_and_tier_stays_unique() {
    // Six pub structs, each with curated example evidence (one
    // examples/ file) AND cross-crate usage at distinct counts, so
    // all six (plus their ::new impl-fn bridges) qualify for the
    // architecture set. The calibration pins base_cap to the floor
    // (1): structure arch cap = round(1*1.5*2.5) = 4, so two
    // structures are cap-cut; impl-fn arch cap = round(1*1.0*2.5)
    // = 3, so three impl-fns are cap-cut. All cut candidates must
    // render in 5.2 (public cap 5 / 4 covers them), and no pattern
    // may appear in more than one workspace-wide section.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();

    let mut lib_src = String::new();
    let mut demo_src = String::from("use lib::{C0, C1, C2, C3, C4, C5};\nfn main() {\n");
    let mut app_src = String::from("use lib::{C0, C1, C2, C3, C4, C5};\n");
    for i in 0..6 {
        lib_src.push_str(&format!(
            "pub struct C{i};\nimpl C{i} {{ pub fn new() -> Self {{ C{i} }} }}\n"
        ));
        demo_src.push_str(&format!("    let _ = C{i}::new();\n"));
        let calls = 12 - 2 * i;
        for j in 0..calls {
            app_src.push_str(&format!(
                "pub fn use_c{i}_{j}() {{ let _x = C{i}::new(); }}\n"
            ));
        }
    }
    demo_src.push_str("}\n");

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
        ("lib/examples/demo.rs", demo_src),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        ("app/src/lib.rs", app_src),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let mut calibration = Calibration::default();
    // Pin every base cap to the floor so the matrix caps are tiny and
    // deterministic regardless of fixture SLOC, and pin the author
    // profile to NEUTRAL - this test's subject is the waterfall
    // arithmetic over the base matrix, not the shipped author scales.
    calibration.picker.top_n_floor = 1;
    calibration.picker.sloc_divisor = 1_000_000;
    calibration.profile.insert(
        "author".to_string(),
        ProfileConfig {
            set_scale: ProfileSetScale::neutral(),
        },
    );
    characterize(root, &out, &calibration).expect("characterize succeeds");
    let templates = Templates::new(None);
    emit(root, &out, &calibration, &templates, None, "author").expect("emit succeeds");
    let orient =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");

    let s51 = section_picks(&orient, "### 5.1", "### 5.2");
    let s52 = section_picks(&orient, "### 5.2", "### 5.3");
    let s53 = section_picks(&orient, "### 5.3", "### 5.4");
    let s54 = section_picks(&orient, "### 5.4", "### 5.5");

    // Every architecture-qualified candidate appears EXACTLY ONCE
    // across the workspace-wide tier: no loss, no duplication.
    let mut candidates: Vec<String> = Vec::new();
    for i in 0..6 {
        candidates.push(format!("structure:C{i}"));
        candidates.push(format!("implementation_functions:C{i}::new"));
    }
    for cand in &candidates {
        let appearances = [&s51, &s52, &s53, &s54]
            .iter()
            .map(|s| s.iter().filter(|p| *p == cand).count())
            .sum::<usize>();
        assert_eq!(
            appearances, 1,
            "{cand} must appear exactly once across 5.1-5.4 (no loss, no dup); \
             s51={s51:?} s52={s52:?} s53={s53:?} s54={s54:?}"
        );
    }

    // The arch cap (4 structure seats) keeps the top four; the two
    // cap-cut structures FALL to 5.2 instead of vanishing.
    for i in 0..4 {
        assert!(
            s51.contains(&format!("structure:C{i}")),
            "structure:C{i} (top-4 score) renders in 5.1; s51={s51:?}"
        );
    }
    for i in 4..6 {
        assert!(
            s52.contains(&format!("structure:C{i}")),
            "structure:C{i} (arch-cap-cut) falls to 5.2; s52={s52:?}"
        );
    }

    // Same waterfall for the impl-fn group (arch cap 3): the three
    // cut impl-fns land in 5.2.
    for i in 0..3 {
        assert!(
            s51.contains(&format!("implementation_functions:C{i}::new")),
            "implementation_functions:C{i}::new renders in 5.1; s51={s51:?}"
        );
    }
    for i in 3..6 {
        assert!(
            s52.contains(&format!("implementation_functions:C{i}::new")),
            "implementation_functions:C{i}::new (arch-cap-cut) falls to 5.2; s52={s52:?}"
        );
    }

    // Global uniqueness across the whole workspace-wide tier (every
    // rendered pick, not just the fixture candidates).
    let mut seen: HashMap<String, usize> = HashMap::new();
    for p in s51.iter().chain(&s52).chain(&s53).chain(&s54) {
        *seen.entry(p.clone()).or_default() += 1;
    }
    let dups: Vec<(&String, &usize)> = seen.iter().filter(|(_, n)| **n > 1).collect();
    assert!(
        dups.is_empty(),
        "all workspace-wide picks are unique (hard invariant); dups: {dups:?}"
    );
}
