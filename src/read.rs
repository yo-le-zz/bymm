//! One-call helpers to read from a binary (open → read → close).

use std::path::Path;

use crate::{Bymm, Result};

/// Reads the value stored under `key` in the binary at `path`.
///
/// Returns `Ok(None)` if the binary has no storage or no such key.
///
/// ```
/// # let dir = std::env::temp_dir().join(format!("bymm-doc-read-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # let exe = dir.join("app.bin");
/// # std::fs::write(&exe, b"binary")?;
/// bymm::write(&exe, "color", "blue")?;
/// assert_eq!(bymm::read(&exe, "color")?.unwrap(), b"blue");
/// assert_eq!(bymm::read(&exe, "missing")?, None);
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), bymm::Error>(())
/// ```
pub fn read(path: impl AsRef<Path>, key: &str) -> Result<Option<Vec<u8>>> {
    Bymm::open(path)?.read(key)
}

/// Reads a value as UTF-8 text.
pub fn read_string(path: impl AsRef<Path>, key: &str) -> Result<Option<String>> {
    Bymm::open(path)?.read_string(key)
}

/// Lists the keys stored in the binary (alphabetical order).
pub fn keys(path: impl AsRef<Path>) -> Result<Vec<String>> {
    Ok(Bymm::open(path)?.keys().map(str::to_owned).collect())
}

/// `true` if `key` exists in the binary.
pub fn contains(path: impl AsRef<Path>, key: &str) -> Result<bool> {
    Ok(Bymm::open(path)?.contains(key))
}

/// Verifies the checksum of every entry stored in the binary.
pub fn verify(path: impl AsRef<Path>) -> Result<()> {
    Bymm::open(path)?.verify()
}
