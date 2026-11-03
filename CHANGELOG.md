# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/) and the project follows
[Semantic Versioning](https://semver.org/).

## [1.0.0] — 2026-11-03

First stable release. **Breaking:** the crate now writes *into the binary* and takes paths.

### Added
- Real embedding: a checksummed key/value storage appended to any executable, found from the
  end of the file; the program keeps running unchanged.
- Automatic detection of ELF, PE and Mach-O (thin and universal): architecture, bit width,
  endianness, image end, digital signature. Unknown files are supported.
- `Bymm` handle with staged changes and an atomic `commit()`: `write`, `append`, `read`,
  `read_string`, `read_to`, `delete`, `clear`, `strip`, `verify`, `discard`, `keys`, `entries`.
- Size limit stored in the binary: `set_limit`, `set_unlimited`, `limit`, `used`, `free`,
  `max_value_len`, `usage`.
- `Bymm::open_self()` so a program can read and rewrite its own executable (Linux and Windows).
- `Options`: `default_limit`, `allow_signed`, `backup`.
- One-call helpers in `bymm::{read, write, see}`.
- A typed `Error` that converts to `std::io::Error`.
- `bymm` command-line tool, `counter` example, `docs/FORMAT.md`, English and French READMEs.
- CRC-32 on every entry and on the header; corruption is reported, never ignored.
- Zero dependencies.

### Changed
- `write`, `append`, `delete`, `clear` now take a path instead of `&mut File`, and are implemented
  (0.1.x had `todo!()` bodies for the last three).
- New on-disk format (version 1). The 0.1 standalone-file format is retired.

### Removed
- The standalone `.bymm` storage file.

## [0.1.1]
- Fix version for publish.

## [0.1.0]
- Initial commit: header and `write` into a standalone storage file.
