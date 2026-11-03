#[path = "../common/mod.rs"]
mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TEST LIMIT ===");
    let path = common::working_copy("t_limit")?;
    println!("default limit: {} bytes", bymm::see::limit(&path)?);

    bymm::set_limit(&path, 1000)?;
    common::check(bymm::see::limit(&path)? == 1000, "limit saved in the binary")?;
    let too_big = vec![0u8; 2000];
    common::check(bymm::write(&path, "big", &too_big).is_err(), "write over the limit is refused")?;
    println!("usage: {}", bymm::see::usage(&path)?);
    println!("OK");
    Ok(())
}
