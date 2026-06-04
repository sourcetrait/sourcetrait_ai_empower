fn main() {
    if let Err(e) = rust_recon_scan::run() {
        eprintln!("rust_recon_scan: {}", e);
        std::process::exit(1);
    }
}
