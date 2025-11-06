use std::path::{Path, PathBuf};

use crate::dir_diff::DiffCache;

pub(crate) fn print_dir_diff(before: &Path, after: &Path) {
    let mut cache = DiffCache::new(before, after);

    print_dir_diff_rec(&mut cache, PathBuf::new());

    fn print_dir_diff_rec(cache: &mut DiffCache, location: PathBuf) {
        let diff = cache.get_diff(location);

        todo!();
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
