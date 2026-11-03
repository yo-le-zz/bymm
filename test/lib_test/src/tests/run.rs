#[path = "../common/mod.rs"]
mod common;

use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TEST RUN (the modified binary must still execute) ===");
    let path = common::working_copy("t_run")?;
    bymm::write(&path, "message", "I live inside the executable")?;

    let output = Command::new(&path).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    common::check(output.status.success(), "the binary still runs")?;
    common::check(stdout.contains("I live inside the executable"), "it reads its own embedded data")?;
    println!("OK");
    Ok(())
}
