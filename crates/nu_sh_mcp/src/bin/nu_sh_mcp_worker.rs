use clap::{Parser, ValueEnum};

/// CLI flag values for `--mode`. Lives only in this binary; the lib
/// crate's `nu_sh_mcp::Mode` enum is clap-agnostic. Translated at the
/// seam in `main`.
#[derive(ValueEnum, Clone, Copy, Debug)]
enum CliMode {
    Stateless,
    Stateful,
}

#[derive(Parser)]
#[command(version, about = "nu_sh_mcp worker subprocess")]
struct Cli {
    /// Worker execution mode. The host (`nu_sh_mcp`) spawns two workers
    /// per server -- one stateless (drives `run()`) and one stateful
    /// (drives `interact()`) -- and passes the corresponding flag.
    #[arg(long)]
    mode: CliMode,
}

fn main() {
    let cli = Cli::parse();
    let mode = match cli.mode {
        CliMode::Stateless => nu_sh_mcp::Mode::Stateless,
        CliMode::Stateful => nu_sh_mcp::Mode::Stateful,
    };
    nu_sh_mcp::run_worker(mode);
}
