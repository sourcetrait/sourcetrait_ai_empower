use crate::*;

pub fn run_main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => clapx::style::srctrait::exit_error(e),
    }
}

pub fn run(cli: Cli) -> TraintrackResult<()> {
    dbg!(cli);
    Ok(())
}
