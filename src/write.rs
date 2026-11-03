//! One-call helpers to modify a binary (open → change → commit).
//!
//! Each call is atomic but rewrites the whole file; to apply many changes at
//! once, open a [`Bymm`] handle and commit only once.

use std::path::Path;

use crate::{Bymm, Options, Result};

/// Creates the entry `key` in the binary at `path`, or replaces its value.
///
/// The storage is created on first use.
pub fn write(path: impl AsRef<Path>, key: &str, data: impl AsRef<[u8]>) -> Result<()> {
    let mut b = Bymm::open(path)?;
    b.write(key, data)?;
    b.commit()
}

/// Adds `data` at the end of the value of `key` (creating it if needed).
pub fn append(path: impl AsRef<Path>, key: &str, data: impl AsRef<[u8]>) -> Result<()> {
    let mut b = Bymm::open(path)?;
    b.append(key, data)?;
    b.commit()
}

/// Removes `key`. Returns `true` if it existed.
pub fn delete(path: impl AsRef<Path>, key: &str) -> Result<bool> {
    let mut b = Bymm::open(path)?;
    let existed = b.delete(key)?;
    b.commit()?;
    Ok(existed)
}

/// Removes every entry, keeping the empty storage and its limit.
pub fn clear(path: impl AsRef<Path>) -> Result<()> {
    let mut b = Bymm::open(path)?;
    b.clear()?;
    b.commit()
}

/// Sets the maximum size of the embedded storage (in bytes, [`u64::MAX`] = unlimited).
pub fn set_limit(path: impl AsRef<Path>, bytes: u64) -> Result<()> {
    let mut b = Bymm::open_with(path, Options::default())?;
    b.set_limit(bytes)?;
    b.commit()
}

/// Removes the storage entirely, restoring the original binary byte for byte.
pub fn strip(path: impl AsRef<Path>) -> Result<()> {
    let mut b = Bymm::open(path)?;
    b.strip()?;
    b.commit()
}
