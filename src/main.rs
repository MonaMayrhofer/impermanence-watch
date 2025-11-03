use std::path::Path;

use crate::dir_diff::format_dir_diff;

pub mod dir_diff;

fn main() {
    let a = dir_diff::dir_diff(
        Some(Path::new(
            "/impermanence/current_root_on_boot_snapshot/home/nionidh/",
        )),
        Some(Path::new("/home/nionidh")),
    );
    format_dir_diff(&a);
}
