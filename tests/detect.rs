mod common;

use bymm::{Arch, BinaryInfo, Endian, Format};
use std::io::Cursor;

fn detect(bytes: &[u8]) -> BinaryInfo {
    BinaryInfo::detect(&mut Cursor::new(bytes)).unwrap()
}

#[test]
fn elf_architectures_bits_and_endianness() {
    use Arch::*;
    let cases: &[(u16, u8, bool, Arch)] = &[
        (3, 32, false, X86),
        (62, 64, false, X86_64),
        (40, 32, false, Arm),
        (183, 64, false, Aarch64),
        (243, 32, false, Riscv32),
        (243, 64, false, Riscv64),
        (8, 32, true, Mips),
        (8, 64, true, Mips64),
        (20, 32, true, PowerPc),
        (21, 64, true, PowerPc64),
        (22, 64, true, S390x),
        (43, 64, true, Sparc64),
        (258, 64, false, LoongArch64),
        (258, 32, false, LoongArch32),
        (50, 64, false, Ia64),
        (4, 32, true, M68k),
        (42, 32, false, SuperH),
        (9999, 64, false, Other(9999)),
    ];
    for &(machine, bits, big, arch) in cases {
        let (bytes, end) = common::elf(machine, bits, big);
        let i = detect(&bytes);
        assert_eq!(i.format, Format::Elf, "machine {machine}");
        assert_eq!(i.arch(), arch, "machine {machine}");
        assert_eq!(i.bits, Some(bits));
        assert_eq!(
            i.endian,
            Some(if big { Endian::Big } else { Endian::Little })
        );
        assert_eq!(i.image_end, Some(end), "image end for machine {machine}");
        assert_eq!(i.file_size, bytes.len() as u64);
        assert!(!i.signed);
    }
}

#[test]
fn pe_architectures_and_signature() {
    let cases: &[(u16, bool, Arch)] = &[
        (0x14C, false, Arch::X86),
        (0x8664, true, Arch::X86_64),
        (0x1C4, false, Arch::Arm),
        (0xAA64, true, Arch::Aarch64),
        (0xA641, true, Arch::Arm64Ec),
        (0x5064, true, Arch::Riscv64),
        (0x6264, true, Arch::LoongArch64),
        (0x0200, true, Arch::Ia64),
    ];
    for &(machine, plus, arch) in cases {
        for signed in [false, true] {
            let (bytes, end) = common::pe(machine, plus, signed);
            let i = detect(&bytes);
            assert_eq!(i.format, Format::Pe);
            assert_eq!(i.arch(), arch);
            assert_eq!(i.bits, Some(if plus { 64 } else { 32 }));
            assert_eq!(i.endian, Some(Endian::Little));
            assert_eq!(i.signed, signed, "signed flag for {machine:#x}");
            assert_eq!(i.image_end, Some(end));
        }
    }
}

#[test]
fn macho_thin_and_universal() {
    let (b, end) = common::macho(0x0100_0007, false);
    let i = detect(&b);
    assert_eq!(
        (i.format, i.arch(), i.bits),
        (Format::MachO, Arch::X86_64, Some(64))
    );
    assert_eq!(i.image_end, Some(end));
    assert!(!i.signed);

    let (b, end) = common::macho(0x0100_000C, true);
    let i = detect(&b);
    assert_eq!(i.arch(), Arch::Aarch64);
    assert!(i.signed);
    assert_eq!(i.image_end, Some(end));

    let (b, end) = common::macho_fat();
    let i = detect(&b);
    assert_eq!(i.format, Format::MachOUniversal);
    assert_eq!(i.archs, vec![Arch::X86_64, Arch::Aarch64]);
    assert_eq!(i.image_end, Some(end));
}

#[test]
fn overlay_is_visible_as_the_gap_after_image_end() {
    let (mut bytes, end) = common::elf(62, 64, false);
    bytes.extend_from_slice(&[0xAB; 777]);
    let i = detect(&bytes);
    assert_eq!(i.image_end, Some(end));
    assert_eq!(i.file_size - end, 777);
}

#[test]
fn truncated_or_garbage_files_never_error() {
    let (full, _) = common::elf(62, 64, false);
    for cut in 0..full.len() {
        let _ = detect(&full[..cut]); // must not panic
    }
    let (full, _) = common::pe(0x8664, true, true);
    for cut in (0..full.len()).step_by(7) {
        let _ = detect(&full[..cut]);
    }
    assert_eq!(detect(b"just some text").format, Format::Unknown);
    assert_eq!(detect(b"just some text").arch(), Arch::Unknown);
}

#[test]
fn the_running_test_binary_is_detected() {
    let exe = std::env::current_exe().unwrap();
    let info = bymm::see::info(&exe).unwrap();
    if cfg!(target_os = "linux") {
        assert_eq!(info.format, Format::Elf);
    }
    if cfg!(windows) {
        assert_eq!(info.format, Format::Pe);
    }
    if cfg!(target_arch = "x86_64") && !cfg!(target_os = "macos") {
        assert_eq!(info.arch(), Arch::X86_64);
    }
    if cfg!(target_arch = "aarch64") && !cfg!(target_os = "macos") {
        assert_eq!(info.arch(), Arch::Aarch64);
    }
}
