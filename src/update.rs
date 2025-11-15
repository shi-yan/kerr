use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{info, warn};

use crate::config::{get_cache_dir, ServerConfig};

/// Platform identifier for download URLs
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const PLATFORM: &str = "mac_aarch64";

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const PLATFORM: &str = "mac_x86_64";

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const PLATFORM: &str = "linux_aarch64";

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const PLATFORM: &str = "linux_x86_64";

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const PLATFORM: &str = "windows_x86_64";

#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
const PLATFORM: &str = "windows_aarch64";

/// Version information from the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub urls: std::collections::HashMap<String, String>,
}

/// Response from the /versions endpoint
pub type VersionsResponse = Vec<VersionInfo>;

/// Check if we're running in debug mode
pub fn is_debug_mode() -> bool {
    cfg!(debug_assertions)
}

/// Get the path to the updater binary
pub fn get_updater_path() -> Result<PathBuf> {
    let self_exe = env::current_exe().context("Failed to get current executable path")?;
    let mut updater_path = self_exe.clone();
    updater_path.pop(); // Remove the binary name

    #[cfg(target_os = "windows")]
    let updater_name = "kerr-updater.exe";
    #[cfg(not(target_os = "windows"))]
    let updater_name = "kerr-updater";

    updater_path.push(updater_name);
    Ok(updater_path)
}

/// Get the cache directory for downloads
pub fn get_download_cache_dir() -> Result<PathBuf> {
    let cache_dir = get_cache_dir()?;
    let download_dir = cache_dir.join("updates");
    std::fs::create_dir_all(&download_dir)
        .with_context(|| format!("Failed to create download cache dir at {}", download_dir.display()))?;
    Ok(download_dir)
}

/// Check for available updates from the server
pub async fn check_for_updates(config: &ServerConfig) -> Result<Option<VersionInfo>> {
    let url = format!("{}/versions", config.server_url);
    info!("Checking for updates from: {}", url);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch versions from {}", url))?;

    if !response.status().is_success() {
        anyhow::bail!("Server returned error: {}", response.status());
    }

    let versions: VersionsResponse = response
        .json()
        .await
        .context("Failed to parse versions response")?;

    if versions.is_empty() {
        return Ok(None);
    }

    // Find the latest version
    let latest = versions
        .into_iter()
        .max_by(|a, b| {
            // Simple version comparison (works for semver-like versions)
            version_compare(&a.version, &b.version)
        })
        .unwrap();

    // Check if it's newer than current version
    let current_version = crate::VERSION;
    if version_compare(&latest.version, current_version) == std::cmp::Ordering::Greater {
        info!("Update available: {} -> {}", current_version, latest.version);
        Ok(Some(latest))
    } else {
        info!("Already on latest version: {}", current_version);
        Ok(None)
    }
}

/// Simple version comparison (semver-like)
fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<u32> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let b_parts: Vec<u32> = b.split('.').filter_map(|s| s.parse().ok()).collect();

    for i in 0..a_parts.len().max(b_parts.len()) {
        let a_val = a_parts.get(i).copied().unwrap_or(0);
        let b_val = b_parts.get(i).copied().unwrap_or(0);

        match a_val.cmp(&b_val) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    std::cmp::Ordering::Equal
}

/// Download the update package
pub async fn download_update(version_info: &VersionInfo) -> Result<PathBuf> {
    let download_url = version_info
        .urls
        .get(PLATFORM)
        .ok_or_else(|| anyhow::anyhow!("No download URL for platform: {}", PLATFORM))?;

    info!("Downloading update from: {}", download_url);

    let cache_dir = get_download_cache_dir()?;
    let zip_path = cache_dir.join(format!("kerr-{}.zip", version_info.version));

    // Download the file
    let client = reqwest::Client::new();
    let response = client
        .get(download_url)
        .send()
        .await
        .with_context(|| format!("Failed to download from {}", download_url))?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status: {}", response.status());
    }

    let mut file = File::create(&zip_path)
        .with_context(|| format!("Failed to create file at {}", zip_path.display()))?;

    let bytes = response
        .bytes()
        .await
        .context("Failed to read download response")?;

    file.write_all(&bytes)
        .with_context(|| format!("Failed to write to {}", zip_path.display()))?;

    info!("Downloaded update to: {}", zip_path.display());
    Ok(zip_path)
}

/// Extract the update package
pub fn extract_update(zip_path: &Path) -> Result<PathBuf> {
    let cache_dir = get_download_cache_dir()?;
    let extract_dir = cache_dir.join("extracted");

    // Remove old extracted directory if it exists
    if extract_dir.exists() {
        std::fs::remove_dir_all(&extract_dir)
            .with_context(|| format!("Failed to remove old extract dir at {}", extract_dir.display()))?;
    }

    std::fs::create_dir_all(&extract_dir)
        .with_context(|| format!("Failed to create extract dir at {}", extract_dir.display()))?;

    info!("Extracting update to: {}", extract_dir.display());

    let file = File::open(zip_path)
        .with_context(|| format!("Failed to open zip file at {}", zip_path.display()))?;

    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("Failed to read zip archive at {}", zip_path.display()))?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = extract_dir.join(file.name());

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;

            // Set executable permissions on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let permissions = std::fs::Permissions::from_mode(0o755);
                std::fs::set_permissions(&outpath, permissions)?;
            }
        }
    }

    info!("Extraction complete");
    Ok(extract_dir)
}

/// Replace the updater binary and launch it
pub fn launch_updater_and_exit(extract_dir: &Path, current_version: &str, new_version: &str) -> Result<()> {
    info!("Launching updater to replace main application (from {} to {})", current_version, new_version);

    let current_exe = env::current_exe().context("Failed to get current executable path")?;
    let current_updater = get_updater_path()?;

    #[cfg(target_os = "windows")]
    let updater_name = "kerr-updater.exe";
    #[cfg(not(target_os = "windows"))]
    let updater_name = "kerr-updater";

    #[cfg(target_os = "windows")]
    let main_name = "kerr.exe";
    #[cfg(not(target_os = "windows"))]
    let main_name = "kerr";

    let new_updater = extract_dir.join(updater_name);
    let new_main = extract_dir.join(main_name);

    // Verify both binaries exist
    if !new_updater.exists() {
        anyhow::bail!("Updater binary not found in update package at {}", new_updater.display());
    }
    if !new_main.exists() {
        anyhow::bail!("Main binary not found in update package at {}", new_main.display());
    }

    // Replace the updater binary
    info!("Replacing updater binary at {}", current_updater.display());
    std::fs::copy(&new_updater, &current_updater)
        .with_context(|| format!("Failed to replace updater at {}", current_updater.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&current_updater, permissions)?;
    }

    // Launch the updater
    info!("Launching updater: {:?}", current_updater);
    let status = Command::new(&current_updater)
        .arg(current_exe.to_str().unwrap_or_default()) // Path to the binary to replace
        .arg(main_name) // Name of the binary
        .arg(new_main.to_str().unwrap_or_default()) // Path to the new binary
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to launch updater at {:?}", current_updater))?;

    info!("Updater launched with PID: {:?}", status.id());
    info!("Main application exiting to allow update");

    // Exit the main application
    std::process::exit(0);
}

/// Perform the full update process
pub async fn perform_update(config: &ServerConfig) -> Result<()> {
    let current_version = crate::VERSION;
    info!("Starting update check (current version: {})", current_version);

    // Check if debug mode
    if is_debug_mode() {
        warn!("Running in debug mode - updates are disabled");
        anyhow::bail!("Updates are disabled in debug mode");
    }

    // Check for updates
    let version_info = match check_for_updates(config).await? {
        Some(info) => info,
        None => {
            info!("No updates available");
            return Ok(());
        }
    };

    let new_version = version_info.version.clone();
    info!("Update available: {} -> {}", current_version, new_version);

    // Download the update
    let zip_path = download_update(&version_info).await?;

    // Extract the update
    let extract_dir = extract_update(&zip_path)?;

    // Launch updater and exit
    launch_updater_and_exit(&extract_dir, current_version, &new_version)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compare() {
        assert_eq!(version_compare("1.0.0", "1.0.0"), std::cmp::Ordering::Equal);
        assert_eq!(version_compare("1.0.1", "1.0.0"), std::cmp::Ordering::Greater);
        assert_eq!(version_compare("1.0.0", "1.0.1"), std::cmp::Ordering::Less);
        assert_eq!(version_compare("2.0.0", "1.9.9"), std::cmp::Ordering::Greater);
        assert_eq!(version_compare("1.10.0", "1.2.0"), std::cmp::Ordering::Greater);
    }
}
