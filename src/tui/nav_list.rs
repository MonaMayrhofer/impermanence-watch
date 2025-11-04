use ratatui::{
    prelude::{Buffer, Rect},
    style::{Style, Stylize as _},
    text::Text,
    widgets::{Block, List, ListState, StatefulWidget},
};

pub trait NavListAdapter {
    type Location;

    fn get_items(
        &mut self,
        location: &Self::Location,
    ) -> Option<Vec<NavListItem<'_, Self::Location>>>;

    fn get_next(
        &mut self,
        location: &Self::Location,
        previous_sub_location: Option<&Self::Location>,
    ) -> Option<Self::Location>;
    fn get_previous(
        &mut self,
        location: &Self::Location,
        next_sub_location: Option<&Self::Location>,
    ) -> Option<Self::Location>;
}

pub struct NavListItem<'a, TLocation> {
    pub text: Text<'a>,
    pub sub_location: Option<TLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NavListState<TLocation> {
    pub offset: usize,
    pub selected: Option<TLocation>,
}

pub struct NavList<'a, TLocation> {
    adapter: &'a mut dyn NavListAdapter<Location = TLocation>,
    location: &'a TLocation,
}

impl<'a, TLocation> NavList<'a, TLocation> {
    pub fn new(
        adapter: &'a mut impl NavListAdapter<Location = TLocation>,
        location: &'a TLocation,
    ) -> Self {
        Self { adapter, location }
    }
}

impl<'a, TLocation> StatefulWidget for NavList<'a, TLocation>
where
    TLocation: PartialEq,
{
    type State = NavListState<TLocation>;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let Some(items) = self.adapter.get_items(self.location) else {
            return;
        };

        let selected_idx = state.selected.as_ref().and_then(|it| {
            items
                .iter()
                .position(|item| item.sub_location.as_ref() == Some(it))
        });

        let list = List::new(items.into_iter().map(|it| it.text))
            .block(Block::bordered().title("List"))
            .highlight_style(Style::new().reversed())
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true);

        let mut list_state = ListState::default()
            .with_offset(state.offset)
            .with_selected(selected_idx);
        StatefulWidget::render(list, area, buf, &mut list_state);
    }
}
