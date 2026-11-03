//! # bymm
//!
//! Store data **inside any executable** — and read it back, edit it, erase it.
//!
//! `bymm` appends a small, checksummed key/value storage to the end of a binary
//! and finds it again by looking at the end of the file. Because executable
//! loaders ignore what comes after the end of the image, the program keeps
//! running exactly as before, on **every CPU architecture**, on Linux and
//! Windows (and macOS), with no dependency.
//!
//! * **Auto-detection** of the format (ELF, PE, Mach-O) and the architecture
//!   (x86, x86_64, ARM, AArch64, RISC-V, MIPS, PowerPC, s390x, LoongArch…) —
//!   see [`BinaryInfo`]. Unknown files work too.
//! * **Write / append / read / delete / clear / strip** — see [`Bymm`] and the
//!   one-call helpers [`write()`], [`append()`], [`read()`], [`delete()`],
//!   [`clear()`], [`strip()`].
//! * **A size limit** you can set and look at: [`Bymm::set_limit`],
//!   [`Bymm::limit`], [`Bymm::free`], [`Bymm::max_value_len`], [`see::usage`].
//! * **Crash-safe**: every commit builds the new binary aside and swaps it in
//!   atomically; checksums catch corruption.
//! * **Self-modifying programs**: [`Bymm::open_self`] lets an executable read
//!   and rewrite its own embedded data.
//!
//! # Quick start
//!
//! ```
//! # let dir = std::env::temp_dir().join(format!("bymm-doc-lib-{}", std::process::id()));
//! # std::fs::create_dir_all(&dir)?;
//! # let exe = dir.join("my-app");
//! # std::fs::write(&exe, b"imagine a real executable here")?;
//! // One call per action…
//! bymm::write(&exe, "username", "yolezz")?;
//! bymm::append(&exe, "history", "hello\n")?;
//! assert_eq!(bymm::read_string(&exe, "username")?.as_deref(), Some("yolezz"));
//!
//! // …or a handle, to batch many changes into a single rewrite.
//! let mut b = bymm::Bymm::open(&exe)?;
//! println!("{} / {}", b.info().format, b.info().arch());   // e.g. "ELF / x86_64"
//! b.set_limit(1024 * 1024)?;                               // at most 1 MiB in the binary
//! b.write("config", br#"{"theme":"dark"}"#)?;
//! b.delete("history")?;
//! b.commit()?;                                             // <- nothing is written before this
//!
//! assert_eq!(bymm::see::limit(&exe)?, 1024 * 1024);
//! bymm::strip(&exe)?;                                      // back to the pristine binary
//! assert_eq!(std::fs::read(&exe)?, b"imagine a real executable here");
//! # std::fs::remove_dir_all(&dir)?;
//! # Ok::<(), bymm::Error>(())
//! ```
//!
//! # Good to know
//!
//! * **Digital signatures**: changing a signed binary invalidates its signature
//!   (Authenticode, macOS code signing). `bymm` refuses by default
//!   ([`Error::SignedBinary`]); opt in with [`Options::allow_signed`].
//! * **Post-processing tools** such as `strip`, `upx` or `objcopy` rewrite the
//!   file and drop data they do not know: embed *after* them.
//! * **Concurrency**: commits from several processes at the same time are not
//!   merged; the loser gets [`Error::Modified`].
//! * **Permissions**: a commit needs write access to the binary's *directory*
//!   (it swaps files); the owner and extended attributes of the file are not
//!   carried over, its permission bits are.
//!
//! The on-disk layout is specified in `docs/FORMAT.md`.

#![warn(missing_docs)]

mod bymm;
mod crc32;
mod detect;
mod error;
mod io;
pub mod read;
pub mod see;
mod store;
pub mod write;

pub use crate::bymm::{Bymm, Options, Usage};
pub use crate::detect::{Arch, BinaryInfo, Endian, Format};
pub use crate::error::{Error, Result};
pub use crate::read::{contains, keys, read, read_string, verify};
pub use crate::write::{append, clear, delete, set_limit, strip, write};

/// Limit given to a new storage unless you choose another one: 64 MiB.
pub const DEFAULT_LIMIT: u64 = 64 * 1024 * 1024;

/// Smallest possible limit: an empty storage (header + footer) takes 48 bytes.
pub const MIN_LIMIT: u64 = 48;

/// Longest key accepted, in bytes.
pub const MAX_KEY_LEN: usize = 1024;

/// Version of the on-disk storage format written by this crate.
pub const FORMAT_VERSION: u32 = store::FORMAT_VERSION;
