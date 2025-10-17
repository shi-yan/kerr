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
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tui_textarea::TextArea;
use ratatui_image::{picker::Picker, StatefulImage};

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

/// Preview mode state
enum PreviewMode {
    None,           // Not previewing
    Text,           // Text file preview
    Image,          // Image preview
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
    let cache = if remote_fs.is_some() {
        Some(FileCache::new()?)
    } else {
        None
    };

    // Preview mode state
    let mut preview_mode = PreviewMode::None;
    let mut text_viewer: Option<TextArea> = None;
    let mut image_state: Option<ratatui_image::protocol::StatefulProtocol> = None;

    // Initialize image picker for terminal
    let mut picker = Picker::from_query_stdio()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{:?}", e)))?;

    // Main loop
    let result = loop {
        terminal.draw(|f| {
            match preview_mode {
                PreviewMode::None => {
                    // Normal browser view
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
                }
                PreviewMode::Text => {
                    // Fullscreen text preview
                    if let Some(ref viewer) = text_viewer {
                        let focused_explorer = match focused_pane {
                            FocusedPane::Local => &local_explorer,
                            FocusedPane::Remote => remote_explorer.as_ref().unwrap_or(&local_explorer),
                        };
                        render_text_preview(f, f.area(), focused_explorer, viewer);
                    }
                }
                PreviewMode::Image => {
                    // Fullscreen image preview
                    let focused_explorer = match focused_pane {
                        FocusedPane::Local => &local_explorer,
                        FocusedPane::Remote => remote_explorer.as_ref().unwrap_or(&local_explorer),
                    };
                    render_image_preview(f, f.area(), focused_explorer, &mut image_state);
                }
            }
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
                match preview_mode {
                    PreviewMode::None => {
                        // Normal browser mode
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
                            KeyCode::Char('q') => break Ok(()),
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                break Ok(())
                            }
                            KeyCode::Char(' ') => {
                                // Space: preview file or enter directory
                                let current = match focused_pane {
                                    FocusedPane::Local => local_explorer.current(),
                                    FocusedPane::Remote => remote_explorer.as_ref().unwrap_or(&local_explorer).current(),
                                };

                                if current.is_dir() {
                                    // Navigate into directory
                                    let result = match focused_pane {
                                        FocusedPane::Local => {
                                            local_explorer.handle(&Event::Key(
                                                ratatui::crossterm::event::KeyEvent::new(
                                                    KeyCode::Right,
                                                    KeyModifiers::NONE,
                                                )
                                            ))
                                        }
                                        FocusedPane::Remote => {
                                            if let Some(ref mut remote) = remote_explorer {
                                                remote.handle(&Event::Key(
                                                    ratatui::crossterm::event::KeyEvent::new(
                                                        KeyCode::Right,
                                                        KeyModifiers::NONE,
                                                    )
                                                ))
                                            } else {
                                                Ok(())
                                            }
                                        }
                                    };
                                    // Silently ignore errors
                                    if let Err(_e) = result {
                                        // Could show error in status bar
                                    }
                                } else if current.is_file() {
                                    // Preview file
                                    if is_image_file(current.path()) {
                                        // Load image
                                        match load_image(&mut picker, current.path(), &cache, &remote_fs) {
                                            Ok(protocol) => {
                                                preview_mode = PreviewMode::Image;
                                                image_state = Some(protocol);
                                            }
                                            Err(e) => {
                                                // Show error in error state instead of trying text preview
                                                if let Ok(mut error) = error_message.lock() {
                                                    *error = Some(ErrorMessage {
                                                        message: format!("Failed to load image: {}", e),
                                                        timestamp: Instant::now(),
                                                    });
                                                }
                                            }
                                        }
                                    } else {
                                        preview_mode = PreviewMode::Text;
                                        text_viewer = Some(load_file_into_textarea(current.path(), &cache, &remote_fs));
                                    }
                                }
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
                    PreviewMode::Text => {
                        // Text preview mode
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => {
                                // Exit preview mode
                                preview_mode = PreviewMode::None;
                                text_viewer = None;
                            }
                            _ => {
                                // Let textarea handle scrolling
                                if let Some(ref mut viewer) = text_viewer {
                                    viewer.input(ratatui::crossterm::event::KeyEvent::from(key));
                                }
                            }
                        }
                    }
                    PreviewMode::Image => {
                        // Image preview mode
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ') => {
                                // Exit preview mode
                                preview_mode = PreviewMode::None;
                                image_state = None;
                            }
                            _ => {}
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

/// Check if a file is an image based on extension
fn is_image_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp")
    } else {
        false
    }
}

/// Get local path for a file (either directly or via cache for remote files)
fn get_local_path(
    path: &std::path::Path,
    cache: &Option<FileCache>,
    remote_fs: &Option<Arc<RemoteFilesystem>>,
) -> io::Result<std::path::PathBuf> {
    if let (Some(_cache), Some(remote_fs)) = (cache, remote_fs) {
        // Remote file - fetch via cache using a new runtime
        let path = path.to_path_buf();
        let remote_fs = Arc::clone(remote_fs);
        let cache_clone = FileCache::new()?;

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            rt.block_on(cache_clone.get_or_fetch(&path, &remote_fs))
        })
        .join()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Thread panicked: {:?}", e)))?
    } else {
        // Local file - return as is
        Ok(path.to_path_buf())
    }
}

/// Load file content into a TextArea widget
fn load_file_into_textarea(
    path: &std::path::Path,
    cache: &Option<FileCache>,
    remote_fs: &Option<Arc<RemoteFilesystem>>,
) -> TextArea<'static> {
    // Get the local path (either original or cached)
    let local_path = match get_local_path(path, cache, remote_fs) {
        Ok(p) => p,
        Err(e) => {
            return TextArea::new(vec![
                format!("[Error accessing file: {}]", e),
            ]);
        }
    };

    let mut textarea = match std::fs::read_to_string(&local_path) {
        Ok(content) => {
            if content.len() > 1_000_000 {
                TextArea::new(vec![
                    format!("[File too large: {} bytes]", content.len()),
                    String::from("Cannot display files larger than 1MB"),
                ])
            } else {
                TextArea::new(content.lines().map(|s| s.to_string()).collect())
            }
        }
        Err(e) => {
            TextArea::new(vec![
                format!("[Error reading file: {}]", e),
            ])
        }
    };

    // Configure textarea for read-only viewing
    textarea.set_cursor_line_style(Style::default());
    textarea.set_line_number_style(Style::default().fg(Color::DarkGray));

    textarea
}

/// Load an image file and create a protocol for rendering
fn load_image(
    picker: &mut Picker,
    path: &Path,
    cache: &Option<FileCache>,
    remote_fs: &Option<Arc<RemoteFilesystem>>,
) -> Result<ratatui_image::protocol::StatefulProtocol, Box<dyn std::error::Error>> {
    // Get the local path (either original or cached)
    let local_path = get_local_path(path, cache, remote_fs)?;
    let dyn_img = image::ImageReader::open(&local_path)?.decode()?;
    let protocol = picker.new_resize_protocol(dyn_img);
    Ok(protocol)
}

/// Render the text preview in fullscreen
fn render_text_preview(
    frame: &mut ratatui::Frame,
    area: Rect,
    file_explorer: &FileExplorer,
    viewer: &TextArea,
) {
    let current = file_explorer.current();
    let title = format!(" Preview: {} ", current.name());

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(Line::from(vec![
            Span::raw(" "),
            Span::styled("Esc/q", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(": back | "),
            Span::styled("↑↓", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(": scroll | "),
            Span::styled("PgUp/PgDn", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(": page "),
        ]))
        .border_style(Style::default().fg(Color::White));

    let mut viewer_clone = viewer.clone();
    viewer_clone.set_block(block);
    frame.render_widget(&viewer_clone, area);
}

/// Render the image preview in fullscreen
fn render_image_preview(
    frame: &mut ratatui::Frame,
    area: Rect,
    file_explorer: &FileExplorer,
    image_state: &mut Option<ratatui_image::protocol::StatefulProtocol>,
) {
    let current = file_explorer.current();
    let title = format!(" Image: {} ", current.name());

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(Line::from(vec![
            Span::raw(" "),
            Span::styled("Esc/q/Space", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(": back "),
        ]))
        .border_style(Style::default().fg(Color::White));

    // Render block first
    frame.render_widget(&block, area);

    if let Some(protocol) = image_state {
        // Calculate inner area for image (inside the block borders)
        let inner = block.inner(area);
        let image = StatefulImage::default();
        frame.render_stateful_widget(image, inner, protocol);
    } else {
        // Show error message if image failed to load
        let inner = block.inner(area);
        let paragraph = Paragraph::new(vec![
            Line::from(Span::styled(
                "Failed to load image",
                Style::default().fg(Color::Red),
            )),
        ])
        .style(Style::default().fg(Color::White));

        frame.render_widget(paragraph, inner);
    }
}
