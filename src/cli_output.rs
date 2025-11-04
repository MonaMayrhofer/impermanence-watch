use std::path::{Path, PathBuf};

use crate::dir_diff::{LeftRightBoth, dir_diff};

pub fn print_dir_diff(before: &Path, after: &Path) {
    print_dir_diff_rec(before, after);

    fn print_dir_diff_rec(before: &Path, after: &Path) {
        let diff = dir_diff(Some(before), Some(after));
        let mut contents = diff.contents.iter().collect::<Vec<_>>();
        contents.sort_by_key(|it| it.0);

        for (path, content) in diff.contents {
            match content {
                LeftRightBoth::Left(before) => {
                    print_deleted(&path, &before);
                }
                LeftRightBoth::Right(after) => {
                    print_created(&path, &after);
                }
                LeftRightBoth::Both(_, _) => {}
            }
        }
    }

    fn print_deleted(path: &Path, target: &PathBuf) {
        println!("Deleted: {}", path.display());
    }

    fn print_created(path: &Path, target: &PathBuf) {
        println!("Created: {}", path.display());
    }

    fn print_retained(path: &Path, before: &PathBuf, after: &PathBuf) {
        println!("Retained: {}", path.display());
    }
}
