use clap::Parser;

#[derive(Debug, Clone, PartialEq, Eq, Parser)]
pub struct Cli {
    #[arg(short, long, default_value_t = false)]
    pub interactive: bool,
}
