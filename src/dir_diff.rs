use std::{
    alloc::System,
    collections::HashMap,
    fmt::Debug,
    fs::{self, File},
    ops::Not,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    thread::JoinHandle,
    time::SystemTime,
};

use colored::Colorize as _;
use walkdir::DirEntry;

use crate::thunk::Thunk;

pub trait DiffLayout {
    type DirectoryDiff;
    type SymlinkDiff;

    fn make_directory_diff(before: Option<&Path>, after: Option<&Path>) -> Self::DirectoryDiff;
    fn make_symlink_diff(before: Option<&Path>, after: Option<&Path>) -> Self::SymlinkDiff;
}

pub struct DirDiff {
    pub contents: HashMap<PathBuf, LeftRightBoth<PathBuf>>,
}

pub enum PathElementDiff<TLayout: DiffLayout> {
    Directory(TLayout::DirectoryDiff),
    File,
    Symlink(TLayout::SymlinkDiff),
    Nonexistent,
    FilesystemBoundary,
    Unknown(String),
}

impl<TLayout> Debug for PathElementDiff<TLayout>
where
    TLayout: DiffLayout,
    TLayout::DirectoryDiff: Debug,
    TLayout::SymlinkDiff: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Directory(arg0) => f.debug_tuple("Directory").field(arg0).finish(),
            Self::File => write!(f, "File"),
            Self::Symlink(arg0) => f.debug_tuple("Symlink").field(arg0).finish(),
            Self::Nonexistent => write!(f, "Nonexistent"),
            Self::FilesystemBoundary => write!(f, "FilesystemBoundary"),
            Self::Unknown(arg0) => f.debug_tuple("Unknown").field(arg0).finish(),
        }
    }
}

impl<TLayout> Clone for PathElementDiff<TLayout>
where
    TLayout: DiffLayout,
    TLayout::DirectoryDiff: Clone,
    TLayout::SymlinkDiff: Clone,
{
    fn clone(&self) -> Self {
        match self {
            Self::Directory(arg0) => Self::Directory(arg0.clone()),
            Self::File => Self::File,
            Self::Symlink(arg0) => Self::Symlink(arg0.clone()),
            Self::Nonexistent => Self::Nonexistent,
            Self::FilesystemBoundary => Self::FilesystemBoundary,
            Self::Unknown(arg0) => Self::Unknown(arg0.clone()),
        }
    }
}

impl<TLayout: DiffLayout> PathElementDiff<TLayout> {
    pub fn from_nothing(path: &Path) -> Self {
        Self::created_or_deleted(path, false)
    }
    pub fn to_nothing(path: &Path) -> Self {
        Self::created_or_deleted(path, true)
    }
    fn created_or_deleted(path: &Path, deleted: bool) -> Self {
        if path.symlink_metadata().is_err() {
            return Self::Nonexistent;
        }

        match FileType::from(path) {
            FileType::Directory => Self::Directory(if deleted {
                TLayout::make_directory_diff(Some(path), None)
            } else {
                TLayout::make_directory_diff(None, Some(path))
            }),
            FileType::Symlink => Self::Symlink(if deleted {
                TLayout::make_symlink_diff(Some(path), None)
            } else {
                TLayout::make_symlink_diff(None, Some(path))
            }),
            FileType::File => Self::File,
            FileType::FilesystemBoundary => Self::FilesystemBoundary,
            FileType::Unknown => Self::Unknown("Unknown".into()),
        }
    }
}

pub enum PathDiff<TLayout: DiffLayout> {
    Recreated {
        before: PathElementDiff<TLayout>,
        after: PathElementDiff<TLayout>,
    },
    Modified(PathElementDiff<TLayout>),
    //Skipped,
}

impl<TLayout: DiffLayout> Clone for PathDiff<TLayout>
where
    PathElementDiff<TLayout>: Clone,
{
    fn clone(&self) -> Self {
        match self {
            PathDiff::Recreated { before, after } => PathDiff::Recreated {
                before: before.clone(),
                after: after.clone(),
            },
            PathDiff::Modified(element) => PathDiff::Modified(element.clone()),
            //PathDiff::Skipped => PathDiff::Skipped,
        }
    }
}

impl<TLayout: DiffLayout> PathDiff<TLayout> {
    pub fn as_directory_diff(&self) -> Option<&TLayout::DirectoryDiff> {
        match self {
            // Cases where more directories are involved (this shouldn't ever exist )
            PathDiff::Recreated {
                before: PathElementDiff::Directory(dir),
                after: PathElementDiff::Directory(dir2),
            } => unreachable!("Recreated directories should be modelled as Modified Directories"),

            // Cases where exactly one directory is involved
            PathDiff::Recreated {
                before: PathElementDiff::Directory(dir),
                after: _,
            } => Some(dir),
            PathDiff::Recreated {
                before: _,
                after: PathElementDiff::Directory(dir),
            } => Some(dir),
            PathDiff::Modified(PathElementDiff::Directory(dir)) => Some(dir),

            // Catch all
            PathDiff::Modified(_) => None,
            PathDiff::Recreated {
                before: _,
                after: _,
            } => None,
        }
    }
}

pub fn dir_diff(before: Option<&Path>, after: Option<&Path>) -> DirDiff {
    let before = before.filter(|it| it.symlink_metadata().is_ok());
    let after = after.filter(|it| it.symlink_metadata().is_ok());

    let mut map = HashMap::new();

    //     let before_root = before_root.filter(|it| {
    //         it.metadata().unwrap().dev() == it.parent().unwrap().metadata().unwrap().dev()
    //     });
    //     let after_root = after_root.filter(|it| {
    //         it.metadata().unwrap().dev() == it.parent().unwrap().metadata().unwrap().dev()
    //     });

    if let Some(before) = before {
        for entry in fs::read_dir(before).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel_path = path.strip_prefix(before).unwrap();

            map.insert(rel_path.to_owned(), LeftRightBoth::Left(path));
        }
    }
    if let Some(after) = after {
        for entry in fs::read_dir(after).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel_path = path.strip_prefix(after).unwrap();

            map.entry(rel_path.to_owned())
                .and_modify(|it| *it = it.clone().with_right(path.clone()))
                .or_insert(LeftRightBoth::Right(path.clone()));
        }
    }

    let map: HashMap<PathBuf, LeftRightBoth<PathBuf>> =
        map.into_iter().map(|(path, entry)| (path, entry)).collect();

    DirDiff { contents: map }
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

pub fn path_diff<TLayout: DiffLayout>(before: &Path, after: &Path) -> PathDiff<TLayout> {
    let before_t = before
        .symlink_metadata()
        .is_ok()
        .then(|| FileType::from(before));
    let after_t = after
        .symlink_metadata()
        .is_ok()
        .then(|| FileType::from(after));

    match (before_t, after_t) {
        //TODO This case is eww
        (None, None) => PathDiff::Modified(PathElementDiff::Nonexistent),

        (None, Some(after_ft)) => PathDiff::Recreated {
            before: PathElementDiff::Nonexistent,
            after: PathElementDiff::from_nothing(&after),
        },
        (Some(before_ft), None) => PathDiff::Recreated {
            before: PathElementDiff::to_nothing(&before),
            after: PathElementDiff::Nonexistent,
        },

        (Some(before_ft), Some(after_ft)) => match (before_ft, after_ft) {
            (FileType::Symlink, FileType::Symlink) => PathDiff::Modified(PathElementDiff::Symlink(
                TLayout::make_symlink_diff(Some(before), Some(after)),
            )),
            (FileType::File, FileType::File) => PathDiff::Modified(PathElementDiff::File),
            (FileType::Directory, FileType::Directory) => PathDiff::Modified(
                PathElementDiff::Directory(TLayout::make_directory_diff(Some(before), Some(after))),
            ),
            (FileType::FilesystemBoundary, FileType::FilesystemBoundary) => {
                PathDiff::Modified(PathElementDiff::FilesystemBoundary)
            }
            (FileType::Unknown, FileType::Unknown) => {
                PathDiff::Modified(PathElementDiff::Unknown("Unknown".into()))
            }
            (a, b) => PathDiff::Recreated {
                before: PathElementDiff::to_nothing(before),
                after: PathElementDiff::from_nothing(after),
            },
        },
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
