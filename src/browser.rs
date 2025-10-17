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
use tui_textarea::TextArea;
use ratatui_image::{picker::Picker, StatefulImage};
use std::io;
use std::path::Path;
use std::sync::Arc;

use crate::custom_explorer::{FileExplorer, Theme, LocalFilesystem, RemoteFilesystem, Filesystem, FileCache};

/// Preview mode state
enum PreviewMode {
    Metadata,  // Show metadata (default)
    Content,   // Show file content (when space is pressed)
    Image,     // Show image preview
}

/// Run the interactive file browser with local filesystem
pub fn run_browser() -> io::Result<()> {
    let filesystem = Arc::new(LocalFilesystem::new());
    run_browser_with_fs(filesystem, None)
}

/// Run the interactive file browser with a specific filesystem implementation
/// If remote_fs is provided, it will be used for caching remote file access
pub fn run_browser_with_fs(
    filesystem: Arc<dyn Filesystem>,
    remote_fs: Option<Arc<RemoteFilesystem>>,
) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create file explorer with custom theme
    let theme = Theme::default()
        .add_default_title()
        .with_block(Block::default().borders(Borders::ALL).title(" Files "))
        .with_dir_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let mut file_explorer = FileExplorer::with_theme(theme, filesystem)?;
    let mut preview_mode = PreviewMode::Metadata;
    let mut content_viewer: Option<TextArea> = None;
    let mut image_state: Option<ratatui_image::protocol::StatefulProtocol> = None;

    // Initialize cache for remote files if remote_fs is provided
    let cache = if remote_fs.is_some() {
        Some(FileCache::new()?)
    } else {
        None
    };

    // Initialize image picker for terminal
    let mut picker = Picker::from_query_stdio()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{:?}", e)))?;

    // Main loop
    let result = loop {
        terminal.draw(|f| {
            // Split screen: left = file list, right = preview
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
                .split(f.area());

            // Render file explorer
            f.render_widget(&file_explorer.widget(), chunks[0]);

            // Render preview based on mode
            match &mut preview_mode {
                PreviewMode::Metadata => {
                    render_metadata_preview(f, chunks[1], &file_explorer);
                }
                PreviewMode::Content => {
                    render_content_preview(f, chunks[1], &file_explorer, &mut content_viewer);
                }
                PreviewMode::Image => {
                    render_image_preview(f, chunks[1], &file_explorer, &mut image_state);
                }
            }
        })?;

        // Handle input
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match &preview_mode {
                    PreviewMode::Content => {
                        // Handle text content preview
                        match key.code {
                            KeyCode::Char(' ') => {
                                // Toggle back to metadata
                                preview_mode = PreviewMode::Metadata;
                                content_viewer = None;
                            }
                            KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                break Ok(())
                            }
                            // Let textarea handle scrolling
                            _ => {
                                if let Some(ref mut viewer) = content_viewer {
                                    viewer.input(ratatui::crossterm::event::KeyEvent::from(key));
                                }
                            }
                        }
                    }
                    PreviewMode::Image => {
                        // Handle image preview
                        match key.code {
                            KeyCode::Char(' ') | KeyCode::Esc => {
                                // Toggle back to metadata
                                preview_mode = PreviewMode::Metadata;
                                image_state = None;
                            }
                            KeyCode::Char('q') => break Ok(()),
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                break Ok(())
                            }
                            _ => {}
                        }
                    }
                    PreviewMode::Metadata => {
                        // In metadata mode, handle normal navigation
                        match key.code {
                            KeyCode::Char(' ') => {
                                // Toggle to content/image preview
                                let current = file_explorer.current();
                                if current.is_file() {
                                    if is_image_file(current.path()) {
                                        // Load image
                                        match load_image(&mut picker, current.path(), &cache, &remote_fs) {
                                            Ok(protocol) => {
                                                preview_mode = PreviewMode::Image;
                                                image_state = Some(protocol);
                                            }
                                            Err(_) => {
                                                // Fall back to text preview if image loading fails
                                                preview_mode = PreviewMode::Content;
                                                content_viewer = Some(load_file_into_textarea(current.path(), &cache, &remote_fs));
                                            }
                                        }
                                    } else {
                                        preview_mode = PreviewMode::Content;
                                        content_viewer = Some(load_file_into_textarea(current.path(), &cache, &remote_fs));
                                    }
                                }
                            }
                            KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                break Ok(())
                            }
                            KeyCode::Char('h') => {
                                // Toggle hidden files
                                let toggle_event = Event::Key(
                                    ratatui::crossterm::event::KeyEvent::new(
                                        KeyCode::Char('.'),
                                        KeyModifiers::CONTROL,
                                    )
                                );
                                file_explorer.handle(&toggle_event)?;
                            }
                            _ => {
                                // Let the file explorer handle the event
                                file_explorer.handle(&Event::Key(key))?;
                            }
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
        let cache_clone = FileCache::new()?; // Create a new cache instance for the thread

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

    load_file_into_textarea_from_local(&local_path)
}

/// Load file content from a local path into a TextArea widget
fn load_file_into_textarea_from_local(path: &std::path::Path) -> TextArea<'static> {
    let mut textarea = match std::fs::read_to_string(path) {
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

/// Render the metadata preview pane
fn render_metadata_preview(
    frame: &mut ratatui::Frame,
    area: Rect,
    file_explorer: &FileExplorer,
) {
    let current = file_explorer.current();
    let title = format!(" Info: {} ", current.name());

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::White));

    // Build metadata display
    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("Path: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(current.path().display().to_string()),
    ]));
    lines.push(Line::from(""));

    if let Some(metadata) = current.metadata() {
        lines.push(Line::from(vec![
            Span::styled("Type: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(if metadata.is_dir { "Directory" } else { "File" }),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Size: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(metadata.format_size()),
        ]));

        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::styled("Modified: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(metadata.format_modified()),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Created:  ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(metadata.format_created()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Type: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(if current.is_dir() { "Directory (Parent)" } else { "Unknown" }),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Hint: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::raw("Press "),
        Span::styled("Space", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" to preview file content"),
    ]));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, area);
}

/// Render the content preview pane with TextArea
fn render_content_preview(
    frame: &mut ratatui::Frame,
    area: Rect,
    file_explorer: &FileExplorer,
    content_viewer: &mut Option<TextArea>,
) {
    let current = file_explorer.current();
    let title = format!(" Preview: {} ", current.name());

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(Line::from(vec![
            Span::raw(" "),
            Span::styled("Space", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(": back | "),
            Span::styled("↑↓", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(": scroll | "),
            Span::styled("PgUp/PgDn", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(": page "),
        ]))
        .border_style(Style::default().fg(Color::White));

    if let Some(viewer) = content_viewer {
        viewer.set_block(block);
        frame.render_widget(&*viewer, area);
    }
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

/// Render the image preview pane
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
            Span::styled("Space/Esc", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
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
        let paragraph = Paragraph::new(vec![
            Line::from(Span::styled(
                "Failed to load image",
                Style::default().fg(Color::Red),
            )),
        ])
        .block(block.clone())
        .style(Style::default().fg(Color::White));

        frame.render_widget(paragraph, area);
    }
}
