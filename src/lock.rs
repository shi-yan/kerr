use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use tracing::{info, warn};

use crate::config::get_cache_dir;

/// File lock for ensuring single instance
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    /// Try to acquire an exclusive lock for the application
    /// Returns an error if another instance is already running
    pub fn try_acquire() -> Result<Self> {
        let lock_path = get_lock_file_path()?;

        info!("Attempting to acquire instance lock at: {}", lock_path.display());

        // Create parent directory if needed
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create lock directory at {}", parent.display()))?;
        }

        // Open or create the lock file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("Failed to open lock file at {}", lock_path.display()))?;

        // Try to acquire exclusive lock (non-blocking)
        match file.try_lock_exclusive() {
            Ok(_) => {
                info!("Instance lock acquired successfully");
                Ok(Self { _file: file })
            }
            Err(e) => {
                warn!("Failed to acquire instance lock (another instance may be running)");
                anyhow::bail!(
                    "Another instance of Kerr is already running. \
                    Only one instance is allowed per system.\n\
                    Lock file: {}\n\
                    Error: {}",
                    lock_path.display(),
                    e
                );
            }
        }
    }

    /// Get the path to the lock file
    /// This is public so the updater can wait for it
    pub fn get_path() -> Result<PathBuf> {
        get_lock_file_path()
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // Lock is automatically released when the file is closed
        info!("Instance lock released");
    }
}

/// Get the path to the lock file
fn get_lock_file_path() -> Result<PathBuf> {
    let cache_dir = get_cache_dir()?;
    Ok(cache_dir.join("kerr.lock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_instance() {
        // First instance should acquire lock
        let _lock1 = InstanceLock::try_acquire().expect("First instance should acquire lock");

        // Second instance should fail
        let result = InstanceLock::try_acquire();
        assert!(result.is_err(), "Second instance should fail to acquire lock");

        // After dropping the first lock, we should be able to acquire again
        drop(_lock1);
        let _lock2 = InstanceLock::try_acquire().expect("Should acquire lock after first is released");
    }
}
