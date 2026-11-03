#[path = "../common/mod.rs"]
mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TEST WRITE ===");
    let path = common::working_copy("t_write")?;
    let info = bymm::see::info(&path)?;
    println!("detected: {} {} ({} bytes)", info.format, info.arch(), info.file_size);

    bymm::write(&path, "username", "yolezz")?;
    bymm::write(&path, "username", "yolezz2")?; // replace
    common::check(bymm::read_string(&path, "username")?.as_deref() == Some("yolezz2"), "value replaced")?;
    println!("OK");
    Ok(())
}
