#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Typed<TS, TD, TF, TFB, TU> {
    Symlink(TS),
    Directory(TD),
    File(TF),
    FilesystemBoundary(TFB),
    Unknown(TU),
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

pub(crate) enum StateOrDiff<TState, TDiff> {
    Diff(TDiff),
    State(TState),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ActionTag {
    Created,
    Deleted,
    Modified,
    Identical,
}
