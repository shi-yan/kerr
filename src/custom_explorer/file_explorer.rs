use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use ratatui::crossterm::event::{Event, KeyCode};

use super::widget::{Renderer, Theme};

/// Metadata for a file, including size and timestamps
#[derive(Debug, Clone)]
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
    file_type: Option<fs::FileType>,
    metadata: Option<FileMetadata>,
}

impl File {
    fn new(path: &Path) -> io::Result<Self> {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file name"))?;

        let metadata = fs::metadata(path)?;
        let file_type = metadata.file_type();
        let is_dir = file_type.is_dir();

        let file_metadata = FileMetadata {
            size: metadata.len(),
            created: metadata.created().ok(),
            modified: metadata.modified().ok(),
            is_dir,
        };

        #[cfg(unix)]
        let is_hidden = file_name.starts_with('.');

        #[cfg(windows)]
        let is_hidden = {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
            (metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN) != 0
        };

        #[cfg(not(any(unix, windows)))]
        let is_hidden = false;

        let name = if is_dir {
            format!("{}/", file_name)
        } else {
            file_name.to_string()
        };

        Ok(Self {
            name,
            path: path.to_path_buf(),
            is_dir,
            is_hidden,
            file_type: Some(file_type),
            metadata: Some(file_metadata),
        })
    }

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

    pub fn file_type(&self) -> Option<&fs::FileType> {
        self.file_type.as_ref()
    }

    pub fn metadata(&self) -> Option<&FileMetadata> {
        self.metadata.as_ref()
    }
}

/// File explorer widget for navigating the file system
pub struct FileExplorer {
    cwd: PathBuf,
    files: Vec<File>,
    show_hidden: bool,
    selected: usize,
    theme: Theme,
}

impl FileExplorer {
    pub fn new() -> io::Result<Self> {
        Self::with_theme(Theme::default())
    }

    pub fn with_theme(theme: Theme) -> io::Result<Self> {
        let cwd = env::current_dir()?;
        let mut explorer = Self {
            cwd: cwd.clone(),
            files: Vec::new(),
            show_hidden: false,
            selected: 0,
            theme,
        };
        explorer.set_cwd(&cwd)?;
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
                    if let Some(parent) = self.cwd.parent().map(|p| p.to_path_buf()) {
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
                    self.refresh()?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn set_cwd(&mut self, path: &Path) -> io::Result<()> {
        self.cwd = path.to_path_buf();
        self.refresh()?;
        Ok(())
    }

    pub fn set_show_hidden(&mut self, show: bool) {
        self.show_hidden = show;
    }

    fn refresh(&mut self) -> io::Result<()> {
        let mut files = Vec::new();

        // Add parent directory entry if not at root
        if self.cwd.parent().is_some() {
            files.push(File {
                name: String::from("../"),
                path: self.cwd.parent().unwrap().to_path_buf(),
                is_dir: true,
                is_hidden: false,
                file_type: None,
                metadata: None,
            });
        }

        // Read directory entries
        for entry in fs::read_dir(&self.cwd)? {
            let entry = entry?;
            match File::new(&entry.path()) {
                Ok(file) => {
                    if !file.is_hidden() || self.show_hidden {
                        files.push(file);
                    }
                }
                Err(_) => continue,
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
}
