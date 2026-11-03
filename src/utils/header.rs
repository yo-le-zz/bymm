use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

use super::bytes::{read_u32, read_u64, write_u32, write_u64};

const MAGIC: &[u8; 4] = b"BYMM";
const VERSION: u32 = 1;
pub(crate) const HEADER_SIZE: u64 = 16;

/// Represents the BYMM storage header.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Header {
    pub version: u32,
    pub entry_count: u64,
}

/// Reads the header at the beginning of the file.
pub(crate) fn read_header(file: &mut File) -> io::Result<Header> {
    file.seek(SeekFrom::Start(0))?;

    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;

    if &magic != MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid BYMM header",
        ));
    }

    let version = read_u32(file)?;
    let entry_count = read_u64(file)?;

    if version != VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unsupported BYMM version",
        ));
    }

    Ok(Header {
        version,
        entry_count,
    })
}

/// Verifies whether the file contains a valid BYMM header.
pub(crate) fn verify_header(file: &mut File) -> io::Result<bool> {
    match read_header(file) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(error),
    }
}

/// Writes a new BYMM header at the beginning of the file.
pub(crate) fn write_header(file: &mut File) -> io::Result<()> {
    file.seek(SeekFrom::Start(0))?;

    file.write_all(MAGIC)?;
    write_u32(file, VERSION)?;
    write_u64(file, 0)?;

    debug_assert_eq!(HEADER_SIZE, 16);

    Ok(())
}

/// Updates the number of entries stored in the header.
pub(crate) fn update_entry_count(
    file: &mut File,
    count: u64,
) -> io::Result<()> {
    file.seek(SeekFrom::Start(8))?;
    write_u64(file, count)?;

    Ok(())
}