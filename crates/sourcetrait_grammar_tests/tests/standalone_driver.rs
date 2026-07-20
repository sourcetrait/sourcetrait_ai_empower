use std::process::Command;

use sourcetrait_grammar_tests::*;
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// A committed library is usable OUTSIDE the MCP via the standalone `nu` driver
/// (`NU_LIB_DIRS=<store>/libraries nu -c "use rig/<author>/<lib>/<mod>; ..."`).
/// Spawns a real `nu` process, so it is a SYSTEM test.
#[test]
#[named]
fn committed_library_invokable_via_standalone_driver() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn_args(t.temp_dir(), &["--id", "drvid"]);
    let src = t.temp_dir().join("src").join("drvilib");
    let _ = host.library_new("sourcetrait/drvilib", &src);
    write_source(&src, "mod.nu", "export module calc\n");
    write_source(&src, "calc/mod.nu", "export use double\n");
    write_source(
        &src,
        "calc/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let committed = host.commit("sourcetrait/drvilib");
    assert!(!has_error_path(&committed), "commit should succeed; got {committed}");

    let libraries_dir = store_dir(host.data_home(), "drvid", "default").join("libraries");
    let out = Command::new("nu")
        .env("NU_LIB_DIRS", &libraries_dir)
        .arg("-c")
        .arg("use rig/sourcetrait/drvilib/calc; calc double {x: 6} | to nuon")
        .output()
        .expect("spawn nu");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "nu failed: stderr={:?}; stdout={:?}",
        String::from_utf8_lossy(&out.stderr),
        stdout,
    );
    assert!(stdout.contains("12"), "expected out: 12; got {stdout:?}");
}
