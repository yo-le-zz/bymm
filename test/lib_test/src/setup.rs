#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BYMM TEST SETUP ===");

    let directory = common::test_directory();
    if directory.exists() {
        fs::remove_dir_all(&directory)?;
    }
    fs::create_dir_all(&directory)?;

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fake_directory = manifest_dir.join("../fake");

    println!("\n[1] Building fake...");
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(&fake_directory)
        .status()?;
    if !status.success() {
        return Err("building fake failed".into());
    }

    // The workspace shares its target/ directory.
    println!("\n[2] Preparing the binary...");
    let source = manifest_dir
        .join("../target/release")
        .join(if cfg!(windows) { "fake.exe" } else { "fake" });
    if !source.exists() {
        return Err(format!("binary not found: {}", source.display()).into());
    }
    fs::copy(&source, common::fake_binary())?;

    println!("\n[3] Done: {}", common::fake_binary().display());
    Ok(())
}
