use crate::*;

/// Result alias for the nubed crate, with `NuBedError` baked in as the error
/// variant (the `std::io::Result<T>` convention).
pub type NuBedResult<T> = Result<T, NuBedError>;

/// Failure of an isolated script run, split by phase.
///
/// `Parse` / `Compile` are pre-runtime (nothing evaluated); `Eval` is the
/// runtime failure, including errors a script raises via `error make`.
/// `MissingMain` enforces the args contract (arguments require a `main`).
/// `Timeout` and `Panicked` come from the per-run worker-thread containment:
/// a timeout cooperatively interrupts the eval, and an engine panic unwinds
/// the worker without taking down the host.
#[derive(Debug, snafu::Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum NuBedError {
    #[snafu(display("could not build the nubed engine: {message}"))]
    Setup { message: String },

    #[snafu(display("could not read script {}: {source}", path.display()))]
    ScriptRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("could not parse {name}: {message}"))]
    Parse { name: String, message: String },

    #[snafu(display("could not compile {name}: {message}"))]
    Compile { name: String, message: String },

    #[snafu(display("eval of {name} failed: {message}"))]
    Eval { name: String, message: String },

    #[snafu(display("{name} does not define `main` but {count} argument(s) were passed"))]
    MissingMain { name: String, count: usize },

    #[snafu(display("{name} exceeded the timeout {timeout:?} and was interrupted"))]
    Timeout { name: String, timeout: Duration },

    #[snafu(display("the engine worker for {name} panicked: {message}"))]
    Panicked { name: String, message: String },
}
