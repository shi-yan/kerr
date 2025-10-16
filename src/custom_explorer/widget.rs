use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListState, StatefulWidget, WidgetRef},
};

use super::file_explorer::FileExplorer;

/// Widget renderer for FileExplorer
pub struct Renderer<'a> {
    explorer: &'a FileExplorer,
}

impl<'a> Renderer<'a> {
    pub fn new(explorer: &'a FileExplorer) -> Self {
        Self { explorer }
    }
}

impl WidgetRef for Renderer<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let theme = self.explorer.theme();
        let files = self.explorer.files();
        let selected_idx = self.explorer.selected_idx();

        // Create list items
        let items: Vec<Line> = files
            .iter()
            .map(|file| {
                let style = if file.is_dir() {
                    theme.dir_style
                } else {
                    theme.style
                };
                Line::from(Span::styled(file.name(), style))
            })
            .collect();

        // Determine highlight style
        let highlight_style = if let Some(file) = files.get(selected_idx) {
            if file.is_dir() {
                theme.highlight_dir_style
            } else {
                theme.highlight_style
            }
        } else {
            theme.highlight_style
        };

        let list = List::new(items)
            .highlight_style(highlight_style)
            .highlight_symbol(&theme.highlight_symbol);

        let list = if let Some(block) = &theme.block {
            list.block(block.clone())
        } else {
            list
        };

        let mut state = ListState::default();
        state.select(Some(selected_idx));

        StatefulWidget::render(list, area, buf, &mut state);
    }
}

/// Theme configuration for the file explorer
#[derive(Clone)]
pub struct Theme {
    pub block: Option<Block<'static>>,
    pub style: Style,
    pub dir_style: Style,
    pub highlight_style: Style,
    pub highlight_dir_style: Style,
    pub highlight_symbol: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            block: None,
            style: Style::default().fg(Color::White),
            dir_style: Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
            highlight_style: Style::default().bg(Color::DarkGray),
            highlight_dir_style: Style::default()
                .bg(Color::DarkGray)
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
            highlight_symbol: String::from(">> "),
        }
    }
}

impl Theme {
    pub fn with_block(mut self, block: Block<'static>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn with_dir_style(mut self, style: Style) -> Self {
        self.dir_style = style;
        self
    }

    pub fn with_highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    pub fn with_highlight_dir_style(mut self, style: Style) -> Self {
        self.highlight_dir_style = style;
        self
    }

    pub fn with_highlight_symbol(mut self, symbol: String) -> Self {
        self.highlight_symbol = symbol;
        self
    }

    pub fn add_default_title(self) -> Self {
        self.with_block(Block::default().title(" File Explorer "))
    }
}
