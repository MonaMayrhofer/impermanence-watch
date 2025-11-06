#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Typed<TS, TD, TF, TFB, TU> {
    Symlink(TS),
    Directory(TD),
    File(TF),
    FilesystemBoundary(TFB),
    Unknown(TU),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action<TState, TDiff> {
    Created(TState),
    Deleted(TState),
    Modified(TDiff),
    Identical(TState),
}

impl<TState, TDiff> Action<TState, TDiff> {
    pub fn to_tagged(self) -> TaggedAction<TState, TDiff> {
        match self {
            Action::Created(state) => TaggedAction {
                tag: ActionTag::Created,
                content: StateOrDiff::State(state),
            },
            Action::Deleted(state) => TaggedAction {
                tag: ActionTag::Deleted,
                content: StateOrDiff::State(state),
            },
            Action::Modified(diff) => TaggedAction {
                tag: ActionTag::Modified,
                content: StateOrDiff::Diff(diff),
            },
            Action::Identical(state) => TaggedAction {
                tag: ActionTag::Identical,
                content: StateOrDiff::State(state),
            },
        }
    }
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

pub enum StateOrDiff<TState, TDiff> {
    Diff(TDiff),
    State(TState),
}

pub struct TaggedAction<TState, TDiff> {
    tag: ActionTag,
    content: StateOrDiff<TState, TDiff>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionTag {
    Created,
    Deleted,
    Modified,
    Identical,
}
