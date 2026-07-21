fn main() {
    if let Err(e) = sourcetrait_grammar_cert::run() {
        eprintln!("grammar_cert: {e}");
        std::process::exit(1);
    }
}
