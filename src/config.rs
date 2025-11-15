use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Configuration for server auto-update functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Base URL for the backend server (e.g., "https://example.com")
    pub server_url: String,
    /// Admin password hash for authorizing updates (plain text, will be hashed with blake3)
    pub admin_password: String,
}

impl ServerConfig {
    /// Load configuration from the default config file
    pub fn load() -> Result<Self> {
        let config_path = get_config_file_path()?;
        Self::load_from_path(&config_path)
    }

    /// Load configuration from a specific path
    pub fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            anyhow::bail!(
                "Server config file not found at {}. Server is not configured for auto-update.",
                path.display()
            );
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file at {}", path.display()))?;

        let config: ServerConfig = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file at {}", path.display()))?;

        Ok(config)
    }

    /// Save configuration to the default config file
    pub fn save(&self) -> Result<()> {
        let config_path = get_config_file_path()?;
        self.save_to_path(&config_path)
    }

    /// Save configuration to a specific path
    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory at {}", parent.display()))?;
        }

        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;

        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config file at {}", path.display()))?;

        Ok(())
    }

    /// Verify if a provided hash matches the admin password
    pub fn verify_admin_hash(&self, provided_hash: &str) -> bool {
        let computed_hash = blake3::hash(self.admin_password.as_bytes());
        let computed_hash_hex = computed_hash.to_hex().to_string();
        computed_hash_hex == provided_hash
    }

    /// Get the blake3 hash of the admin password
    pub fn get_admin_hash(&self) -> String {
        let hash = blake3::hash(self.admin_password.as_bytes());
        hash.to_hex().to_string()
    }
}

/// Get the default config directory
pub fn get_config_dir() -> Result<PathBuf> {
    ProjectDirs::from("app", "freewill", "kerr")
        .map(|proj_dirs| proj_dirs.config_dir().to_path_buf())
        .context("Failed to determine config directory")
}

/// Get the default cache directory
pub fn get_cache_dir() -> Result<PathBuf> {
    ProjectDirs::from("app", "freewill", "kerr")
        .map(|proj_dirs| proj_dirs.cache_dir().to_path_buf())
        .context("Failed to determine cache directory")
}

/// Get the path to the config file
pub fn get_config_file_path() -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    Ok(config_dir.join("config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admin_hash_verification() {
        let config = ServerConfig {
            server_url: "https://example.com".to_string(),
            admin_password: "test_password".to_string(),
        };

        let hash = config.get_admin_hash();
        assert!(config.verify_admin_hash(&hash));
        assert!(!config.verify_admin_hash("wrong_hash"));
    }
}
