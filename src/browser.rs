//! Interactive TUI file browser with preview

use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{self, Event, KeyCode, KeyModifiers},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::io;
use std::sync::Arc;
use std::time::Instant;

use crate::custom_explorer::{FileExplorer, Theme, LocalFilesystem, RemoteFilesystem, Filesystem, FileCache};

/// Which file browser has focus
#[derive(Debug, Clone, Copy, PartialEq)]
enum FocusedPane {
    Local,
    Remote,
}

/// Error message with timestamp for display in status bar
#[derive(Debug, Clone)]
struct ErrorMessage {
    message: String,
    timestamp: Instant,
}

/// Run the interactive file browser with local filesystem
pub fn run_browser() -> io::Result<()> {
    let filesystem = Arc::new(LocalFilesystem::new());
    run_browser_with_fs(filesystem, None)
}

/// Run the interactive file browser with a specific filesystem implementation
/// If remote_fs is provided, it will be used for caching remote file access
pub fn run_browser_with_fs(
    _filesystem: Arc<dyn Filesystem>,
    remote_fs: Option<Arc<RemoteFilesystem>>,
) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create local file explorer
    let local_fs = Arc::new(LocalFilesystem::new());
    let local_theme = Theme::default()
        .add_default_title()
        .with_block(Block::default().borders(Borders::ALL).title(" Local "))
        .with_dir_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let mut local_explorer = FileExplorer::with_theme(local_theme, local_fs)?;

    // Create remote file explorer if remote_fs is provided
    let mut remote_explorer = if let Some(ref remote_fs) = remote_fs {
        let remote_theme = Theme::default()
            .add_default_title()
            .with_block(Block::default().borders(Borders::ALL).title(" Remote "))
            .with_dir_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
        Some(FileExplorer::with_theme(remote_theme, Arc::clone(remote_fs) as Arc<dyn Filesystem>)?)
    } else {
        None
    };

    // Track which pane has focus
    let mut focused_pane = FocusedPane::Local;

    // Error message state (shared between callback and main loop)
    let error_message: Arc<std::sync::Mutex<Option<ErrorMessage>>> = Arc::new(std::sync::Mutex::new(None));

    // Set up error callback for remote filesystem if provided
    if let Some(ref remote_fs) = remote_fs {
        let error_msg_clone = Arc::clone(&error_message);
        remote_fs.set_error_callback(move |msg: String| {
            if let Ok(mut error) = error_msg_clone.lock() {
                *error = Some(ErrorMessage {
                    message: msg,
                    timestamp: Instant::now(),
                });
            }
        });
    }

    // Initialize cache for remote files if remote_fs is provided
    let _cache = if remote_fs.is_some() {
        Some(FileCache::new()?)
    } else {
        None
    };

    // Main loop
    let result = loop {
        terminal.draw(|f| {
            // Split screen: top = two browsers side-by-side, bottom = status bar
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)].as_ref())
                .split(f.area());

            // Split top area into left and right browsers
            let browser_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(main_chunks[0]);

            // Render local file browser with highlight border if focused
            f.render_widget(&local_explorer.widget(), browser_chunks[0]);
            if focused_pane == FocusedPane::Local {
                // Overlay highlight border
                let highlight_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    .title(" Local ");
                f.render_widget(highlight_block, browser_chunks[0]);
            }

            // Render remote file browser if available
            if let Some(ref remote) = remote_explorer {
                f.render_widget(&remote.widget(), browser_chunks[1]);
                if focused_pane == FocusedPane::Remote {
                    // Overlay highlight border
                    let highlight_block = Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                        .title(" Remote ");
                    f.render_widget(highlight_block, browser_chunks[1]);
                }
            }

            // Render status bar with metadata from focused browser or error message
            let focused_explorer = match focused_pane {
                FocusedPane::Local => &local_explorer,
                FocusedPane::Remote => remote_explorer.as_ref().unwrap_or(&local_explorer),
            };

            // Check for error message
            let current_error = if let Ok(error) = error_message.lock() {
                error.clone()
            } else {
                None
            };

            render_status_bar(f, main_chunks[1], focused_explorer, &current_error);
        })?;

        // Clear error messages after 3 seconds
        if let Ok(mut error) = error_message.lock() {
            if let Some(ref err) = *error {
                if err.timestamp.elapsed().as_secs() >= 3 {
                    *error = None;
                }
            }
        }

        // Handle input
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Tab => {
                        // Switch focus between local and remote
                        if remote_explorer.is_some() {
                            focused_pane = match focused_pane {
                                FocusedPane::Local => FocusedPane::Remote,
                                FocusedPane::Remote => FocusedPane::Local,
                            };
                        }
                    }
                    KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break Ok(())
                    }
                    KeyCode::Char('h') => {
                        // Toggle hidden files on focused browser
                        let toggle_event = Event::Key(
                            ratatui::crossterm::event::KeyEvent::new(
                                KeyCode::Char('.'),
                                KeyModifiers::CONTROL,
                            )
                        );
                        let result = match focused_pane {
                            FocusedPane::Local => local_explorer.handle(&toggle_event),
                            FocusedPane::Remote => {
                                if let Some(ref mut remote) = remote_explorer {
                                    remote.handle(&toggle_event)
                                } else {
                                    Ok(())
                                }
                            }
                        };
                        // Silently ignore errors (e.g., permission denied)
                        if let Err(_e) = result {
                            // Could show error in status bar in the future
                        }
                    }
                    _ => {
                        // Let the focused file explorer handle the event
                        let result = match focused_pane {
                            FocusedPane::Local => local_explorer.handle(&Event::Key(key)),
                            FocusedPane::Remote => {
                                if let Some(ref mut remote) = remote_explorer {
                                    remote.handle(&Event::Key(key))
                                } else {
                                    Ok(())
                                }
                            }
                        };
                        // Silently ignore errors (e.g., permission denied, invalid directory)
                        if let Err(_e) = result {
                            // Could show error in status bar in the future
                        }
                    }
                }
            }
        }
    };

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

/// Render the status bar showing metadata from the focused browser or error message
fn render_status_bar(
    frame: &mut ratatui::Frame,
    area: Rect,
    file_explorer: &FileExplorer,
    error: &Option<ErrorMessage>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    // Build status bar content
    let line = if let Some(err) = error {
        // Show error message in red
        Line::from(vec![
            Span::styled(" ERROR: ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(&err.message, Style::default().fg(Color::Red)),
        ])
    } else {
        // Show normal metadata
        let current = file_explorer.current();
        let mut spans = Vec::new();

        // Path
        spans.push(Span::styled(" Path: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
        spans.push(Span::raw(current.path().display().to_string()));
        spans.push(Span::raw(" │ "));

        if let Some(metadata) = current.metadata() {
            // Type
            spans.push(Span::styled("Type: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(if metadata.is_dir { "Dir" } else { "File" }));
            spans.push(Span::raw(" │ "));

            // Size
            spans.push(Span::styled("Size: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(metadata.format_size()));
            spans.push(Span::raw(" │ "));

            // Modified
            spans.push(Span::styled("Modified: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(metadata.format_modified()));
        } else {
            spans.push(Span::styled("Type: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(if current.is_dir() { "Parent Dir" } else { "Unknown" }));
        }

        Line::from(spans)
    };

    let paragraph = Paragraph::new(vec![line])
        .block(block)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, area);
}
