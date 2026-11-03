//! Error type of the crate.

use std::fmt;
use std::io;

/// Convenient result alias used by the whole crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong while using `bymm`.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// An operating-system error (file not found, permission denied, disk full…).
    Io(io::Error),
    /// The key is empty or longer than [`MAX_KEY_LEN`](crate::MAX_KEY_LEN) bytes.
    InvalidKey(&'static str),
    /// The requested key does not exist in the storage.
    KeyNotFound(String),
    /// The operation would make the embedded storage bigger than its limit.
    LimitExceeded {
        /// The limit currently in force, in bytes.
        limit: u64,
        /// The size the storage would need to have, in bytes.
        required: u64,
    },
    /// The limit asked for cannot be applied.
    InvalidLimit {
        /// The limit that was requested.
        requested: u64,
        /// The smallest limit that is currently acceptable.
        minimum: u64,
    },
    /// The embedded storage exists but is damaged (bad structure or checksum).
    Corrupt(String),
    /// The storage was written by a newer, incompatible version of `bymm`.
    UnsupportedVersion(u32),
    /// The binary carries a digital signature that writing would invalidate.
    ///
    /// Use [`Options::allow_signed`](crate::Options::allow_signed) to write anyway.
    SignedBinary,
    /// The binary was changed by someone else since it was opened.
    Modified,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::InvalidKey(why) => write!(f, "invalid key: {why}"),
            Error::KeyNotFound(k) => write!(f, "key not found: {k:?}"),
            Error::LimitExceeded { limit, required } => write!(
                f,
                "storage limit exceeded: {required} bytes needed but the limit is {limit} bytes"
            ),
            Error::InvalidLimit { requested, minimum } => write!(
                f,
                "invalid limit {requested}: it must be at least {minimum} bytes"
            ),
            Error::Corrupt(why) => write!(f, "corrupt bymm storage: {why}"),
            Error::UnsupportedVersion(v) => {
                write!(
                    f,
                    "unsupported bymm storage version {v} (this build understands version 1)"
                )
            }
            Error::SignedBinary => write!(
                f,
                "the binary is digitally signed and writing would invalidate its signature"
            ),
            Error::Modified => write!(
                f,
                "the binary was modified by another process since it was opened"
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<Error> for io::Error {
    fn from(e: Error) -> Self {
        let kind = match &e {
            Error::Io(inner) => return io::Error::new(inner.kind(), e),
            Error::InvalidKey(_) | Error::InvalidLimit { .. } => io::ErrorKind::InvalidInput,
            Error::KeyNotFound(_) => io::ErrorKind::NotFound,
            Error::LimitExceeded { .. } => io::ErrorKind::StorageFull,
            Error::Corrupt(_) | Error::UnsupportedVersion(_) => io::ErrorKind::InvalidData,
            Error::SignedBinary => io::ErrorKind::PermissionDenied,
            Error::Modified => io::ErrorKind::Other,
        };
        io::Error::new(kind, e)
    }
}

pub(crate) fn corrupt(why: impl Into<String>) -> Error {
    Error::Corrupt(why.into())
}
