fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        // The renderer-support probe: puremd cleans HTML for any backend.
        Some("supports") => std::process::exit(0),
        Some(arg) => {
            eprintln!("unknown argument: {arg}");
            std::process::exit(1);
        }
        None => {}
    }

    if let Err(err) = sourcetrait_grammar_puremd_mdbook::handle_preprocessing() {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}
