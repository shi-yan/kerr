use std::env;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use fs2::FileExt;

fn main() {
    println!("[Kerr Updater] Starting...");

    let args: Vec<String> = env::args().collect();

    if args.len() < 4 {
        eprintln!("[Kerr Updater] Error: Insufficient arguments");
        eprintln!("Usage: kerr-updater <path_to_old_binary> <binary_name> <path_to_new_binary>");
        eprintln!("  path_to_old_binary: Full path to the current kerr executable to be replaced");
        eprintln!("  binary_name: Name of the binary (e.g., 'kerr' or 'kerr.exe')");
        eprintln!("  path_to_new_binary: Full path to the new kerr executable");
        std::process::exit(1);
    }

    let old_binary_path = PathBuf::from(&args[1]);
    let binary_name = &args[2];
    let new_binary_path = PathBuf::from(&args[3]);

    println!("[Kerr Updater] Update process:");
    println!("  Old binary: {}", old_binary_path.display());
    println!("  New binary: {}", new_binary_path.display());
    println!("  Binary name: {}", binary_name);

    // Verify the new binary exists
    if !new_binary_path.exists() {
        eprintln!("[Kerr Updater] Error: New binary not found at {}", new_binary_path.display());
        std::process::exit(1);
    }

    // Wait for the old process to release the file lock
    println!("[Kerr Updater] Waiting for old process to exit...");
    wait_for_file_lock(&old_binary_path);

    // Give it a bit more time to be safe
    sleep(Duration::from_millis(500));

    // Perform the replacement
    println!("[Kerr Updater] Replacing binary...");
    match replace_binary(&old_binary_path, &new_binary_path) {
        Ok(_) => {
            println!("[Kerr Updater] ✓ Binary replaced successfully");
        }
        Err(e) => {
            eprintln!("[Kerr Updater] ✗ Failed to replace binary: {}", e);
            eprintln!("[Kerr Updater] The old process may still be running or file permissions may be incorrect");
            std::process::exit(1);
        }
    }

    // Relaunch the new application
    println!("[Kerr Updater] Relaunching application: {}", binary_name);
    match relaunch_application(&old_binary_path) {
        Ok(_) => {
            println!("[Kerr Updater] ✓ Application relaunched successfully");
            println!("[Kerr Updater] Update complete. Exiting updater.");
        }
        Err(e) => {
            eprintln!("[Kerr Updater] ✗ Failed to relaunch application: {}", e);
            eprintln!("[Kerr Updater] You may need to start the application manually");
            std::process::exit(1);
        }
    }
}

/// Wait for the file lock on the binary to be released
fn wait_for_file_lock(path: &PathBuf) {
    let max_wait = Duration::from_secs(30);
    let wait_interval = Duration::from_millis(100);
    let mut total_wait = Duration::from_secs(0);

    loop {
        // Try to acquire an exclusive lock
        match OpenOptions::new().write(true).open(path) {
            Ok(file) => {
                match file.try_lock_exclusive() {
                    Ok(_) => {
                        println!("[Kerr Updater] ✓ Lock released after {:?}", total_wait);
                        // Unlock before proceeding
                        let _ = file.unlock();
                        break;
                    }
                    Err(_) => {
                        // File is still locked
                    }
                }
            }
            Err(e) if e.kind() == ErrorKind::PermissionDenied => {
                // On Windows, the OS often denies opening the file while it's being executed
                // This is expected, continue waiting
            }
            Err(e) => {
                eprintln!("[Kerr Updater] Warning: Unexpected error while checking lock: {}", e);
                // Continue anyway, the rename will fail if the lock is still held
                break;
            }
        }

        if total_wait >= max_wait {
            eprintln!("[Kerr Updater] Warning: Exceeded max wait time of {:?}", max_wait);
            eprintln!("[Kerr Updater] Proceeding with replacement attempt anyway");
            break;
        }

        sleep(wait_interval);
        total_wait += wait_interval;

        // Print progress dots
        if total_wait.as_millis() % 1000 == 0 {
            print!(".");
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
    }

    if total_wait > Duration::from_secs(0) {
        println!(); // New line after progress dots
    }
}

/// Replace the old binary with the new one
fn replace_binary(old_path: &PathBuf, new_path: &PathBuf) -> Result<(), String> {
    // On Windows, we can't replace a running executable directly
    // So we rename the old one first, then copy the new one
    #[cfg(target_os = "windows")]
    {
        let backup_path = old_path.with_extension("old");

        // Remove old backup if exists
        if backup_path.exists() {
            fs::remove_file(&backup_path)
                .map_err(|e| format!("Failed to remove old backup: {}", e))?;
        }

        // Rename old binary
        fs::rename(old_path, &backup_path)
            .map_err(|e| format!("Failed to rename old binary: {}", e))?;

        // Copy new binary to old location
        fs::copy(new_path, old_path)
            .map_err(|e| {
                // Try to restore the backup
                let _ = fs::rename(&backup_path, old_path);
                format!("Failed to copy new binary: {}", e)
            })?;

        // Clean up backup
        let _ = fs::remove_file(&backup_path);

        Ok(())
    }

    // On Unix, we can replace the file directly (process keeps the old inode open)
    #[cfg(not(target_os = "windows"))]
    {
        fs::copy(new_path, old_path)
            .map_err(|e| format!("Failed to copy new binary: {}", e))?;

        // Ensure executable permissions
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o755);
        fs::set_permissions(old_path, permissions)
            .map_err(|e| format!("Failed to set permissions: {}", e))?;

        Ok(())
    }
}

/// Relaunch the application
fn relaunch_application(path: &PathBuf) -> Result<(), String> {
    Command::new(path)
        .spawn()
        .map_err(|e| format!("Failed to spawn process: {}", e))?;

    Ok(())
}
