mod nav_list;

use color_eyre::Result;
use std::{
    collections::HashMap,
    fs::OpenOptions,
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
    text::{Span, Text},
};

use crate::{
    dir_diff::{self, DiffLayout, DirDiff, LeftRightBoth, PathDiff, PathElementDiff, path_diff},
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

pub struct DiffCacheLayout;
impl DiffLayout for DiffCacheLayout {
    type DirectoryDiff = Vec<(PathBuf, LeftRightBoth<PathBuf>)>;

    fn make_directory_diff(before: Option<&Path>, after: Option<&Path>) -> Self::DirectoryDiff {
        let diff = dir_diff::dir_diff(before, after);
        let mut items = diff
            .contents
            .iter()
            .map(|(path, diff)| {
                let path = path.clone();
                let diff = diff.clone();
                (path, diff)
            })
            .collect::<Vec<_>>();
        items.sort_by_cached_key(|it| it.0.clone());
        items
    }
}

pub struct DiffCache {
    pub diffs: HashMap<PathBuf, PathDiff<DiffCacheLayout>>,
    pub before: PathBuf,
    pub after: PathBuf,
}

impl DiffCache {
    fn get_diff(&mut self, location: PathBuf) -> &PathDiff<DiffCacheLayout> {
        self.diffs.entry(location.clone()).or_insert_with(|| {
            let before = self.before.join(&location);
            let after = self.after.join(&location);

            path_diff(&before, &after)
        })
    }
}

enum EntryState {
    Created,
    Deleted,
    Modified,
    Unchanged,
    Unimportant,
}

fn display_diff(
    path: &Path,
    diff: &PathElementDiff<DiffCacheLayout>,
    state: EntryState,
) -> Text<'static> {
    let text = match diff {
        PathElementDiff::Directory(_) => Text::from(format!(" {}/", path.display())),
        PathElementDiff::File => Text::from(format!(" {}", path.display())),
        PathElementDiff::Symlink => Text::from(format!(" {}", path.display())),
        PathElementDiff::Nonexistent => Text::from(format!(" {}", path.display())),
        PathElementDiff::Unknown(_) => Text::from(format!("? {}", path.display())),
        PathElementDiff::FilesystemBoundary => Text::from(format!(" {}/", path.display())),
    };
    match state {
        EntryState::Created => text.green(),
        EntryState::Deleted => text.red(),
        EntryState::Modified => text.yellow(),
        EntryState::Unchanged => text.gray(),
        EntryState::Unimportant => text.dark_gray(),
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
                .clone()
                .into_iter()
                .flat_map(|(rel_path, _)| {
                    let child_diff = self.get_diff(location.join(&rel_path));
                    let sub_location = Some(rel_path.clone());

                    match child_diff {
                        PathDiff::Recreated { before, after } => match (before, after) {
                            (
                                diff_before @ PathElementDiff::Directory(before_dir),
                                diff_after @ PathElementDiff::FilesystemBoundary,
                            ) => {
                                if before_dir.is_empty() {
                                    vec![NavListItem {
                                        text: display_diff(
                                            &rel_path,
                                            diff_after,
                                            EntryState::Unimportant,
                                        ),
                                        sub_location: sub_location.clone(),
                                    }]
                                } else {
                                    vec![
                                        NavListItem {
                                            text: display_diff(
                                                &rel_path,
                                                diff_before,
                                                EntryState::Deleted,
                                            ),
                                            sub_location: sub_location.clone(),
                                        },
                                        NavListItem {
                                            text: display_diff(
                                                &rel_path,
                                                diff_after,
                                                EntryState::Created,
                                            ),
                                            sub_location: sub_location.clone(),
                                        },
                                    ]
                                }
                            }
                            (before, after) => {
                                let mut vec = Vec::new();
                                match before {
                                    PathElementDiff::Nonexistent => {}
                                    diff => vec.push(NavListItem {
                                        text: display_diff(&rel_path, diff, EntryState::Deleted),
                                        sub_location: sub_location.clone(),
                                    }),
                                }

                                match after {
                                    PathElementDiff::Nonexistent => {}
                                    diff => vec.push(NavListItem {
                                        text: display_diff(&rel_path, diff, EntryState::Created),
                                        sub_location: sub_location.clone(),
                                    }),
                                }
                                vec
                            }
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
            let iter = diff.iter();
            let mut iter = iter.skip_while(|(it, _)| prev != it);

            if let Some((current, _)) = iter.next() {
                if let Some((path, _)) = iter.next() {
                    Some(path.clone())
                } else {
                    Some(current.clone())
                }
            } else {
                diff.first().map(|(path, _)| path.clone())
            }
        } else {
            diff.first().map(|(path, _)| path.clone())
        }
    }

    fn get_previous<'a>(
        &'a mut self,
        location: &Self::Location,
        next_sub_location: Option<&'a Self::Location>,
    ) -> Option<Self::Location> {
        let diff = self.get_diff(location.clone()).as_directory_diff()?;

        if let Some(prev) = &next_sub_location {
            let iter = diff.iter().rev();
            let mut iter = iter.skip_while(|(it, _)| prev != &it);

            if let Some((current, _)) = iter.next() {
                if let Some((path, _)) = iter.next() {
                    Some(path.clone())
                } else {
                    Some(current.clone())
                }
            } else {
                diff.last().map(|(path, _)| path.clone())
            }
        } else {
            diff.last().map(|(path, _)| path.clone())
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
            Constraint::Percentage(25),
            Constraint::Percentage(50),
            Constraint::Percentage(25),
        ])
        .areas(frame.area());

        let navlist = NavList::new(&mut self.diff_cache, &self.path);
        let state = self.nav_list_states.entry(self.path.clone()).or_default();
        let selected = state.selected.clone();
        frame.render_stateful_widget(navlist, middle, state);

        if let Some(selected) = &selected {
            let selected_full_path = self.path.join(selected);
            let state = self
                .nav_list_states
                .entry(selected_full_path.clone())
                .or_default();
            let navlist = NavList::new(&mut self.diff_cache, &selected_full_path);
            frame.render_stateful_widget(navlist, right, state);
        }

        if let Some(parent) = self.path.parent() {
            let parent = parent.to_path_buf();
            let state = self.nav_list_states.entry(parent.clone()).or_default();
            let navlist = NavList::new(&mut self.diff_cache, &parent);
            frame.render_stateful_widget(navlist, left, state);
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
