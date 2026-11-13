//! Print the commands an entry would run, as a SolidCraft script (for timing and debugging):
//! `cargo run --example plan -- bin '{"width":2}'`. `@pattern` and `@created` are left as they are.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let params: serde_json::Value = serde_json::from_str(a.get(2).map(String::as_str).unwrap_or("{}")).unwrap();
    match gridfinity::plan(&a[1], &params) {
        Ok((_, cmds)) => println!("{}", serde_json::json!({ "commands": cmds })),
        Err(e) => eprintln!("{e}"),
    }
}
