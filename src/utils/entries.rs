use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};

use super::bytes::{
    read_bytes,
    read_u32,
    read_u64,
    write_bytes,
    write_u32,
    write_u64,
};

use super::header::HEADER_SIZE;

/// Contains the location and metadata of an entry.
#[derive(Debug, Clone)]
pub(crate) struct EntryInfo {
    pub key: String,
    pub data_offset: u64,
    pub data_length: u64,
}

/// Scans the storage and returns all entry metadata.
fn scan_entries(file: &mut File) -> io::Result<Vec<EntryInfo>> {
    let file_size = file.metadata()?.len();
    let mut entries = Vec::new();

    let mut position = HEADER_SIZE;

    while position < file_size {
        // An entry header requires at least 12 bytes.
        if file_size - position < 12 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Incomplete entry header",
            ));
        }

        file.seek(SeekFrom::Start(position))?;

        let key_length = read_u32(file)? as u64;
        let data_length = read_u64(file)?;

        let key_offset = position.checked_add(12)
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidData,
                "Entry offset overflow",
            ))?;

        let data_offset = key_offset.checked_add(key_length)
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidData,
                "Entry size overflow",
            ))?;

        let entry_end = data_offset.checked_add(data_length)
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidData,
                "Entry size overflow",
            ))?;

        if entry_end > file_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Incomplete entry data",
            ));
        }

        let key_size = usize::try_from(key_length)
            .map_err(|_| io::Error::new(
                io::ErrorKind::InvalidData,
                "Key is too large",
            ))?;

        file.seek(SeekFrom::Start(key_offset))?;

        let key_bytes = read_bytes(file, key_size)?;

        let key = String::from_utf8(key_bytes)
            .map_err(|_| io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid UTF-8 key",
            ))?;

        if key.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Empty entry key",
            ));
        }

        entries.push(EntryInfo {
            key,
            data_offset,
            data_length,
        });

        position = entry_end;
    }

    Ok(entries)
}

/// Finds an entry by its key.
pub(crate) fn find_entry(
    file: &mut File,
    key: &str,
) -> io::Result<Option<EntryInfo>> {
    let entries = scan_entries(file)?;

    Ok(entries.into_iter().find(|entry| entry.key == key))
}

/// Reads the data of an entry.
pub(crate) fn read_entry(
    file: &mut File,
    key: &str,
) -> io::Result<Option<Vec<u8>>> {
    let Some(entry) = find_entry(file, key)? else {
        return Ok(None);
    };

    let length = usize::try_from(entry.data_length)
        .map_err(|_| io::Error::new(
            io::ErrorKind::InvalidData,
            "Entry is too large",
        ))?;

    file.seek(SeekFrom::Start(entry.data_offset))?;

    let data = read_bytes(file, length)?;

    Ok(Some(data))
}

/// Reads all entries and returns their keys and data.
pub(crate) fn read_all_entries(
    file: &mut File,
) -> io::Result<Vec<(String, Vec<u8>)>> {
    let entries = scan_entries(file)?;
    let mut result = Vec::with_capacity(entries.len());

    for entry in entries {
        let length = usize::try_from(entry.data_length)
            .map_err(|_| io::Error::new(
                io::ErrorKind::InvalidData,
                "Entry is too large",
            ))?;

        file.seek(SeekFrom::Start(entry.data_offset))?;

        let data = read_bytes(file, length)?;

        result.push((entry.key, data));
    }

    Ok(result)
}

/// Appends a new entry to the storage.
pub(crate) fn append_entry(
    file: &mut File,
    key: &str,
    data: &[u8],
) -> io::Result<()> {
    if key.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Entry key cannot be empty",
        ));
    }

    let key_bytes = key.as_bytes();

    let key_length = u32::try_from(key_bytes.len())
        .map_err(|_| io::Error::new(
            io::ErrorKind::InvalidInput,
            "Key is too large",
        ))?;

    let data_length = u64::try_from(data.len())
        .map_err(|_| io::Error::new(
            io::ErrorKind::InvalidInput,
            "Data is too large",
        ))?;

    if find_entry(file, key)?.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Entry already exists",
        ));
    }

    file.seek(SeekFrom::End(0))?;

    write_u32(file, key_length)?;
    write_u64(file, data_length)?;

    write_bytes(file, key_bytes)?;
    write_bytes(file, data)?;

    file.flush()?;

    Ok(())
}

/// Rebuilds the storage with the provided entries.
pub(crate) fn replace_storage(
    file: &mut File,
    entries: Vec<(String, Vec<u8>)>,
) -> io::Result<()> {
    let mut buffer = Vec::new();

    for (key, data) in &entries {
        if key.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Entry key cannot be empty",
            ));
        }

        let key_length = u32::try_from(key.len())
            .map_err(|_| io::Error::new(
                io::ErrorKind::InvalidInput,
                "Key is too large",
            ))?;

        let data_length = u64::try_from(data.len())
            .map_err(|_| io::Error::new(
                io::ErrorKind::InvalidInput,
                "Data is too large",
            ))?;

        write_u32(&mut buffer, key_length)?;
        write_u64(&mut buffer, data_length)?;

        write_bytes(&mut buffer, key.as_bytes())?;
        write_bytes(&mut buffer, data)?;
    }

    let entry_count = u64::try_from(entries.len())
        .map_err(|_| io::Error::new(
            io::ErrorKind::InvalidInput,
            "Too many entries",
        ))?;

    // Rebuild the dedicated BYMM storage.
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;

    super::header::write_header(file)?;
    super::header::update_entry_count(file, entry_count)?;

    write_bytes(file, &buffer)?;

    file.flush()?;

    Ok(())
}
