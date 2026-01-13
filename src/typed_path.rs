use std::{
    os::unix::fs::MetadataExt as _,
    path::{Path, PathBuf},
};

use crate::typed_actions::Typed;

/// Keep in mind that path existence can change between creation and usage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExistentPath(PathBuf);

impl TryFrom<PathBuf> for ExistentPath {
    type Error = std::io::Error;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        //Symlink_metadata instead of exists in order to not follow symlinks
        value.symlink_metadata()?;
        Ok(Self(value))
    }
}

impl ExistentPath {
    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }
}

pub(crate) trait AsPath {
    fn as_path(&self) -> &Path;
}

macro_rules! typed_path_type {
    ($name: ident) => {
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub(crate) struct $name(ExistentPath);

        impl AsPath for $name {
            fn as_path(&self) -> &Path {
                &self.0.as_path()
            }
        }
    };
}

typed_path_type!(SymlinkPath);
typed_path_type!(DirectoryPath);
typed_path_type!(FilePath);
typed_path_type!(FilesystemBoundaryPath);
typed_path_type!(UnknownPath);

impl SymlinkPath {
    pub(crate) fn target(&self) -> PathBuf {
        std::fs::read_link(self.0.as_path())
            .expect("path has already been checked to exist and be a symlink. The filesystem must have changed while the program was running.")
    }
}
impl DirectoryPath {
    pub(crate) fn read_dir(&self) -> std::fs::ReadDir {
        std::fs::read_dir(self.0.as_path()).unwrap_or_else(|_| panic!("path '{}' has already been checked to exist and be a directory. The filesystem must have changed while the program was running.", self.0.as_path().display()))
    }
}

pub(crate) type TypedPath =
    Typed<SymlinkPath, DirectoryPath, FilePath, FilesystemBoundaryPath, UnknownPath>;

impl TypedPath {
    pub fn from_existent(path: ExistentPath, parent_fs: Option<u64>) -> Self {
        if Some(path.0.symlink_metadata().unwrap().dev()) != parent_fs {
            TypedPath::FilesystemBoundary(FilesystemBoundaryPath(path))
        } else if path.0.is_symlink() {
            TypedPath::Symlink(SymlinkPath(path))
        } else if path.0.is_dir() {
            TypedPath::Directory(DirectoryPath(path))
        } else if path.0.is_file() {
            TypedPath::File(FilePath(path))
        } else {
            TypedPath::Unknown(UnknownPath(path))
        }
    }
}
