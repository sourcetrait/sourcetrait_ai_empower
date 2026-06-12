//! Integration tests for `know_rust measure consumers` (the
//! roster-driven batch): per-pair traces resolved from
//! consumer_repos.txt, the role split (weight rows feed the blob and
//! never gate; audit rows carry the zero-miss gate), and the
//! aggregated weight blob's name/pair cells.

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

/// Target: `lib` exposes Core::new (picked via heavy app usage) and a
/// pub fn `ghost` nobody uses internally (no pick -> a consumer
/// demanding it misses).
fn build_target(root: &Path) -> std::path::PathBuf {
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
            "lib/src/lib.rs",
            String::from(
                "pub use std::time::Duration;\npub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\npub fn ghost() {}\npub fn boot() {}\n",
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
                "use lib::{Core, boot};\npub fn a() { let _ = Core::new(); boot(); }\npub fn b() { let _ = Core::new(); }\n",
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
    let templates = Templates::new(None);
    emit(root, &out, &calibration, &templates, None, "author").expect("emit succeeds");
    out
}

fn consumer(clean: bool) -> HashMap<&'static str, String> {
    let body = if clean {
        // Covered demand only: Core (pick) + Core::new pair + boot.
        "use lib::{Core, boot};\npub fn go() { let _c = Core::new(); boot(); }\n"
    } else {
        // Demands the unpicked `ghost` (one name miss) plus the
        // foreign-served Duration (foreign bucket, non-gating).
        "use lib::{Core, ghost, Duration};\npub fn go() { let _c = Core::new(); ghost(); ghost(); }\npub fn t(_d: Duration) {}\n"
    };
    [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        ("src/lib.rs", body.to_string()),
    ]
    .into_iter()
    .collect()
}

struct BatchFixture {
    _target: TempDir,
    _pairs: TempDir,
    _outputs: TempDir,
    roster_dir: TempDir,
    roster: std::path::PathBuf,
    pairs_root: std::path::PathBuf,
    outputs_root: std::path::PathBuf,
    weights: std::path::PathBuf,
}

/// Lay out: a target pass under outputs_root/<target>, two pair dirs
/// whose consumers live at `# root:` overridden paths, and the
/// roster file declaring one weight + one audit row.
fn build_batch(weight_clean: bool, audit_clean: bool) -> BatchFixture {
    let target = TempDir::new().expect("target tempdir");
    let pass = build_target(target.path());

    let outputs = TempDir::new().expect("outputs tempdir");
    let outputs_root = outputs.path().to_path_buf();
    let target_out = outputs_root.join("lib");
    std::fs::create_dir_all(&target_out).expect("mkdir target out");
    for f in ["facts.json", "fingerprint.json", "orientation.md"] {
        std::fs::copy(pass.join(f), target_out.join(f)).expect("copy pass file");
    }

    let pairs = TempDir::new().expect("pairs tempdir");
    let pairs_root = pairs.path().to_path_buf();
    let w_consumer = pairs_root.join("wsrc");
    let a_consumer = pairs_root.join("asrc");
    write_tree(&w_consumer, &consumer(weight_clean));
    write_tree(&a_consumer, &consumer(audit_clean));
    std::fs::create_dir_all(pairs_root.join("wpair")).expect("mkdir wpair");
    std::fs::create_dir_all(pairs_root.join("apair")).expect("mkdir apair");

    let roster_dir = TempDir::new().expect("roster tempdir");
    let roster = roster_dir.path().join("consumer_repos.txt");
    let text = format!(
        "# test roster\n\
         lib wpair weight https://example.com/wpair.git v1\n\
         # root: {}\n\
         lib apair audit https://example.com/apair.git v1\n\
         # root: {}\n",
        w_consumer.display(),
        a_consumer.display(),
    );
    std::fs::write(&roster, text).expect("write roster");
    let weights = roster_dir.path().join("weights.json");
    BatchFixture {
        _target: target,
        _pairs: pairs,
        _outputs: outputs,
        roster_dir,
        roster,
        pairs_root,
        outputs_root,
        weights,
    }
}

#[test]
fn weight_misses_do_not_gate_and_blob_aggregates() {
    // Weight pair demands the unpicked ghost (a miss); audit pair is
    // clean. The batch must pass the gate, write per-pair traces into
    // the pair dirs, and the blob must carry ghost's site count.
    let fx = build_batch(false, true);
    measure_consumers(
        &fx.roster,
        &fx.pairs_root,
        &fx.outputs_root,
        Some(&fx.weights),
    )
    .expect("weight-role misses must not gate the batch");

    let blob: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&fx.weights).expect("weights blob written"),
    )
    .expect("parse weights blob");
    assert_eq!(
        blob.pointer("/targets/lib/names/ghost/consumers")
            .and_then(|v| v.as_u64()),
        Some(1),
        "ghost demanded by one weight consumer; blob: {blob}"
    );
    assert_eq!(
        blob.pointer("/targets/lib/names/ghost/sites")
            .and_then(|v| v.as_u64()),
        Some(3),
        "ghost: one use-import site + two call sites"
    );
    assert!(
        blob.pointer("/targets/lib/pairs/Core::new/sites")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            >= 1,
        "Core::new pair demand rides the blob"
    );
    assert!(
        blob.pointer("/targets/lib/sources/0/consumer")
            .and_then(|v| v.as_str())
            == Some("wpair"),
        "source provenance recorded"
    );
    // Foreign-bucket item demand folds too: the hit/foreign split
    // consults the rendered sets, so a partial fold would make the
    // blob depend on which blob the pass was emitted with. Folding
    // every item-demand bucket keeps it a pure function of
    // (roster, pins).
    assert_eq!(
        blob.pointer("/targets/lib/names/Duration/consumers")
            .and_then(|v| v.as_u64()),
        Some(1),
        "foreign-served Duration demand rides the blob; blob: {blob}"
    );

    let wtrace = fx.pairs_root.join("wpair").join("consumer_trace_wpair.json");
    let atrace = fx.pairs_root.join("apair").join("consumer_trace_apair.json");
    assert!(wtrace.is_file() && atrace.is_file(), "per-pair traces written");
    drop(fx.roster_dir);
}

#[test]
fn audit_misses_gate_the_batch() {
    let fx = build_batch(true, false);
    let err = measure_consumers(&fx.roster, &fx.pairs_root, &fx.outputs_root, None)
        .expect_err("audit-role misses must gate");
    assert_eq!(
        err.to_string(),
        "demand misses: 1 name(s), 0 pair(s) uncovered"
    );
}
