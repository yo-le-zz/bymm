#![allow(dead_code)]

use std::path::PathBuf;

pub fn test_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data")
}

/// The fake executable, copied here by `setup`.
pub fn fake_binary() -> PathBuf {
    test_directory().join(if cfg!(windows) { "fake.exe" } else { "fake" })
}

/// A fresh copy of the fake executable that the current test may modify.
pub fn working_copy(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let source = fake_binary();
    if !source.exists() {
        return Err("fake binary not found: run `cargo run -p lib_test --bin setup` first".into());
    }
    let copy = test_directory().join(if cfg!(windows) { format!("{name}.exe") } else { name.to_string() });
    std::fs::copy(&source, &copy)?;
    Ok(copy)
}

pub fn check(condition: bool, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    if condition { Ok(()) } else { Err(format!("CHECK FAILED: {message}").into()) }
}
