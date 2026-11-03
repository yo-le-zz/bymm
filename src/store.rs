//! On-disk layout of the embedded storage (format version 1, always little-endian).
//!
//! ```text
//! [ original binary ... ][ header 32 ][ entry ]*[ footer 16 ]
//! ```
//!
//! See `docs/FORMAT.md` for the full specification.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufReader, Read};

use crate::crc32::Crc32;
use crate::error::{Error, Result, corrupt};
use crate::io::{At, read_exact_at};

pub(crate) const HEADER_MAGIC: [u8; 4] = *b"BYMM";
pub(crate) const FOOTER_MAGIC: [u8; 8] = *b"BYMMEND\0";
pub(crate) const FORMAT_VERSION: u32 = 1;

pub(crate) const HEADER_LEN: u64 = 32;
pub(crate) const ENTRY_HEADER_LEN: u64 = 16;
pub(crate) const FOOTER_LEN: u64 = 16;

/// Stored in the header when the storage has no size limit.
pub(crate) const UNLIMITED: u64 = u64::MAX;

/// Where the bytes of a value currently live.
#[derive(Debug)]
pub(crate) enum Source {
    /// Inside the opened binary, at this absolute offset.
    Disk { offset: u64 },
    /// Staged in memory, not committed yet.
    Mem(Vec<u8>),
}

#[derive(Debug)]
pub(crate) struct Entry {
    pub len: u64,
    pub crc: u32,
    pub source: Source,
}

/// Total on-disk size of one entry.
pub(crate) fn entry_size(key: &str, data_len: u64) -> u64 {
    ENTRY_HEADER_LEN + key.len() as u64 + data_len
}

pub(crate) struct Header {
    pub entry_count: u64,
    pub limit: u64,
}

pub(crate) fn encode_header(entry_count: u64, limit: u64) -> [u8; HEADER_LEN as usize] {
    let mut h = [0u8; HEADER_LEN as usize];
    h[0..4].copy_from_slice(&HEADER_MAGIC);
    h[4..8].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    h[8..16].copy_from_slice(&entry_count.to_le_bytes());
    h[16..24].copy_from_slice(&limit.to_le_bytes());
    let mut crc = Crc32::new();
    crc.update(&h[0..24]);
    h[24..28].copy_from_slice(&crc.finish().to_le_bytes());
    h
}

fn decode_header(h: &[u8]) -> Result<Header> {
    if h[0..4] != HEADER_MAGIC {
        return Err(corrupt("footer found but the header magic is missing"));
    }
    let version = u32::from_le_bytes(h[4..8].try_into().unwrap());
    if version != FORMAT_VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let mut crc = Crc32::new();
    crc.update(&h[0..24]);
    if crc.finish() != u32::from_le_bytes(h[24..28].try_into().unwrap()) {
        return Err(corrupt("header checksum mismatch"));
    }
    Ok(Header {
        entry_count: u64::from_le_bytes(h[8..16].try_into().unwrap()),
        limit: u64::from_le_bytes(h[16..24].try_into().unwrap()),
    })
}

/// A storage found at the end of a file.
pub(crate) struct Located {
    /// Absolute offset of the header = length of the original binary.
    pub region_start: u64,
    /// Header + entries, without the footer.
    pub region_len: u64,
    pub header: Header,
}

/// Looks for a storage at the very end of the file.
///
/// `Ok(None)` means "no footer magic": the file simply has no storage.
pub(crate) fn locate(file: &File, file_len: u64) -> Result<Option<Located>> {
    if file_len < FOOTER_LEN {
        return Ok(None);
    }
    let footer = read_exact_at(file, file_len - FOOTER_LEN, FOOTER_LEN as usize)?;
    if footer[8..16] != FOOTER_MAGIC {
        return Ok(None);
    }
    let region_len = u64::from_le_bytes(footer[0..8].try_into().unwrap());
    if region_len < HEADER_LEN || region_len > file_len - FOOTER_LEN {
        return Err(corrupt("footer points outside of the file"));
    }
    let region_start = file_len - FOOTER_LEN - region_len;
    let header = decode_header(&read_exact_at(file, region_start, HEADER_LEN as usize)?)?;
    Ok(Some(Located {
        region_start,
        region_len,
        header,
    }))
}

fn eof_is_corrupt(e: io::Error) -> Error {
    if e.kind() == io::ErrorKind::UnexpectedEof {
        corrupt("storage ends in the middle of an entry")
    } else {
        Error::Io(e)
    }
}

/// Walks every entry (skipping the data) and builds the index.
pub(crate) fn scan(file: &File, loc: &Located) -> Result<BTreeMap<String, Entry>> {
    let mut rdr = BufReader::with_capacity(64 * 1024, At::new(file, loc.region_start + HEADER_LEN));
    let mut entries = BTreeMap::new();
    let mut pos = HEADER_LEN; // relative to region_start

    for _ in 0..loc.header.entry_count {
        let mut eh = [0u8; ENTRY_HEADER_LEN as usize];
        rdr.read_exact(&mut eh).map_err(eof_is_corrupt)?;
        let key_len = u32::from_le_bytes(eh[0..4].try_into().unwrap()) as u64;
        let data_len = u64::from_le_bytes(eh[4..12].try_into().unwrap());
        let crc = u32::from_le_bytes(eh[12..16].try_into().unwrap());

        if key_len == 0 || key_len > crate::MAX_KEY_LEN as u64 {
            return Err(corrupt("entry with an invalid key length"));
        }
        let total = ENTRY_HEADER_LEN
            .checked_add(key_len)
            .and_then(|v| v.checked_add(data_len))
            .and_then(|v| v.checked_add(pos))
            .filter(|&end| end <= loc.region_len)
            .ok_or_else(|| corrupt("entry larger than the storage"))?;

        let mut key = vec![0u8; key_len as usize];
        rdr.read_exact(&mut key).map_err(eof_is_corrupt)?;
        let key = String::from_utf8(key).map_err(|_| corrupt("key is not valid UTF-8"))?;

        let data_offset = loc.region_start + pos + ENTRY_HEADER_LEN + key_len;
        rdr.seek_relative(i64::try_from(data_len).map_err(|_| corrupt("entry too large"))?)
            .map_err(Error::Io)?;

        let entry = Entry {
            len: data_len,
            crc,
            source: Source::Disk {
                offset: data_offset,
            },
        };
        if entries.insert(key, entry).is_some() {
            return Err(corrupt("duplicate key"));
        }
        pos = total;
    }
    if pos != loc.region_len {
        return Err(corrupt("unexpected bytes after the last entry"));
    }
    Ok(entries)
}

/// Copies `len` bytes found at `offset` to `out`, optionally hashing them.
pub(crate) fn copy_from_disk<W: io::Write + ?Sized>(
    file: &File,
    offset: u64,
    len: u64,
    out: &mut W,
    mut crc: Option<&mut Crc32>,
) -> Result<()> {
    let mut at = At::new(file, offset);
    let mut buf = vec![0u8; 256 * 1024];
    let mut left = len;
    while left > 0 {
        let want = left.min(buf.len() as u64) as usize;
        let n = at.read(&mut buf[..want])?;
        if n == 0 {
            return Err(Error::Io(io::ErrorKind::UnexpectedEof.into()));
        }
        if let Some(c) = crc.as_deref_mut() {
            c.update(&buf[..n]);
        }
        out.write_all(&buf[..n])?;
        left -= n as u64;
    }
    Ok(())
}
