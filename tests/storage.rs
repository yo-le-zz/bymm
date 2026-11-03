mod common;

use bymm::{Bymm, DEFAULT_LIMIT, Error, MIN_LIMIT, Options};
use common::{TempDir, all_fixtures};
use std::fs;

fn fixture() -> (TempDir, std::path::PathBuf, Vec<u8>) {
    let (bytes, _) = common::elf(62, 64, false);
    let dir = TempDir::new();
    let p = dir.file("app", &bytes);
    (dir, p, bytes)
}

#[test]
fn works_on_every_architecture_and_format_and_restores_the_original() {
    for (name, original, image_end) in all_fixtures() {
        let dir = TempDir::new();
        let path = dir.file(&name, &original);
        let before = bymm::see::info(&path).unwrap();

        let mut b = Bymm::open(&path).unwrap();
        assert!(!b.has_storage(), "{name}");
        b.write("user", "yolezz").unwrap();
        b.write("blob", vec![7u8; 5000]).unwrap();
        b.append("log", "a").unwrap();
        b.append("log", "b").unwrap();
        b.commit().unwrap();

        // The original bytes are untouched and only appended to.
        let after_bytes = fs::read(&path).unwrap();
        assert_eq!(
            &after_bytes[..original.len()],
            &original[..],
            "{name}: prefix changed"
        );
        assert!(after_bytes.len() > original.len());

        // Headers still describe the very same image.
        let after = bymm::see::info(&path).unwrap();
        assert_eq!(after.format, before.format, "{name}");
        assert_eq!(after.archs, before.archs, "{name}");
        assert_eq!(after.image_end, before.image_end, "{name}");
        if image_end != 0 {
            assert_eq!(after.image_end, Some(image_end), "{name}");
        }

        // Read back from a fresh handle.
        let b = Bymm::open(&path).unwrap();
        assert_eq!(b.original_len(), original.len() as u64, "{name}");
        assert_eq!(b.read_string("user").unwrap().as_deref(), Some("yolezz"));
        assert_eq!(b.read("blob").unwrap().unwrap(), vec![7u8; 5000]);
        assert_eq!(b.read("log").unwrap().unwrap(), b"ab");
        b.verify().unwrap();

        // strip = byte-identical original.
        bymm::strip(&path).unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            original,
            "{name}: strip did not restore the original"
        );
    }
}

#[test]
fn the_real_test_executable_still_runs_after_embedding() {
    let exe = std::env::current_exe().unwrap();
    let dir = TempDir::new();
    let copy = dir
        .path()
        .join(if cfg!(windows) { "copy.exe" } else { "copy" });
    fs::copy(&exe, &copy).unwrap();

    bymm::write(&copy, "hello", "world").unwrap();
    bymm::write(&copy, "payload", vec![1u8; 100_000]).unwrap();

    // The copy is a full test harness: listing its tests proves the loader still runs it.
    let out = std::process::Command::new(&copy)
        .arg("--list")
        .output()
        .unwrap();
    assert!(out.status.success(), "binary no longer runs: {:?}", out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("the_real_test_executable_still_runs"));
    assert_eq!(bymm::read(&copy, "hello").unwrap().unwrap(), b"world");
}

#[test]
fn nothing_is_written_before_commit_and_discard_works() {
    let (_d, path, original) = fixture();
    let mut b = Bymm::open(&path).unwrap();
    b.write("k", "v").unwrap();
    assert!(b.is_dirty());
    assert_eq!(b.read("k").unwrap().unwrap(), b"v"); // reads see staged data
    assert_eq!(fs::read(&path).unwrap(), original);
    b.discard().unwrap();
    assert!(!b.is_dirty() && !b.contains("k"));
    b.commit().unwrap(); // nothing to do
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn write_replace_append_delete_clear_across_several_commits() {
    let (_d, path, _) = fixture();
    let mut b = Bymm::open(&path).unwrap();
    b.write("a", "1").unwrap();
    b.write("b", "2").unwrap();
    b.commit().unwrap();

    b.write("a", "one").unwrap(); // replace
    b.append("b", "22").unwrap(); // data currently on disk
    b.write("c", "3").unwrap();
    b.commit().unwrap();
    assert_eq!(b.read_string("a").unwrap().unwrap(), "one");
    assert_eq!(b.read_string("b").unwrap().unwrap(), "222");

    assert!(b.delete("a").unwrap());
    assert!(!b.delete("a").unwrap());
    assert!(!b.delete("never-existed").unwrap());
    b.commit().unwrap();
    assert_eq!(b.keys().collect::<Vec<_>>(), ["b", "c"]);
    b.verify().unwrap();

    b.clear().unwrap();
    b.commit().unwrap();
    assert!(b.is_empty() && b.has_storage());
    assert_eq!(b.used(), 48);
    assert_eq!(bymm::keys(&path).unwrap(), Vec::<String>::new());

    // an empty-but-present storage keeps its limit; strip removes it
    b.set_limit(5000).unwrap();
    b.commit().unwrap();
    bymm::clear(&path).unwrap();
    assert_eq!(bymm::see::limit(&path).unwrap(), 5000);
    bymm::strip(&path).unwrap();
    assert_eq!(bymm::see::limit(&path).unwrap(), DEFAULT_LIMIT);
    assert!(!bymm::see::usage(&path).unwrap().has_storage);
}

#[test]
fn empty_values_unicode_keys_and_many_entries() {
    let (_d, path, _) = fixture();
    let mut b = Bymm::open(&path).unwrap();
    b.write("empty", b"").unwrap();
    b.write("clé/日本語/🦀", "ok").unwrap();
    for i in 0..2000 {
        b.write(&format!("key-{i:05}"), format!("value-{i}"))
            .unwrap();
    }
    b.commit().unwrap();
    let b = Bymm::open(&path).unwrap();
    assert_eq!(b.len(), 2002);
    assert_eq!(b.read("empty").unwrap().unwrap(), b"");
    assert_eq!(b.read_string("clé/日本語/🦀").unwrap().unwrap(), "ok");
    assert_eq!(b.read_string("key-01234").unwrap().unwrap(), "value-1234");
    assert_eq!(b.value_len("key-00007"), Some(7));
    b.verify().unwrap();
}

#[test]
fn big_values_and_streaming_reads() {
    let (_d, path, _) = fixture();
    let big: Vec<u8> = (0..8 * 1024 * 1024u32)
        .map(|i| (i.wrapping_mul(2654435761) >> 24) as u8)
        .collect();
    bymm::write(&path, "big", &big).unwrap();
    let b = Bymm::open(&path).unwrap();
    assert_eq!(b.read("big").unwrap().unwrap(), big);
    let mut out = Vec::new();
    assert_eq!(b.read_to("big", &mut out).unwrap(), Some(big.len() as u64));
    assert_eq!(out, big);
    assert_eq!(b.read_to("nope", &mut out).unwrap(), None);
}

#[test]
fn invalid_keys_are_rejected() {
    let (_d, path, _) = fixture();
    let mut b = Bymm::open(&path).unwrap();
    assert!(matches!(b.write("", "x"), Err(Error::InvalidKey(_))));
    assert!(matches!(
        b.write(&"k".repeat(bymm::MAX_KEY_LEN + 1), "x"),
        Err(Error::InvalidKey(_))
    ));
    b.write(&"k".repeat(bymm::MAX_KEY_LEN), "x").unwrap();
}

// ----------------------------------------------------------------------- limits

#[test]
fn limit_can_be_seen_set_and_is_enforced() {
    let (_d, path, _) = fixture();
    let mut b = Bymm::open(&path).unwrap();
    assert_eq!(b.limit(), DEFAULT_LIMIT);
    assert_eq!(b.used(), 0);

    assert!(matches!(
        b.set_limit(MIN_LIMIT - 1),
        Err(Error::InvalidLimit { .. })
    ));
    b.set_limit(200).unwrap();
    assert_eq!(b.free(), 200 - 48);

    // Exactly the biggest value fits, one more byte does not.
    let max = b.max_value_len("data").unwrap();
    assert_eq!(max, 200 - 48 - 16 - 4);
    assert!(matches!(
        b.write("data", vec![0u8; max as usize + 1]),
        Err(Error::LimitExceeded {
            limit: 200,
            required: 201
        })
    ));
    assert!(!b.contains("data"));
    b.write("data", vec![0u8; max as usize]).unwrap();
    assert_eq!(b.free(), 0);
    assert_eq!(b.used(), 200);
    b.commit().unwrap();

    // Persisted inside the binary.
    let mut b = Bymm::open(&path).unwrap();
    assert_eq!(b.limit(), 200);
    assert_eq!(b.usage().used, 200);
    assert!(matches!(
        b.write("more", "x"),
        Err(Error::LimitExceeded { .. })
    ));
    assert!(matches!(
        b.append("data", "x"),
        Err(Error::LimitExceeded { .. })
    ));
    // Replacing with something no bigger is fine.
    b.write("data", vec![1u8; 10]).unwrap();
    // Cannot shrink under what is used.
    assert!(matches!(
        b.set_limit(60),
        Err(Error::InvalidLimit { minimum: 78, .. })
    ));
    b.set_limit(78).unwrap();
    b.set_unlimited().unwrap();
    assert!(b.is_unlimited());
    b.write("huge", vec![0u8; 100_000]).unwrap();
    b.commit().unwrap();
    assert_eq!(bymm::see::limit(&path).unwrap(), u64::MAX);
    assert!(bymm::see::usage(&path).unwrap().is_unlimited());
}

#[test]
fn max_value_len_accounts_for_replacement() {
    let (_d, path, _) = fixture();
    let mut b = Bymm::open(&path).unwrap();
    b.set_limit(300).unwrap();
    b.write("x", vec![0u8; 100]).unwrap();
    let max = b.max_value_len("x").unwrap();
    b.write("x", vec![0u8; max as usize]).unwrap();
    assert_eq!(b.free(), 0);
}

#[test]
fn default_limit_option_applies_to_new_storage_only() {
    let (_d, path, _) = fixture();
    let mut b = Bymm::open_with(&path, Options::new().default_limit(1000)).unwrap();
    assert_eq!(b.limit(), 1000);
    b.write("k", "v").unwrap();
    b.commit().unwrap();
    let b = Bymm::open_with(&path, Options::new().default_limit(5000)).unwrap();
    assert_eq!(b.limit(), 1000); // the storage keeps its own limit
    assert!(matches!(
        Bymm::open_with(&path, Options::new().default_limit(1)),
        Err(Error::InvalidLimit { .. })
    ));
}

// -------------------------------------------------------------- robustness

#[test]
fn corruption_is_detected() {
    let (_d, path, _) = fixture();
    bymm::write(&path, "key", "some important value").unwrap();
    let good = fs::read(&path).unwrap();

    // flip a byte of the value
    let mut bad = good.clone();
    let at = bad.len() - 16 - 5;
    bad[at] ^= 0xFF;
    fs::write(&path, &bad).unwrap();
    let b = Bymm::open(&path).unwrap(); // index is fine…
    assert!(matches!(b.read("key"), Err(Error::Corrupt(_)))); // …data is not
    assert!(matches!(b.verify(), Err(Error::Corrupt(_))));
    assert!(matches!(
        b.read_to("key", &mut Vec::new()),
        Err(Error::Corrupt(_))
    ));

    // flip a byte of the header
    let mut bad = good.clone();
    let header_at = bad.len() - 16 - (32 + 16 + 3 + 20);
    bad[header_at + 9] ^= 0x01;
    fs::write(&path, &bad).unwrap();
    assert!(matches!(Bymm::open(&path), Err(Error::Corrupt(_))));

    // footer pointing outside the file
    let mut bad = good.clone();
    let n = bad.len();
    bad[n - 16..n - 8].copy_from_slice(&u64::MAX.to_le_bytes());
    fs::write(&path, &bad).unwrap();
    assert!(matches!(Bymm::open(&path), Err(Error::Corrupt(_))));

    // future version
    let mut bad = good.clone();
    bad[header_at + 4] = 9;
    fs::write(&path, &bad).unwrap();
    assert!(matches!(
        Bymm::open(&path),
        Err(Error::UnsupportedVersion(9))
    ));

    // a lost footer simply means "no storage": the binary is still fine
    fs::write(&path, &good[..good.len() - 1]).unwrap();
    assert!(!Bymm::open(&path).unwrap().has_storage());
}

#[test]
fn binaries_with_an_existing_overlay_are_preserved() {
    let (mut bytes, _) = common::elf(183, 64, false);
    bytes.extend((0..1000).map(|i| (i % 7) as u8)); // e.g. an installer payload
    let dir = TempDir::new();
    let path = dir.file("app", &bytes);
    bymm::write(&path, "k", "v").unwrap();
    assert_eq!(&fs::read(&path).unwrap()[..bytes.len()], &bytes[..]);
    bymm::strip(&path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn concurrent_modification_is_refused() {
    let (_d, path, original) = fixture();
    let mut b = Bymm::open(&path).unwrap();
    b.write("k", "v").unwrap();
    let mut changed = original.clone();
    changed.push(0);
    fs::write(&path, &changed).unwrap(); // someone else edits the file
    assert!(matches!(b.commit(), Err(Error::Modified)));
    assert_eq!(fs::read(&path).unwrap(), changed); // and nothing was clobbered
    // no temporary file is left behind
    let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".bymm-tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn signed_binaries_are_refused_unless_allowed() {
    let (bytes, _) = common::pe(0x8664, true, true);
    let dir = TempDir::new();
    let path = dir.file("signed.exe", &bytes);
    assert!(Bymm::open(&path).unwrap().info().signed);
    assert!(matches!(
        bymm::write(&path, "k", "v"),
        Err(Error::SignedBinary)
    ));
    assert_eq!(fs::read(&path).unwrap(), bytes);

    let mut b = Bymm::open_with(&path, Options::new().allow_signed(true)).unwrap();
    b.write("k", "v").unwrap();
    b.commit().unwrap();
    assert_eq!(bymm::read(&path, "k").unwrap().unwrap(), b"v");
}

#[test]
fn backup_option_keeps_the_previous_binary() {
    let (_d, path, original) = fixture();
    let mut b = Bymm::open_with(&path, Options::new().backup(true)).unwrap();
    b.write("k", "v").unwrap();
    b.commit().unwrap();
    let bak = path.with_file_name("app.bymm.bak");
    assert_eq!(fs::read(&bak).unwrap(), original);
}

#[test]
fn a_directory_or_missing_file_is_an_error() {
    let dir = TempDir::new();
    assert!(matches!(
        Bymm::open(dir.path().join("nope")),
        Err(Error::Io(_))
    ));
    assert!(Bymm::open(dir.path()).is_err());
}

#[cfg(unix)]
#[test]
fn unix_permissions_and_symlinks_are_respected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (dir, path, _) = fixture();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();
    bymm::write(&path, "k", "v").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
        0o751
    );

    let link = dir.path().join("link");
    symlink(&path, &link).unwrap();
    bymm::write(&link, "via", "link").unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(bymm::read(&path, "via").unwrap().unwrap(), b"link");
}

#[test]
fn error_converts_to_io_error() {
    let e: std::io::Error = Error::LimitExceeded {
        limit: 1,
        required: 2,
    }
    .into();
    assert_eq!(e.kind(), std::io::ErrorKind::StorageFull);
    assert!(e.to_string().contains("limit"));
}
