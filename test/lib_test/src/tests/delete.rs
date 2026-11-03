#[path = "../common/mod.rs"]
mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TEST DELETE ===");
    let path = common::working_copy("t_delete")?;
    bymm::write(&path, "a", "1")?;
    bymm::write(&path, "b", "2")?;
    common::check(bymm::delete(&path, "a")?, "a existed")?;
    common::check(!bymm::delete(&path, "a")?, "a is gone")?;
    common::check(bymm::keys(&path)? == ["b"], "only b is left")?;
    println!("OK");
    Ok(())
}
