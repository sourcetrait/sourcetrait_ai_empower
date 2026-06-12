use crate::*;

/// What: clap-derived enum mirroring the worker binary's `--mode` CLI
/// flag values. Translated to the lib's `Mode` at the seam in
/// `parse_worker_mode`.
///
/// Why: the host passes one of `stateless` / `stateful` on the worker
/// spawn; clap's `ValueEnum` derive handles parsing the string into
/// the enum. Keeping a CLI-shaped enum separate from the lib's `Mode`
/// enum lets `Mode` stay clap-agnostic.
///
/// Where: parsed via `WorkerCli` in `parse_worker_mode`; mapped to
/// `Mode::Stateless` / `Mode::Stateful` immediately after parse.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub(crate) enum CliMode {
    Stateless,
    Stateful,
}

/// What: clap-derived struct for the worker binary's CLI surface. One
/// required `--mode` flag whose value is parsed as `CliMode`.
///
/// Why: the host spawns the worker with `--mode stateless|stateful`;
/// the worker reads the flag via `WorkerCli::parse()` and translates
/// to `Mode` at the seam.
///
/// Where: constructed via `WorkerCli::parse()` in `parse_worker_mode`.
/// Both `worker_main` and any test that wraps the worker binary route
/// through this parser.
#[derive(clap::Parser)]
#[command(version, about = "nushell_mcp worker subprocess")]
pub(crate) struct WorkerCli {
    #[arg(long)]
    pub mode: CliMode,
}

/// What: parses the worker process's CLI args via `WorkerCli::parse()`
/// and returns the resulting lib-side `Mode`. Exits the process via
/// clap's standard error path if the args are malformed.
///
/// Why: factors the clap-specific parsing out of `worker_main` so the
/// lib's worker entry stays a one-liner + the CLI surface lives in a
/// dedicated cli module. `Parser` is in scope via the lib.rs prelude
/// (`pub(crate) use clap::Parser`); the `clap::Parser` /
/// `clap::ValueEnum` derives reach the macros by path.
///
/// Where: called by `worker::run::worker_main` once per worker process
/// at startup.
pub(crate) fn parse_worker_mode() -> Mode {
    let p = WorkerCli::parse();
    match p.mode {
        CliMode::Stateless => Mode::Stateless,
        CliMode::Stateful => Mode::Stateful,
    }
}
