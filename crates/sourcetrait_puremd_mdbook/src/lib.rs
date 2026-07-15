mod html;
mod transform;
mod preprocess;

#[cfg(test)]
mod tests {
    mod transform;
    mod html;
}

pub(crate) use crate::html::clean_html;

pub(crate) use std::ops::Range;

// The Preprocessor trait in global scope so `Puremd.run(..)` dispatches.
pub(crate) use mdbook_preprocessor::Preprocessor;

pub(crate) mod pd {
    pub(crate) use pulldown_cmark::{
        Event,
        Options,
        Parser,
    };
}

pub(crate) mod mdbook {
    pub(crate) use mdbook_preprocessor::{
        PreprocessorContext,
        parse_input,
        book::Book,
        errors::Result as MdResult,
    };
}

pub use crate::{
    preprocess::handle_preprocessing,
    transform::transform_markdown,
};
