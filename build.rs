use std::path::Path;
use std::process::Command;

fn main() {
    let frontend_dir = Path::new("frontend");
    let dist_dir = frontend_dir.join("dist");

    // If frontend directory doesn't exist, skip building
    if !frontend_dir.exists() {
        println!("cargo:warning=Frontend directory not found, skipping frontend build");
        return;
    }

    // Check if npm is available
    let npm_cmd = if cfg!(target_os = "windows") {
        "npm.cmd"
    } else {
        "npm"
    };

    if which::which(npm_cmd).is_err() {
        println!("cargo:warning=npm not found in PATH, skipping frontend build");
        println!("cargo:warning=Install Node.js and npm to build the frontend");
        return;
    }

    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/vite.config.ts");
    println!("cargo:rerun-if-changed=frontend/tsconfig.json");
    println!("cargo:rerun-if-changed=frontend/index.html");

    // Always run npm install to ensure dependencies are up to date
    // npm install is fast if nothing changed, so this is safe
    println!("cargo:warning=Installing frontend dependencies...");
    let status = Command::new(npm_cmd)
        .arg("install")
        .current_dir(frontend_dir)
        .status()
        .expect("Failed to run npm install");

    if !status.success() {
        panic!("npm install failed");
    }

    // Build the frontend
    println!("cargo:warning=Building frontend...");
    let status = Command::new(npm_cmd)
        .arg("run")
        .arg("build")
        .current_dir(frontend_dir)
        .status()
        .expect("Failed to run npm run build");

    if !status.success() {
        panic!("Frontend build failed");
    }

    // Verify dist directory was created
    if !dist_dir.exists() {
        panic!("Frontend build succeeded but dist directory not found");
    }

    println!("cargo:warning=Frontend build completed successfully");
}
