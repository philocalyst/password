use ratatui::symbols::border::Set;

pub const REGULAR_SET: Set = Set {
	top_left:          "▛",
	top_right:         "▜",
	bottom_left:       "▔",
	bottom_right:      "▔",
	vertical_left:     "▏",
	vertical_right:    "▕",
	horizontal_top:    "▔",
	horizontal_bottom: "▔",
};

pub const WONKY_SET: Set = Set {
	top_left:          "╭",
	top_right:         "╮",
	bottom_left:       "▔",
	bottom_right:      "▔",
	vertical_left:     "║",
	vertical_right:    "║",
	horizontal_top:    "═",
	horizontal_bottom: "▔",
};
