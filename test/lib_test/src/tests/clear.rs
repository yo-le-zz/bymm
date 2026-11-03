#[path = "../common/mod.rs"]
mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TEST CLEAR / STRIP ===");
    let original = std::fs::read(common::fake_binary())?;
    let path = common::working_copy("t_clear")?;
    bymm::write(&path, "a", "1")?;
    bymm::clear(&path)?;
    common::check(bymm::keys(&path)?.is_empty(), "cleared")?;
    bymm::strip(&path)?;
    common::check(std::fs::read(&path)? == original, "strip restores the original binary byte for byte")?;
    println!("OK");
    Ok(())
}
