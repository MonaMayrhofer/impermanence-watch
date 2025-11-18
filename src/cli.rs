use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Clone, PartialEq, Eq, Parser)]
pub(crate) struct Cli {
    #[arg(short, long, default_value_t = false)]
    pub(crate) interactive: bool,

    #[arg()]
    pub before: PathBuf,

    #[arg()]
    pub after: PathBuf,
}
