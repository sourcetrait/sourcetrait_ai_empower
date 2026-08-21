use crate::*;

/// Training material tool
#[derive(Debug, clap::Parser)]
#[clap(version,about)]
#[clap(styles = clapx::STYLE_SOURCETRAIT)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,

    /// Current track override, if unspecified
    #[arg(global = true, long, short = 't')]
    pub track: Option<datum::Nonce>,
    
    /// Current rail override, if unspecified
    #[arg(global = true, long, short = 'r')]
    pub rail: Option<datum::Nonce>
}

#[derive(Debug, clap::Parser)]
pub enum Cmd {
    New(NewCmd),
    Open(OpenCmd),
    Close(CloseCmd),
    Fetch(FetchCmd),
}

/// Concrete track or virtual rail
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ContextKind {
    Track,
    Rail,
}

#[derive(Debug, clap::Parser)]
pub struct NewCmd {
    context: ContextKind,
}

#[derive(Debug, clap::Parser)]
pub struct OpenCmd {
    context: ContextKind,
    nonce: datum::Nonce,
}

#[derive(Debug, clap::Parser)]
pub struct CloseCmd {
    context: ContextKind,
    /// Current if not specified
    nonce: Option<datum::Nonce>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum MaterialKind {
    Origin,
    Source,
    Derived,
}

#[derive(Debug, clap::Parser)]
pub struct FetchCmd {
    pub material: MaterialKind,
    pub namepath: PathBuf,
}