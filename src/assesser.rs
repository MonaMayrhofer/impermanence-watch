use std::path::{Path, PathBuf};

pub type AssessedActionInverted = Typed<
    Action<SymlinkState, SymlinkDiff>,
    Action<DirectoryDiff, DirectoryDiff>,
    Action<(), ()>,
    Action<(), ()>,
    String,
>;

impl From<AssessedAction> for AssessedActionInverted {
    fn from(value: AssessedAction) -> Self {
        lift_typed!(
            value =>
            Action::Created
            Action::Deleted
            Action::Modified
            Action::Identical
        )
    }
}

// ===========================

use crate::{
    dir_diff::{
        DiffCache, DirectoryDiff, LeftRightBoth, PathDiff, PathElementDiff, PathElementState,
        SymlinkDiff, SymlinkState,
    },
    lift_typed,
    typed_actions::{Action, Typed},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssessmentGrade {
    Meaningful,
    Meaningless,
}

pub type AssessedAction = Action<PathElementState, PathElementDiff>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assessment {
    pub action: AssessedAction,
    pub grade: AssessmentGrade,
}

pub struct Assesser<'a> {
    pub diff_cache: &'a mut DiffCache,
}

impl<'a> Assesser<'a> {
    pub fn assess(&mut self, location: &Path, diff: PathDiff) -> Vec<Assessment> {
        match diff {
            PathDiff::Recreated { state } => match (state) {
                (LeftRightBoth::Both(
                    PathElementState::Directory(before_dir),
                    diff_after @ PathElementState::FilesystemBoundary(_),
                )) if before_dir.entries.is_empty() => {
                    // An empty directory was replaced by a mountpoint
                    vec![
                        Assessment {
                            action: AssessedAction::Deleted(PathElementState::Directory(
                                before_dir,
                            )),
                            grade: AssessmentGrade::Meaningless,
                        },
                        Assessment {
                            action: AssessedAction::Created(diff_after),
                            grade: AssessmentGrade::Meaningless,
                        },
                    ]
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
                PathElementDiff::Directory(directory_diff) => {
                    vec![assess_directory(self, location, directory_diff)]
                }
                PathElementDiff::File(()) => todo!(),
                PathElementDiff::Symlink(symlink_diff) => vec![assess_symlink(symlink_diff)],
                PathElementDiff::FilesystemBoundary(()) => todo!(),
                PathElementDiff::Unknown(_) => todo!(),
            },
        }
    }
}

pub fn assess_directory(silf: &mut Assesser, location: &Path, diff: DirectoryDiff) -> Assessment {
    let contains_meaningful_changes = diff.entries.iter().any(|(child_path, _)| {
        // We could also return false here, instead of expecting...
        let child_assessments = silf
            .diff_cache
            .get_diff(location.join(child_path))
            .expect("child path diffs should always exist");

        child_assessments
            .iter()
            .any(|child_diff| matches!(child_diff.grade, AssessmentGrade::Meaningful))
    });

    Assessment {
        action: AssessedAction::Modified(PathElementDiff::Directory(diff)),
        grade: if contains_meaningful_changes {
            AssessmentGrade::Meaningful
        } else {
            AssessmentGrade::Meaningless
        },
    }
}

//TODO Honestly the assesser should take care of constructing the diffs in a way?
pub fn assess_symlink(diff: SymlinkDiff) -> Assessment {
    match (&diff.before, &diff.after) {
        (before, after) if before == after => {
            // Both links are literally identical
            Assessment {
                action: AssessedAction::Identical(PathElementState::Symlink(diff.before)),
                // TODO add an assessment summary for each file type. That would require a unfication of all Typed<..> things
                grade: AssessmentGrade::Meaningless,
            }
            // return SymlinkDiff::IdenticalInNix {
            //     target: before_target.clone(),
            // };
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
                action: AssessedAction::Modified(PathElementDiff::Symlink(diff)),
                grade: AssessmentGrade::Meaningless,
            }
            // return SymlinkDiff::NixGenerationChanged {
            //     path_within_derivation: before_path.to_string(),
            //     before: before_hash.to_owned(),
            //     after: after_hash.to_owned(),
            // };
        }
        (_, _) => {
            // We know they are not identical due to the first match
            Assessment {
                action: AssessedAction::Modified(PathElementDiff::Symlink(diff)),
                grade: AssessmentGrade::Meaningful,
            }
            // return SymlinkDiff::DifferentInNix {
            //     before: PathBuf::from(before_nix_path),
            //     after: PathBuf::from(after_nix_path),
            // };
        }
    }
}
