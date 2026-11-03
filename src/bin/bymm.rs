//! `bymm` — command-line front end of the library.
//!
//! ```text
//! bymm info   <binary>                  format, architecture, storage usage
//! bymm ls     <binary>                  list keys and sizes
//! bymm get    <binary> <key>            write the value to stdout
//! bymm set    <binary> <key> <value>    value = text, @file or - (stdin)
//! bymm append <binary> <key> <value>
//! bymm rm     <binary> <key>
//! bymm clear  <binary>                  remove every entry
//! bymm strip  <binary>                  remove the storage, restore the original
//! bymm limit  <binary> [bytes|unlimited]
//! bymm verify <binary>                  check every checksum
//! bymm self-get <key>                   read from this very executable
//! bymm self-set <key> <value>           write into this very executable
//!
//! flags: --backup  keep <binary>.bymm.bak     --allow-signed  write into signed binaries
//! ```

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use bymm::{Bymm, Error, Options};

const USAGE: &str = "\
usage: bymm <command> ...

  info   <binary>                  format, architecture, storage usage
  ls     <binary>                  list keys and sizes
  get    <binary> <key>            write the value to stdout
  set    <binary> <key> <value>    value = text, @file or - (stdin)
  append <binary> <key> <value>
  rm     <binary> <key>
  clear  <binary>                  remove every entry
  strip  <binary>                  remove the storage, restore the original
  limit  <binary> [bytes|unlimited]
  verify <binary>                  check every checksum
  self-get <key>                   read from this very executable
  self-set <key> <value>           write into this very executable

flags: --backup  --allow-signed";

fn main() -> ExitCode {
    let mut opts = Options::new();
    let mut args = Vec::new();
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--backup" => opts = opts.backup(true),
            "--allow-signed" => opts = opts.allow_signed(true),
            "-h" | "--help" | "help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ => args.push(a),
        }
    }
    match run(&args, opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage) => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Failed(e)) => {
            eprintln!("bymm: {e}");
            ExitCode::FAILURE
        }
    }
}

enum CliError {
    Usage,
    Failed(Error),
}

impl<E: Into<Error>> From<E> for CliError {
    fn from(e: E) -> Self {
        CliError::Failed(e.into())
    }
}

/// `text`, `@path` (file contents) or `-` (stdin).
fn value_arg(v: &str) -> io::Result<Vec<u8>> {
    if v == "-" {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf)?;
        Ok(buf)
    } else if let Some(path) = v.strip_prefix('@') {
        std::fs::read(path)
    } else {
        Ok(v.as_bytes().to_vec())
    }
}

fn run(args: &[String], opts: Options) -> Result<(), CliError> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["info", bin] => info(Path::new(bin), opts),
        ["ls", bin] => {
            let b = Bymm::open_with(bin, opts)?;
            for (key, len) in b.entries() {
                println!("{len:>12}  {key}");
            }
            Ok(())
        }
        ["get", bin, key] => get(Bymm::open_with(bin, opts)?, key),
        ["self-get", key] => get(Bymm::open_self_with(opts)?, key),
        ["set", bin, key, value] => modify(Bymm::open_with(bin, opts)?, |b| {
            Ok(b.write(key, value_arg(value)?)?)
        }),
        ["self-set", key, value] => modify(Bymm::open_self_with(opts)?, |b| {
            Ok(b.write(key, value_arg(value)?)?)
        }),
        ["append", bin, key, value] => modify(Bymm::open_with(bin, opts)?, |b| {
            Ok(b.append(key, value_arg(value)?)?)
        }),
        ["rm", bin, key] => {
            let mut b = Bymm::open_with(bin, opts)?;
            if !b.delete(key)? {
                eprintln!("bymm: no such key: {key}");
            }
            b.commit()?;
            Ok(())
        }
        ["clear", bin] => modify(Bymm::open_with(bin, opts)?, |b| Ok(b.clear()?)),
        ["strip", bin] => modify(Bymm::open_with(bin, opts)?, |b| Ok(b.strip()?)),
        ["limit", bin] => {
            let b = Bymm::open_with(bin, opts)?;
            if b.is_unlimited() {
                println!("unlimited");
            } else {
                println!("{}", b.limit());
            }
            Ok(())
        }
        ["limit", bin, value] => {
            let bytes = if value.eq_ignore_ascii_case("unlimited") {
                u64::MAX
            } else {
                value.parse::<u64>().map_err(|_| CliError::Usage)?
            };
            modify(Bymm::open_with(bin, opts)?, |b| Ok(b.set_limit(bytes)?))
        }
        ["verify", bin] => {
            let b = Bymm::open_with(bin, opts)?;
            b.verify()?;
            println!("ok: {} entries verified", b.len());
            Ok(())
        }
        _ => Err(CliError::Usage),
    }
}

fn modify(mut b: Bymm, f: impl FnOnce(&mut Bymm) -> Result<(), CliError>) -> Result<(), CliError> {
    f(&mut b)?;
    b.commit()?;
    Ok(())
}

fn get(b: Bymm, key: &str) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match b.read_to(key, &mut out)? {
        Some(_) => {
            out.flush().map_err(Error::Io)?;
            Ok(())
        }
        None => Err(CliError::Failed(Error::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no such key: {key}"),
        )))),
    }
}

fn info(path: &Path, opts: Options) -> Result<(), CliError> {
    let b = Bymm::open_with(path, opts)?;
    let i = b.info();
    let archs: Vec<String> = i.archs.iter().map(|a| a.to_string()).collect();
    println!("file        {}", path.display());
    println!("format      {}", i.format);
    println!(
        "arch        {}",
        if archs.is_empty() {
            "unknown".into()
        } else {
            archs.join(", ")
        }
    );
    if let Some(bits) = i.bits {
        println!("bits        {bits}");
    }
    if let Some(e) = i.endian {
        println!("endianness  {e}");
    }
    println!(
        "size        {} bytes ({} without storage)",
        i.file_size,
        b.original_len()
    );
    if let Some(end) = i.image_end {
        println!("image end   {end}");
    }
    println!(
        "signed      {}",
        if i.signed {
            "yes (writing breaks the signature)"
        } else {
            "no"
        }
    );
    println!("storage     {}", b.usage());
    Ok(())
}
