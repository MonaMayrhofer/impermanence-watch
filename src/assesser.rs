use std::path::Path;

use crate::{
    diff_util::LeftRightBoth,
    dir_diff::{
        Differ, DirectoryDiff, PathDiff, SymlinkDiff, SymlinkState, TypedPathDiff, TypedPathState,
    },
    typed_actions::Action,
};

pub(crate) trait AssessmentCache {
    fn get_assessment(&mut self, location: &Path) -> Option<&Vec<Assessment>>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AssessmentGrade {
    Meaningful,
    Meaningless,
}

pub(crate) type AssessedAction = Action<TypedPathState, TypedPathDiff>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Assessment {
    pub(crate) action: AssessedAction,
    pub(crate) grade: AssessmentGrade,
}

pub(crate) struct Assesser<TCache> {
    pub(crate) diff_cache: TCache,
}

impl<TCache: AssessmentCache> Assesser<TCache> {
    pub(crate) fn assess(&mut self, location: &Path, diff: PathDiff) -> Vec<Assessment> {
        match diff {
            PathDiff::Recreated { state } => match state {
                LeftRightBoth::Both(
                    TypedPathState::Directory(before_dir),
                    diff_after @ TypedPathState::FilesystemBoundary(_),
                ) if before_dir.entries.is_empty() => {
                    // An empty directory was turned into a mountpoint => The empty directory didn't actually get deleted
                    vec![Assessment {
                        action: AssessedAction::Created(diff_after),
                        grade: AssessmentGrade::Meaningless,
                    }]
                }
                LeftRightBoth::Both(diff_before, diff_after) => {
                    // Something was recreated
                    vec![
                        Assessment {
                            action: AssessedAction::Deleted(diff_before),
                            grade: AssessmentGrade::Meaningful,
                        },
                        Assessment {
                            action: AssessedAction::Created(diff_after),
                            grade: AssessmentGrade::Meaningful,
                        },
                    ]
                }

                // Watch out if empty directories are created or deleted
                LeftRightBoth::Left(TypedPathState::Directory(directory)) => {
                    vec![Assessment {
                        grade: assess_directory(self, location, &directory),
                        action: AssessedAction::Deleted(TypedPathState::Directory(directory)),
                    }]
                }
                // Watch out if empty directories are created or deleted
                LeftRightBoth::Right(TypedPathState::Directory(directory)) => {
                    vec![Assessment {
                        grade: assess_directory(self, location, &directory),
                        action: AssessedAction::Created(TypedPathState::Directory(directory)),
                    }]
                }

                LeftRightBoth::Left(diff_before) =>
                // Something was deleted
                {
                    vec![Assessment {
                        action: AssessedAction::Deleted(diff_before),
                        grade: AssessmentGrade::Meaningful,
                    }]
                }
                LeftRightBoth::Right(diff_after) =>
                // Something was created
                {
                    vec![Assessment {
                        action: AssessedAction::Created(diff_after),
                        grade: AssessmentGrade::Meaningful,
                    }]
                }
            },
            PathDiff::Modified(path_element_diff) => match path_element_diff {
                TypedPathDiff::Directory(directory_diff) => {
                    vec![Assessment {
                        grade: assess_directory(self, location, &directory_diff),
                        action: AssessedAction::Modified(TypedPathDiff::Directory(directory_diff)),
                    }]
                }
                TypedPathDiff::Symlink(symlink_diff) => vec![assess_symlink(symlink_diff)],
                TypedPathDiff::FilesystemBoundary(()) => vec![],
                TypedPathDiff::File(()) => vec![],
                TypedPathDiff::Unknown(_) => todo!(),
            },
        }
    }
}

pub(crate) fn assess_directory<TCache: AssessmentCache>(
    silf: &mut Assesser<TCache>,
    location: &Path,
    diff: &DirectoryDiff,
) -> AssessmentGrade {
    let contains_meaningful_changes = diff.entries.iter().any(|(child_path, _)| {
        // We could also return false here, instead of expecting...
        let child_assessments = silf
            .diff_cache
            .get_assessment(&location.join(child_path))
            .expect("child path diffs should always exist");

        child_assessments
            .iter()
            .any(|child_diff| matches!(child_diff.grade, AssessmentGrade::Meaningful))
    });

    if contains_meaningful_changes {
        AssessmentGrade::Meaningful
    } else {
        AssessmentGrade::Meaningless
    }
}

//TODO Honestly the assesser should take care of constructing the diffs in a way?
pub(crate) fn assess_symlink(diff: SymlinkDiff) -> Assessment {
    match (&diff.before, &diff.after) {
        (before, after) if before == after => {
            // Both links are literally identical
            Assessment {
                action: AssessedAction::Identical(TypedPathState::Symlink(diff.before)),
                // TODO add an assessment summary for each file type. That would require a unfication of all Typed<..> things
                grade: AssessmentGrade::Meaningless,
            }
        }
        (
            SymlinkState::NixStoreLink {
                path_within_derivation: b_p,
                ..
            },
            SymlinkState::NixStoreLink {
                path_within_derivation: a_p,
                ..
            },
        ) if b_p == a_p => {
            // The path is identical, and we know something differs (because of the first match), so its the generation
            Assessment {
                action: AssessedAction::Modified(TypedPathDiff::Symlink(diff)),
                grade: AssessmentGrade::Meaningless,
            }
        }
        (_, _) => {
            // We know they are not identical due to the first match
            Assessment {
                action: AssessedAction::Modified(TypedPathDiff::Symlink(diff)),
                grade: AssessmentGrade::Meaningful,
            }
        }
    }
}
