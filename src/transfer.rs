//! File transfer utilities for send and pull operations

use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;
use anyhow::{Result, Context};

/// Calculate total size of a file or directory
pub fn calculate_size(path: &Path) -> Result<u64> {
    if path.is_file() {
        Ok(fs::metadata(path)?.len())
    } else if path.is_dir() {
        let mut total = 0;
        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                total += entry.metadata()?.len();
            }
        }
        Ok(total)
    } else {
        anyhow::bail!("Path does not exist: {}", path.display())
    }
}

/// Get all files in a directory recursively
pub fn get_files_recursive(path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    if path.is_file() {
        files.push(path.to_path_buf());
    } else if path.is_dir() {
        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                files.push(entry.path().to_path_buf());
            }
        }
    }

    Ok(files)
}

/// Create parent directories if they don't exist
pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }
    Ok(())
}

/// Chunk size for file transfers (64KB)
pub const CHUNK_SIZE: usize = 65536;
