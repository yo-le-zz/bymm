//! Automatic detection of the executable format and CPU architecture.
//!
//! Detection is **best effort and never fails on malformed files**: a file that
//! looks like nothing known is reported as [`Format::Unknown`] and can still hold
//! embedded data, because `bymm` only relies on the very end of the file.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

/// Container format of an executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// ELF — Linux, BSD, Android, most embedded systems.
    Elf,
    /// PE/COFF — Windows `.exe`, `.dll`, `.sys`, UEFI applications.
    Pe,
    /// Mach-O, a single architecture — macOS / iOS.
    MachO,
    /// Mach-O "fat" (universal) binary holding several architectures.
    MachOUniversal,
    /// Anything else (scripts, firmware, raw files…). Still usable.
    Unknown,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Elf => "ELF",
            Format::Pe => "PE",
            Format::MachO => "Mach-O",
            Format::MachOUniversal => "Mach-O universal",
            Format::Unknown => "unknown",
        })
    }
}

/// Byte order of the executable's own data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Endian {
    /// Least significant byte first (x86, ARM, RISC-V…).
    Little,
    /// Most significant byte first (s390x, classic PowerPC, SPARC…).
    Big,
}

impl fmt::Display for Endian {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Endian::Little => "little-endian",
            Endian::Big => "big-endian",
        })
    }
}

/// CPU architecture an executable was built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Arch {
    /// 32-bit x86 (i386 / i686).
    X86,
    /// 64-bit x86 (amd64).
    X86_64,
    /// 32-bit ARM (ARMv7 and older, including Thumb-2 on Windows).
    Arm,
    /// 64-bit ARM (ARMv8 / Apple silicon / Windows on ARM).
    Aarch64,
    /// Windows ARM64EC (x64-compatible ARM64).
    Arm64Ec,
    /// 32-bit RISC-V.
    Riscv32,
    /// 64-bit RISC-V.
    Riscv64,
    /// 32-bit MIPS.
    Mips,
    /// 64-bit MIPS.
    Mips64,
    /// 32-bit PowerPC.
    PowerPc,
    /// 64-bit PowerPC.
    PowerPc64,
    /// IBM s390 (31-bit).
    S390,
    /// IBM s390x (z/Architecture).
    S390x,
    /// 32-bit SPARC.
    Sparc,
    /// 64-bit SPARC (v9).
    Sparc64,
    /// 32-bit LoongArch.
    LoongArch32,
    /// 64-bit LoongArch.
    LoongArch64,
    /// Intel Itanium.
    Ia64,
    /// Motorola 68000 family.
    M68k,
    /// Renesas SuperH.
    SuperH,
    /// Qualcomm Hexagon.
    Hexagon,
    /// Tensilica Xtensa (ESP32…).
    Xtensa,
    /// Linux eBPF.
    Bpf,
    /// A machine id this version of `bymm` has no name for (raw id inside).
    Other(u32),
    /// The architecture could not be determined (unknown format).
    Unknown,
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Arch::X86 => "x86",
            Arch::X86_64 => "x86_64",
            Arch::Arm => "arm",
            Arch::Aarch64 => "aarch64",
            Arch::Arm64Ec => "arm64ec",
            Arch::Riscv32 => "riscv32",
            Arch::Riscv64 => "riscv64",
            Arch::Mips => "mips",
            Arch::Mips64 => "mips64",
            Arch::PowerPc => "powerpc",
            Arch::PowerPc64 => "powerpc64",
            Arch::S390 => "s390",
            Arch::S390x => "s390x",
            Arch::Sparc => "sparc",
            Arch::Sparc64 => "sparc64",
            Arch::LoongArch32 => "loongarch32",
            Arch::LoongArch64 => "loongarch64",
            Arch::Ia64 => "ia64",
            Arch::M68k => "m68k",
            Arch::SuperH => "superh",
            Arch::Hexagon => "hexagon",
            Arch::Xtensa => "xtensa",
            Arch::Bpf => "bpf",
            Arch::Other(id) => return write!(f, "other({id:#x})"),
            Arch::Unknown => "unknown",
        };
        f.write_str(s)
    }
}

/// Everything `bymm` could learn about a file by looking at its headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryInfo {
    /// The container format.
    pub format: Format,
    /// Architectures found (one entry, except for universal Mach-O binaries).
    pub archs: Vec<Arch>,
    /// Pointer width in bits (32 or 64) when the format tells it.
    pub bits: Option<u8>,
    /// Byte order of the binary when the format tells it.
    pub endian: Option<Endian>,
    /// Total size of the file in bytes (including any embedded storage).
    pub file_size: u64,
    /// End of the part described by the headers, when it could be computed.
    ///
    /// Anything between this offset and `file_size` is an "overlay" (installer
    /// payload, certificate, or `bymm` storage).
    pub image_end: Option<u64>,
    /// `true` if the binary carries a digital signature (Authenticode or Mach-O).
    pub signed: bool,
}

impl BinaryInfo {
    /// The (first) architecture, or [`Arch::Unknown`].
    pub fn arch(&self) -> Arch {
        self.archs.first().copied().unwrap_or(Arch::Unknown)
    }

    /// Detects the format of anything that can be read and seeked.
    ///
    /// Only I/O errors are reported; malformed headers just yield less information.
    pub fn detect<R: Read + Seek>(reader: &mut R) -> io::Result<BinaryInfo> {
        let file_size = reader.seek(SeekFrom::End(0))?;
        let mut info = BinaryInfo {
            format: Format::Unknown,
            archs: Vec::new(),
            bits: None,
            endian: None,
            file_size,
            image_end: None,
            signed: false,
        };
        let head = read_at(reader, file_size, 0, 4)?;
        let Some(head) = head else { return Ok(info) };
        let magic = [head[0], head[1], head[2], head[3]];

        match magic {
            [0x7F, b'E', b'L', b'F'] => elf(reader, &mut info)?,
            [b'M', b'Z', _, _] => pe(reader, &mut info)?,
            [0xFE, 0xED, 0xFA, 0xCE]
            | [0xFE, 0xED, 0xFA, 0xCF]
            | [0xCE, 0xFA, 0xED, 0xFE]
            | [0xCF, 0xFA, 0xED, 0xFE] => macho(reader, &mut info)?,
            [0xCA, 0xFE, 0xBA, 0xBE] | [0xCA, 0xFE, 0xBA, 0xBF] => macho_fat(reader, &mut info)?,
            _ => {}
        }
        if info.format == Format::Unknown {
            info.archs.clear();
            info.bits = None;
            info.endian = None;
            info.image_end = None;
            info.signed = false;
        }
        Ok(info)
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Reads `len` bytes at `offset`, or `None` if they are not entirely inside the file.
fn read_at<R: Read + Seek>(
    r: &mut R,
    file_size: u64,
    offset: u64,
    len: u64,
) -> io::Result<Option<Vec<u8>>> {
    match offset.checked_add(len) {
        Some(end) if end <= file_size => {
            r.seek(SeekFrom::Start(offset))?;
            let mut buf = vec![0u8; len as usize];
            r.read_exact(&mut buf)?;
            Ok(Some(buf))
        }
        _ => Ok(None),
    }
}

#[derive(Clone, Copy)]
struct Rd {
    big: bool,
}

impl Rd {
    fn u16(self, b: &[u8], o: usize) -> u16 {
        let a = [b[o], b[o + 1]];
        if self.big {
            u16::from_be_bytes(a)
        } else {
            u16::from_le_bytes(a)
        }
    }
    fn u32(self, b: &[u8], o: usize) -> u32 {
        let a = [b[o], b[o + 1], b[o + 2], b[o + 3]];
        if self.big {
            u32::from_be_bytes(a)
        } else {
            u32::from_le_bytes(a)
        }
    }
    fn u64(self, b: &[u8], o: usize) -> u64 {
        let mut a = [0u8; 8];
        a.copy_from_slice(&b[o..o + 8]);
        if self.big {
            u64::from_be_bytes(a)
        } else {
            u64::from_le_bytes(a)
        }
    }
}

fn bump(end: &mut u64, candidate: Option<u64>) {
    if let Some(c) = candidate {
        *end = (*end).max(c);
    }
}

// ---------------------------------------------------------------------------
// ELF
// ---------------------------------------------------------------------------

fn elf<R: Read + Seek>(r: &mut R, info: &mut BinaryInfo) -> io::Result<()> {
    let size = info.file_size;
    let Some(id) = read_at(r, size, 0, 20)? else {
        return Ok(());
    };
    let bits = match id[4] {
        1 => 32u8,
        2 => 64u8,
        _ => return Ok(()),
    };
    let big = match id[5] {
        1 => false,
        2 => true,
        _ => return Ok(()),
    };
    let rd = Rd { big };
    let machine = rd.u16(&id, 18);

    info.format = Format::Elf;
    info.bits = Some(bits);
    info.endian = Some(if big { Endian::Big } else { Endian::Little });
    info.archs = vec![elf_arch(machine, bits)];

    let hdr_len: u64 = if bits == 64 { 64 } else { 52 };
    let Some(h) = read_at(r, size, 0, hdr_len)? else {
        return Ok(());
    };

    let (phoff, shoff, phentsize, phnum, shentsize, shnum) = if bits == 64 {
        (
            rd.u64(&h, 32),
            rd.u64(&h, 40),
            rd.u16(&h, 54),
            rd.u16(&h, 56),
            rd.u16(&h, 58),
            rd.u16(&h, 60),
        )
    } else {
        (
            rd.u32(&h, 28) as u64,
            rd.u32(&h, 32) as u64,
            rd.u16(&h, 42),
            rd.u16(&h, 44),
            rd.u16(&h, 46),
            rd.u16(&h, 48),
        )
    };

    let mut end = hdr_len;

    // Program headers: the end of every segment's file image.
    let pe = phentsize as u64;
    let min_ph = if bits == 64 { 56 } else { 32 };
    if phnum > 0 && pe >= min_ph {
        bump(&mut end, phoff.checked_add(pe * phnum as u64));
        if let Some(table) = read_at(r, size, phoff, pe * phnum as u64)? {
            for i in 0..phnum as usize {
                let b = &table[i * pe as usize..];
                let (off, filesz) = if bits == 64 {
                    (rd.u64(b, 8), rd.u64(b, 32))
                } else {
                    (rd.u32(b, 4) as u64, rd.u32(b, 16) as u64)
                };
                bump(&mut end, off.checked_add(filesz));
            }
        }
    }

    // Section headers: the table itself and the file image of every section.
    let se = shentsize as u64;
    let min_sh = if bits == 64 { 64 } else { 40 };
    if shoff != 0 && shnum > 0 && se >= min_sh {
        bump(&mut end, shoff.checked_add(se * shnum as u64));
        if let Some(table) = read_at(r, size, shoff, se * shnum as u64)? {
            for i in 0..shnum as usize {
                let b = &table[i * se as usize..];
                let (ty, off, sz) = if bits == 64 {
                    (rd.u32(b, 4), rd.u64(b, 24), rd.u64(b, 32))
                } else {
                    (rd.u32(b, 4), rd.u32(b, 16) as u64, rd.u32(b, 20) as u64)
                };
                if ty != 8 {
                    // 8 = SHT_NOBITS (.bss): takes no room in the file
                    bump(&mut end, off.checked_add(sz));
                }
            }
        }
    }

    if end <= size {
        info.image_end = Some(end);
    }
    Ok(())
}

fn elf_arch(machine: u16, bits: u8) -> Arch {
    let b64 = bits == 64;
    match machine {
        3 => Arch::X86,
        62 => Arch::X86_64,
        40 => Arch::Arm,
        183 => Arch::Aarch64,
        243 => {
            if b64 {
                Arch::Riscv64
            } else {
                Arch::Riscv32
            }
        }
        8 | 10 => {
            if b64 {
                Arch::Mips64
            } else {
                Arch::Mips
            }
        }
        20 => Arch::PowerPc,
        21 => Arch::PowerPc64,
        22 => {
            if b64 {
                Arch::S390x
            } else {
                Arch::S390
            }
        }
        2 | 18 => Arch::Sparc,
        43 => Arch::Sparc64,
        258 => {
            if b64 {
                Arch::LoongArch64
            } else {
                Arch::LoongArch32
            }
        }
        50 => Arch::Ia64,
        4 => Arch::M68k,
        42 => Arch::SuperH,
        164 => Arch::Hexagon,
        94 => Arch::Xtensa,
        247 => Arch::Bpf,
        other => Arch::Other(other as u32),
    }
}

// ---------------------------------------------------------------------------
// PE / COFF
// ---------------------------------------------------------------------------

fn pe<R: Read + Seek>(r: &mut R, info: &mut BinaryInfo) -> io::Result<()> {
    let size = info.file_size;
    let rd = Rd { big: false };
    let Some(dos) = read_at(r, size, 0, 64)? else {
        return Ok(());
    };
    let lfanew = rd.u32(&dos, 0x3C) as u64;
    let Some(nt) = read_at(r, size, lfanew, 24)? else {
        return Ok(());
    };
    if &nt[0..4] != b"PE\0\0" {
        return Ok(());
    }
    let machine = rd.u16(&nt, 4);
    let nsections = rd.u16(&nt, 6) as u64;
    let symtab = rd.u32(&nt, 12) as u64;
    let nsyms = rd.u32(&nt, 16) as u64;
    let opt_size = rd.u16(&nt, 20) as u64;

    let opt_off = lfanew + 24;
    let Some(magic) = read_at(r, size, opt_off, 2)? else {
        return Ok(());
    };
    let bits = match rd.u16(&magic, 0) {
        0x10B => 32u8,
        0x20B => 64u8,
        _ => return Ok(()),
    };

    info.format = Format::Pe;
    info.bits = Some(bits);
    info.endian = Some(Endian::Little);
    info.archs = vec![pe_arch(machine)];

    let mut end = 0u64;

    // Optional header: SizeOfHeaders and the certificate (signature) directory.
    let (dd_off, nrva_off) = if bits == 64 {
        (112u64, 108u64)
    } else {
        (96u64, 92u64)
    };
    if let Some(opt) = read_at(r, size, opt_off, opt_size.min(dd_off + 8 * 16))? {
        if opt.len() >= 64 {
            bump(&mut end, Some(rd.u32(&opt, 60) as u64));
        }
        if opt.len() > nrva_off as usize + 4 {
            let n = rd.u32(&opt, nrva_off as usize);
            let sec = dd_off as usize + 4 * 8; // index 4 = certificate table
            if n > 4 && opt.len() >= sec + 8 {
                let cert_off = rd.u32(&opt, sec) as u64; // a *file offset*, not an RVA
                let cert_len = rd.u32(&opt, sec + 4) as u64;
                if cert_off != 0 && cert_len != 0 {
                    info.signed = true;
                    bump(&mut end, cert_off.checked_add(cert_len));
                }
            }
        }
    }

    // Section table: raw data of every section.
    let sec_off = opt_off + opt_size;
    if let Some(table) = read_at(r, size, sec_off, nsections * 40)? {
        bump(&mut end, Some(sec_off + nsections * 40));
        for i in 0..nsections as usize {
            let b = &table[i * 40..];
            let raw_size = rd.u32(b, 16) as u64;
            let raw_ptr = rd.u32(b, 20) as u64;
            if raw_ptr != 0 {
                bump(&mut end, raw_ptr.checked_add(raw_size));
            }
        }
    }

    // Legacy COFF symbol table + string table (MinGW binaries keep them).
    if symtab != 0 && nsyms != 0 {
        if let Some(sym_end) = symtab.checked_add(nsyms * 18) {
            if let Some(len) = read_at(r, size, sym_end, 4)? {
                bump(&mut end, sym_end.checked_add(rd.u32(&len, 0) as u64));
            } else {
                bump(&mut end, Some(sym_end));
            }
        }
    }

    if end > 0 && end <= size {
        info.image_end = Some(end);
    }
    Ok(())
}

fn pe_arch(machine: u16) -> Arch {
    match machine {
        0x014C => Arch::X86,
        0x8664 => Arch::X86_64,
        0x01C0 | 0x01C2 | 0x01C4 => Arch::Arm,
        0xAA64 | 0xA64E => Arch::Aarch64,
        0xA641 => Arch::Arm64Ec,
        0x5032 => Arch::Riscv32,
        0x5064 => Arch::Riscv64,
        0x0200 => Arch::Ia64,
        0x01F0 | 0x01F1 => Arch::PowerPc,
        0x0166 | 0x0266 | 0x0366 | 0x0466 => Arch::Mips,
        0x01A2 | 0x01A3 | 0x01A6 | 0x01A8 => Arch::SuperH,
        0x6232 => Arch::LoongArch32,
        0x6264 => Arch::LoongArch64,
        other => Arch::Other(other as u32),
    }
}

// ---------------------------------------------------------------------------
// Mach-O
// ---------------------------------------------------------------------------

fn macho_arch(cpu: u32, sub: u32) -> Arch {
    match cpu {
        7 => Arch::X86,
        0x0100_0007 => Arch::X86_64,
        12 => Arch::Arm,
        0x0100_000C => {
            let _ = sub; // arm64e is still aarch64 for our purposes
            Arch::Aarch64
        }
        18 => Arch::PowerPc,
        0x0100_0012 => Arch::PowerPc64,
        other => Arch::Other(other),
    }
}

fn macho<R: Read + Seek>(r: &mut R, info: &mut BinaryInfo) -> io::Result<()> {
    let size = info.file_size;
    let Some(h) = read_at(r, size, 0, 32)? else {
        // 32-bit header is 28 bytes; a tiny file can still be valid.
        return Ok(());
    };
    let magic = [h[0], h[1], h[2], h[3]];
    let (big, bits) = match magic {
        [0xCE, 0xFA, 0xED, 0xFE] => (false, 32u8),
        [0xCF, 0xFA, 0xED, 0xFE] => (false, 64u8),
        [0xFE, 0xED, 0xFA, 0xCE] => (true, 32u8),
        _ => (true, 64u8),
    };
    let rd = Rd { big };
    info.format = Format::MachO;
    info.bits = Some(bits);
    info.endian = Some(if big { Endian::Big } else { Endian::Little });
    info.archs = vec![macho_arch(rd.u32(&h, 4), rd.u32(&h, 8))];

    let ncmds = rd.u32(&h, 16) as u64;
    let sizeofcmds = rd.u32(&h, 20) as u64;
    let hdr_len: u64 = if bits == 64 { 32 } else { 28 };
    let mut end = hdr_len;
    bump(&mut end, hdr_len.checked_add(sizeofcmds));

    if let Some(cmds) = read_at(r, size, hdr_len, sizeofcmds)? {
        let mut pos = 0usize;
        for _ in 0..ncmds {
            if pos + 8 > cmds.len() {
                break;
            }
            let cmd = rd.u32(&cmds, pos);
            let cmdsize = rd.u32(&cmds, pos + 4) as usize;
            if cmdsize < 8 || pos + cmdsize > cmds.len() {
                break;
            }
            let c = &cmds[pos..pos + cmdsize];
            match cmd {
                0x1 if c.len() >= 56 => bump(
                    &mut end,
                    (rd.u32(c, 32) as u64).checked_add(rd.u32(c, 36) as u64),
                ),
                0x19 if c.len() >= 72 => bump(&mut end, rd.u64(c, 40).checked_add(rd.u64(c, 48))),
                0x1D if c.len() >= 16 => {
                    info.signed = true;
                    bump(
                        &mut end,
                        (rd.u32(c, 8) as u64).checked_add(rd.u32(c, 12) as u64),
                    );
                }
                _ => {}
            }
            pos += cmdsize;
        }
    }
    if end <= size {
        info.image_end = Some(end);
    }
    Ok(())
}

fn macho_fat<R: Read + Seek>(r: &mut R, info: &mut BinaryInfo) -> io::Result<()> {
    let size = info.file_size;
    let rd = Rd { big: true };
    let Some(h) = read_at(r, size, 0, 8)? else {
        return Ok(());
    };
    let is64 = h[3] == 0xBF;
    let n = rd.u32(&h, 4) as u64;
    // Java class files share the 0xCAFEBABE magic; a real fat header has few slices.
    if n == 0 || n > 32 {
        return Ok(());
    }
    let entry = if is64 { 32u64 } else { 20u64 };
    let Some(table) = read_at(r, size, 8, n * entry)? else {
        return Ok(());
    };

    let mut end = 8 + n * entry;
    let mut archs = Vec::new();
    for i in 0..n as usize {
        let b = &table[i * entry as usize..];
        archs.push(macho_arch(rd.u32(b, 0), rd.u32(b, 4)));
        let (off, len) = if is64 {
            (rd.u64(b, 8), rd.u64(b, 16))
        } else {
            (rd.u32(b, 8) as u64, rd.u32(b, 12) as u64)
        };
        bump(&mut end, off.checked_add(len));
    }
    info.format = Format::MachOUniversal;
    info.archs = archs;
    info.endian = None;
    if end <= size {
        info.image_end = Some(end);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn detect(bytes: Vec<u8>) -> BinaryInfo {
        BinaryInfo::detect(&mut Cursor::new(bytes)).unwrap()
    }

    #[test]
    fn empty_and_garbage_are_unknown() {
        assert_eq!(detect(vec![]).format, Format::Unknown);
        assert_eq!(detect(b"hello world".to_vec()).format, Format::Unknown);
        assert_eq!(detect(b"\x7fELF".to_vec()).format, Format::Unknown);
        assert_eq!(detect(b"MZ".to_vec()).format, Format::Unknown);
    }

    #[test]
    fn java_class_is_not_macho() {
        let mut v = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52];
        v.extend_from_slice(&[0; 64]);
        assert_eq!(detect(v).format, Format::Unknown);
    }

    #[test]
    fn display_names() {
        assert_eq!(Arch::X86_64.to_string(), "x86_64");
        assert_eq!(Arch::Other(0x1234).to_string(), "other(0x1234)");
        assert_eq!(Format::Pe.to_string(), "PE");
    }
}
