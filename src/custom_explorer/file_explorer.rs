use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use ratatui::crossterm::event::{Event, KeyCode};

use super::widget::{Renderer, Theme};
use super::filesystem::{Filesystem, FileEntry};

/// Metadata for a file, including size and timestamps
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileMetadata {
    pub size: u64,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub is_dir: bool,
}

impl FileMetadata {
    /// Format file size in human-readable format
    pub fn format_size(&self) -> String {
        if self.is_dir {
            return String::from("<DIR>");
        }

        let size = self.size;
        if size < 1024 {
            format!("{} B", size)
        } else if size < 1024 * 1024 {
            format!("{:.2} KB", size as f64 / 1024.0)
        } else if size < 1024 * 1024 * 1024 {
            format!("{:.2} MB", size as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }

    /// Format modified time in human-readable format
    pub fn format_modified(&self) -> String {
        match self.modified {
            Some(time) => {
                use std::time::UNIX_EPOCH;
                match time.duration_since(UNIX_EPOCH) {
                    Ok(duration) => {
                        let secs = duration.as_secs();
                        let datetime = chrono::DateTime::from_timestamp(secs as i64, 0)
                            .unwrap_or_default();
                        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                    }
                    Err(_) => String::from("Unknown"),
                }
            }
            None => String::from("Unknown"),
        }
    }

    /// Format created time in human-readable format
    pub fn format_created(&self) -> String {
        match self.created {
            Some(time) => {
                use std::time::UNIX_EPOCH;
                match time.duration_since(UNIX_EPOCH) {
                    Ok(duration) => {
                        let secs = duration.as_secs();
                        let datetime = chrono::DateTime::from_timestamp(secs as i64, 0)
                            .unwrap_or_default();
                        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                    }
                    Err(_) => String::from("Unknown"),
                }
            }
            None => String::from("Unknown"),
        }
    }
}

/// A file or directory in the file system
#[derive(Debug, Clone)]
pub struct File {
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_hidden: bool,
    metadata: Option<FileMetadata>,
}

impl File {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn is_file(&self) -> bool {
        !self.is_dir
    }

    pub fn is_hidden(&self) -> bool {
        self.is_hidden
    }

    pub fn metadata(&self) -> Option<&FileMetadata> {
        self.metadata.as_ref()
    }
}

impl From<FileEntry> for File {
    fn from(entry: FileEntry) -> Self {
        Self {
            name: entry.name,
            path: entry.path,
            is_dir: entry.is_dir,
            is_hidden: entry.is_hidden,
            metadata: entry.metadata,
        }
    }
}

/// File explorer widget for navigating the file system
pub struct FileExplorer {
    cwd: PathBuf,
    files: Vec<File>,
    show_hidden: bool,
    selected: usize,
    theme: Theme,
    filesystem: Arc<dyn Filesystem>,
}

impl FileExplorer {
    pub fn new(filesystem: Arc<dyn Filesystem>) -> io::Result<Self> {
        Self::with_theme(Theme::default(), filesystem)
    }

    pub fn with_theme(theme: Theme, filesystem: Arc<dyn Filesystem>) -> io::Result<Self> {
        let cwd = filesystem.current_dir()?;

        let mut explorer = Self {
            cwd: cwd.clone(),
            files: Vec::new(),
            show_hidden: false,
            selected: 0,
            theme,
            filesystem,
        };

        // Initial directory load
        explorer.refresh_sync()?;
        Ok(explorer)
    }

    pub fn widget(&self) -> Renderer<'_> {
        Renderer::new(self)
    }

    pub fn handle(&mut self, event: &Event) -> io::Result<()> {
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.selected + 1 < self.files.len() {
                        self.selected += 1;
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    // Go to parent directory
                    if let Some(parent) = self.filesystem.parent(&self.cwd) {
                        self.set_cwd(&parent)?;
                    }
                }
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                    // Enter directory
                    let path = self.files.get(self.selected)
                        .filter(|f| f.is_dir())
                        .map(|f| f.path().to_path_buf());
                    if let Some(path) = path {
                        self.set_cwd(&path)?;
                    }
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    self.selected = 0;
                }
                KeyCode::End | KeyCode::Char('G') => {
                    if !self.files.is_empty() {
                        self.selected = self.files.len() - 1;
                    }
                }
                KeyCode::PageUp => {
                    self.selected = self.selected.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    self.selected = (self.selected + 10).min(self.files.len().saturating_sub(1));
                }
                KeyCode::Char('.') if key.modifiers.contains(ratatui::crossterm::event::KeyModifiers::CONTROL) => {
                    self.set_show_hidden(!self.show_hidden);
                    self.refresh_sync()?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn set_cwd(&mut self, path: &Path) -> io::Result<()> {
        self.cwd = path.to_path_buf();
        self.refresh_sync()?;
        Ok(())
    }

    pub fn set_show_hidden(&mut self, show: bool) {
        self.show_hidden = show;
    }

    /// Synchronous refresh using blocking on async operations
    fn refresh_sync(&mut self) -> io::Result<()> {
        let cwd = self.cwd.clone();
        let filesystem = Arc::clone(&self.filesystem);

        // Use spawn_blocking to run async code without blocking the runtime
        let entries = std::thread::spawn(move || {
            // Create a new runtime in this thread
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            rt.block_on(filesystem.read_dir(&cwd))
        })
        .join()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Thread panicked: {:?}", e)))??;

        let mut files: Vec<File> = Vec::new();

        // Add parent directory entry if not at root
        if let Some(parent) = self.filesystem.parent(&self.cwd) {
            files.push(File {
                name: String::from("../"),
                path: parent,
                is_dir: true,
                is_hidden: false,
                metadata: None,
            });
        }

        // Convert FileEntry to File and filter hidden files
        for entry in entries {
            if !entry.is_hidden || self.show_hidden {
                files.push(entry.into());
            }
        }

        // Sort: directories first, then by name
        files.sort_by(|a, b| {
            if a.name == "../" {
                std::cmp::Ordering::Less
            } else if b.name == "../" {
                std::cmp::Ordering::Greater
            } else if a.is_dir() && !b.is_dir() {
                std::cmp::Ordering::Less
            } else if !a.is_dir() && b.is_dir() {
                std::cmp::Ordering::Greater
            } else {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            }
        });

        self.files = files;
        self.selected = self.selected.min(self.files.len().saturating_sub(1));
        Ok(())
    }

    pub fn current(&self) -> &File {
        &self.files[self.selected]
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn files(&self) -> &[File] {
        &self.files
    }

    pub fn selected_idx(&self) -> usize {
        self.selected
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn filesystem(&self) -> &Arc<dyn Filesystem> {
        &self.filesystem
    }

    /// Read file content (async operation, blocking wrapper)
    pub fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = path.to_path_buf();
        let filesystem = Arc::clone(&self.filesystem);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            rt.block_on(filesystem.read_file(&path))
        })
        .join()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Thread panicked: {:?}", e)))?
    }

    /// Read file content as string (async operation, blocking wrapper)
    pub fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let path = path.to_path_buf();
        let filesystem = Arc::clone(&self.filesystem);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            rt.block_on(filesystem.read_to_string(&path))
        })
        .join()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Thread panicked: {:?}", e)))?
    }
}
