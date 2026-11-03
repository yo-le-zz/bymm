//! Positional (thread-safe) file reading, identical behaviour on Unix and Windows.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

#[cfg(not(any(unix, windows)))]
compile_error!("bymm supports Unix (Linux, macOS, BSD) and Windows targets only");

/// Reads some bytes at `offset` without touching the file cursor.
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    #[cfg(unix)]
    {
        std::os::unix::fs::FileExt::read_at(file, buf, offset)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::FileExt::seek_read(file, buf, offset)
    }
}

/// A `Read + Seek` view over a shared `&File` with its own private cursor.
///
/// Several `At` can read the same file at the same time without racing.
pub(crate) struct At<'a> {
    file: &'a File,
    pos: u64,
}

impl<'a> At<'a> {
    pub(crate) fn new(file: &'a File, pos: u64) -> Self {
        At { file, pos }
    }
}

impl Read for At<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = read_at(self.file, buf, self.pos)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for At<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let new = match from {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
            SeekFrom::End(d) => self.file.metadata()?.len().checked_add_signed(d),
        };
        match new {
            Some(p) => {
                self.pos = p;
                Ok(p)
            }
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before the start of the file",
            )),
        }
    }
}

/// Reads exactly `len` bytes at `offset`.
pub(crate) fn read_exact_at(file: &File, offset: u64, len: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    At::new(file, offset).read_exact(&mut buf)?;
    Ok(buf)
}
