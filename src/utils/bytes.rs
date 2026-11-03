use std::io::{self, Read, Write};

/// Reads a u32 from a reader using little-endian encoding.
pub(crate) fn read_u32<R: Read>(reader: &mut R) -> io::Result<u32> {
    let mut buffer = [0u8; 4];
    reader.read_exact(&mut buffer)?;

    Ok(u32::from_le_bytes(buffer))
}

/// Reads a u64 from a reader using little-endian encoding.
pub(crate) fn read_u64<R: Read>(reader: &mut R) -> io::Result<u64> {
    let mut buffer = [0u8; 8];
    reader.read_exact(&mut buffer)?;

    Ok(u64::from_le_bytes(buffer))
}

/// Writes a u32 to a writer using little-endian encoding.
pub(crate) fn write_u32<W: Write>(writer: &mut W, value: u32) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

/// Writes a u64 to a writer using little-endian encoding.
pub(crate) fn write_u64<W: Write>(writer: &mut W, value: u64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

/// Reads an exact number of bytes from a reader.
pub(crate) fn read_bytes<R: Read>(
    reader: &mut R,
    length: usize,
) -> io::Result<Vec<u8>> {
    let mut buffer = vec![0u8; length];
    reader.read_exact(&mut buffer)?;

    Ok(buffer)
}

/// Writes a byte slice to a writer.
pub(crate) fn write_bytes<W: Write>(
    writer: &mut W,
    data: &[u8],
) -> io::Result<()> {
    writer.write_all(data)
}
