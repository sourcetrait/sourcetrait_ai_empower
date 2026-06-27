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

/// Spawn the built claudeline binary with `payload` on stdin under the given
/// XDG_CACHE_HOME + ALT_TZ; return its stdout, asserting a clean exit.
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

/// The emptwo identity's status/ dir under a test cache root.
fn status_dir(cache: impl AsRef<Path>) -> PathBuf {
    claudeline_dir(cache).join("status")
}

/// The emptwo identity's context/ dir under a test cache root.
fn context_dir(cache: impl AsRef<Path>) -> PathBuf {
    claudeline_dir(cache).join("context")
}

fn claudeline_dir(cache: impl AsRef<Path>) -> PathBuf {
    claudeline_dir_for(cache, "emptwo")
}

/// The cache root for an explicit identity (the colony test uses ant_<fae>).
fn claudeline_dir_for(
    cache: impl AsRef<Path>,
    identity: &str,
) -> PathBuf {
    cache
        .as_ref()
        .join("sourcetrait")
        .join("empower")
        .join("claudeline")
        .join(identity)
}

/// A full, schema-current payload for session `sid` (project emptwo). The
/// SID placeholder avoids format!-brace doubling; sids never contain "SID".
fn full_payload(sid: &str) -> String {
    r#"{"session_id":"SID","workspace":{"project_dir":"/home/box/ai/emptwo"},"model":{"display_name":"Opus 4.8"},"effort":{"level":"max"},"context_window":{"used_percentage":10,"total_input_tokens":135109,"total_output_tokens":42,"context_window_size":1000000},"rate_limits":{"five_hour":{"used_percentage":3,"resets_at":1781933400},"seven_day":{"used_percentage":1}}}"#
        .replace("SID", sid)
}

/// Like run_claudeline but also exports XDGX_SHM_DIR + XDGX_TMP_HOME so the
/// transitive scratch setup + prune are exercised. ALT_TZ pinned to UTC.
fn run_claudeline_scratch(
    payload: &str,
    cache: impl AsRef<Path>,
    shm: impl AsRef<Path>,
    tmp: impl AsRef<Path>,
) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_claudeline"))
        .env("XDG_CACHE_HOME", cache.as_ref())
        .env("XDGX_SHM_DIR", shm.as_ref())
        .env("XDGX_TMP_HOME", tmp.as_ref())
        .env("ALT_TZ", "UTC")
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

/// The emptwo identity's SHM session-root under a test XDGX_SHM_DIR.
fn shm_session_root(shm: impl AsRef<Path>) -> PathBuf {
    shm.as_ref().join("ai").join("emptwo")
}

/// The emptwo identity's TMP session-root under a test XDGX_TMP_HOME.
fn tmp_session_root(tmp: impl AsRef<Path>) -> PathBuf {
    tmp.as_ref().join("ai").join("emptwo")
}

/// The current session nom, from status/latest.yaml's symlink target
/// (`<nom>.yaml` -> `<nom>`).
fn current_nom(cache: impl AsRef<Path>) -> String {
    let target = std::fs::read_link(status_dir(&cache).join("latest.yaml"))
        .expect("status latest.yaml is a symlink");
    Path::new(&target)
        .file_stem()
        .expect("nom stem")
        .to_string_lossy()
        .into_owned()
}

#[tested]
fn renders_fae_one_and_writes_status_and_context() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let cache = test.temp_dir().to_path_buf();
    let sid = "5ee9d3bc-d9db-4b6f-b964-db91c214c80a";

    // 1781933400 == 2026-06-20 05:30:00 UTC -> "0530" under ALT_TZ=UTC.
    let line = run_claudeline(&full_payload(sid), &cache, "UTC");
    assert_eq!(line, "emptwo: Opus 4.8 (max) 135k [3% 0530] {1%}\n");

    // status/: full lossless mirror + relative pointers at one real file.
    let status = status_dir(&cache);
    let latest_target =
        std::fs::read_link(status.join("latest.yaml")).expect("status latest.yaml is a symlink");
    let sid_target = std::fs::read_link(status.join(format!("{sid}.yaml")))
        .expect("status {sid}.yaml is a symlink");
    assert!(
        latest_target.is_relative(),
        "symlink target must be relative: {latest_target:?}"
    );
    assert_eq!(latest_target, sid_target, "both status pointers target one file");
    let status_yaml =
        std::fs::read_to_string(status.join("latest.yaml")).expect("read status latest.yaml");
    assert!(status_yaml.contains("session_nom:"), "status carries session_nom: {status_yaml}");
    assert!(
        status_yaml.contains("display_name: Opus 4.8"),
        "status mirrors the full payload: {status_yaml}"
    );

    // context/: minimized model + its own latest.yaml, no {sid}.yaml.
    let context = context_dir(&cache);
    assert!(
        std::fs::read_link(context.join("latest.yaml"))
            .expect("context latest.yaml is a symlink")
            .is_relative()
    );
    assert!(
        !context.join(format!("{sid}.yaml")).exists(),
        "context has no per-sid pointer"
    );
    let context_yaml =
        std::fs::read_to_string(context.join("latest.yaml")).expect("read context latest.yaml");
    assert!(context_yaml.contains("session_nom:"), "context carries nom: {context_yaml}");
    assert!(
        context_yaml.contains("total_input_tokens: 135109"),
        "context has input tokens: {context_yaml}"
    );
    assert!(
        context_yaml.contains("total_output_tokens: 42"),
        "context has output tokens: {context_yaml}"
    );
    assert!(
        context_yaml.contains("context_window_size: 1000000"),
        "context has window size: {context_yaml}"
    );
    assert!(
        !context_yaml.contains("display_name"),
        "context is minimized, not the full payload: {context_yaml}"
    );
    assert!(
        !context_yaml.contains("used_percentage"),
        "context omits used_percentage: {context_yaml}"
    );
}

#[tested]
fn new_session_prunes_older_keeps_previous() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let cache = test.temp_dir().to_path_buf();
    let status = status_dir(&cache);
    let context = context_dir(&cache);

    let sid_a = "aaaaaaaa-0000-0000-0000-000000000000";
    let sid_b = "bbbbbbbb-1111-1111-1111-111111111111";
    let sid_c = "cccccccc-2222-2222-2222-222222222222";

    run_claudeline(&full_payload(sid_a), &cache, "UTC");
    let nom_a = std::fs::read_link(status.join("latest.yaml")).expect("latest -> a");
    run_claudeline(&full_payload(sid_b), &cache, "UTC");
    let nom_b = std::fs::read_link(status.join("latest.yaml")).expect("latest -> b");
    run_claudeline(&full_payload(sid_c), &cache, "UTC");
    let nom_c = std::fs::read_link(status.join("latest.yaml")).expect("latest -> c");

    // current (C) + previous (B) survive; the older (A) is pruned, both dirs.
    for dir in [&status, &context] {
        assert!(dir.join(&nom_c).exists(), "current session kept in {dir:?}");
        assert!(dir.join(&nom_b).exists(), "previous session kept in {dir:?}");
        assert!(!dir.join(&nom_a).exists(), "older session pruned in {dir:?}");
    }
    // status sid pointers: A dropped, B + C kept; latest still points at C.
    assert!(!status.join(format!("{sid_a}.yaml")).exists(), "A sid pointer pruned");
    assert!(status.join(format!("{sid_b}.yaml")).exists(), "B sid pointer kept");
    assert!(status.join(format!("{sid_c}.yaml")).exists(), "C sid pointer kept");
    assert_eq!(std::fs::read_link(status.join("latest.yaml")).unwrap(), nom_c);
}

#[tested]
fn schema_change_writes_context_canary() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let cache = test.temp_dir().to_path_buf();
    let sid = "dddddddd-3333-3333-3333-333333333333";

    // context_window present but missing the depended-on token fields.
    let payload = r#"{"session_id":"SID","workspace":{"project_dir":"/home/box/ai/emptwo"},"context_window":{"used_percentage":7}}"#
        .replace("SID", sid);
    run_claudeline(&payload, &cache, "UTC");

    // context degrades to the canary; status still holds the full payload.
    let context_yaml = std::fs::read_to_string(context_dir(&cache).join("latest.yaml"))
        .expect("read context latest.yaml");
    assert_eq!(context_yaml, "error: statusline JSON schema has changed\n");
    let status_yaml = std::fs::read_to_string(status_dir(&cache).join("latest.yaml"))
        .expect("read status latest.yaml");
    assert!(
        status_yaml.contains("used_percentage: 7"),
        "status mirrors the drifted payload for diffing: {status_yaml}"
    );
}

#[tested]
fn no_session_id_writes_nothing_and_still_renders() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let cache = test.temp_dir().to_path_buf();

    // pre-seed a status latest.yaml for the emptwo identity.
    let status = status_dir(&cache);
    std::fs::create_dir_all(&status).unwrap();
    let seeded = status.join("latest.yaml");
    std::fs::write(&seeded, "stale").unwrap();

    // a payload with an identity (project_dir) but NO session id.
    let payload = r#"{"workspace":{"project_dir":"/home/box/ai/emptwo"},"model":{"display_name":"M"},"context_window":{"total_input_tokens":5000}}"#;
    let line = run_claudeline(payload, &cache, "UTC");

    assert_eq!(line, "emptwo: M 5k\n");
    assert_eq!(
        std::fs::read_to_string(&seeded).unwrap(),
        "stale",
        "no-sid render leaves files untouched"
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

#[tested]
fn new_session_provisions_shm_and_tmp() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let base = test.temp_dir().to_path_buf();
    let cache = base.join("cache");
    let shm = base.join("shm");
    let tmp = base.join("tmp");
    std::fs::create_dir_all(&cache).expect("cache dir");
    let sid = "5ee9d3bc-d9db-4b6f-b964-db91c214c80a";

    run_claudeline_scratch(&full_payload(sid), &cache, &shm, &tmp);
    let nom = current_nom(&cache);

    assert!(
        shm_session_root(&shm).join(&nom).is_dir(),
        "shm session dir provisioned"
    );
    assert!(
        tmp_session_root(&tmp).join(&nom).is_dir(),
        "tmp session dir provisioned"
    );
}

#[tested]
fn new_session_prunes_old_scratch_keeps_previous() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let base = test.temp_dir().to_path_buf();
    let cache = base.join("cache");
    let shm = base.join("shm");
    let tmp = base.join("tmp");
    std::fs::create_dir_all(&cache).expect("cache dir");

    let sid_a = "aaaaaaaa-0000-0000-0000-000000000000";
    let sid_b = "bbbbbbbb-1111-1111-1111-111111111111";
    let sid_c = "cccccccc-2222-2222-2222-222222222222";

    run_claudeline_scratch(&full_payload(sid_a), &cache, &shm, &tmp);
    let nom_a = current_nom(&cache);
    run_claudeline_scratch(&full_payload(sid_b), &cache, &shm, &tmp);
    let nom_b = current_nom(&cache);
    run_claudeline_scratch(&full_payload(sid_c), &cache, &shm, &tmp);
    let nom_c = current_nom(&cache);

    // current (C) + previous (B) survive; the older (A) is pruned, both roots.
    for root in [shm_session_root(&shm), tmp_session_root(&tmp)] {
        assert!(root.join(&nom_c).is_dir(), "current scratch kept in {root:?}");
        assert!(root.join(&nom_b).is_dir(), "previous scratch kept in {root:?}");
        assert!(!root.join(&nom_a).exists(), "older scratch pruned in {root:?}");
    }
}

#[tested]
fn scratch_prune_leaves_non_dir_strays() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let base = test.temp_dir().to_path_buf();
    let cache = base.join("cache");
    let shm = base.join("shm");
    let tmp = base.join("tmp");
    std::fs::create_dir_all(&cache).expect("cache dir");

    let sid_a = "aaaaaaaa-0000-0000-0000-000000000000";
    let sid_b = "bbbbbbbb-1111-1111-1111-111111111111";

    run_claudeline_scratch(&full_payload(sid_a), &cache, &shm, &tmp);
    // a stray file alongside the session dirs survives the prune.
    let stray = shm_session_root(&shm).join("stray.txt");
    std::fs::write(&stray, "x").expect("write stray");

    // a second session confirms a new-session transition (A kept as prev).
    run_claudeline_scratch(&full_payload(sid_b), &cache, &shm, &tmp);

    assert!(stray.exists(), "non-dir stray left for external cleanup");
}

#[tested]
fn colony_project_dir_maps_to_ant_identity() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let cache = test.temp_dir().to_path_buf();
    let sid = "eeeeeeee-4444-4444-4444-444444444444";

    // A colony worktree (home-relative .../ant/colony/<fae>): its basename
    // (emptwo) would collide with the bonded fae's own identity, so it must
    // scope to ant_emptwo - for both the cache tree and the rendered prefix.
    let payload = r#"{"session_id":"SID","workspace":{"project_dir":"/home/box/ai/ant/colony/emptwo"},"model":{"display_name":"M"},"context_window":{"used_percentage":5,"total_input_tokens":100,"total_output_tokens":2,"context_window_size":1000000}}"#
        .replace("SID", sid);
    let line = run_claudeline(&payload, &cache, "UTC");
    assert_eq!(line, "ant_emptwo: M 100\n", "render prefix is the ant identity");

    let colony = claudeline_dir_for(&cache, "ant_emptwo");
    assert!(
        colony.join("status").join("latest.yaml").exists(),
        "status tree under ant_emptwo"
    );
    assert!(
        colony.join("context").join("latest.yaml").exists(),
        "context tree under ant_emptwo"
    );
    assert!(
        !claudeline_dir_for(&cache, "emptwo").exists(),
        "no bare-fae collision tree"
    );
}

#[tested]
fn context_usage_renders_compact() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let cache = test.temp_dir().to_path_buf();
    // total_input_tokens -> compact form: raw <=999, floored k, rounded M.
    let cases = [
        (0_i64, "0"),
        (999, "999"),
        (1000, "1k"),
        (135109, "135k"),
        (999_999, "999k"),
        (1_000_000, "1M"),
        (1_600_000, "2M"),
    ];
    for (tokens, expected) in cases {
        let payload = r#"{"workspace":{"project_dir":"/home/box/ai/emptwo"},"model":{"display_name":"M"},"context_window":{"total_input_tokens":TOKENS}}"#
            .replace("TOKENS", &tokens.to_string());
        let line = run_claudeline(&payload, &cache, "UTC");
        assert_eq!(line, format!("emptwo: M {expected}\n"), "tokens={tokens}");
    }
}
