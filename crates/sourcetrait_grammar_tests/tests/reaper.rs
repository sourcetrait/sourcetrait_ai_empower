//! The subreaper's reaping duty.
//!
//! `PR_SET_CHILD_SUBREAPER` is installed so an eval's orphaned grandchildren stay on our
//! /proc ppid chain for the tree-kill, but the same syscall makes us their PARENT - and a
//! parent must `wait()` them. Left unharvested they are zombies for the host's lifetime.
//!
//! SYSTEM tier: it needs a REAL host process to be the subreaper, and a real /proc view of
//! that process's children. Neither is reachable in-process.

use std::path::Path;
use std::time::{Duration, Instant};

use sourcetrait_grammar_tests::*;
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// Pids in state `Z` whose parent is `parent`. Fields are counted after the LAST ')' of
/// `/proc/<pid>/stat`, since the unquoted comm can contain spaces and parens.
fn zombie_children_of(parent: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let Some(rparen) = stat.rfind(')') else {
            continue;
        };
        let mut fields = stat[rparen + 1..].split_whitespace();
        let state = fields.next();
        let ppid = fields.next().and_then(|p| p.parse::<u32>().ok());
        if state == Some("Z") && ppid == Some(parent) {
            out.push(pid);
        }
    }
    out
}

/// Poll `f` until it returns true or the deadline passes.
fn wait_until(secs: u64, mut f: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if f() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Drive the host through git-committing work, then assert every zombie it adopts is
/// harvested.
///
/// The producer is `git commit`, which detaches its own auto-maintenance (`gc --auto`);
/// the rig lifecycle commits on `rig new` and on `commit`, so a couple of
/// operations reliably orphans something. The reaper harvests on the watchdog tick once a
/// zombie is past its grace window, so the bound below is grace + a tick + slack.
#[test]
#[named]
fn adopted_orphans_are_reaped() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn(t.temp_dir());
    let pid = host.pid();

    let src = t.temp_dir().join("zlib");
    let est = host.rig_new("zombielib", &src);
    assert!(!has_error_path(&est), "rig new should succeed; got {est}");
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use double\n");
    write_source(
        &src,
        "m/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let committed = host.commit("zombielib");
    assert!(!has_error_path(&committed), "commit should succeed; got {committed}");

    // The PRECONDITION, asserted rather than assumed: this test is only meaningful if the
    // host actually adopts an orphan. If git ever stops detaching its maintenance child,
    // the eventual-zero assertion below would pass vacuously - so fail loudly here
    // instead, with the reason, rather than silently testing nothing.
    let saw = wait_until(10, || !zombie_children_of(pid).is_empty());
    assert!(
        saw,
        "no orphan was adopted within 10s, so this test proved nothing. The producer is \
         git's detached `gc --auto` child; if git's behaviour changed, replace the \
         producer rather than deleting the assertion.",
    );

    // Grace is 5s and the watchdog ticks every 2s, so a harvest lands ~7s after the
    // zombie appears; 30s is slack for a loaded box, not a guess at the real latency.
    let reaped = wait_until(30, || zombie_children_of(pid).is_empty());
    assert!(
        reaped,
        "adopted zombies were never harvested; still present: {:?}",
        zombie_children_of(pid),
    );
}

/// The reaper must not disturb the waiters it shares the process with: `run_git`'s
/// `Command::output()` and nushell's own external handling both `wait()` their children,
/// and a blanket `waitpid(-1)` would steal those exit statuses (ECHILD). Rig work and
/// an external-running eval keep succeeding across several reaper ticks.
#[test]
#[named]
fn reaping_does_not_steal_exit_statuses() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn(t.temp_dir());

    for i in 0..3 {
        let name = format!("racelib{i}");
        let src = t.temp_dir().join(&name);
        let est = host.rig_new(&name, &src);
        assert!(!has_error_path(&est), "rig new {i} should succeed; got {est}");

        // An eval whose external exit status must survive: nushell reports a non-zero
        // exit as an error, so a stolen status would surface here.
        let ran = host.run(serde_json::json!({
            "args_schema": {},
            "result_schema": {"out": "string"},
            "args": {},
            "body": "{ out: (^printf ok | str trim) }"
        }));
        assert!(!has_error_path(&ran), "external eval {i} should succeed; got {ran}");
        assert_eq!(
            structured(&ran)["result"]["out"].as_str(),
            Some("ok"),
            "external output should survive the reaper; got {ran}",
        );

        // Straddle a reaper tick (2s) so the next iteration runs on the far side of one.
        std::thread::sleep(Duration::from_millis(2500));
    }
}

/// A guard on the location of the /proc contract this file depends on.
#[test]
fn proc_exposes_our_own_stat() {
    assert!(
        Path::new("/proc/self/stat").exists(),
        "these tests read process state from /proc; the crate targets Linux",
    );
}
