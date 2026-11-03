//! Shared helpers: temp directories and synthetic executables for many architectures.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

/// A temporary directory removed on drop.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("bymm-test-{}-{n}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    pub fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, bytes).unwrap();
        p
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------- byte helpers

struct W {
    big: bool,
    v: Vec<u8>,
}

impl W {
    fn u16(&mut self, x: u16) {
        self.v.extend_from_slice(&if self.big {
            x.to_be_bytes()
        } else {
            x.to_le_bytes()
        });
    }
    fn u32(&mut self, x: u32) {
        self.v.extend_from_slice(&if self.big {
            x.to_be_bytes()
        } else {
            x.to_le_bytes()
        });
    }
    fn u64(&mut self, x: u64) {
        self.v.extend_from_slice(&if self.big {
            x.to_be_bytes()
        } else {
            x.to_le_bytes()
        });
    }
    fn zeros(&mut self, n: usize) {
        self.v.resize(self.v.len() + n, 0);
    }
    fn pad_to(&mut self, n: usize) {
        assert!(self.v.len() <= n);
        self.v.resize(n, 0);
    }
}

/// Builds a minimal but structurally valid ELF.
/// Returns the bytes and the offset where the image (as the headers see it) ends.
pub fn elf(machine: u16, bits: u8, big: bool) -> (Vec<u8>, u64) {
    let mut w = W { big, v: Vec::new() };
    let b64 = bits == 64;
    let (ehsize, phentsize, shentsize) = if b64 {
        (64u16, 56u16, 64u16)
    } else {
        (52, 32, 40)
    };
    let data_off = ehsize as u64 + phentsize as u64;
    let data_len = 100u64;
    let shoff = data_off + data_len;
    let shnum = 2u16;

    w.v.extend_from_slice(&[0x7F, b'E', b'L', b'F']);
    w.v.push(if b64 { 2 } else { 1 });
    w.v.push(if big { 2 } else { 1 });
    w.v.push(1); // EI_VERSION
    w.zeros(9);
    w.u16(2); // e_type = EXEC
    w.u16(machine);
    w.u32(1);
    if b64 {
        w.u64(0x400000);
        w.u64(ehsize as u64); // e_phoff
        w.u64(shoff);
    } else {
        w.u32(0x400000);
        w.u32(ehsize as u32);
        w.u32(shoff as u32);
    }
    w.u32(0); // flags
    w.u16(ehsize);
    w.u16(phentsize);
    w.u16(1); // phnum
    w.u16(shentsize);
    w.u16(shnum);
    w.u16(0); // shstrndx
    assert_eq!(w.v.len(), ehsize as usize);

    // one PT_LOAD covering headers + data
    let filesz = data_off + data_len;
    if b64 {
        w.u32(1);
        w.u32(5);
        w.u64(0); // offset
        w.u64(0x400000);
        w.u64(0x400000);
        w.u64(filesz);
        w.u64(filesz);
        w.u64(0x1000);
    } else {
        w.u32(1);
        w.u32(0);
        w.u32(0x400000);
        w.u32(0x400000);
        w.u32(filesz as u32);
        w.u32(filesz as u32);
        w.u32(5);
        w.u32(0x1000);
    }
    // "code"
    for i in 0..data_len as usize {
        w.v.push((i % 251) as u8);
    }
    assert_eq!(w.v.len() as u64, shoff);
    // section 0: null; section 1: .text (PROGBITS)
    w.zeros(shentsize as usize);
    let start = w.v.len();
    w.u32(0);
    w.u32(1);
    if b64 {
        w.u64(6);
        w.u64(0x400000 + data_off);
        w.u64(data_off);
        w.u64(data_len);
    } else {
        w.u32(6);
        w.u32(0x400000 + data_off as u32);
        w.u32(data_off as u32);
        w.u32(data_len as u32);
    }
    w.pad_to(start + shentsize as usize);
    let end = w.v.len() as u64;
    (w.v, end)
}

/// Builds a minimal PE (PE32 or PE32+), optionally with a certificate table.
/// Returns the bytes and the end of the image as the headers describe it.
pub fn pe(machine: u16, plus: bool, signed: bool) -> (Vec<u8>, u64) {
    let mut w = W {
        big: false,
        v: Vec::new(),
    };
    w.v.extend_from_slice(b"MZ");
    w.zeros(0x3C - 2);
    w.u32(0x40); // e_lfanew
    w.v.extend_from_slice(b"PE\0\0");
    let opt_size: u16 = if plus { 112 + 128 } else { 96 + 128 };
    w.u16(machine);
    w.u16(1); // sections
    w.u32(0);
    w.u32(0); // no symbol table
    w.u32(0);
    w.u16(opt_size);
    w.u16(0x0022);
    let opt_start = w.v.len();
    w.u16(if plus { 0x20B } else { 0x10B });
    w.pad_to(opt_start + 60);
    w.u32(0x200); // SizeOfHeaders
    let nrva_off = if plus { 108 } else { 92 };
    w.pad_to(opt_start + nrva_off);
    w.u32(16);
    let dd = opt_start + nrva_off + 4;
    w.pad_to(dd + 4 * 8);
    let raw_end = 0x400u32;
    let cert_len = 0x40u32;
    if signed {
        w.u32(raw_end);
        w.u32(cert_len);
    } else {
        w.u32(0);
        w.u32(0);
    }
    w.pad_to(opt_start + opt_size as usize);
    // section header
    w.v.extend_from_slice(b".text\0\0\0");
    w.u32(0x100);
    w.u32(0x1000);
    w.u32(0x200); // SizeOfRawData
    w.u32(0x200); // PointerToRawData
    w.zeros(16);
    w.pad_to(0x200);
    for i in 0..0x200usize {
        w.v.push((i % 253) as u8);
    }
    let mut end = raw_end as u64;
    if signed {
        w.v.extend(std::iter::repeat_n(0xCC, cert_len as usize));
        end += cert_len as u64;
    }
    (w.v, end)
}

/// Thin 64-bit little-endian Mach-O with one segment (and optional code signature).
pub fn macho(cpu: u32, signed: bool) -> (Vec<u8>, u64) {
    let mut w = W {
        big: false,
        v: Vec::new(),
    };
    let ncmds = if signed { 2 } else { 1 };
    let sizeofcmds = 72 + if signed { 16 } else { 0 };
    w.u32(0xFEEDFACF);
    w.u32(cpu);
    w.u32(0);
    w.u32(2);
    w.u32(ncmds);
    w.u32(sizeofcmds);
    w.u32(0);
    w.u32(0);
    // LC_SEGMENT_64
    w.u32(0x19);
    w.u32(72);
    w.v.extend_from_slice(b"__TEXT\0\0\0\0\0\0\0\0\0\0");
    w.u64(0x1_0000_0000);
    w.u64(0x1000);
    w.u64(0); // fileoff
    w.u64(0x1000); // filesize
    w.zeros(72 - 56);
    if signed {
        w.u32(0x1D);
        w.u32(16);
        w.u32(0x1000);
        w.u32(0x80);
    }
    w.pad_to(0x1000);
    let mut end = 0x1000u64;
    if signed {
        w.zeros(0x80);
        end += 0x80;
    }
    (w.v, end)
}

/// Universal Mach-O with two slices.
pub fn macho_fat() -> (Vec<u8>, u64) {
    let mut w = W {
        big: true,
        v: Vec::new(),
    };
    w.u32(0xCAFEBABE);
    w.u32(2);
    for (cpu, off) in [(0x0100_0007u32, 0x1000u32), (0x0100_000C, 0x2000)] {
        w.u32(cpu);
        w.u32(0);
        w.u32(off);
        w.u32(0x1000);
        w.u32(12);
    }
    w.pad_to(0x3000);
    (w.v, 0x3000)
}

/// Every synthetic executable: (name, bytes, expected image end).
pub fn all_fixtures() -> Vec<(String, Vec<u8>, u64)> {
    let mut out = Vec::new();
    // ELF: (e_machine, name, bits, big-endian)
    let elf_cases: &[(u16, &str, u8, bool)] = &[
        (3, "x86", 32, false),
        (62, "x86_64", 64, false),
        (40, "arm", 32, false),
        (183, "aarch64", 64, false),
        (243, "riscv32", 32, false),
        (243, "riscv64", 64, false),
        (8, "mips", 32, true),
        (8, "mipsel", 32, false),
        (8, "mips64", 64, true),
        (20, "powerpc", 32, true),
        (21, "powerpc64", 64, true),
        (21, "powerpc64le", 64, false),
        (22, "s390x", 64, true),
        (43, "sparc64", 64, true),
        (258, "loongarch64", 64, false),
        (258, "loongarch32", 32, false),
        (50, "ia64", 64, false),
        (4, "m68k", 32, true),
        (42, "superh", 32, false),
    ];
    for &(m, name, bits, big) in elf_cases {
        let (b, end) = elf(m, bits, big);
        out.push((format!("elf-{name}"), b, end));
    }
    let pe_cases: &[(u16, &str, bool)] = &[
        (0x14C, "x86", false),
        (0x8664, "x86_64", true),
        (0x1C4, "arm", false),
        (0xAA64, "aarch64", true),
        (0xA641, "arm64ec", true),
        (0x5064, "riscv64", true),
        (0x6264, "loongarch64", true),
    ];
    for &(m, name, plus) in pe_cases {
        let (b, end) = pe(m, plus, false);
        out.push((format!("pe-{name}"), b, end));
    }
    for (name, cpu) in [("x86_64", 0x0100_0007u32), ("aarch64", 0x0100_000C)] {
        let (b, end) = macho(cpu, false);
        out.push((format!("macho-{name}"), b, end));
    }
    let (b, end) = macho_fat();
    out.push(("macho-universal".into(), b, end));
    // Not an executable at all.
    out.push(("unknown-text".into(), b"#!/bin/sh\necho hi\n".to_vec(), 0));
    out.push(("unknown-empty".into(), Vec::new(), 0));
    out
}
