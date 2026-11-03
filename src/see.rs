//! One-call helpers to *see* what a binary is and how full its storage is.

use std::fs::File;
use std::path::Path;

use crate::io::At;
use crate::{BinaryInfo, Bymm, Result, Usage};

/// Detects the format and architecture of any file, without loading the storage.
///
/// ```
/// # let dir = std::env::temp_dir().join(format!("bymm-doc-see-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # let exe = dir.join("data.bin");
/// # std::fs::write(&exe, b"not an executable")?;
/// let info = bymm::see::info(&exe)?;
/// assert_eq!(info.format, bymm::Format::Unknown);   // still usable!
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), bymm::Error>(())
/// ```
pub fn info(path: impl AsRef<Path>) -> Result<BinaryInfo> {
    let file = File::open(path)?;
    Ok(BinaryInfo::detect(&mut At::new(&file, 0))?)
}

/// Limit, used and free space of the storage inside the binary.
pub fn usage(path: impl AsRef<Path>) -> Result<Usage> {
    Ok(Bymm::open(path)?.usage())
}

/// The size limit of the storage in bytes ([`u64::MAX`] = unlimited).
///
/// For a binary without storage this is the limit a new storage would get.
pub fn limit(path: impl AsRef<Path>) -> Result<u64> {
    Ok(Bymm::open(path)?.limit())
}

/// Bytes still available in the storage before hitting the limit.
pub fn free(path: impl AsRef<Path>) -> Result<u64> {
    Ok(Bymm::open(path)?.free())
}
