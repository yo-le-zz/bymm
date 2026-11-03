#[path = "../common/mod.rs"]
mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TEST APPEND ===");
    let path = common::working_copy("t_append")?;
    bymm::append(&path, "log", "one;")?;
    bymm::append(&path, "log", "two;")?;
    common::check(bymm::read(&path, "log")?.as_deref() == Some(&b"one;two;"[..]), "appended")?;
    println!("OK");
    Ok(())
}
