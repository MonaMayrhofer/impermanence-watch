#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Typed<TS, TD, TF, TFB, TU> {
    Symlink(TS),
    Directory(TD),
    File(TF),
    FilesystemBoundary(TFB),
    Unknown(TU),
}

pub(crate) enum FileType {
    Symlink,
    Directory,
    File,
    FilesystemBoundary,
    Unknown,
}

impl<TS, TD, TF, TFB, TU> Typed<TS, TD, TF, TFB, TU> {
    pub(crate) fn file_type(&self) -> FileType {
        match self {
            Self::Directory(_) => FileType::Directory,
            Self::File(_) => FileType::File,
            Self::Symlink(_) => FileType::Symlink,
            Self::FilesystemBoundary(_) => FileType::FilesystemBoundary,
            Self::Unknown(_) => FileType::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action<TState, TDiff> {
    Created(TState),
    Deleted(TState),
    Modified(TDiff),
    Identical(TState),
}

#[macro_export]
macro_rules! lift_typed {
    ($value:ident => $($enum_variant:path)+) => {
        match $value {
            $(
              $enum_variant(v) => match v {
                  Typed::Symlink(it) => Typed::Symlink($enum_variant(it)),
                  Typed::Directory(it) => Typed::Directory($enum_variant(it)),
                  Typed::File(it) => Typed::File($enum_variant(it)),
                  Typed::FilesystemBoundary(it) => Typed::FilesystemBoundary($enum_variant(it)),
                  Typed::Unknown(it) => Typed::Unknown(it),
              }
            )+
        }
    };
}
