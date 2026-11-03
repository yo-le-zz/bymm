//! A program that remembers how many times it was launched — inside its own file.
//!
//! ```text
//! cargo build --example counter
//! cp target/debug/examples/counter ./counter     # work on a copy
//! ./counter    # "launched 0 time(s) before"
//! ./counter    # "launched 1 time(s) before"
//! ```

fn main() -> bymm::Result<()> {
    let mut me = bymm::Bymm::open_self()?;

    let before: u32 = me
        .read_string("launches")?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    println!("launched {before} time(s) before");

    me.write("launches", (before + 1).to_string())?;
    me.commit()?; // rewrites this very executable (atomically)
    Ok(())
}
