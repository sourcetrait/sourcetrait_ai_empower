use crate::{generate::generate_grammar_skill, *};

pub fn run() -> process::ExitCode {
    let cli = Cli::parse();
    let v = run_with(cli).unwrap();
    println!("{}", into_string_nuon(v));
    process::ExitCode::SUCCESS
}

pub fn run_with(cli: Cli) -> DocNuResult {
    match cli {
        Cli::Skill(subcmd) => match subcmd {
            SkillSubCmd::Grammar(cmd) => run_generate_skill(cmd),
        },
    }
}

pub(crate) fn run_generate_skill(cmd: GrammarSkillCmd) -> DocNuResult {
    generate_grammar_skill(&cmd.skills_dir)
}