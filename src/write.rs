use std::fs::File;
use std::io::{self, ErrorKind};

use crate::utils::entries::{
    append_entry,
    find_entry,
    read_all_entries,
    replace_storage,
};

use crate::utils::header::{
    read_header,
    update_entry_count,
    verify_header,
    write_header,
};

/// Creates an entry or replaces its existing data.
pub fn write(
    file: &mut File,
    key: &str,
    data: &[u8],
) -> io::Result<()> {
    // Validate the key.
    if key.is_empty() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Entry key cannot be empty",
        ));
    }

    // Verify or initialize the storage.
    if !verify_header(file)? {
        if file.metadata()?.len() != 0 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Invalid BYMM storage",
            ));
        }

        write_header(file)?;
    }

    // Check whether the entry already exists.
    match find_entry(file, key)? {
        Some(_) => {
            // Read all existing entries.
            let mut entries = read_all_entries(file)?;

            // Replace the matching entry.
            for (existing_key, existing_data) in &mut entries {
                if existing_key == key {
                    *existing_data = data.to_vec();
                    break;
                }
            }

            // Rebuild the storage.
            replace_storage(file, entries)?;
        }

        None => {
            // Append the new entry.
            append_entry(file, key, data)?;

            // Update the entry count.
            let header = read_header(file)?;

            let new_count = header.entry_count
                .checked_add(1)
                .ok_or_else(|| io::Error::new(
                    ErrorKind::InvalidData,
                    "Entry count overflow",
                ))?;

            update_entry_count(file, new_count)?;
        }
    }

    Ok(())
}

pub fn append() {
    todo!();
}

pub fn delete() {
    todo!();
}

pub fn clear() {
    todo!();
}
