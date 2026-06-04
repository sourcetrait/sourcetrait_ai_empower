fn main() {
    if let Err(e) = know_rust::run() {
        eprintln!("know_rust: {}", e);
        std::process::exit(1);
    }
}
