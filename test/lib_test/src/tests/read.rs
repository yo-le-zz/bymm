#[path = "../common/mod.rs"]
mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TEST READ ===");
    let path = common::working_copy("t_read")?;
    common::check(bymm::read(&path, "nothing")?.is_none(), "no storage yet")?;
    bymm::write(&path, "a", [1u8, 2, 3])?;
    common::check(bymm::read(&path, "a")?.as_deref() == Some(&[1u8, 2, 3][..]), "binary value")?;
    common::check(bymm::read(&path, "b")?.is_none(), "missing key")?;
    bymm::verify(&path)?;
    println!("OK");
    Ok(())
}
