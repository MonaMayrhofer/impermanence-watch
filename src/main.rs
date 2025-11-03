use std::path::Path;

use clap::Parser as _;

use crate::{
    cli::Cli,
    dir_diff::{LeftRightBoth, format_dir_diff},
    tui::tui,
};

pub mod cli;
pub mod dir_diff;
pub mod thunk;
pub mod tui;

fn main() {
    let args = Cli::parse();

    if args.interactive {
        tui().unwrap();
    } else {
        let mut a = dir_diff::dir_diff(LeftRightBoth::Both(
            Path::new("/impermanence/current_root_on_boot_snapshot/home/nionidh/").to_owned(),
            Path::new("/home/nionidh").to_owned(),
        ));
        format_dir_diff(&mut a);
    }
}
