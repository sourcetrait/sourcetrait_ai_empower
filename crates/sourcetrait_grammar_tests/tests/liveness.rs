//! The destruction half: the per-namespace host lock, and the signal handlers that run
//! the shutdown sweep. Both need a REAL process that can be signalled and killed, so
//! both are system tests.

use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::time::Duration;

use nix::fcntl::{Flock, FlockArg};
use nix::sys::signal::Signal;
use serde_json::json;
use sourcetrait_grammar_tests::{Host, namespace_dir, structured};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// Try to take the namespace's host lock the way an outside watcher would: any process,
/// any language, one non-blocking exclusive lock.
///
/// TRUE means we ACQUIRED it, which is the reading "the owner is gone". FALSE means it
/// is held, which is "the owner is alive". The guard drops immediately, so a successful
/// probe leaves nothing behind.
fn watcher_sees_host_gone(namespace: &Path) -> bool {
    let path = namespace.join("host.lock");
    let Ok(file) = std::fs::OpenOptions::new().read(true).write(true).open(&path) else {
        // No file at all is not "alive" - it is a namespace no host ever locked.
        return true;
    };
    Flock::lock(file, FlockArg::LockExclusiveNonblock).is_ok()
}

#[tested]
fn the_host_lock_is_held_while_alive_and_dropped_on_sigkill() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn_args(t.temp_dir(), &["--id", "locktest", "--namespace", "default"]);
    let namespace = namespace_dir(host.data_home(), "locktest", "default");

    // Identity is in the CONTENTS, for a reader that wants to know which host holds it.
    let info = host.call("info", json!({}));
    let mcp_nom = structured(&info)["mcp_nom"]
        .as_str()
        .expect("info carries mcp_nom")
        .to_string();
    let pid = host.pid();
    let contents = std::fs::read_to_string(namespace.join("host.lock")).expect("read host.lock");
    assert!(
        contents.contains(&mcp_nom) && contents.contains(&pid.to_string()),
        "the lock file should name its holder; got {contents:?}",
    );

    assert!(
        !watcher_sees_host_gone(&namespace),
        "a live host must hold its namespace lock, or every watcher reads it as dead",
    );

    // Drop SIGKILLs and reaps. This is the death no exit hook can ever run on - the one
    // that orphans a background job's external child - so it is the case the lock exists
    // for, and the kernel dropping the lock is what keeps "gone" the default reading.
    drop(host);
    assert!(
        watcher_sees_host_gone(&namespace),
        "the lock must be released when the host dies, even under SIGKILL",
    );
}

#[tested]
fn sigterm_runs_the_sweep_and_exits_on_its_own_terms() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn_args(t.temp_dir(), &["--id", "sigtest", "--namespace", "default"]);
    // A completed call proves the host is up and serving before it is signalled.
    let _ = host.call("info", json!({}));

    let status = host
        .signal_and_wait(Signal::SIGTERM, Duration::from_secs(15))
        .expect("a signalled host should exit rather than hang");

    assert_eq!(
        status.code(),
        Some(128 + Signal::SIGTERM as i32),
        "the handler must sweep and exit 128+signo; a plain signal death reports \
         code() == None instead, which is how the two are told apart",
    );
    assert!(
        status.signal().is_none(),
        "exiting on our own terms means NOT dying from the signal's default action",
    );
}

#[tested]
fn sighup_is_handled_too() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn_args(t.temp_dir(), &["--id", "huptest", "--namespace", "default"]);
    let _ = host.call("info", json!({}));

    let status = host
        .signal_and_wait(Signal::SIGHUP, Duration::from_secs(15))
        .expect("a signalled host should exit rather than hang");

    assert_eq!(status.code(), Some(128 + Signal::SIGHUP as i32));
}
