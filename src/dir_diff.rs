use std::{
    alloc::System,
    collections::HashMap,
    fs,
    ops::Not,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    thread::JoinHandle,
    time::SystemTime,
};

use colored::Colorize as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LeftRightBoth<T> {
    Left(T),
    Right(T),
    Both(T, T),
}

impl<T> LeftRightBoth<T> {
    pub fn with_left(self, left: T) -> Self {
        match self {
            LeftRightBoth::Left(_) => LeftRightBoth::Left(left),
            LeftRightBoth::Right(r) => LeftRightBoth::Both(left, r),
            LeftRightBoth::Both(_, right) => LeftRightBoth::Both(left, right),
        }
    }

    pub fn with_right(self, right: T) -> Self {
        match self {
            LeftRightBoth::Left(l) => LeftRightBoth::Both(l, right),
            LeftRightBoth::Right(_) => LeftRightBoth::Right(right),
            LeftRightBoth::Both(left, _) => LeftRightBoth::Both(left, right),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum DiffStatus<TModified> {
    OnlyInA,
    OnlyInB,
    InBoth { diff: TModified },
}

pub enum DirDiffEntry {
    File {
        status: DiffStatus<(SystemTime, SystemTime)>,
    },
    Symlink {
        target: LeftRightBoth<PathBuf>,
    },
    Dir {
        result: DiffStatus<DirDiffResult>,
    },
    Skipped,
}

pub struct DirDiffResult {
    pub paths: HashMap<PathBuf, DirDiffEntry>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
enum DirDiffKey {
    File(PathBuf),
    Dir(PathBuf),
    Symlink(PathBuf),
    Unknown(PathBuf),
}

pub fn dir_diff(before_root: Option<&Path>, after_root: Option<&Path>) -> DirDiffResult {
    let mut map = HashMap::new();

    if let Some(before) = before_root {
        let before_dev = before.symlink_metadata().unwrap().dev();

        for entry in fs::read_dir(before).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel_path = path.strip_prefix(before).unwrap();

            // let path_dev = path.symlink_metadata().unwrap().dev();
            // if path_dev != before_dev {
            //     println!("{:?} {:?}!={:?}", path, path_dev, before_dev);
            //     continue;
            // }

            let key = if path.is_symlink() {
                DirDiffKey::Symlink(rel_path.to_owned())
            } else if path.is_dir() {
                DirDiffKey::Dir(rel_path.to_owned())
            } else if path.is_file() {
                DirDiffKey::File(rel_path.to_owned())
            } else {
                DirDiffKey::Unknown(rel_path.to_owned())
            };

            map.insert(key, LeftRightBoth::Left(path));
        }
    }

    if let Some(after) = after_root {
        let after_dev = after.symlink_metadata().unwrap().dev();
        for entry in fs::read_dir(after).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel_path = path.strip_prefix(after).unwrap();

            // let path_dev = path.symlink_metadata().unwrap().dev();
            // if path_dev != after_dev {
            //     println!("{:?} {:?}!={:?}", path, path_dev, after_dev);
            //     continue;
            // }

            let key = if path.is_symlink() {
                DirDiffKey::Symlink(rel_path.to_owned())
            } else if path.is_dir() {
                DirDiffKey::Dir(rel_path.to_owned())
            } else if path.is_file() {
                DirDiffKey::File(rel_path.to_owned())
            } else {
                DirDiffKey::Unknown(rel_path.to_owned())
            };

            map.entry(key)
                .and_modify(|it| *it = it.clone().with_right(path.clone()))
                .or_insert(LeftRightBoth::Right(path.clone()));
        }
    }

    let map: HashMap<PathBuf, DirDiffEntry> = map
        .into_iter()
        .map(|(path, entry)| match path {
            DirDiffKey::File(path_buf) => (
                path_buf,
                match entry {
                    LeftRightBoth::Left(before) => DirDiffEntry::File {
                        status: DiffStatus::OnlyInA,
                    },
                    LeftRightBoth::Right(after) => DirDiffEntry::File {
                        status: DiffStatus::OnlyInB,
                    },
                    LeftRightBoth::Both(before, after) => DirDiffEntry::File {
                        status: DiffStatus::InBoth {
                            diff: (
                                before.metadata().unwrap().modified().unwrap(),
                                after.metadata().unwrap().modified().unwrap(),
                            ),
                        },
                    },
                },
            ),
            DirDiffKey::Dir(path_buf) => {
                let res = match entry {
                    LeftRightBoth::Left(before) => DiffStatus::OnlyInA,
                    LeftRightBoth::Right(after) => DiffStatus::OnlyInB,
                    LeftRightBoth::Both(before, after) => {
                        let before_is_boundary = before.metadata().unwrap().dev()
                            != before.parent().unwrap().metadata().unwrap().dev();
                        let after_is_boundary = after.metadata().unwrap().dev()
                            != after.parent().unwrap().metadata().unwrap().dev();

                        DiffStatus::InBoth {
                            diff: dir_diff(
                                before_is_boundary.not().then_some(&before),
                                after_is_boundary.not().then_some(&after),
                            ),
                        }
                    }
                };
                (path_buf, DirDiffEntry::Dir { result: res })
            }
            DirDiffKey::Symlink(path_buf) => (
                path_buf,
                match entry {
                    LeftRightBoth::Left(before) => DirDiffEntry::Symlink {
                        target: LeftRightBoth::Left(std::fs::read_link(before).unwrap()),
                    },
                    LeftRightBoth::Right(after) => DirDiffEntry::Symlink {
                        target: LeftRightBoth::Right(std::fs::read_link(after).unwrap()),
                    },
                    LeftRightBoth::Both(before, after) => DirDiffEntry::Symlink {
                        target: LeftRightBoth::Both(
                            std::fs::read_link(before).unwrap(),
                            std::fs::read_link(after).unwrap(),
                        ),
                    },
                },
            ),
            DirDiffKey::Unknown(path_buf) => todo!(),
        })
        .collect();

    DirDiffResult { paths: map }
}

pub fn format_dir_diff(diff: &DirDiffResult) {
    pub fn format_dir_diff_rec(path_root: &Path, diff: &DirDiffResult) {
        let mut paths = diff.paths.iter().collect::<Vec<_>>();
        paths.sort_by_key(|(rel_path, _)| rel_path.to_string_lossy());

        for (rel_path, entry) in paths {
            let path = path_root.join(rel_path);
            match entry {
                DirDiffEntry::File { status } => match status {
                    DiffStatus::OnlyInA => {
                        println!("{}", format!("- {}: Removed", path.display()).red())
                    }
                    DiffStatus::OnlyInB => {
                        println!("{}", format!("+ {}: Created", path.display()).green())
                    }
                    DiffStatus::InBoth {
                        diff: (before, after),
                    } => {
                        if before != after {
                            println!("M {}: Modified {:?} {:?}", path.display(), before, after);
                        }
                    }
                },
                DirDiffEntry::Dir { result: status } => match status {
                    DiffStatus::OnlyInA => {
                        println!("{}", format!("- {}/: Removed", path.display()).red())
                    }
                    DiffStatus::OnlyInB => {
                        println!("{}", format!("+ {}/: Created", path.display()).green())
                    }
                    DiffStatus::InBoth { diff } => format_dir_diff_rec(&path, diff),
                },
                DirDiffEntry::Skipped => println!("x {}: Skipped", path.display()),
                DirDiffEntry::Symlink { target } => match target {
                    LeftRightBoth::Left(before) => {
                        println!("{}", format!("- {}: Removed", before.display()).red());
                    }
                    LeftRightBoth::Right(after) => {
                        println!("{}", format!("+ {}: Added", after.display()).green());
                    }
                    LeftRightBoth::Both(before, after) => {
                        if before == after {
                            continue;
                        }

                        println!(
                            "M {}: Modified Before: {} After: {}",
                            path.display(),
                            before.display(),
                            after.display()
                        );
                    }
                },
            }
        }
    }

    format_dir_diff_rec(Path::new(""), diff);
}
