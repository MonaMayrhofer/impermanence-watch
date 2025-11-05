mod nav_list;

use color_eyre::Result;
use std::{
    collections::HashMap,
    fs::OpenOptions,
    marker::PhantomData,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tracing::level_filters::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_subscriber::{
    Layer as _, filter::Directive, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame, Terminal,
    layout::{Constraint, Layout, Rect},
    prelude::Backend,
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph},
};

use crate::{
    dir_diff::{
        self, DiffCache, DiffCacheLayout, DiffLayout, FileType, LeftRightBoth, PathDiff,
        PathElementDiff, PathElementState, SymlinkDiff,
    },
    tui::nav_list::{NavList, NavListAdapter, NavListItem, NavListState},
};

pub fn tui() -> Result<()> {
    color_eyre::install()?;

    let mut open = OpenOptions::new();
    open.truncate(true);
    open.write(true);
    open.create(true);
    let log_file = open.open("LOG.log")?;
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(
            tracing_subscriber::filter::EnvFilter::builder()
                .with_default_directive(LevelFilter::DEBUG.into())
                .from_env_lossy(),
        );
    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(ErrorLayer::default())
        .init();

    tracing::info!("Starting Tui");
    let mut terminal = ratatui::init();

    tracing::info!("Starting App");
    let app = App::new();
    let res = run_app(&mut terminal, app);

    tracing::info!("Shutting down");
    ratatui::restore();
    res
}

enum EntryState {
    Created,
    Deleted,
    Modified,
    Unchanged,
    Unimportant,
}

fn display_diff(path: &Path, diff: &PathElementDiff, state: EntryState) -> Text<'static> {
    let entry_style = match state {
        EntryState::Created => Style::new().green(),
        EntryState::Deleted => Style::new().red(),
        EntryState::Modified => Style::new().yellow(),
        EntryState::Unchanged => Style::new().gray(),
        EntryState::Unimportant => Style::new().dark_gray(),
    };

    match diff {
        PathElementDiff::Directory(_) => {
            Text::from(format!(" {}/", path.display())).style(entry_style)
        }
        PathElementDiff::File => Text::from(format!(" {}", path.display())).style(entry_style),
        PathElementDiff::Symlink(diff) => match diff {
            dir_diff::PathElementStateAction::Create(diff) => {
                Text::from(format!(" {}", diff.target().display()))
                    .style(entry_style)
                    .green()
            }
            dir_diff::PathElementStateAction::Delete(diff) => {
                Text::from(format!(" {}", diff.target().display()))
                    .style(entry_style)
                    .red()
            }
            dir_diff::PathElementStateAction::Modify(diff) => match diff {
                SymlinkDiff::Different { before, after } => Text::from(Line::from(vec![
                    Span::from(format!(" {}", path.display())),
                    Span::from(" -> ???"),
                ]))
                .yellow(),

                SymlinkDiff::IdenticalInNix { target } | SymlinkDiff::Identical { target } => {
                    Text::from(Line::from(vec![
                        Span::from(format!(" {}", path.display())),
                        Span::from(format!(" -> {}", target.display())),
                    ]))
                    .dark_gray()
                }
                SymlinkDiff::DifferentInNix { before, after } => Text::from(Line::from(vec![
                    Span::from(format!(" {}", path.display())),
                    Span::from(format!(" -> Nix: ???")),
                ]))
                .yellow(),
                SymlinkDiff::NixGenerationChanged {
                    before,
                    after,
                    path_within_derivation,
                } => Text::from(Line::from(vec![
                    Span::from(format!(" {}", path.display())),
                    Span::from(" -> /nix/store/"),
                    Span::from("???").yellow(),
                    Span::from("-"),
                    Span::from(path_within_derivation.to_string()),
                ]))
                .dark_gray(),
            },
        },
        PathElementDiff::Nonexistent => {
            Text::from(format!(" {}", path.display())).style(entry_style)
        }
        PathElementDiff::Unknown(_) => {
            Text::from(format!("? {}", path.display())).style(entry_style)
        }
        PathElementDiff::FilesystemBoundary => {
            Text::from(format!(" {}/", path.display())).dark_gray()
        }
    }
}

impl NavListAdapter for DiffCache {
    type Location = PathBuf;

    fn get_items(
        &mut self,
        location: &Self::Location,
    ) -> Option<Vec<NavListItem<'_, Self::Location>>> {
        let diff = self.get_diff(location.clone());

        if let Some(dir) = diff.as_directory_diff() {
            let items = dir
                .entries
                .clone()
                .into_iter()
                .flat_map(|(rel_path, _)| {
                    let child_diff = self.get_diff(location.join(&rel_path));

                    let sub_location = Some(rel_path.clone());

                    match child_diff {
                        PathDiff::Recreated { state } => match (state) {
                            (LeftRightBoth::Both(
                                diff_before @ PathElementState::Directory(before_dir),
                                diff_after @ PathElementState::FilesystemBoundary,
                            )) => {
                                if before_dir.entries.is_empty() {
                                    vec![NavListItem {
                                        text: display_diff(
                                            &rel_path,
                                            &PathElementDiff::create(diff_after.clone()),
                                            EntryState::Unimportant,
                                        ),
                                        sub_location: sub_location.clone(),
                                    }]
                                } else {
                                    vec![
                                        NavListItem {
                                            text: display_diff(
                                                &rel_path,
                                                &PathElementDiff::delete(diff_before.clone()),
                                                EntryState::Deleted,
                                            ),
                                            sub_location: sub_location.clone(),
                                        },
                                        NavListItem {
                                            text: display_diff(
                                                &rel_path,
                                                &PathElementDiff::create(diff_after.clone()),
                                                EntryState::Created,
                                            ),
                                            sub_location: sub_location.clone(),
                                        },
                                    ]
                                }
                            }
                            LeftRightBoth::Both(before, after) => {
                                vec![
                                    (NavListItem {
                                        text: display_diff(
                                            &rel_path,
                                            &PathElementDiff::delete(before.clone()),
                                            EntryState::Deleted,
                                        ),
                                        sub_location: sub_location.clone(),
                                    }),
                                    (NavListItem {
                                        text: display_diff(
                                            &rel_path,
                                            &PathElementDiff::create(after.clone()),
                                            EntryState::Created,
                                        ),
                                        sub_location: sub_location.clone(),
                                    }),
                                ]
                            }
                            LeftRightBoth::Left(before) => vec![
                                (NavListItem {
                                    text: display_diff(
                                        &rel_path,
                                        &PathElementDiff::delete(before.clone()),
                                        EntryState::Deleted,
                                    ),
                                    sub_location: sub_location.clone(),
                                }),
                            ],
                            LeftRightBoth::Right(after) => vec![
                                (NavListItem {
                                    text: display_diff(
                                        &rel_path,
                                        &PathElementDiff::create(after.clone()),
                                        EntryState::Created,
                                    ),
                                    sub_location: sub_location.clone(),
                                }),
                            ],
                        },
                        PathDiff::Modified(path_element_diff) => {
                            vec![NavListItem {
                                text: display_diff(
                                    &rel_path,
                                    path_element_diff,
                                    EntryState::Modified,
                                ),
                                sub_location,
                            }]
                        }
                    }
                })
                .collect();
            Some(items)
        } else {
            None
        }
    }

    fn get_next(
        &mut self,
        location: &Self::Location,
        previous_sub_location: Option<&Self::Location>,
    ) -> Option<Self::Location> {
        let diff = self.get_diff(location.clone()).as_directory_diff()?;

        if let Some(prev) = previous_sub_location {
            let iter = diff.entries.iter();
            let mut iter = iter.skip_while(|(it, _)| prev != it);

            if let Some((current, _)) = iter.next() {
                if let Some((path, _)) = iter.next() {
                    Some(path.clone())
                } else {
                    Some(current.clone())
                }
            } else {
                diff.entries.first().map(|(path, _)| path.clone())
            }
        } else {
            diff.entries.first().map(|(path, _)| path.clone())
        }
    }

    fn get_previous<'a>(
        &'a mut self,
        location: &Self::Location,
        next_sub_location: Option<&'a Self::Location>,
    ) -> Option<Self::Location> {
        let diff = self.get_diff(location.clone()).as_directory_diff()?;

        if let Some(prev) = &next_sub_location {
            let iter = diff.entries.iter().rev();
            let mut iter = iter.skip_while(|(it, _)| prev != &it);

            if let Some((current, _)) = iter.next() {
                if let Some((path, _)) = iter.next() {
                    Some(path.clone())
                } else {
                    Some(current.clone())
                }
            } else {
                diff.entries.last().map(|(path, _)| path.clone())
            }
        } else {
            diff.entries.last().map(|(path, _)| path.clone())
        }
    }
}

struct App {
    path: PathBuf,

    nav_list_states: HashMap<PathBuf, NavListState<PathBuf>>,
    diff_cache: DiffCache,
}

impl App {
    fn new() -> Self {
        // let differ = Differ::new(
        //     Path::new("/impermanence/current_root_on_boot_snapshot/home/nionidh/").to_owned(),
        //     Path::new("/home/nionidh").to_owned(),
        // );
        let initial_path = PathBuf::new();
        let diff_cache = DiffCache {
            diffs: HashMap::new(),
            before: Path::new("/impermanence/current_root_on_boot_snapshot/home/nionidh/")
                .to_owned(),
            after: Path::new("/home/nionidh").to_owned(),
        };

        Self {
            path: initial_path.clone(),
            nav_list_states: HashMap::new(),
            diff_cache,
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        let [left, middle, right] = Layout::horizontal(vec![
            Constraint::Percentage(15),
            Constraint::Percentage(50),
            Constraint::Percentage(35),
        ])
        .areas(frame.area());

        self.display_path(self.path.to_path_buf(), frame, middle);

        let state = self.nav_list_states.entry(self.path.clone()).or_default();
        let selected = state.selected.clone();

        if let Some(selected) = &selected {
            let selected_full_path = self.path.join(selected);
            self.display_path(selected_full_path, frame, right);
        }

        if let Some(parent) = self.path.parent() {
            self.display_path(parent.to_path_buf(), frame, left);
        }
    }

    fn display_path(&mut self, path: PathBuf, frame: &mut Frame, area: Rect) {
        let diff: PathDiff = self.diff_cache.get_diff(path.clone()).clone();
        match diff {
            //TODO This clone shows a fundamental flaw with the current design. Get rid of it
            PathDiff::Recreated {
                state: LeftRightBoth::Both(before, after),
            } => {
                let [top, bottom] =
                    Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .areas(area);

                self.display_element_diff(
                    path.clone(),
                    PathElementDiff::delete(before.clone()),
                    frame,
                    top,
                );
                self.display_element_diff(
                    path.clone(),
                    PathElementDiff::create(after.clone()),
                    frame,
                    bottom,
                );
            }

            PathDiff::Recreated {
                state: LeftRightBoth::Left(before),
            } => {
                let [top, bottom] =
                    Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .areas(area);

                self.display_element_diff(
                    path.clone(),
                    PathElementDiff::delete(before.clone()),
                    frame,
                    top,
                );
            }

            PathDiff::Recreated {
                state: LeftRightBoth::Right(after),
            } => {
                let [top, bottom] =
                    Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .areas(area);

                self.display_element_diff(
                    path.clone(),
                    PathElementDiff::create(after.clone()),
                    frame,
                    bottom,
                );
            }

            PathDiff::Modified(element) => {
                self.display_element_diff(path, element.clone(), frame, area)
            }
        }
    }

    fn display_element_diff(
        &mut self,
        path: PathBuf,
        element: PathElementDiff,
        frame: &mut Frame,
        area: Rect,
    ) {
        match element {
            PathElementDiff::Directory(_) => {
                let state = self.nav_list_states.entry(path.clone()).or_default();
                let navlist = NavList::new(&mut self.diff_cache, &path);
                frame.render_stateful_widget(navlist, area, state);
            }
            PathElementDiff::Symlink(link_diff) => match link_diff {
                dir_diff::PathElementStateAction::Create(_) => todo!(),
                dir_diff::PathElementStateAction::Delete(_) => todo!(),
                dir_diff::PathElementStateAction::Modify(link_diff) => {
                    match link_diff {
                        SymlinkDiff::Different { before, after } => {
                            let text = Text::from(vec![
                                Line::from(before.display().to_string()),
                                Line::from(after.display().to_string()),
                            ]);
                            let paragraph = Paragraph::new(text)
                                .block(Block::bordered().title("Symlink Changed"));
                            frame.render_widget(paragraph, area);
                        }
                        SymlinkDiff::Identical { target } => {
                            let text = Text::from(vec![Line::from(target.display().to_string())]);
                            let paragraph = Paragraph::new(text)
                                .block(Block::bordered().title("Symlink Identical"));
                            frame.render_widget(paragraph, area);
                        }
                        SymlinkDiff::IdenticalInNix { target } => {
                            let text = Text::from(vec![Line::from(target.display().to_string())]);
                            let paragraph = Paragraph::new(text)
                                .block(Block::bordered().title("Symlink Identical"));
                            frame.render_widget(paragraph, area);
                        }
                        SymlinkDiff::DifferentInNix { before, after } => {
                            let text = Text::from(vec![
                                Line::from(before.display().to_string()),
                                Line::from(after.display().to_string()),
                            ]);
                            let paragraph = Paragraph::new(text)
                                .block(Block::bordered().title("Symlink Changed"));
                            frame.render_widget(paragraph, area);
                        }
                        SymlinkDiff::NixGenerationChanged {
                            path_within_derivation,
                            before,
                            after,
                        } => {
                            let text = Text::from(vec![
                                Line::from(vec![
                                    Span::from("/nix/store/").dark_gray(),
                                    Span::from(before).red(),
                                    Span::from("-").dark_gray(),
                                    Span::from(path_within_derivation.clone()).dark_gray(),
                                ]),
                                Line::from(vec![
                                    Span::from("/nix/store/").dark_gray(),
                                    Span::from(after).green(),
                                    Span::from("-").dark_gray(),
                                    Span::from(path_within_derivation).dark_gray(),
                                ]),
                            ]);
                            let paragraph = Paragraph::new(text)
                                .block(Block::bordered().title("Nix-Symlink Generation Changed"));
                            frame.render_widget(paragraph, area);
                        }
                    };
                }
            },
            PathElementDiff::File => {
                let text = Text::from(vec![Line::from("File")]);
                let paragraph = Paragraph::new(text).block(Block::bordered().title("File"));
                frame.render_widget(paragraph, area);
            }
            PathElementDiff::Nonexistent => {
                let text = Text::from(vec![Line::from("Nonexistent")]);
                let paragraph = Paragraph::new(text).block(Block::bordered().title("Nonexistent"));
                frame.render_widget(paragraph, area);
            }
            PathElementDiff::FilesystemBoundary => {
                let text = Text::from(vec![Line::from("FS Boundary")]);
                let paragraph = Paragraph::new(text).block(Block::bordered().title("FS Boundary"));
                frame.render_widget(paragraph, area);
            }
            PathElementDiff::Unknown(_) => {
                let text = Text::from(vec![Line::from("Unknown")]);
                let paragraph = Paragraph::new(text).block(Block::bordered().title("Unknown"));
                frame.render_widget(paragraph, area);
            }
        }
    }
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<()> {
    const DEBOUNCE: Duration = Duration::from_millis(20); // 50 FPS

    let before = Instant::now();
    terminal.draw(|frame| app.draw(frame))?;
    let mut last_render_took = before.elapsed();

    let mut debounce: Option<Instant> = None;

    loop {
        let timeout = debounce.map_or(DEBOUNCE, |start| DEBOUNCE.saturating_sub(start.elapsed()));
        if crossterm::event::poll(timeout)? {
            let update = match crossterm::event::read()? {
                Event::Key(key) if !matches!(key.kind, KeyEventKind::Press) => false,
                Event::Key(key) => {
                    let state = app.nav_list_states.entry(app.path.clone()).or_default();

                    match key.code {
                        KeyCode::Down => {
                            state.selected =
                                app.diff_cache.get_next(&app.path, state.selected.as_ref());

                            tracing::info!("Selected to {:?} -> {:?}", app.path, state.selected);
                            true
                        }
                        KeyCode::Up => {
                            state.selected = app
                                .diff_cache
                                .get_previous(&app.path, state.selected.as_ref());

                            tracing::info!("Selected to {:?} -> {:?}", app.path, state.selected);
                            true
                        }
                        KeyCode::Right => {
                            if let Some(selected) = &state.selected {
                                app.path = app.path.join(selected);
                                tracing::info!("Navigated to {:?}", app.path);
                                true
                            } else {
                                false
                            }
                        }
                        KeyCode::Left => {
                            if let Some(parent) = app.path.parent() {
                                app.path = parent.to_path_buf();
                                tracing::info!("Navigated to {:?}", app.path);
                                true
                            } else {
                                false
                            }
                        }
                        KeyCode::Char('q') => return Ok(()),
                        _ => false,
                    }
                }
                // Event::Mouse(mouse) => match mouse.kind {
                //     MouseEventKind::ScrollDown => app.state.scroll_down(1),
                //     MouseEventKind::ScrollUp => app.state.scroll_up(1),
                //     MouseEventKind::Down(_button) => {
                //         app.state.click_at(Position::new(mouse.column, mouse.row))
                //     }
                //     _ => false,
                // },
                Event::Resize(_, _) => true,
                _ => false,
            };
            if update {
                debounce.get_or_insert_with(Instant::now);
            }
        }
        if debounce.is_some_and(|debounce| debounce.elapsed() > DEBOUNCE) {
            let before = Instant::now();
            terminal.draw(|frame| {
                app.draw(frame);

                // Performance info in top right corner
                {
                    let text = format!(
                        " {} {last_render_took:?} {:.1} FPS",
                        frame.count(),
                        1.0 / last_render_took.as_secs_f64()
                    );
                    #[allow(clippy::cast_possible_truncation)]
                    let area = Rect {
                        y: 0,
                        height: 1,
                        x: frame.area().width.saturating_sub(text.len() as u16),
                        width: text.len() as u16,
                    };
                    frame.render_widget(
                        Span::styled(text, Style::new().fg(Color::Black).bg(Color::Gray)),
                        area,
                    );
                }
            })?;
            last_render_took = before.elapsed();

            debounce = None;
        }
    }
}
