# bymm

**Store data inside any executable — and read it back, edit it, erase it.**
By [yolezz](https://github.com/yo-le-zz) · [Français](README.fr.md) · MIT licensed · zero dependencies

`bymm` appends a small, checksummed key/value storage to the end of a binary and
finds it again by looking at the end of the file. Executable loaders ignore what
comes after the end of the image, so **the program keeps running exactly as
before** — on every CPU architecture, on Linux and Windows (and macOS).

```rust
bymm::write("./my-app", "username", "yolezz")?;           // really written into ./my-app
let name = bymm::read_string("./my-app", "username")?;    // Some("yolezz")
```

## Features

| | |
|---|---|
| **Real writes into the binary** | Data lives in the executable file itself, not in a side file. |
| **Auto-detection** | ELF, PE (Windows), Mach-O (thin and universal); architecture, bit width, endianness, image end, signature. Unknown files work too. |
| **All architectures** | x86, x86_64, ARM, AArch64, ARM64EC, RISC-V 32/64, MIPS, PowerPC, s390x, SPARC, LoongArch, Itanium, m68k, SuperH… — the storage is architecture-independent. |
| **Full control** | `write`, `append`, `read`, `delete`, `clear`, `strip` (restore the pristine binary byte for byte), `verify`. |
| **Size limit** | Set it (`set_limit`), see it (`limit`, `used`, `free`, `max_value_len`), or remove it (`set_unlimited`). Stored inside the binary. |
| **Crash-safe** | Every commit builds the new binary in a temp file and swaps it in atomically. CRC-32 checksums on every entry. |
| **Self-modifying programs** | `Bymm::open_self()` — an executable can read *and rewrite* the file it is running from (Linux and Windows). |
| **Safe by default** | Refuses to silently break a digital signature; refuses to overwrite a file changed behind its back. |
| **CLI included** | `cargo install bymm` gives you a `bymm` command. |

## Install

```toml
[dependencies]
bymm = "1"
```

Requires Rust 1.85+. CLI: `cargo install bymm`.

## Usage

### One call per action

```rust
use bymm::{write, append, read, read_string, delete, keys};

write("./my-app", "config", br#"{"theme":"dark"}"#)?;
append("./my-app", "history", "launched\n")?;
assert_eq!(read_string("./my-app", "config")?.as_deref(), Some(r#"{"theme":"dark"}"#));
delete("./my-app", "history")?;
println!("{:?}", keys("./my-app")?);
```

Each call is atomic but rewrites the file. To make many changes at once, use a handle:

### A handle: stage many changes, commit once

```rust
use bymm::Bymm;

let mut b = Bymm::open("./my-app")?;
println!("{} / {} / {} bits", b.info().format, b.info().arch(), b.info().bits.unwrap_or(0));
// ELF / x86_64 / 64 bits

b.set_limit(1024 * 1024)?;            // at most 1 MiB may ever be embedded
b.write("name", "yolezz")?;
b.append("log", "hello\n")?;
b.delete("old-key")?;
b.commit()?;                          // nothing touches the file before this line
```

Changes are *staged in memory*; reads see the staged state; `b.discard()?` throws
them away; `b.is_dirty()` tells whether a commit is pending.

### Limits: see them, set them

```rust
let b = bymm::Bymm::open("./my-app")?;
b.limit();                  // 67108864 (64 MiB by default); u64::MAX = unlimited
b.used();                   // bytes the storage takes now (header+entries+footer)
b.free();                   // bytes you can still add
b.max_value_len("key")?;    // biggest value write("key", …) would accept right now
b.usage();                  // all of the above in one struct (implements Display)
```

`set_limit(n)` fails if `n` is smaller than what is already used. A write that
would go over the limit fails with `Error::LimitExceeded` and changes nothing.
The limit is saved inside the binary. The default for a *new* storage can be
chosen with `Options::new().default_limit(…)`.

### A program that stores data in itself

```rust
fn main() -> bymm::Result<()> {
    let mut me = bymm::Bymm::open_self()?;
    let n: u32 = me.read_string("launches")?.and_then(|s| s.parse().ok()).unwrap_or(0);
    println!("launched {n} time(s) before");
    me.write("launches", (n + 1).to_string())?;
    me.commit()?;               // rewrites this very executable, atomically
    Ok(())
}
```

(Run it: `cargo build --example counter`, copy `target/debug/examples/counter` somewhere, run the copy.)
The *current* process keeps running its old image; the next launch sees the new data.

### Detecting what a file is

```rust
let info = bymm::see::info("./some-binary")?;
info.format;      // Format::Elf | Pe | MachO | MachOUniversal | Unknown
info.arch();      // Arch::X86_64, Arch::Aarch64, Arch::Riscv64, …
info.bits;        // Some(64)
info.endian;      // Some(Endian::Little)
info.image_end;   // where the executable image ends (the rest is overlay)
info.signed;      // true if Authenticode / Mach-O signed
```

### Command line

```text
bymm info   <binary>                  format, architecture, storage usage
bymm ls     <binary>                  list keys and sizes
bymm get    <binary> <key>            write the value to stdout
bymm set    <binary> <key> <value>    value = text, @file or - (stdin)
bymm append <binary> <key> <value>
bymm rm     <binary> <key>
bymm clear  <binary>                  remove every entry
bymm strip  <binary>                  remove the storage, restore the original
bymm limit  <binary> [bytes|unlimited]
bymm verify <binary>                  check every checksum
flags: --backup (keep <binary>.bymm.bak)   --allow-signed
```

## How it works

```text
[ original executable ][ header 32 B ][ entry ]…[ footer 16 B ]
                                                   └─ "BYMMEND\0" + length: found from the end of the file
```

Opening a file reads its last 16 bytes; if they are a footer, the storage is
indexed (values are *not* loaded). Writing builds *original bytes + new storage*
in a temp file next to the binary, `fsync`s it, copies the permission bits and
renames it over the original. That rename is also why a running program can
update itself: Linux refuses to open a running executable for writing, but
happily lets you replace its directory entry. On Windows, where a running `.exe`
cannot be overwritten but can be renamed, the old file is moved aside first.

The complete specification is in [`docs/FORMAT.md`](docs/FORMAT.md).

## Things to know

* **Digital signatures.** Changing a signed binary invalidates its signature
  (Authenticode, macOS code signing). `bymm` returns `Error::SignedBinary`
  unless you pass `Options::new().allow_signed(true)`. Re-sign afterwards.
* **Post-processing tools** (`strip`, `upx`, `objcopy`, installers' "pack" steps)
  rewrite the file and drop what they do not know. Embed *after* them.
* **Directory access.** A commit needs write access to the binary's *directory*
  (it swaps files). Permission bits are preserved; owner, ACLs and extended
  attributes are not, and hard links are not followed.
* **Concurrency.** Two processes committing at the same time are not merged:
  the second gets `Error::Modified` instead of overwriting the first.
* **Memory.** Values are staged in memory until commit; reading with
  `read_to` streams. Committing streams unchanged data from disk.
* **Leftovers.** On Windows, if the running image of the *old* file cannot be
  deleted right away, a hidden `.<name>.bymm-old` file is removed at the next commit.
* **macOS.** Mach-O is detected and the file-level mechanism is identical, but
  a signed/notarized app must be re-signed after a write.

## Errors

All fallible calls return `bymm::Result<T>`. `Error` is `#[non_exhaustive]`:
`Io`, `InvalidKey`, `LimitExceeded`, `InvalidLimit`, `Corrupt`,
`UnsupportedVersion`, `SignedBinary`, `Modified`. It implements
`std::error::Error` and converts into `std::io::Error`.

## Testing

`cargo test` runs the unit tests, doc-tests and integration tests: 31 synthetic files
executables (ELF/PE/Mach-O across many architectures, endiannesses and bit
widths), real system binaries, limit boundaries, corruption detection, and an
end-to-end test that embeds data into the `bymm` executable itself, **runs it**,
and has it rewrite itself. CI (`.github/workflows/ci.yml`) is set up to run everything on
Linux x86_64, Linux ARM64, Windows and macOS.
`test/` holds a separate workspace (`test/run_all.sh` or `test/run_all.ps1`)
that builds a fake executable and exercises the library on it.

## Migrating from 0.1.x

0.1.x wrote a standalone `.bymm` file through `&mut File`. 1.0.0 writes **into
the binary** and takes paths: `bymm::write(&mut file, key, data)` becomes
`bymm::write(path, key, data)`. The 0.1 file format is not read by 1.0.

## License

MIT — © 2026 yolezz
