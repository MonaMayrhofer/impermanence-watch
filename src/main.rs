use std::path::Path;

use clap::Parser as _;

use crate::{cli::Cli, tui::tui};

pub(crate) mod assesser;
pub(crate) mod cli;
pub(crate) mod cli_output;
pub(crate) mod dir_diff;
pub(crate) mod tui;
pub(crate) mod typed_actions;
pub(crate) mod typed_path;

fn main() {
    let args = Cli::parse();

    if args.interactive {
        tui().unwrap();
    } else {
        cli_output::print_dir_diff(
            Path::new("/impermanence/current_root_on_boot_snapshot/home/nionidh/"),
            Path::new("/home/nionidh"),
        );
    }
}
