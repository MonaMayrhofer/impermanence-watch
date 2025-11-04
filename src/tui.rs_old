use std::{
    collections::{HashMap, VecDeque},
    ffi::OsStr,
    os::linux::raw::stat,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

use color_eyre::{Result, owo_colors::OwoColorize};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use ratatui::{
    DefaultTerminal, Frame, Terminal,
    layout::{Constraint, Layout, Position, Rect},
    prelude::Backend,
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, List, ListState, Paragraph, Scrollbar, ScrollbarOrientation, Widget as _,
        canvas::Label,
    },
};
use tui_tree_widget::{Tree, TreeItem, TreeState};

use crate::dir_diff::{DiffStatus, DirDiffEntry, LeftRightBoth, dir_diff};

pub fn tui() -> Result<()> {
    color_eyre::install()?;
    let mut terminal = ratatui::init();

    let app = App::new();
    let res = run_app(&mut terminal, app);

    ratatui::restore();
    res
}

struct Differ {
    pub path_before: PathBuf,
    pub path_after: PathBuf,
    pub diff: DirDiffEntry,
}

impl Differ {
    pub fn new(path_before: PathBuf, path_after: PathBuf) -> Self {
        let diff = dir_diff(LeftRightBoth::Both(
            path_before.to_owned(),
            path_after.to_owned(),
        ));
        Self {
            path_before,
            path_after,
            diff,
        }
    }

    pub fn get_at_path_mut(&mut self, path: &Path) -> Option<&mut DirDiffEntry> {
        let segments = path
            .components()
            .map(|it| match it {
                Component::Prefix(..)
                | Component::RootDir
                | Component::CurDir
                | Component::ParentDir => todo!(),
                Component::Normal(os_str) => PathBuf::from(os_str),
            })
            .collect::<Vec<_>>();

        self.get_at_mut(&segments)
    }
    pub fn get_at_mut(&mut self, path: &[PathBuf]) -> Option<&mut DirDiffEntry> {
        let mut e = &mut self.diff;

        for segment in path {
            match e {
                DirDiffEntry::Dir { result } => match result {
                    DiffStatus::OnlyInA { content, .. } => {
                        e = content.get_mut().entries.get_mut(segment.as_path())?;
                    }
                    DiffStatus::OnlyInB { content, .. } => {
                        e = content.get_mut().entries.get_mut(segment.as_path())?;
                    }
                    DiffStatus::InBoth { diff, .. } => {
                        e = diff.get_mut().entries.get_mut(segment.as_path())?;
                    }
                },
                DirDiffEntry::File { .. }
                | DirDiffEntry::Symlink { .. }
                | DirDiffEntry::Skipped => return None,
            }
        }

        Some(e)
    }
}

struct DirViewState {
    list: ListState,
}

impl Default for DirViewState {
    fn default() -> Self {
        Self {
            list: ListState::default().with_selected(Some(0)),
        }
    }
}

struct App {
    path: PathBuf,
    differ: Differ,
    view_states: HashMap<PathBuf, DirViewState>,
}

impl App {
    fn new() -> Self {
        let differ = Differ::new(
            Path::new("/impermanence/current_root_on_boot_snapshot/home/nionidh/").to_owned(),
            Path::new("/home/nionidh").to_owned(),
        );
        Self {
            differ,
            path: PathBuf::new(),
            view_states: HashMap::new(),
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

        if let Some(parent) = self.path.parent()
            && let Some(result) = self.differ.get_at_path_mut(parent)
        {
            let state = self.view_states.entry(parent.to_path_buf()).or_default();

            render_dir_diff_entry(result, state, frame, left);
        }

        if let Some(result) = self.differ.get_at_path_mut(&self.path) {
            let state = self.view_states.entry(self.path.clone()).or_default();

            render_dir_diff_entry(result, state, frame, middle);

            if let DirDiffEntry::Dir { result } = result {
                let contents = match result {
                    DiffStatus::OnlyInA { content, .. } => content.get(),
                    DiffStatus::OnlyInB { content, .. } => content.get(),
                    DiffStatus::InBoth { diff, .. } => diff.get(),
                };

                let mut items = contents.entries.keys().collect::<Vec<_>>();
                items.sort();

                if let Some(selected) = state
                    .list
                    .selected()
                    .and_then(|selected| items.get(selected))
                {
                    let selected_path = self.path.join(selected);
                    if let Some(result) = self.differ.get_at_path_mut(&selected_path) {
                        let state = self.view_states.entry(selected_path.clone()).or_default();
                        render_dir_diff_entry(result, state, frame, right);
                    }
                }
            }
        }
    }
}

fn render_dir_diff_entry(
    entry: &mut DirDiffEntry,
    state: &mut DirViewState,
    frame: &mut Frame,
    area: Rect,
) {
    match entry {
        DirDiffEntry::File { status } => {
            let (before, after) = match status {
                DiffStatus::OnlyInA { content, .. } => (Some(content.get()), None),
                DiffStatus::OnlyInB { content, .. } => (None, Some(content.get())),
                DiffStatus::InBoth { diff, .. } => {
                    let (before, after) = diff.get();
                    (Some(before), Some(after))
                }
            };

            let paragraph = Paragraph::new(vec![
                Line::from(format!("Before: {:?}", before)),
                Line::from(format!("After: {:?}", after)),
            ])
            .block(Block::bordered().title("File"));

            frame.render_widget(paragraph, area);
        }
        DirDiffEntry::Symlink { target } => {
            let (before, after) = match target {
                DiffStatus::OnlyInA { content, .. } => (Some(content.get()), None),
                DiffStatus::OnlyInB { content, .. } => (None, Some(content.get())),
                DiffStatus::InBoth { diff, .. } => {
                    let (before, after) = diff.get();
                    (Some(before), Some(after))
                }
            };

            let paragraph = Paragraph::new(vec![
                Line::from(format!("Before: {:?}", before)),
                Line::from(format!("After: {:?}", after)),
            ])
            .block(Block::bordered().title("Symlink"));

            frame.render_widget(paragraph, area);
        }
        DirDiffEntry::Dir { result } => {
            let contents = match result {
                DiffStatus::OnlyInA { content, .. } => content.get_mut(),
                DiffStatus::OnlyInB { content, .. } => content.get_mut(),
                DiffStatus::InBoth { diff, .. } => diff.get_mut(),
            };

            let mut items = contents.entries.iter_mut().collect::<Vec<_>>();
            items.sort_by_key(|it| it.0);

            let list = List::new(items.into_iter().map(|(path, state)| match state {
                DirDiffEntry::File { status } => match status {
                    DiffStatus::OnlyInA { .. } => Text::from(path.display().to_string()).red(),
                    DiffStatus::OnlyInB { .. } => Text::from(path.display().to_string()).green(),
                    DiffStatus::InBoth { .. } => Text::from(path.display().to_string()).yellow(),
                },
                DirDiffEntry::Symlink { target } => match target {
                    DiffStatus::OnlyInA { .. } => Text::from(path.display().to_string()).red(),
                    DiffStatus::OnlyInB { .. } => Text::from(path.display().to_string()).green(),
                    DiffStatus::InBoth { .. } => Text::from(path.display().to_string()).yellow(),
                },
                DirDiffEntry::Dir { result } => {
                    let text = Text::from(format!("{}/", path.display()));

                    match result {
                        DiffStatus::OnlyInA { .. } => text.red(),
                        DiffStatus::OnlyInB { .. } => text.green(),
                        DiffStatus::InBoth { diff, .. } => {
                            if diff.get_mut().has_meaningful_changes() {
                                text.yellow()
                            } else {
                                text.dark_gray()
                            }
                        }
                    }
                }
                DirDiffEntry::Skipped => Text::from(path.display().to_string()).dark_gray(),
            }))
            .block(Block::bordered().title("List"))
            .highlight_style(Style::new().reversed())
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true);

            frame.render_stateful_widget(list, area, &mut state.list);
        }
        DirDiffEntry::Skipped => {
            let paragraph = Paragraph::new(vec![Line::from("Skipped")])
                .block(Block::bordered().title("Skipped"));

            frame.render_widget(paragraph, area);
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
                    let state = app.view_states.entry(app.path.clone()).or_default();

                    match key.code {
                        KeyCode::Down => {
                            state.list.select_next();
                            true
                        }
                        KeyCode::Up => {
                            state.list.select_previous();
                            true
                        }
                        KeyCode::Right => {
                            if let Some(result) = app.differ.get_at_path_mut(&app.path) {
                                let state = app.view_states.entry(app.path.clone()).or_default();

                                if let DirDiffEntry::Dir { result } = result {
                                    let contents = match result {
                                        DiffStatus::OnlyInA { content, .. } => content.get(),
                                        DiffStatus::OnlyInB { content, .. } => content.get(),
                                        DiffStatus::InBoth { diff, .. } => diff.get(),
                                    };

                                    let mut items = contents.entries.keys().collect::<Vec<_>>();
                                    items.sort();

                                    if let Some(selected) = state
                                        .list
                                        .selected()
                                        .and_then(|selected| items.get(selected))
                                    {
                                        let selected_path = app.path.join(selected);
                                        app.path = selected_path;
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        }
                        KeyCode::Left => {
                            if let Some(parent) = app.path.parent() {
                                app.path = parent.to_path_buf();
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
