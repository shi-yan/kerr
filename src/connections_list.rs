//! Interactive TUI for listing and selecting connections

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use n0_snafu::Result;
use ratatui::{
    prelude::*,
    widgets::*,
};
use tui_widget_list::{ListBuilder, ListState, ListView};
use crate::auth::Connection;

#[derive(Debug, Clone)]
pub struct ConnectionListItem {
    connection: Connection,
    style: Style,
}

impl ConnectionListItem {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            style: Style::default(),
        }
    }

    fn get_display_text(&self) -> String {
        let alias = self.connection.alias.as_deref().unwrap_or("(no alias)");
        let host = &self.connection.host_name;
        let timestamp = chrono::DateTime::from_timestamp(self.connection.registered_at as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        format!("{} @ {} - registered: {}", alias, host, timestamp)
    }
}

impl Widget for ConnectionListItem {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Line::from(self.get_display_text())
            .style(self.style)
            .render(area, buf);
    }
}

pub struct ConnectionsListApp {
    connections: Vec<Connection>,
    state: ListState,
    selected_connection: Option<Connection>,
}

impl ConnectionsListApp {
    pub fn new(connections: Vec<Connection>) -> Self {
        Self {
            connections,
            state: ListState::default(),
            selected_connection: None,
        }
    }

    pub fn selected_connection(&self) -> Option<&Connection> {
        self.selected_connection.as_ref()
    }

    fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
        let connections = self.connections.clone();

        let builder = ListBuilder::new(move |context| {
            let connection = connections[context.index].clone();
            let mut item = ConnectionListItem::new(connection);

            // Alternating styles
            if context.index % 2 == 0 {
                item.style = Style::default().bg(Color::Rgb(28, 28, 32));
            } else {
                item.style = Style::default().bg(Color::Rgb(0, 0, 0));
            }

            // Style the selected element
            if context.is_selected {
                item.style = Style::default()
                    .bg(Color::Rgb(255, 153, 0))
                    .fg(Color::Rgb(28, 28, 32));
            }

            // Return the size of the widget along the main axis
            let main_axis_size = 1;

            (item, main_axis_size)
        });

        let item_count = self.connections.len();
        let list = ListView::new(builder, item_count);

        list.render(area, buf, &mut self.state);
    }
}

impl Widget for &mut ConnectionsListApp {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Create a bordered block
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Available Connections ")
            .title_alignment(Alignment::Center)
            .border_style(Style::default().fg(Color::Cyan));

        let inner_area = block.inner(area);
        block.render(area, buf);

        // Render help text at the bottom
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner_area);

        // Render the list
        self.render_list(chunks[0], buf);

        // Render help text
        let help_text = Line::from(vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" Navigate | "),
            Span::styled("Enter", Style::default().fg(Color::Green)),
            Span::raw(" Select | "),
            Span::styled("q/Esc", Style::default().fg(Color::Red)),
            Span::raw(" Quit"),
        ]);
        help_text.render(chunks[1], buf);
    }
}

pub fn run_connections_list(connections: Vec<Connection>) -> Result<Option<Connection>> {
    if connections.is_empty() {
        println!("No connections registered.");
        return Ok(None);
    }

    // Setup terminal
    enable_raw_mode()
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to enable raw mode: {}", e)))?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to enter alternate screen: {}", e)))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to create terminal: {}", e)))?;

    let mut app = ConnectionsListApp::new(connections);
    let mut result = None;

    loop {
        terminal
            .draw(|f| {
                f.render_widget(&mut app, f.area());
            })
            .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to draw: {}", e)))?;

        if let Event::Key(key) = event::read()
            .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to read event: {}", e)))?
        {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        break;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.state.next();
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.state.previous();
                    }
                    KeyCode::Enter => {
                        if let Some(selected_index) = app.state.selected {
                            if selected_index < app.connections.len() {
                                result = Some(app.connections[selected_index].clone());
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to disable raw mode: {}", e)))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to leave alternate screen: {}", e)))?;
    terminal
        .show_cursor()
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to show cursor: {}", e)))?;

    Ok(result)
}
