fn main() {
    if let Err(e) = rust_recon::run() {
        eprintln!("rust_recon: {}", e);
        std::process::exit(1);
    }
}
