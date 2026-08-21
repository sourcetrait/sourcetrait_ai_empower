use crate::*;

pub fn run_main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => clapx::run_error_srctrait(e),
    }
}

pub fn run(cli: Cli) -> TraintrackResult<()> {
    dbg!(cli);
    Ok(())
}
