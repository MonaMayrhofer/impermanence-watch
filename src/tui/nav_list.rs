use std::fmt::Debug;

use ratatui::{
    prelude::{Buffer, Rect},
    style::{Style, Stylize as _},
    text::Text,
    widgets::{Block, List, ListState, Paragraph, StatefulWidget, Widget},
};

pub(crate) trait NavListAdapter {
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

pub(crate) struct NavListItem<'a, TLocation> {
    pub(crate) text: Text<'a>,
    pub(crate) sub_location: Option<TLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct NavListState<TLocation> {
    pub(crate) offset: usize,
    pub(crate) selected: Option<TLocation>,
}

pub(crate) struct NavList<'a, TLocation> {
    adapter: &'a mut dyn NavListAdapter<Location = TLocation>,
    location: &'a TLocation,

    //Style stuff
    block: Block<'static>,
}

impl<'a, TLocation> NavList<'a, TLocation> {
    pub(crate) fn new(
        adapter: &'a mut impl NavListAdapter<Location = TLocation>,
        location: &'a TLocation,
        block: Block<'static>,
    ) -> Self {
        Self {
            adapter,
            location,
            block,
        }
    }
}

impl<'a, TLocation> StatefulWidget for NavList<'a, TLocation>
where
    TLocation: PartialEq + Debug,
{
    type State = NavListState<TLocation>;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let Some(items) = self.adapter.get_items(self.location) else {
            let paragraph = Paragraph::new("???").block(Block::bordered().title("List"));
            Widget::render(paragraph, area, buf);
            return;
        };

        let selected_idx = state.selected.as_ref().and_then(|it| {
            items
                .iter()
                .position(|item| item.sub_location.as_ref() == Some(it))
        });
        tracing::debug!(
            "{:?}",
            items.iter().map(|it| &it.sub_location).collect::<Vec<_>>()
        );

        tracing::info!(
            "Finding Selected idx: {:?} {:?}",
            selected_idx,
            state.selected
        );

        let list = List::new(items.into_iter().map(|it| it.text))
            .block(self.block)
            .highlight_style(Style::new().reversed())
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true);

        let mut list_state = ListState::default()
            .with_offset(state.offset)
            .with_selected(selected_idx);
        StatefulWidget::render(list, area, buf, &mut list_state);
    }
}
