fn main() {
    if let Err(e) = sourcetrait_grammar_know_rust::run() {
        eprintln!("know_rust: {}", e);
        std::process::exit(1);
    }
}
