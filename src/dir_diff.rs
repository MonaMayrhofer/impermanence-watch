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
use walkdir::DirEntry;

use crate::thunk::Thunk;

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

#[derive(Clone, Debug)]
pub enum DiffStatus<TData, TModified> {
    OnlyInA {
        path: PathBuf,
        content: TData,
    },
    OnlyInB {
        path: PathBuf,
        content: TData,
    },
    InBoth {
        before: PathBuf,
        after: PathBuf,
        diff: TModified,
    },
}

pub enum DirDiffEntry {
    File {
        status: DiffStatus<Thunk<SystemTime>, Thunk<(SystemTime, SystemTime)>>,
    },
    Symlink {
        target: DiffStatus<Thunk<PathBuf>, Thunk<(PathBuf, PathBuf)>>,
    },
    Dir {
        result: DiffStatus<Thunk<DirDiffData>, Thunk<DirDiffData>>,
    },
    Skipped,
}

pub struct DirDiffData {
    pub entries: HashMap<PathBuf, DirDiffEntry>,
}

impl DirDiffData {
    pub fn has_meaningful_changes(&mut self) -> bool {
        self.entries.iter_mut().any(|(_, diff)| match diff {
            DirDiffEntry::File { .. } | DirDiffEntry::Symlink { .. } => true,
            DirDiffEntry::Dir { result } => match result {
                DiffStatus::OnlyInA { .. } | DiffStatus::OnlyInB { .. } => true,
                DiffStatus::InBoth { diff, .. } => {
                    let diff = diff.get_mut();
                    diff.has_meaningful_changes()
                }
            },
            DirDiffEntry::Skipped => false,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
enum DirDiffKey {
    File(PathBuf),
    Dir(PathBuf),
    Symlink(PathBuf),
    Unknown(PathBuf),
}
pub fn dir_diff(paths: LeftRightBoth<PathBuf>) -> DirDiffEntry {
    match paths {
        LeftRightBoth::Right(after) => DirDiffEntry::Dir {
            result: DiffStatus::OnlyInB {
                path: after.clone(),
                content: Thunk::lazy(move || dir_diff_rec(None, Some(after.as_path()))),
            },
        },
        LeftRightBoth::Left(before) => DirDiffEntry::Dir {
            result: DiffStatus::OnlyInA {
                path: before.clone(),
                content: Thunk::lazy(move || dir_diff_rec(Some(before.as_path()), None)),
            },
        },

        LeftRightBoth::Both(before, after) => DirDiffEntry::Dir {
            result: DiffStatus::InBoth {
                after: after.clone(),
                before: before.clone(),
                diff: Thunk::lazy(move || {
                    dir_diff_rec(Some(before.as_path()), Some(after.as_path()))
                }),
            },
        },
    }
}

pub fn dir_diff_rec(before_root: Option<&Path>, after_root: Option<&Path>) -> DirDiffData {
    let mut map = HashMap::new();

    let before_root = before_root.filter(|it| {
        it.metadata().unwrap().dev() == it.parent().unwrap().metadata().unwrap().dev()
    });
    let after_root = after_root.filter(|it| {
        it.metadata().unwrap().dev() == it.parent().unwrap().metadata().unwrap().dev()
    });

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
                        status: DiffStatus::OnlyInA {
                            content: Thunk::present(before.metadata().unwrap().modified().unwrap()),
                            path: before,
                        },
                    },
                    LeftRightBoth::Right(after) => DirDiffEntry::File {
                        status: DiffStatus::OnlyInB {
                            content: Thunk::present(after.metadata().unwrap().modified().unwrap()),
                            path: after,
                        },
                    },
                    LeftRightBoth::Both(before, after) => DirDiffEntry::File {
                        status: DiffStatus::InBoth {
                            diff: Thunk::present((
                                before.metadata().unwrap().modified().unwrap(),
                                after.metadata().unwrap().modified().unwrap(),
                            )),
                            before,
                            after,
                        },
                    },
                },
            ),
            DirDiffKey::Dir(path_buf) => {
                let res = dir_diff(entry);
                (path_buf, res)
            }
            DirDiffKey::Symlink(path_buf) => (
                path_buf,
                match entry {
                    LeftRightBoth::Left(before) => DirDiffEntry::Symlink {
                        target: DiffStatus::OnlyInA {
                            path: before.clone(),
                            content: Thunk::present(std::fs::read_link(before).unwrap()),
                        },
                    },
                    LeftRightBoth::Right(after) => DirDiffEntry::Symlink {
                        target: DiffStatus::OnlyInA {
                            path: after.clone(),
                            content: Thunk::present(std::fs::read_link(after).unwrap()),
                        },
                    },

                    LeftRightBoth::Both(before, after) => DirDiffEntry::Symlink {
                        target: DiffStatus::InBoth {
                            after: after.clone(),
                            before: before.clone(),
                            diff: Thunk::present((
                                std::fs::read_link(before).unwrap(),
                                std::fs::read_link(after).unwrap(),
                            )),
                        },
                    },
                },
            ),
            DirDiffKey::Unknown(path_buf) => todo!(),
        })
        .collect();

    DirDiffData { entries: map }
}

pub fn format_dir_diff(diff: &mut DirDiffEntry) {
    pub fn format_dir_diff_rec(path: &Path, diff: &mut DirDiffEntry) {
        match diff {
            DirDiffEntry::File { status } => match status {
                DiffStatus::OnlyInA { path: before, .. } => {
                    println!("{}", format!("- {}: Removed", path.display()).red())
                }
                DiffStatus::OnlyInB { path: after, .. } => {
                    println!("{}", format!("+ {}: Created", path.display()).green())
                }
                DiffStatus::InBoth {
                    before,
                    after,
                    diff,
                } => {
                    let (before_time, after_time) = diff.get();
                    if before_time != after_time {
                        println!("M {}: Modified {:?} {:?}", path.display(), before, after);
                    }
                }
            },
            DirDiffEntry::Dir { result: status } => match status {
                DiffStatus::OnlyInA { .. } => {
                    println!("{}", format!("- {}/: Removed", path.display()).red())
                }
                DiffStatus::OnlyInB {
                    path: created_path, ..
                } => {
                    let is_empty = fs::read_dir(created_path).unwrap().next().is_none();
                    if is_empty {
                        println!("{}", format!("+ {}/: Created", path.display()).white())
                    } else {
                        println!("{}", format!("+ {}/...: Created", path.display()).green())
                    }
                }
                DiffStatus::InBoth { diff, .. } => {
                    let mut paths = diff.get_mut().entries.iter_mut().collect::<Vec<_>>();
                    paths.sort_by_key(|(rel_path, _)| rel_path.to_string_lossy());

                    for (rel_path, entry) in paths {
                        let child_path = path.join(rel_path);
                        format_dir_diff_rec(&child_path, entry);
                    }
                }
            },
            DirDiffEntry::Skipped => println!(
                "{}",
                format!("x {}: Skipped", path.display()).bright_black()
            ),
            DirDiffEntry::Symlink { target } => match target {
                DiffStatus::OnlyInA { .. } => {
                    println!("{}", format!("- {}: Removed", path.display()).red());
                }
                DiffStatus::OnlyInB { content, .. } => {
                    if let Ok(after_nixpath) = path.strip_prefix("/nix/store")
                        && let Some((after_generation, after_subpath)) =
                            after_nixpath.display().to_string().split_once("-")
                    {
                        println!(
                            "{}",
                            format!(
                                "+ {}: Added NixPath to {}",
                                path.display(),
                                content.get().display()
                            )
                            .bright_black()
                        );
                    } else {
                        println!(
                            "{}",
                            format!(
                                "+ {}: Added Symlink to {}",
                                path.display(),
                                content.get().display()
                            )
                            .green()
                        );
                    }
                }
                DiffStatus::InBoth { diff, .. } => {
                    let (before, after) = diff.get();
                    if before == after {
                        return;
                    }

                    if let Ok(before_nixpath) = before.strip_prefix("/nix/store")
                        && let Ok(after_nixpath) = after.strip_prefix("/nix/store")
                        && let Some((before_generation, before_subpath)) =
                            before_nixpath.display().to_string().split_once("-")
                        && let Some((after_generation, after_subpath)) =
                            after_nixpath.display().to_string().split_once("-")
                    {
                        // Nixos Symlink
                        if before_subpath == after_subpath {
                            // Only changed generation
                            println!(
                                "{}",
                                format!(
                                    "M {}: Changed Nix Generation {} to {}",
                                    path.display(),
                                    before_generation,
                                    after_generation
                                )
                                .bright_black()
                            );
                        } else {
                            println!(
                                "{}",
                                format!(
                                    "M {}: Changed Nix Symlink {} to {}",
                                    path.display(),
                                    before_nixpath.display(),
                                    after_nixpath.display()
                                )
                                .white()
                            );
                        }
                    } else {
                        // Non-Nixos Symlink

                        println!(
                            "M {}: Modified Before: {} After: {}",
                            path.display(),
                            before.display(),
                            after.display()
                        );
                    }
                }
            },
        }
    }

    format_dir_diff_rec(Path::new(""), diff);
}
