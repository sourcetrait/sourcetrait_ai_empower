pub type Result<T> = std::result::Result<T, ArxivmdError>;

/// Failures fetching or converting an arXiv paper.
///
/// Constructed directly at the call sites (the underlying reqwest / io / process
/// errors are flattened to a message), so there are no snafu context selectors.
#[derive(Debug, snafu::Snafu)]
pub enum ArxivmdError {
    #[snafu(display("network error: {message}"))]
    Network { message: String },

    #[snafu(display("arXiv paper not found: {id}"))]
    NotFound { id: String },

    #[snafu(display("{id}: no e-print source, and the pdftotext fallback failed: {message}"))]
    PdfFallback { id: String, message: String },

    #[snafu(display("conversion failed: {message}"))]
    Convert { message: String },

    #[snafu(display("io error ({context}): {source}"))]
    Io {
        context: String,
        source: std::io::Error,
    },

    #[snafu(display("nuon serialization failed: {message}"))]
    Nuon { message: String },
}
