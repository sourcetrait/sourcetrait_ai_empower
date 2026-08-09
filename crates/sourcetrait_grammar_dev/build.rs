use std::path::Path;
fn main() {
    println!("cargo::rerun-if-changed={}", Path::new("assets").to_str().unwrap());
}