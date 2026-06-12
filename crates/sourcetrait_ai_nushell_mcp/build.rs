use std::env;
use std::path::PathBuf;

/// Capture the embedded `nu-protocol` crate version from the workspace
/// `Cargo.lock` and emit it as the `NU_VERSION` env var so the runtime
/// `info()` tool can echo it without an extra runtime dep.
///
/// The lockfile is two directories up from the crate manifest
/// (`crates/nushell_mcp/Cargo.toml` -> `Cargo.lock` at workspace root).
/// `cargo:rerun-if-changed=Cargo.lock` keeps the env var in sync with
/// any future pin bump.
fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR set by cargo");
    let lockfile = PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("Cargo.lock");
    let content = std::fs::read_to_string(&lockfile)
        .unwrap_or_else(|e| panic!("read {}: {e}", lockfile.display()));
    let parsed: toml::Value = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("parse {}: {e}", lockfile.display()));
    let nu_version = parsed["package"]
        .as_array()
        .expect("Cargo.lock has [[package]] array")
        .iter()
        .find(|p| p["name"].as_str() == Some("nu-protocol"))
        .and_then(|p| p["version"].as_str())
        .expect("nu-protocol package missing from Cargo.lock");
    println!("cargo:rustc-env=NU_VERSION={nu_version}");
    println!("cargo:rerun-if-changed={}", lockfile.display());
}
