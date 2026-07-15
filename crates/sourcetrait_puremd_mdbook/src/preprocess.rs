use crate::*;

// The puremd preprocessor: reduce every chapter to pure markdown by stripping
// unwanted HTML, keeping only bare table elements.
pub(crate) struct Puremd;

impl Preprocessor for Puremd {
    fn name(&self) -> &str {
        "puremd"
    }

    fn run(
        &self,
        _ctx: &mdbook::PreprocessorContext,
        mut book: mdbook::Book,
    ) -> mdbook::MdResult<mdbook::Book> {
        book.for_each_chapter_mut(|chapter| {
            chapter.content = transform_markdown(&chapter.content);
        });
        Ok(book)
    }
}

// Read the `[context, book]` JSON from stdin, run the preprocessor, and write
// the processed book back to stdout as JSON.
pub fn handle_preprocessing() -> mdbook::MdResult<()> {
    let (context, book) = mdbook::parse_input(std::io::stdin())?;
    let processed = Puremd.run(&context, book)?;
    serde_json::to_writer(std::io::stdout(), &processed)?;
    Ok(())
}
