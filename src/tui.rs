mod nav_list;

use color_eyre::Result;
use std::{
    collections::HashMap,
    fs::OpenOptions,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tracing::level_filters::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_subscriber::{Layer as _, layer::SubscriberExt as _, util::SubscriberInitExt as _};

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
    assesser::{AssessedAction, Assessment, AssessmentGrade},
    dir_diff::{DiffCache, DirectoryDiff, FileType, PathElementDiff, PathElementState},
    tui::nav_list::{NavList, NavListAdapter, NavListItem, NavListState},
    typed_actions::{Action, Typed},
};

pub(crate) fn tui() -> Result<()> {
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

fn display_diff(path: &Path, diff: &Assessment) -> Text<'static> {
    // let entry_style = match state {
    //     EntryState::Created => Style::new().green(),
    //     EntryState::Deleted => Style::new().red(),
    //     EntryState::Modified => Style::new().yellow(),
    //     EntryState::Unchanged => Style::new().gray(),
    //     EntryState::Unimportant => Style::new().dark_gray(),
    // };
    let created_style = Style::new().green();
    let deleted_style = Style::new().red();
    let modified_style = Style::new().yellow();
    let unimportant_style = Style::new().dark_gray();

    let file_type = match &diff.action {
        AssessedAction::Created(path_element_state) => path_element_state.file_type(),
        AssessedAction::Deleted(path_element_state) => path_element_state.file_type(),
        AssessedAction::Modified(path_element_diff) => path_element_diff.file_type(),
        AssessedAction::Identical(path_element_state) => path_element_state.file_type(),
    };

    let style = match diff.grade {
        AssessmentGrade::Meaningful => match &diff.action {
            AssessedAction::Created(_) => created_style,
            AssessedAction::Deleted(_) => deleted_style,
            AssessedAction::Modified(_) => modified_style,
            AssessedAction::Identical(_) => unimportant_style,
        },
        AssessmentGrade::Meaningless => unimportant_style,
    };

    match file_type {
        FileType::Directory => Text::from(format!(" {}/", path.display())).style(style),
        FileType::File => Text::from(format!(" {}", path.display())).style(style),
        FileType::Symlink => Text::from(format!(" {}", path.display())).style(style),
        FileType::Unknown => Text::from(format!("? {}", path.display())).style(style),
        FileType::FilesystemBoundary => Text::from(format!(" {}/", path.display())).style(style),
    }
}

trait AsDirDiff {
    fn as_directory_diff(&self) -> Option<&DirectoryDiff>;
}
impl AsDirDiff for &Vec<Assessment> {
    fn as_directory_diff(&self) -> Option<&DirectoryDiff> {
        let mut directory_diffs = self.iter().filter_map(|it| it.action.as_directory_diff());
        let directory_diff = directory_diffs.next();
        assert!(
            directory_diffs.next().is_none(),
            "there should be no location that contains more than one action on a directory."
        );
        directory_diff
    }
}
impl AsDirDiff for AssessedAction {
    fn as_directory_diff(&self) -> Option<&DirectoryDiff> {
        match self {
            AssessedAction::Created(path_element_state) => match path_element_state {
                PathElementState::Directory(directory_diff) => Some(directory_diff),
                _ => None,
            },
            AssessedAction::Deleted(path_element_state) => match path_element_state {
                PathElementState::Directory(directory_diff) => Some(directory_diff),
                _ => None,
            },

            AssessedAction::Modified(path_element_diff) => match path_element_diff {
                PathElementDiff::Directory(directory_diff) => Some(directory_diff),
                _ => None,
            },
            AssessedAction::Identical(path_element_state) => match path_element_state {
                PathElementState::Directory(directory_diff) => Some(directory_diff),
                _ => None,
            },
        }
    }
}

impl NavListAdapter for DiffCache {
    type Location = PathBuf;

    fn get_items(
        &mut self,
        location: &Self::Location,
    ) -> Option<Vec<NavListItem<'_, Self::Location>>> {
        let diff = self.get_diff(location.clone())?;
        let directory_diff = diff.as_directory_diff();

        if let Some(dir) = directory_diff {
            let items = dir
                .entries
                .clone()
                .into_iter()
                .flat_map(|(rel_path, _)| {
                    // We could also just return vec![] here, but lets have it this way to catch bugs
                    let child_diff = self
                        .get_diff(location.join(&rel_path))
                        .expect("Child diffs of a dir should always exist");

                    child_diff
                        .iter()
                        .map(|diff| {
                            NavListItem {
                                //TODO This does not contain meaningful changes
                                text: display_diff(rel_path.as_path(), diff),
                                sub_location: Some(rel_path.clone()),
                            }
                        })
                        .collect::<Vec<_>>()
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
        let diff = self.get_diff(location.clone())?;
        let diff = diff.as_directory_diff()?;

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
        let diff = self.get_diff(location.clone())?;
        let diff = diff.as_directory_diff()?;

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
        let Some(diff) = self.diff_cache.get_diff(path.clone()) else {
            return;
        };

        let areas = Layout::vertical(diff.iter().map(|_| Constraint::Percentage(99))).split(area);

        //TODO These clones annoy me
        let diff: Vec<Assessment> = diff.clone();
        for (area, diff) in areas.iter().zip(diff.clone().iter()) {
            self.display_element_diff(path.clone(), diff, frame, *area);
        }
    }

    fn display_element_diff(
        &mut self,
        path: PathBuf,
        element: &Assessment,
        frame: &mut Frame,
        area: Rect,
    ) {
        //let action: AssessedActionInverted = element.action.to_tagged();

        enum ActionTag {
            Created,
            Deleted,
            Identical,
        }

        match &element.action {
            Action::Created(it) => render_state(self, path, it, ActionTag::Created, frame, area),
            Action::Deleted(it) => render_state(self, path, it, ActionTag::Deleted, frame, area),
            Action::Identical(it) => {
                render_state(self, path, it, ActionTag::Identical, frame, area)
            }
            Action::Modified(it) => render_diff(self, path, it, frame, area),
        }

        fn render_state(
            app: &mut App,
            path: PathBuf,
            state: &PathElementState,
            tag: ActionTag,
            frame: &mut Frame,
            area: Rect,
        ) {
            let style = match tag {
                ActionTag::Created => Style::default().green(),
                ActionTag::Deleted => Style::default().red(),
                ActionTag::Identical => Style::default().white(),
            };
            let block = Block::bordered().border_style(style);

            match state {
                Typed::Symlink(link) => {
                    let text = Text::from(vec![Line::from(link.target().display().to_string())]);
                    let paragraph = Paragraph::new(text).block(block.title("Symlink"));
                    frame.render_widget(paragraph, area);
                }
                Typed::Directory(_) => {
                    let state = app.nav_list_states.entry(path.clone()).or_default();
                    let navlist = NavList::new(
                        &mut app.diff_cache,
                        &path,
                        block.title(path.display().to_string()),
                    );
                    frame.render_stateful_widget(navlist, area, state);
                }
                Typed::File(_) => {
                    let text = Text::from(vec![]);
                    let paragraph = Paragraph::new(text).block(block.title("File"));
                    frame.render_widget(paragraph, area);
                }
                Typed::FilesystemBoundary(_) => {
                    let text = Text::from(vec![]);
                    let paragraph = Paragraph::new(text).block(block.title("FS Boundary"));
                    frame.render_widget(paragraph, area);
                }
                Typed::Unknown(text) => {
                    let text = Text::from(vec![Line::from(text.clone())]);
                    let paragraph = Paragraph::new(text).block(block.title("Unknown"));
                    frame.render_widget(paragraph, area);
                }
            };
        }

        fn render_diff(
            app: &mut App,
            path: PathBuf,
            diff: &PathElementDiff,
            frame: &mut Frame,
            area: Rect,
        ) {
            let block = Block::bordered().border_style(Style::default().yellow());
            match diff {
                Typed::Symlink(diff) => {
                    let text = Text::from(vec![
                        Line::from(diff.before.target().display().to_string()).red(),
                        Line::from(diff.after.target().display().to_string()).green(),
                    ]);
                    let paragraph = Paragraph::new(text).block(block.title("Symlink changed"));
                    frame.render_widget(paragraph, area);
                }
                Typed::Directory(_) => {
                    let state = app.nav_list_states.entry(path.clone()).or_default();
                    let navlist = NavList::new(
                        &mut app.diff_cache,
                        &path,
                        block.title(path.display().to_string()),
                    );
                    frame.render_stateful_widget(navlist, area, state);
                }
                Typed::File(_) => {
                    let text = Text::from(vec![
                        //Line::from(diff.before.target().display().to_string()).red(),
                        //Line::from(diff.after.target().display().to_string()).green(),
                    ]);
                    let paragraph = Paragraph::new(text).block(block.title("File changed"));
                    frame.render_widget(paragraph, area);
                }
                Typed::FilesystemBoundary(_) => {
                    let text = Text::from(vec![
                        //Line::from(diff.before.target().display().to_string()).red(),
                        //Line::from(diff.after.target().display().to_string()).green(),
                    ]);
                    let paragraph = Paragraph::new(text).block(block.title("FS Boundary changed"));
                    frame.render_widget(paragraph, area);
                }
                Typed::Unknown(text) => {
                    let text = Text::from(vec![Line::from(text.clone())]);
                    let paragraph = Paragraph::new(text).block(block.title("Unknown"));
                    frame.render_widget(paragraph, area);
                }
            };
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
