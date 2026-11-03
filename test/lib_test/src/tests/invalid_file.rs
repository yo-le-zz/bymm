#[path = "../common/mod.rs"]
mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TEST INVALID FILES ===");
    let dir = common::test_directory();
    std::fs::create_dir_all(&dir)?;

    common::check(bymm::read(dir.join("does-not-exist"), "k").is_err(), "missing file is an error")?;

    // Any file is usable, even plain text.
    let text = dir.join("plain.txt");
    std::fs::write(&text, "not a binary at all")?;
    bymm::write(&text, "k", "v")?;
    common::check(bymm::read_string(&text, "k")?.as_deref() == Some("v"), "unknown formats work too")?;

    // A damaged storage is reported, not silently ignored.
    let mut bytes = std::fs::read(&text)?;
    let n = bytes.len();
    bytes[n - 20] ^= 0xFF;
    std::fs::write(&text, bytes)?;
    common::check(bymm::verify(&text).is_err(), "corruption is detected")?;
    println!("OK");
    Ok(())
}
