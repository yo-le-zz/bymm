//! The [`Bymm`] handle: open a binary, inspect it, stage changes, commit them.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use crate::crc32::{Crc32, entry_crc};
use crate::detect::BinaryInfo;
use crate::error::{Error, Result, corrupt};
use crate::io::{At, read_exact_at};
use crate::store::{
    ENTRY_HEADER_LEN, Entry, FOOTER_LEN, FOOTER_MAGIC, HEADER_LEN, Source, UNLIMITED,
    copy_from_disk, encode_header, entry_size, locate, scan,
};
use crate::{DEFAULT_LIMIT, MAX_KEY_LEN, MIN_LIMIT};

/// Settings for [`Bymm::open_with`].
///
/// ```
/// use bymm::Options;
/// let opts = Options::new().default_limit(1024 * 1024).backup(true);
/// # let _ = opts;
/// ```
#[derive(Debug, Clone)]
pub struct Options {
    default_limit: u64,
    allow_signed: bool,
    backup: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            default_limit: DEFAULT_LIMIT,
            allow_signed: false,
            backup: false,
        }
    }
}

impl Options {
    /// Same as [`Options::default`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Size limit given to a storage that does not exist yet (default [`DEFAULT_LIMIT`]).
    ///
    /// Ignored when the binary already holds a storage: that one keeps its own limit.
    /// Use [`u64::MAX`] for "no limit".
    pub fn default_limit(mut self, bytes: u64) -> Self {
        self.default_limit = bytes;
        self
    }

    /// Allow writing into a digitally signed binary, which **invalidates the signature**.
    /// Refused by default with [`Error::SignedBinary`].
    pub fn allow_signed(mut self, yes: bool) -> Self {
        self.allow_signed = yes;
        self
    }

    /// Keep a copy of the previous binary next to it (`<name>.bymm.bak`) at each commit.
    pub fn backup(mut self, yes: bool) -> Self {
        self.backup = yes;
        self
    }
}

/// How full the embedded storage of a binary is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    /// Whether the binary currently holds a storage at all.
    pub has_storage: bool,
    /// Maximum size of the storage in bytes ([`u64::MAX`] when unlimited).
    pub limit: u64,
    /// Bytes the storage takes in the binary (header, entries and footer included).
    pub used: u64,
    /// Bytes that can still be added before reaching the limit.
    pub free: u64,
    /// Number of entries (keys).
    pub entries: usize,
}

impl Usage {
    /// `true` when there is no size limit.
    pub fn is_unlimited(&self) -> bool {
        self.limit == UNLIMITED
    }
}

impl fmt::Display for Usage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.has_storage {
            write!(
                f,
                "no storage (a new one would get a limit of {})",
                self.limit_text()
            )
        } else {
            write!(
                f,
                "{} entries, {} bytes used, {} bytes free, limit {}",
                self.entries,
                self.used,
                if self.is_unlimited() {
                    "unlimited".to_string()
                } else {
                    self.free.to_string()
                },
                self.limit_text()
            )
        }
    }
}

impl Usage {
    fn limit_text(&self) -> String {
        if self.is_unlimited() {
            "none".into()
        } else {
            format!("{} bytes", self.limit)
        }
    }
}

/// A binary opened for reading and editing its embedded storage.
///
/// Changes (`write`, `append`, `delete`, `clear`, `set_limit`, `strip`) are
/// **staged in memory**; nothing touches the file until [`commit`](Bymm::commit).
/// A commit rebuilds the binary in a temporary file next to it and swaps it in
/// atomically, so the original is never left half-written — and a program can
/// safely modify the very executable it is running from.
///
/// Reads always see the staged state.
///
/// ```
/// # let dir = std::env::temp_dir().join(format!("bymm-doc-handle-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # let exe = dir.join("app.bin");
/// # std::fs::write(&exe, b"pretend this is an executable")?;
/// let mut b = bymm::Bymm::open(&exe)?;
/// b.write("user", "yolezz")?;
/// b.append("log", "first line\n")?;
/// b.append("log", "second line\n")?;
/// b.commit()?;                                   // <- really written into the file
///
/// let b = bymm::Bymm::open(&exe)?;               // later, even from another process
/// assert_eq!(b.read_string("user")?.as_deref(), Some("yolezz"));
/// assert_eq!(b.read("log")?.unwrap(), b"first line\nsecond line\n");
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), bymm::Error>(())
/// ```
#[derive(Debug)]
pub struct Bymm {
    path: PathBuf,
    opts: Options,
    /// Read handle on the version of the file this state was loaded from.
    file: File,
    snapshot_len: u64,
    snapshot_mtime: Option<SystemTime>,
    info: BinaryInfo,
    /// Length of the original binary (everything before the storage).
    base_len: u64,
    present: bool,
    limit: u64,
    entries: BTreeMap<String, Entry>,
    entries_bytes: u64,
    dirty: bool,
}

impl Bymm {
    /// Opens a binary with default [`Options`].
    pub fn open(path: impl AsRef<Path>) -> Result<Bymm> {
        Self::open_with(path, Options::default())
    }

    /// Opens a binary.
    ///
    /// Any regular file works: the format is detected automatically and an
    /// unrecognised file is simply reported as [`Format::Unknown`](crate::Format::Unknown).
    pub fn open_with(path: impl AsRef<Path>, opts: Options) -> Result<Bymm> {
        if opts.default_limit < MIN_LIMIT {
            return Err(Error::InvalidLimit {
                requested: opts.default_limit,
                minimum: MIN_LIMIT,
            });
        }
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        Self::load(path, opts, file)
    }

    /// Opens the executable that is currently running.
    ///
    /// Reading is always possible. Writing works too — on Linux the old inode
    /// stays alive for the running process, on Windows the old file is renamed
    /// aside — but the *current* process keeps running the old image.
    pub fn open_self() -> Result<Bymm> {
        Self::open_self_with(Options::default())
    }

    /// Like [`open_self`](Bymm::open_self) with custom [`Options`].
    pub fn open_self_with(opts: Options) -> Result<Bymm> {
        let exe = std::env::current_exe()?;
        #[cfg(target_os = "linux")]
        let exe = {
            // After a commit, /proc/self/exe reads "<path> (deleted)".
            if fs::metadata(&exe).is_err() {
                exe.to_str()
                    .and_then(|s| s.strip_suffix(" (deleted)"))
                    .map(PathBuf::from)
                    .unwrap_or(exe)
            } else {
                exe
            }
        };
        Self::open_with(exe, opts)
    }

    fn load(path: PathBuf, opts: Options, file: File) -> Result<Bymm> {
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a regular file",
            )));
        }
        let file_len = meta.len();
        let info = BinaryInfo::detect(&mut At::new(&file, 0))?;

        let (present, base_len, limit, entries) = match locate(&file, file_len)? {
            Some(loc) => {
                let entries = scan(&file, &loc)?;
                (true, loc.region_start, loc.header.limit, entries)
            }
            None => (false, file_len, opts.default_limit, BTreeMap::new()),
        };
        let entries_bytes = entries.iter().map(|(k, e)| entry_size(k, e.len)).sum();

        Ok(Bymm {
            path,
            opts,
            file,
            snapshot_len: file_len,
            snapshot_mtime: meta.modified().ok(),
            info,
            base_len,
            present,
            limit,
            entries,
            entries_bytes,
            dirty: false,
        })
    }

    // ------------------------------------------------------------------ inspect

    /// Path this handle was opened with.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What was detected about the binary (format, architecture, bits…).
    ///
    /// Describes the file as it is on disk (refreshed at every commit).
    pub fn info(&self) -> &BinaryInfo {
        &self.info
    }

    /// Size in bytes of the binary without any embedded storage.
    pub fn original_len(&self) -> u64 {
        self.base_len
    }

    /// `true` if the binary holds (or, once committed, will hold) a storage.
    pub fn has_storage(&self) -> bool {
        self.present
    }

    /// `true` if there are staged changes that [`commit`](Bymm::commit) would write.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` if there is no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `true` if `key` exists.
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// All keys, in alphabetical order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// All `(key, value length in bytes)` pairs, in alphabetical order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, u64)> {
        self.entries.iter().map(|(k, e)| (k.as_str(), e.len))
    }

    /// Length in bytes of the value stored under `key`.
    pub fn value_len(&self, key: &str) -> Option<u64> {
        self.entries.get(key).map(|e| e.len)
    }

    // ------------------------------------------------------------------- limits

    /// The size limit of the storage in bytes ([`u64::MAX`] = unlimited).
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// `true` when the storage has no size limit.
    pub fn is_unlimited(&self) -> bool {
        self.limit == UNLIMITED
    }

    /// Bytes the storage takes (header, entries and footer), `0` if there is none.
    pub fn used(&self) -> u64 {
        if self.present {
            HEADER_LEN + FOOTER_LEN + self.entries_bytes
        } else {
            0
        }
    }

    /// Bytes that can still be added before hitting the limit.
    pub fn free(&self) -> u64 {
        let base = if self.present {
            self.used()
        } else {
            HEADER_LEN + FOOTER_LEN
        };
        self.limit.saturating_sub(base)
    }

    /// Snapshot of the limit and of the space used.
    pub fn usage(&self) -> Usage {
        Usage {
            has_storage: self.present,
            limit: self.limit,
            used: self.used(),
            free: self.free(),
            entries: self.entries.len(),
        }
    }

    /// The biggest value that [`write`](Bymm::write) would accept for `key` right now
    /// (replacing the existing value if there is one).
    pub fn max_value_len(&self, key: &str) -> Result<u64> {
        validate_key(key)?;
        let old = self
            .entries
            .get(key)
            .map(|e| entry_size(key, e.len))
            .unwrap_or(0);
        let others = HEADER_LEN + FOOTER_LEN + self.entries_bytes - old;
        let overhead = ENTRY_HEADER_LEN + key.len() as u64;
        Ok(self.limit.saturating_sub(others).saturating_sub(overhead))
    }

    /// Sets the maximum size of the storage, in bytes.
    ///
    /// It cannot be lower than what is already used (nor than [`MIN_LIMIT`]).
    /// The limit is saved inside the binary at the next commit.
    pub fn set_limit(&mut self, bytes: u64) -> Result<()> {
        self.guard()?;
        let minimum = MIN_LIMIT.max(self.used());
        if bytes < minimum {
            return Err(Error::InvalidLimit {
                requested: bytes,
                minimum,
            });
        }
        self.limit = bytes;
        self.present = true;
        self.dirty = true;
        Ok(())
    }

    /// Removes the size limit.
    pub fn set_unlimited(&mut self) -> Result<()> {
        self.set_limit(UNLIMITED)
    }

    // -------------------------------------------------------------------- reads

    /// Reads the value stored under `key`, checking its checksum.
    pub fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let Some(entry) = self.entries.get(key) else {
            return Ok(None);
        };
        match &entry.source {
            Source::Mem(data) => Ok(Some(data.clone())),
            Source::Disk { offset } => {
                let len = usize::try_from(entry.len)
                    .map_err(|_| corrupt("value too large for this platform's memory"))?;
                let data = read_exact_at(&self.file, *offset, len)?;
                if entry_crc(key.as_bytes(), &data) != entry.crc {
                    return Err(corrupt(format!("checksum mismatch for key {key:?}")));
                }
                Ok(Some(data))
            }
        }
    }

    /// Reads a value as UTF-8 text.
    pub fn read_string(&self, key: &str) -> Result<Option<String>> {
        match self.read(key)? {
            None => Ok(None),
            Some(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| {
                Error::Io(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "value is not valid UTF-8",
                ))
            }),
        }
    }

    /// Streams a value into `out` without loading it in memory.
    ///
    /// Returns the number of bytes written, or `None` if the key does not exist.
    /// The checksum is verified at the end; on mismatch the data has already been
    /// written to `out` and [`Error::Corrupt`] is returned.
    pub fn read_to<W: Write>(&self, key: &str, out: &mut W) -> Result<Option<u64>> {
        let Some(entry) = self.entries.get(key) else {
            return Ok(None);
        };
        match &entry.source {
            Source::Mem(data) => out.write_all(data)?,
            Source::Disk { offset } => {
                let mut crc = Crc32::new();
                crc.update(key.as_bytes());
                copy_from_disk(&self.file, *offset, entry.len, out, Some(&mut crc))?;
                if crc.finish() != entry.crc {
                    return Err(corrupt(format!("checksum mismatch for key {key:?}")));
                }
            }
        }
        Ok(Some(entry.len))
    }

    /// Checks the checksum of every entry stored in the binary.
    pub fn verify(&self) -> Result<()> {
        for (key, entry) in &self.entries {
            if let Source::Disk { offset } = &entry.source {
                let mut crc = Crc32::new();
                crc.update(key.as_bytes());
                copy_from_disk(
                    &self.file,
                    *offset,
                    entry.len,
                    &mut io::sink(),
                    Some(&mut crc),
                )?;
                if crc.finish() != entry.crc {
                    return Err(corrupt(format!("checksum mismatch for key {key:?}")));
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------- writes

    /// Creates the entry `key`, or replaces its value.
    pub fn write(&mut self, key: &str, data: impl AsRef<[u8]>) -> Result<()> {
        validate_key(key)?;
        self.guard()?;
        self.put(key, data.as_ref().to_vec())
    }

    /// Adds `data` at the end of the value of `key` (creating the entry if needed).
    pub fn append(&mut self, key: &str, data: impl AsRef<[u8]>) -> Result<()> {
        validate_key(key)?;
        self.guard()?;
        let extra = data.as_ref();
        let old_len = self.entries.get(key).map(|e| e.len).unwrap_or(0);
        let new_len = old_len
            .checked_add(extra.len() as u64)
            .ok_or(Error::LimitExceeded {
                limit: self.limit,
                required: u64::MAX,
            })?;
        self.check_fits(key, new_len)?; // fail before loading a big value
        let mut buf = self.read(key)?.unwrap_or_default();
        buf.extend_from_slice(extra);
        self.put(key, buf)
    }

    /// Removes `key`. Returns `true` if it existed.
    pub fn delete(&mut self, key: &str) -> Result<bool> {
        self.guard()?;
        match self.entries.remove(key) {
            Some(e) => {
                self.entries_bytes -= entry_size(key, e.len);
                self.dirty = true;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Removes every entry but keeps the (now empty) storage and its limit.
    pub fn clear(&mut self) -> Result<()> {
        self.guard()?;
        if self.present {
            self.entries.clear();
            self.entries_bytes = 0;
            self.dirty = true;
        }
        Ok(())
    }

    /// Removes the storage completely: after [`commit`](Bymm::commit) the file is
    /// byte-for-byte the original binary again, limit included.
    pub fn strip(&mut self) -> Result<()> {
        self.guard()?;
        if self.present {
            self.entries.clear();
            self.entries_bytes = 0;
            self.present = false;
            self.limit = self.opts.default_limit;
            self.dirty = true;
        }
        Ok(())
    }

    /// Drops every staged change and reloads the binary from disk.
    pub fn discard(&mut self) -> Result<()> {
        self.reload()
    }

    fn guard(&self) -> Result<()> {
        if self.info.signed && !self.opts.allow_signed {
            return Err(Error::SignedBinary);
        }
        Ok(())
    }

    /// Checks that the storage would still fit if `key` held `new_len` bytes.
    fn check_fits(&self, key: &str, new_len: u64) -> Result<()> {
        let old = self
            .entries
            .get(key)
            .map(|e| entry_size(key, e.len))
            .unwrap_or(0);
        let required = (HEADER_LEN + FOOTER_LEN)
            .checked_add(self.entries_bytes - old)
            .and_then(|v| v.checked_add(entry_size(key, new_len)))
            .unwrap_or(u64::MAX);
        if required > self.limit {
            return Err(Error::LimitExceeded {
                limit: self.limit,
                required,
            });
        }
        Ok(())
    }

    fn put(&mut self, key: &str, data: Vec<u8>) -> Result<()> {
        let len = data.len() as u64;
        self.check_fits(key, len)?;
        let old = self
            .entries
            .get(key)
            .map(|e| entry_size(key, e.len))
            .unwrap_or(0);
        let crc = entry_crc(key.as_bytes(), &data);
        self.entries.insert(
            key.to_string(),
            Entry {
                len,
                crc,
                source: Source::Mem(data),
            },
        );
        self.entries_bytes = self.entries_bytes - old + entry_size(key, len);
        self.present = true;
        self.dirty = true;
        Ok(())
    }

    // ------------------------------------------------------------------- commit

    /// Writes the staged changes into the binary.
    ///
    /// The new binary is built in a temporary file in the same directory, flushed
    /// to disk, given the permissions of the original, then swapped in. If anything
    /// fails the original file is left untouched. Needs write access to the
    /// *directory* of the binary.
    ///
    /// Returns [`Error::Modified`] if someone else changed the file since it was opened.
    pub fn commit(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let target = fs::canonicalize(&self.path)?; // write through symlinks
        let current = fs::metadata(&target)?;
        if current.len() != self.snapshot_len || current.modified().ok() != self.snapshot_mtime {
            return Err(Error::Modified);
        }
        let dir = target.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "binary has no parent directory",
            )
        })?;

        let (mut tmp, tmp_path) = create_temp(dir)?;
        let outcome = self
            .build(&mut tmp)
            .and_then(|()| tmp.sync_all().map_err(Error::Io))
            .map(|()| drop(tmp))
            .and_then(|()| fs::set_permissions(&tmp_path, current.permissions()).map_err(Error::Io))
            .and_then(|()| {
                if self.opts.backup {
                    fs::copy(&target, backup_path(&target)).map_err(Error::Io)?;
                }
                replace_file(&tmp_path, &target).map_err(Error::Io)
            });
        if let Err(e) = outcome {
            let _ = fs::remove_file(&tmp_path);
            return Err(e);
        }
        self.reload()
    }

    /// Writes original binary + storage into `tmp`.
    fn build(&self, tmp: &mut File) -> Result<()> {
        let mut out = BufWriter::with_capacity(1 << 20, tmp);
        copy_from_disk(&self.file, 0, self.base_len, &mut out, None)?;
        if self.present {
            out.write_all(&encode_header(self.entries.len() as u64, self.limit))?;
            for (key, entry) in &self.entries {
                let mut eh = [0u8; ENTRY_HEADER_LEN as usize];
                eh[0..4].copy_from_slice(&(key.len() as u32).to_le_bytes());
                eh[4..12].copy_from_slice(&entry.len.to_le_bytes());
                eh[12..16].copy_from_slice(&entry.crc.to_le_bytes());
                out.write_all(&eh)?;
                out.write_all(key.as_bytes())?;
                match &entry.source {
                    Source::Mem(data) => out.write_all(data)?,
                    Source::Disk { offset } => {
                        copy_from_disk(&self.file, *offset, entry.len, &mut out, None)?
                    }
                }
            }
            out.write_all(&(HEADER_LEN + self.entries_bytes).to_le_bytes())?;
            out.write_all(&FOOTER_MAGIC)?;
        }
        out.flush()?;
        Ok(())
    }

    fn reload(&mut self) -> Result<()> {
        let file = File::open(&self.path)?;
        let fresh = Self::load(self.path.clone(), self.opts.clone(), file)?;
        *self = fresh;
        Ok(())
    }
}

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() {
        Err(Error::InvalidKey("the key cannot be empty"))
    } else if key.len() > MAX_KEY_LEN {
        Err(Error::InvalidKey(
            "the key is longer than MAX_KEY_LEN bytes",
        ))
    } else {
        Ok(())
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn create_temp(dir: &Path) -> io::Result<(File, PathBuf)> {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for _ in 0..32 {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(".bymm-tmp-{}-{nanos}-{n}", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => return Ok((f, path)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create a temporary file",
    ))
}

fn sibling(target: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(target.file_name().unwrap_or_default());
    name.push(suffix);
    target.with_file_name(name)
}

fn backup_path(target: &Path) -> PathBuf {
    let mut name = OsString::from(target.file_name().unwrap_or_default());
    name.push(".bymm.bak");
    target.with_file_name(name)
}

/// Atomically puts `tmp` in place of `target`.
///
/// Windows refuses to overwrite a running `.exe` but allows renaming it, so on
/// failure there we move the old file aside first.
fn replace_file(tmp: &Path, target: &Path) -> io::Result<()> {
    match fs::rename(tmp, target) {
        Ok(()) => Ok(()),
        Err(first) if cfg!(windows) => {
            let aside = sibling(target, ".bymm-old");
            let _ = fs::remove_file(&aside); // leftover of a previous commit
            if fs::rename(target, &aside).is_err() {
                return Err(first);
            }
            if let Err(e) = fs::rename(tmp, target) {
                let _ = fs::rename(&aside, target); // roll back
                return Err(e);
            }
            let _ = fs::remove_file(&aside); // fails while the old image still runs: harmless
            Ok(())
        }
        Err(e) => Err(e),
    }
}
