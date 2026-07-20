fn main() {
    if let Err(err) = sourcetrait_grammar_arxivmd::run() {
        eprintln!("arxivmd: {err}");
        std::process::exit(1);
    }
}
