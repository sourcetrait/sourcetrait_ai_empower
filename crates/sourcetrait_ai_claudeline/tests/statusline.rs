use sourcetrait_testing::prelude::*;
use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};
use std::process::{
    Command,
    Stdio,
};

static TESTING: testing::Module = testing::module!(Integration, {
    .using_temp_dir()
});

/// Spawn the built claudeline binary with `payload` on stdin under the
/// given XDG_CACHE_HOME + ALT_TZ; return its stdout, asserting a clean exit.
fn run_claudeline(
    payload: &str,
    cache: impl AsRef<Path>,
    alt_tz: &str,
) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_claudeline"))
        .env("XDG_CACHE_HOME", cache.as_ref())
        .env("ALT_TZ", alt_tz)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn claudeline");
    child
        .stdin
        .take()
        .expect("stdin handle")
        .write_all(payload.as_bytes())
        .expect("write payload to stdin");
    let out = child.wait_with_output().expect("wait for claudeline");
    assert!(out.status.success(), "claudeline exited non-zero");
    String::from_utf8(out.stdout).expect("stdout is utf-8")
}

fn statusline_dir(cache: impl AsRef<Path>) -> PathBuf {
    cache
        .as_ref()
        .join("sourcetrait")
        .join("empower")
        .join("statusline")
}

#[tested]
fn renders_fae_one_and_writes_artifacts() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let cache = test.temp_dir().to_path_buf();
    let sid = "5ee9d3bc-d9db-4b6f-b964-db91c214c80a";
    let payload = r#"{"session_id":"5ee9d3bc-d9db-4b6f-b964-db91c214c80a","workspace":{"project_dir":"/home/box/ai/emptwo"},"model":{"display_name":"Opus 4.8"},"effort":{"level":"max"},"context_window":{"used_percentage":10},"rate_limits":{"five_hour":{"used_percentage":3,"resets_at":1781933400},"seven_day":{"used_percentage":1}}}"#;

    // 1781933400 == 2026-06-20 05:30:00 UTC -> "0530" under ALT_TZ=UTC.
    let line = run_claudeline(payload, &cache, "UTC");
    assert_eq!(line, "emptwo: Opus 4.8 (max) 10% [3% 0530] {1%}\n");

    let sl = statusline_dir(&cache);
    let latest = sl.join("latest.yaml");
    let sid_link = sl.join(format!("{sid}.yaml"));

    // Both pointers are RELATIVE symlinks aimed at the same {nom}.yaml file.
    let latest_target = std::fs::read_link(&latest).expect("latest.yaml is a symlink");
    let sid_target = std::fs::read_link(&sid_link).expect("{sid}.yaml is a symlink");
    assert!(
        latest_target.is_relative(),
        "symlink target must be relative: {latest_target:?}"
    );
    assert_eq!(latest_target, sid_target, "both pointers target one real file");
    assert!(latest_target.to_string_lossy().ends_with(".yaml"));

    // The real file mirrors the full payload and carries the injected nom.
    let yaml = std::fs::read_to_string(&latest).expect("read through latest.yaml");
    assert!(yaml.contains("session_nom:"), "yaml carries session_nom: {yaml}");
    assert!(
        yaml.contains("display_name: Opus 4.8"),
        "yaml mirrors the model payload: {yaml}"
    );
}

#[tested]
fn missing_session_id_removes_latest_and_still_renders() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let cache = test.temp_dir().to_path_buf();
    let sl = statusline_dir(&cache);
    std::fs::create_dir_all(&sl).unwrap();
    let latest = sl.join("latest.yaml");
    std::fs::write(&latest, "stale").unwrap();
    assert!(latest.exists(), "precondition: a stale latest.yaml exists");

    let payload = r#"{"model":{"display_name":"M"},"context_window":{"used_percentage":5}}"#;
    let line = run_claudeline(payload, &cache, "UTC");

    assert_eq!(line, "M 5%\n");
    assert!(
        std::fs::symlink_metadata(&latest).is_err(),
        "latest.yaml is removed when the payload carries no session id"
    );
}

#[tested]
fn omits_absent_segments() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let cache = test.temp_dir().to_path_buf();
    let payload = r#"{"model":{"display_name":"M"}}"#;
    let line = run_claudeline(payload, &cache, "UTC");
    assert_eq!(line, "M\n");
}
