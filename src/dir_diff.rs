use std::{
    collections::HashMap,
    fmt::Debug,
    fs::{self, File},
    hash::Hash,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use crate::typed_path::{AsPath, DirectoryPath, ExistentPath, SymlinkPath, TypedPath};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathElementState {
    Directory(DirectoryDiff),
    File,
    Symlink(SymlinkState),
    FilesystemBoundary,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathElementDiff {
    Directory(DirectoryDiff),
    File,
    Symlink(SymlinkDiff),
    Nonexistent,
    FilesystemBoundary,
    Unknown(String),
}

#[derive(Clone, Debug)]
pub enum PathDiff {
    Recreated {
        state: LeftRightBoth<PathElementState>,
    },
    Modified(PathElementDiff),
}

impl PathDiff {
    pub fn as_directory_diff(&self) -> Option<&DirectoryDiff> {
        match self {
            // Cases where more directories are involved (this shouldn't ever exist )
            PathDiff::Recreated {
                state:
                    LeftRightBoth::Both(PathElementState::Directory(_), PathElementState::Directory(_)),
            } => unreachable!("Recreated directories should be modelled as Modified Directories"),

            // Cases where exactly one directory is involved
            PathDiff::Recreated {
                state: LeftRightBoth::Left(PathElementState::Directory(dir)),
            } => Some(dir),
            PathDiff::Recreated {
                state: LeftRightBoth::Right(PathElementState::Directory(dir)),
            } => Some(dir),
            PathDiff::Modified(PathElementDiff::Directory(dir)) => Some(dir),

            // Catch all
            PathDiff::Modified(_) => None,
            PathDiff::Recreated { state: _ } => None,
        }
    }
}

pub fn hashmap_diff<TKey, TVal>(
    a: HashMap<TKey, TVal>,
    b: HashMap<TKey, TVal>,
) -> HashMap<TKey, LeftRightBoth<TVal>>
where
    TKey: Eq + Hash,
{
    let mut result = HashMap::new();

    for (key, value) in a {
        result.insert(key, LeftRightBoth::Left(value));
    }

    for (key, value) in b {
        if let Some(old) = result.remove(&key) {
            result.insert(key, old.with_right(value));
        } else {
            result.insert(key, LeftRightBoth::Right(value));
        }
    }

    result
}

// pub fn dir_diff(before: Option<&Path>, after: Option<&Path>) -> DirDiff {
//     let before = before.filter(|it| it.symlink_metadata().is_ok());
//     let after = after.filter(|it| it.symlink_metadata().is_ok());

//     let mut map = HashMap::new();

//     //     let before_root = before_root.filter(|it| {
//     //         it.metadata().unwrap().dev() == it.parent().unwrap().metadata().unwrap().dev()
//     //     });
//     //     let after_root = after_root.filter(|it| {
//     //         it.metadata().unwrap().dev() == it.parent().unwrap().metadata().unwrap().dev()
//     //     });

//     if let Some(before) = before {
//         for entry in fs::read_dir(before).unwrap() {
//             let entry = entry.unwrap();
//             let path = entry.path();
//             let rel_path = path.strip_prefix(before).unwrap();

//             map.insert(rel_path.to_owned(), LeftRightBoth::Left(path));
//         }
//     }
//     if let Some(after) = after {
//         for entry in fs::read_dir(after).unwrap() {
//             let entry = entry.unwrap();
//             let path = entry.path();
//             let rel_path = path.strip_prefix(after).unwrap();

//             map.entry(rel_path.to_owned())
//                 .and_modify(|it| *it = it.clone().with_right(path.clone()))
//                 .or_insert(LeftRightBoth::Right(path.clone()));
//         }
//     }

//     let map: HashMap<PathBuf, LeftRightBoth<PathBuf>> =
//         map.into_iter().map(|(path, entry)| (path, entry)).collect();

//     DirDiff { contents: map }
// }

pub enum FileType {
    Symlink,
    Directory,
    File,
    FilesystemBoundary,
    Unknown,
}

impl From<&Path> for FileType {
    fn from(path: &Path) -> Self {
        assert!(path.symlink_metadata().is_ok());
        if Some(path.symlink_metadata().unwrap().dev())
            != path.parent().map(|it| it.symlink_metadata().unwrap().dev())
        {
            FileType::FilesystemBoundary
        } else if path.is_symlink() {
            FileType::Symlink
        } else if path.is_dir() {
            FileType::Directory
        } else if path.is_file() {
            FileType::File
        } else {
            FileType::FilesystemBoundary
        }
    }
}

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

    pub fn map<F, U>(self, mut f: F) -> LeftRightBoth<U>
    where
        F: FnMut(T) -> U,
    {
        match self {
            LeftRightBoth::Left(l) => LeftRightBoth::Left(f(l)),
            LeftRightBoth::Right(r) => LeftRightBoth::Right(f(r)),
            LeftRightBoth::Both(l, r) => LeftRightBoth::Both(f(l), f(r)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymlinkState {
    Link {
        target: PathBuf,
        broken: bool,
    },
    NixStoreLink {
        target: PathBuf,
        hash: String,
        path_within_derivation: String,
    },
}

impl SymlinkState {
    pub fn target(&self) -> &PathBuf {
        match self {
            SymlinkState::Link { target, .. } => target,
            SymlinkState::NixStoreLink { target, .. } => target,
        }
    }
}

impl From<&SymlinkPath> for SymlinkState {
    fn from(value: &SymlinkPath) -> Self {
        let target = value.target();

        if let Ok(nix_path) = target.strip_prefix("/nix/store")
            && let Some((hash, path_within_derivation)) =
                nix_path.display().to_string().split_once("-")
        {
            Self::NixStoreLink {
                target,
                hash: hash.to_string(),
                path_within_derivation: path_within_derivation.to_string(),
            }
        } else {
            Self::Link {
                broken: target.exists(),
                target,
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymlinkDiff {
    Different {
        before: PathBuf,
        after: PathBuf,
    },
    Identical {
        target: PathBuf,
    },
    IdenticalInNix {
        target: PathBuf,
    },
    DifferentInNix {
        before: PathBuf,
        after: PathBuf,
    },
    NixGenerationChanged {
        path_within_derivation: String,
        before: String,
        after: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryDiff {
    pub entries: Vec<(PathBuf, LeftRightBoth<PathBuf>)>,
    pub contains_meaningful_changes: bool,
}

pub struct DiffCache {
    pub diffs: HashMap<PathBuf, PathDiff>,

    pub before: PathBuf,
    pub after: PathBuf,
}

impl DiffCache {
    pub fn new(before: &Path, after: &Path) -> Self {
        Self {
            diffs: HashMap::new(),
            before: before.to_path_buf(),
            after: after.to_path_buf(),
        }
    }

    pub fn get_diff(&mut self, location: PathBuf) -> &PathDiff {
        //TODO This is a lifetime mess, but it don't think i can solve it before polonius hits

        if !self.diffs.contains_key(&location) {
            let new = self.calculate_diff(&location);
            self.diffs.insert(location.clone(), new);
        }
        self.diffs
            .get(&location)
            .expect("We literally just inserted it.")
    }

    fn calculate_diff(&mut self, location: &PathBuf) -> PathDiff {
        let before = self.before.join(location);
        let after = self.after.join(location);

        let before_t = ExistentPath::try_from(before.clone())
            .ok()
            .map(TypedPath::from);
        let after_t = ExistentPath::try_from(after.clone())
            .ok()
            .map(TypedPath::from);

        match (before_t, after_t) {
            //TODO This case is eww
            (None, None) => PathDiff::Modified(PathElementDiff::Nonexistent),

            (None, Some(after_ft)) => PathDiff::Recreated {
                state: LeftRightBoth::Right(self.from_nothing(location, &after_ft)),
            },
            (Some(before_ft), None) => PathDiff::Recreated {
                state: LeftRightBoth::Left(self.to_nothing(location, &before_ft)),
            },

            (Some(before_ft), Some(after_ft)) => match (before_ft, after_ft) {
                (TypedPath::Symlink(before_ft), TypedPath::Symlink(after_ft)) => {
                    PathDiff::Modified(PathElementDiff::Symlink(
                        self.make_symlink_diff(&before_ft, &after_ft),
                    ))
                }
                (TypedPath::File(_), TypedPath::File(_)) => {
                    PathDiff::Modified(PathElementDiff::File)
                }
                (TypedPath::Directory(before), TypedPath::Directory(after)) => {
                    PathDiff::Modified(PathElementDiff::Directory(self.make_directory_diff(
                        location,
                        Some(&before),
                        Some(&after),
                    )))
                }
                (TypedPath::FilesystemBoundary(_), TypedPath::FilesystemBoundary(_)) => {
                    PathDiff::Modified(PathElementDiff::FilesystemBoundary)
                }
                (TypedPath::Unknown(_), TypedPath::Unknown(_)) => {
                    PathDiff::Modified(PathElementDiff::Unknown("Unknown".into()))
                }
                (before, after) => PathDiff::Recreated {
                    state: LeftRightBoth::Both(
                        self.to_nothing(location, &before),
                        self.from_nothing(location, &after),
                    ),
                },
            },
        }
    }

    fn make_directory_diff(
        &mut self,
        location: &Path,
        before: Option<&DirectoryPath>,
        after: Option<&DirectoryPath>,
    ) -> DirectoryDiff {
        let diff = {
            let before_map = before
                .map(|dir| {
                    dir.read_dir()
                        .map(|entry| {
                            let entry = entry.unwrap();
                            let path = entry.path();
                            let child_location = path.strip_prefix(dir.as_path()).unwrap();

                            (child_location.to_owned(), path)
                        })
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default();

            let after_map = after
                .map(|dir| {
                    dir.read_dir()
                        .map(|entry| {
                            let entry = entry.unwrap();
                            let path = entry.path();
                            let child_location = path.strip_prefix(dir.as_path()).unwrap();

                            (child_location.to_owned(), path)
                        })
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default();

            hashmap_diff(before_map, after_map)
        };
        let mut items = diff.into_iter().collect::<Vec<_>>();
        items.sort_by_cached_key(|it| it.0.clone());

        let contains_meaningful_changes = items.iter().any(|(child_path, _)| {
            let child_diff = self.get_diff(location.join(child_path));
            match child_diff {
                // If it changed type, then that's a meaningful change
                PathDiff::Recreated { .. } => true,

                // If it was modified we need to look in what way
                PathDiff::Modified(path_element_diff) => match path_element_diff {
                    PathElementDiff::Directory(diff) => diff.contains_meaningful_changes,
                    PathElementDiff::Symlink(diff) => match diff {
                        SymlinkDiff::Different { .. } => true,
                        SymlinkDiff::DifferentInNix { .. } => true,
                        SymlinkDiff::Identical { .. } => false,
                        SymlinkDiff::IdenticalInNix { .. } => false,
                        SymlinkDiff::NixGenerationChanged { .. } => false,
                    },
                    PathElementDiff::File => true,
                    PathElementDiff::Nonexistent => true,
                    PathElementDiff::FilesystemBoundary => true,
                    PathElementDiff::Unknown(_) => true,
                },
            }
        });

        DirectoryDiff {
            entries: items,
            contains_meaningful_changes,
        }
    }

    fn make_symlink_diff(&mut self, before: &SymlinkPath, after: &SymlinkPath) -> SymlinkDiff {
        let before_target = before.target();
        let after_target = after.target();

        if let Ok(before_nix_path) = before_target.strip_prefix("/nix/store")
            && let Some((before_hash, before_path)) =
                before_nix_path.display().to_string().split_once("-")
            && let Ok(after_nix_path) = after_target.strip_prefix("/nix/store")
            && let Some((after_hash, after_path)) =
                after_nix_path.display().to_string().split_once("-")
        {
            if before_path == after_path {
                if before_hash == after_hash {
                    return SymlinkDiff::IdenticalInNix {
                        target: before_target.clone(),
                    };
                } else {
                    return SymlinkDiff::NixGenerationChanged {
                        path_within_derivation: before_path.to_string(),
                        before: before_hash.to_owned(),
                        after: after_hash.to_owned(),
                    };
                }
            } else {
                return SymlinkDiff::DifferentInNix {
                    before: PathBuf::from(before_nix_path),
                    after: PathBuf::from(after_nix_path),
                };
            }
        }

        if before_target == after_target {
            return SymlinkDiff::Identical {
                target: before_target,
            };
        }

        SymlinkDiff::Different {
            before: before_target,
            after: after_target,
        }
    }

    fn from_nothing(&mut self, location: &Path, path: &TypedPath) -> PathElementState {
        self.created_or_deleted(location, path, false)
    }
    fn to_nothing(&mut self, location: &Path, path: &TypedPath) -> PathElementState {
        self.created_or_deleted(location, path, true)
    }
    fn created_or_deleted(
        &mut self,
        location: &Path,
        path: &TypedPath,
        deleted: bool,
    ) -> PathElementState {
        match path {
            TypedPath::Directory(dir_path) => PathElementState::Directory(if deleted {
                self.make_directory_diff(location, Some(dir_path), None)
            } else {
                self.make_directory_diff(location, None, Some(dir_path))
            }),
            TypedPath::Symlink(symlink_path) => {
                PathElementState::Symlink(SymlinkState::from(symlink_path))
            }
            TypedPath::File(_) => PathElementState::File,
            TypedPath::FilesystemBoundary(_) => PathElementState::FilesystemBoundary,
            TypedPath::Unknown(_) => PathElementState::Unknown("Unknown".into()),
        }
    }
}
