use crate::*;

#[derive(Debug, clap::Parser)]
#[clap(version, about)]
pub enum Cli {
    #[clap(subcommand)]
    Skill(SkillSubCmd),
}

/// Generates skills
#[derive(Debug, clap::Subcommand)]
pub enum SkillSubCmd {
    Grammar(GrammarSkillCmd),
}

/// Generates grammar/SKILL.md
#[derive(Debug, clap::Parser)]
pub struct GrammarSkillCmd {
    /// Path to where grammar/SKILL.md will be created
    pub skills_dir: PathBuf,
}
