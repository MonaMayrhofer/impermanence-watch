use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use crate::{
    assesser::{Assesser, Assessment},
    typed_actions::Typed,
    typed_path::{AsPath, DirectoryPath, ExistentPath, SymlinkPath, TypedPath},
};

pub type PathElementState = Typed<SymlinkState, DirectoryDiff, (), (), String>;

impl PathElementState {
    pub fn file_type(&self) -> FileType {
        match self {
            PathElementState::Directory(_) => FileType::Directory,
            PathElementState::File(_) => FileType::File,
            PathElementState::Symlink(_) => FileType::Symlink,
            PathElementState::FilesystemBoundary(_) => FileType::FilesystemBoundary,
            PathElementState::Unknown(_) => FileType::Unknown,
        }
    }
}

pub type PathElementDiff = Typed<SymlinkDiff, DirectoryDiff, (), (), String>;

impl PathElementDiff {
    pub fn file_type(&self) -> FileType {
        match self {
            PathElementDiff::Directory(_) => FileType::Directory,
            PathElementDiff::File(_) => FileType::File,
            PathElementDiff::Symlink(_) => FileType::Symlink,
            PathElementDiff::FilesystemBoundary(_) => FileType::FilesystemBoundary,
            PathElementDiff::Unknown(_) => FileType::Unknown,
        }
    }
}

#[derive(Clone, Debug)]
pub enum PathDiff {
    Recreated {
        state: LeftRightBoth<PathElementState>,
    },
    Modified(PathElementDiff),
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
pub struct SymlinkDiff {
    pub before: SymlinkState,
    pub after: SymlinkState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryDiff {
    pub entries: Vec<(PathBuf, LeftRightBoth<PathBuf>)>,
}

pub struct DiffCache {
    pub diffs: HashMap<PathBuf, Vec<Assessment>>,

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

    pub fn get_diff(&mut self, location: PathBuf) -> Option<&Vec<Assessment>> {
        //TODO This is a lifetime mess, but it don't think i can solve it before polonius hits

        if !self.diffs.contains_key(&location) {
            let new = self.calculate_diff(&location)?;

            let mut assesser = Assesser { diff_cache: self };
            let assessment = assesser.assess(&location, new);

            self.diffs.insert(location.clone(), assessment);
        }
        Some(
            self.diffs
                .get(&location)
                .expect("We literally just inserted it."),
        )
    }

    fn calculate_diff(&mut self, location: &PathBuf) -> Option<PathDiff> {
        let before = self.before.join(location);
        let after = self.after.join(location);

        let before_t = ExistentPath::try_from(before.clone())
            .ok()
            .map(TypedPath::from);
        let after_t = ExistentPath::try_from(after.clone())
            .ok()
            .map(TypedPath::from);

        match (before_t, after_t) {
            (None, None) => None,

            (None, Some(after_ft)) => Some(PathDiff::Recreated {
                state: LeftRightBoth::Right(Self::from_nothing(&after_ft)),
            }),
            (Some(before_ft), None) => Some(PathDiff::Recreated {
                state: LeftRightBoth::Left(Self::to_nothing(&before_ft)),
            }),

            (Some(before_ft), Some(after_ft)) => Some(match (before_ft, after_ft) {
                (TypedPath::Symlink(before_ft), TypedPath::Symlink(after_ft)) => {
                    PathDiff::Modified(PathElementDiff::Symlink(Self::make_symlink_diff(
                        &before_ft, &after_ft,
                    )))
                }
                (TypedPath::File(_), TypedPath::File(_)) => {
                    PathDiff::Modified(PathElementDiff::File(()))
                }
                (TypedPath::Directory(before), TypedPath::Directory(after)) => {
                    PathDiff::Modified(PathElementDiff::Directory(Self::make_directory_diff(
                        Some(&before),
                        Some(&after),
                    )))
                }
                (TypedPath::FilesystemBoundary(_), TypedPath::FilesystemBoundary(_)) => {
                    PathDiff::Modified(PathElementDiff::FilesystemBoundary(()))
                }
                (TypedPath::Unknown(_), TypedPath::Unknown(_)) => {
                    PathDiff::Modified(PathElementDiff::Unknown("Unknown".into()))
                }
                (before, after) => PathDiff::Recreated {
                    state: LeftRightBoth::Both(
                        Self::to_nothing(&before),
                        Self::from_nothing(&after),
                    ),
                },
            }),
        }
    }

    fn make_directory_diff(
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

        DirectoryDiff { entries: items }
    }

    fn make_symlink_diff(before: &SymlinkPath, after: &SymlinkPath) -> SymlinkDiff {
        SymlinkDiff {
            before: SymlinkState::from(before),
            after: SymlinkState::from(after),
        }
    }

    fn from_nothing(path: &TypedPath) -> PathElementState {
        Self::created_or_deleted(path, false)
    }
    fn to_nothing(path: &TypedPath) -> PathElementState {
        Self::created_or_deleted(path, true)
    }
    fn created_or_deleted(path: &TypedPath, deleted: bool) -> PathElementState {
        match path {
            TypedPath::Directory(dir_path) => PathElementState::Directory(if deleted {
                Self::make_directory_diff(Some(dir_path), None)
            } else {
                Self::make_directory_diff(None, Some(dir_path))
            }),
            TypedPath::Symlink(symlink_path) => {
                PathElementState::Symlink(SymlinkState::from(symlink_path))
            }
            TypedPath::File(_) => PathElementState::File(()),
            TypedPath::FilesystemBoundary(_) => PathElementState::FilesystemBoundary(()),
            TypedPath::Unknown(_) => PathElementState::Unknown("Unknown".into()),
        }
    }
}
