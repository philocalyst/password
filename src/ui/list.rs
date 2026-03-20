use std::collections::HashMap;
use ratatui::{prelude::*, widgets::{Block, Borders, List, ListItem}};
use crate::models::Item;
use crate::ui::theme::REGULAR_SET;

#[derive(Clone)]
pub struct ItemList<'a>(pub &'a HashMap<String, Item>);

impl<'a> From<ItemList<'a>> for List<'a> {
	fn from(items: ItemList<'a>) -> Self {
		let mut sorted_items: Vec<(&String, &Item)> = items.0.iter().collect();
		sorted_items.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));

		let list_items: Vec<ListItem<'a>> = sorted_items
			.iter()
			.map(|(name, _)| {
				let line =
					Line::from(vec![Span::styled(name.to_string(), Style::default().fg(Color::Gray))]);
				ListItem::new(line)
			})
			.collect();

		let block = Block::new().borders(Borders::ALL).border_set(REGULAR_SET).bg(Color::DarkGray);

		List::new(list_items)
			.block(block)
			.highlight_symbol("!")
			.highlight_style(Style::default().add_modifier(Modifier::BOLD))
	}
}

impl<'a> ItemList<'a> {
	/// Get an item by its index in the alphabetically sorted list
	pub fn get_by_index(&self, index: usize) -> Option<(&String, &Item)> {
		let mut sorted_items: Vec<(&String, &Item)> = self.0.iter().collect();
		sorted_items.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));
		sorted_items.get(index).copied()
	}
}
