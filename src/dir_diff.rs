use std::{
    collections::HashMap,
    fmt::Debug,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use crate::{
    assesser::{Assesser, Assessment, AssessmentCache},
    diff_util::{LeftRightBoth, hashmap_diff},
    typed_actions::Typed,
    typed_path::{AsPath, DirectoryPath, ExistentPath, SymlinkPath, TypedPath},
};

pub(crate) struct Differ {
    pub assessment_cache: HashMap<PathBuf, Vec<Assessment>>,
    pub(crate) before_root: PathBuf,
    pub(crate) after_root: PathBuf,
}

impl Differ {
    pub(crate) fn new(before: &Path, after: &Path) -> Self {
        Self {
            assessment_cache: HashMap::new(),
            before_root: before.to_path_buf(),
            after_root: after.to_path_buf(),
        }
    }

    pub(crate) fn get_diff(&mut self, location: &Path) -> Option<&Vec<Assessment>> {
        let mut this = self; //Have self be a variable, so that we can temporarily move out of it into assesser
        //TODO This is a lifetime mess, but it don't think i can solve it before polonius hits
        if !this.assessment_cache.contains_key(location) {
            let before = this.before_root.join(location);
            let after = this.after_root.join(location);

            let before_t = ExistentPath::try_from(before.to_path_buf()).ok().map(|it| {
                TypedPath::from_existent(
                    it,
                    this.before_root.symlink_metadata().ok().map(|it| it.dev()),
                )
            });
            let after_t = ExistentPath::try_from(after.to_path_buf()).ok().map(|it| {
                TypedPath::from_existent(
                    it,
                    this.after_root.symlink_metadata().ok().map(|it| it.dev()),
                )
            });

            let new = calculate_diff(before_t, after_t)?;

            let mut assesser = Assesser { diff_cache: this };
            let assessment = assesser.assess(location, new);

            this = assesser.diff_cache;
            this.assessment_cache
                .insert(location.to_path_buf(), assessment);
        }
        Some(
            this.assessment_cache
                .get(location)
                .expect("We literally just inserted it."),
        )
    }
}

impl AssessmentCache for &mut Differ {
    fn get_assessment(&mut self, location: &Path) -> Option<&Vec<Assessment>> {
        self.get_diff(location)
    }
}

// ======================

fn calculate_diff(before: Option<TypedPath>, after: Option<TypedPath>) -> Option<PathDiff> {
    match (before, after) {
        (None, None) => None,

        (None, Some(after_ft)) => Some(PathDiff::Recreated {
            state: LeftRightBoth::Right(from_nothing(&after_ft)),
        }),
        (Some(before_ft), None) => Some(PathDiff::Recreated {
            state: LeftRightBoth::Left(to_nothing(&before_ft)),
        }),

        (Some(before_ft), Some(after_ft)) => Some(match (before_ft, after_ft) {
            (TypedPath::Symlink(before_ft), TypedPath::Symlink(after_ft)) => PathDiff::Modified(
                TypedPathDiff::Symlink(SymlinkDiff::diff_paths(&before_ft, &after_ft)),
            ),
            (TypedPath::File(_), TypedPath::File(_)) => PathDiff::Modified(TypedPathDiff::File(())),
            (TypedPath::Directory(before), TypedPath::Directory(after)) => PathDiff::Modified(
                TypedPathDiff::Directory(DirectoryDiff::diff_paths(Some(&before), Some(&after))),
            ),
            (TypedPath::FilesystemBoundary(_), TypedPath::FilesystemBoundary(_)) => {
                PathDiff::Modified(TypedPathDiff::FilesystemBoundary(()))
            }
            (TypedPath::Unknown(_), TypedPath::Unknown(_)) => {
                PathDiff::Modified(TypedPathDiff::Unknown("Unknown".into()))
            }
            (before, after) => PathDiff::Recreated {
                state: LeftRightBoth::Both(to_nothing(&before), from_nothing(&after)),
            },
        }),
    }
}

fn from_nothing(path: &TypedPath) -> TypedPathState {
    match path {
        TypedPath::Directory(dir_path) => {
            TypedPathState::Directory(DirectoryDiff::diff_paths(None, Some(dir_path)))
        }
        TypedPath::Symlink(symlink_path) => {
            TypedPathState::Symlink(SymlinkState::from(symlink_path))
        }
        TypedPath::File(_) => TypedPathState::File(()),
        TypedPath::FilesystemBoundary(_) => TypedPathState::FilesystemBoundary(()),
        TypedPath::Unknown(_) => TypedPathState::Unknown("Unknown file path type".into()),
    }
}
fn to_nothing(path: &TypedPath) -> TypedPathState {
    match path {
        TypedPath::Directory(dir_path) => {
            TypedPathState::Directory(DirectoryDiff::diff_paths(Some(dir_path), None))
        }
        TypedPath::Symlink(symlink_path) => {
            TypedPathState::Symlink(SymlinkState::from(symlink_path))
        }
        TypedPath::File(_) => TypedPathState::File(()),
        TypedPath::FilesystemBoundary(_) => TypedPathState::FilesystemBoundary(()),
        TypedPath::Unknown(_) => TypedPathState::Unknown("Unknown file path type".into()),
    }
}

pub(crate) type TypedPathState = Typed<SymlinkState, DirectoryDiff, (), (), String>;
pub(crate) type TypedPathDiff = Typed<SymlinkDiff, DirectoryDiff, (), (), String>;

#[derive(Clone, Debug)]
pub(crate) enum PathDiff {
    Recreated {
        state: LeftRightBoth<TypedPathState>,
    },
    Modified(TypedPathDiff),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SymlinkState {
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
    pub(crate) fn target(&self) -> &PathBuf {
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
pub(crate) struct SymlinkDiff {
    pub(crate) before: SymlinkState,
    pub(crate) after: SymlinkState,
}

impl SymlinkDiff {
    pub(crate) fn diff_paths(before: &SymlinkPath, after: &SymlinkPath) -> Self {
        SymlinkDiff {
            before: SymlinkState::from(before),
            after: SymlinkState::from(after),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectoryDiff {
    pub(crate) entries: Vec<(PathBuf, LeftRightBoth<PathBuf>)>,
}

impl DirectoryDiff {
    pub(crate) fn diff_paths(
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
}
