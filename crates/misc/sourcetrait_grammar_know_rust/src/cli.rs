use crate::*;

/// What: parsed CLI arguments via clap derive. Parent `scan`
/// subcommand with noun children. `usages` captures AST-derived
/// cross-item type-reference signals; `items` captures per-file
/// lex+structure facts (ported from rustscan.py).
///
/// Why: clap derive gives subcommand structure + automatic
/// help/version + future extensibility. Surface mirrors the python
/// CLI structure; future iterations port more subcommands
/// (characterize / emit / measure-overlap / etc.) as top-level
/// peers of `scan`.
///
/// Where: parsed in run.rs entrypoint via `Cli::parse()`; dispatched
/// by subcommand match.
#[derive(clap::Parser, Debug)]
#[command(
    name = "know_rust",
    version,
    about = "Rust source scanner for the know_rust orientation pipeline",
)]
pub(crate) struct Cli {
    /// Path to a custom calibration.toml; defaults to the embedded
    /// calibration shipped with the binary.
    #[arg(short = 'c', long = "calibration", global = true)]
    pub(crate) calibration_path: Option<PathBuf>,

    /// Path to a custom templates root containing `prompts/` and
    /// `templates/` subdirectories; defaults to the embedded
    /// templates shipped with the binary.
    #[arg(short = 't', long = "templates", global = true)]
    pub(crate) templates_path: Option<PathBuf>,

    /// Path to a consumer-demand weight blob (written by
    /// `measure consumers --weights-out`); when present, emit's
    /// picker scores declared-API picks by revealed consumer demand.
    #[arg(short = 'w', long = "weights", global = true)]
    pub(crate) weights_path: Option<PathBuf>,

    /// Documentation-kind profile for emit (which audience the
    /// bundle is for): `author` (default; all six sets) or
    /// `consumer` (consumer-facing surface; no per-crate internals),
    /// plus any `[profile.<name>]` declared in calibration.toml.
    #[arg(short = 'p', long = "profile", global = true, default_value = "author")]
    pub(crate) profile: String,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum Command {
    /// Source scans (AST cross-item usages + lex/structure facts).
    Scan {
        #[command(subcommand)]
        scan: ScanCommand,
    },
    /// Characterize a workspace: build the dependency graph, run the
    /// item + usage scans in-process, aggregate per-crate, compute
    /// pattern_metrics + workspace_shape + use-classification, and
    /// write facts.json + fingerprint.json (plus the intermediate
    /// know_rust_items.json + know_rust_usages.json) to the output
    /// directory.
    Characterize {
        /// Workspace root to characterize.
        workspace_root: PathBuf,
        /// Output directory; facts.json + fingerprint.json (plus
        /// know_rust_items.json + know_rust_usages.json) are written
        /// here.
        out_dir: PathBuf,
    },
    /// Emit orientation.md + reference.md from a characterize output
    /// directory. Reads fingerprint.json + facts.json; renders the
    /// agent-facing artifact via liquid templates.
    Emit {
        /// Workspace root the characterize output was produced from.
        workspace_root: PathBuf,
        /// Directory containing fingerprint.json + facts.json (also
        /// where orientation.md + reference.md are written).
        out_dir: PathBuf,
    },
    /// Measure picker output quality (supply-side overlap canary +
    /// demand-side consumer trace).
    Measure {
        #[command(subcommand)]
        measure: MeasureCommand,
    },
    /// Apply the rustdoc semantic overlay (hard-requires cargo
    /// +nightly per locked decision 4). Invokes
    /// `cargo +nightly rustdoc -p <pkg> --lib -- -Z unstable-options
    /// --output-format json` at the workspace root, reconciles the
    /// resulting rustdoc JSON against `facts.json`, and writes
    /// `rustdoc_overlay.json` to the orientation directory.
    RustdocOverlay {
        /// Workspace root the characterize output was produced from.
        workspace_root: PathBuf,
        /// Directory containing facts.json + fingerprint.json;
        /// rustdoc_overlay.json is written here.
        orientation_dir: PathBuf,
        /// Package to pass to `cargo rustdoc -p`. When omitted,
        /// resolved via cargo metadata + the characterize
        /// fingerprint's most-depended-on in-workspace crate.
        package: Option<String>,
    },
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum MeasureCommand {
    /// Trace a CONSUMER workspace's demand on a target's internals
    /// and require full coverage by the target's picks + carry (the
    /// zero-miss bar). Scans the consumer in-process; exits nonzero
    /// when any demanded name or pair is uncovered.
    Demand {
        /// Consumer workspace root to scan.
        consumer_root: PathBuf,
        /// Directory containing the target's facts.json +
        /// fingerprint.json + orientation.md (a probe .orientation/
        /// dir or a sample's output/<target>/).
        target_out_dir: PathBuf,
        /// Output JSON path; defaults to
        /// <target_out_dir>/consumer_trace.json.
        #[arg(long = "out")]
        out: Option<PathBuf>,
    },
    /// Roster-driven batch demand: trace every consumer_repos.txt
    /// pair, print per-pair scoreboards + an aggregate table, emit
    /// the consumer-demand weight blob from weight-role rows, and
    /// gate (nonzero exit) on audit-role misses only.
    Consumers {
        /// Path to consumer_repos.txt (5 columns:
        /// target snake role url ref; `# root:` comment overrides a
        /// row's consumer root).
        consumer_repos: PathBuf,
        /// Root of the pair dirs
        /// (e.g. ~/repos/know_rust_consumer).
        pairs_root: PathBuf,
        /// Root of per-target pass outputs used when a pair has no
        /// own orientation_<target>/ (e.g. a sample's output/ dir).
        outputs_root: PathBuf,
        /// Write the aggregated weight blob JSON here.
        #[arg(long = "weights-out")]
        weights_out: Option<PathBuf>,
    },
    /// Measure picker overlap against a manual ground-truth list.
    /// Walks each target's orientation.md under `samples_dir`, parses
    /// the S5.1..5.6 pick lists, and reports per-target overlap +
    /// aggregate against the JSON ground-truth file.
    Overlap {
        /// Directory containing per-target subdirs, each with an
        /// orientation.md.
        samples_dir: PathBuf,
        /// Path to the manual ground-truth JSON
        /// (notes/know_rust/manual_ground_truth.json).
        ground_truth: PathBuf,
    },
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum ScanCommand {
    /// AST-derived cross-item usage signals (fn-sig + field +
    /// type-alias + method-ref usages). Writes know_rust_usages.json
    /// in the output directory.
    Usages {
        /// Workspace root to scan.
        workspace_root: PathBuf,
        /// Output directory; know_rust_usages.json is written here.
        out_dir: PathBuf,
    },
    /// Per-file lex+structure facts (impls, derives, types,
    /// traits, fns, macros, uses, mods, seams, type_usages).
    /// Writes know_rust_items.json in the output directory.
    Items {
        /// Workspace root to scan.
        workspace_root: PathBuf,
        /// Output directory; know_rust_items.json is written here.
        out_dir: PathBuf,
    },
}
