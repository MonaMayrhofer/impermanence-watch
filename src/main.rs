use std::path::Path;

use clap::Parser as _;

use crate::{cli::Cli, tui::tui};

pub mod assesser;
pub mod cli;
pub mod cli_output;
pub mod dir_diff;
pub mod thunk;
pub mod tui;
pub mod typed_path;

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
