//! A tiny program that reads the data embedded in *itself* with bymm.

fn main() {
    println!("Hello from fake executable!");

    match bymm::Bymm::open_self() {
        Ok(me) if me.has_storage() => {
            println!("embedded storage: {}", me.usage());
            for (key, len) in me.entries() {
                let preview = me.read_string(key).ok().flatten().unwrap_or_else(|| "<binary>".into());
                println!("  {key} ({len} bytes) = {preview}");
            }
        }
        Ok(_) => println!("no embedded storage"),
        Err(e) => println!("cannot read embedded storage: {e}"),
    }
}
