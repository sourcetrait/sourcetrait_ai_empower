// Plugin commands are not re-exported; referenced long-hand via `crate::...`.
//
// `liquid` below is THIS crate's templating-command module; the external liquid
// crate is referenced as `::liquid` (leading colon) where needed, to disambiguate.

pub(crate) mod eye {
    pub(crate) mod md {
        pub(crate) mod find;
    }
    pub(crate) mod tree;
    #[cfg(test)]
    mod tests {
        mod tree;
    }
}
pub(crate) mod liquid {
    pub(crate) mod render;
    pub(crate) mod schema;
    pub(crate) mod from;
    pub(crate) mod soak;
    #[cfg(test)]
    mod tests {
        mod soak;
    }
}
pub(crate) mod md {
    pub(crate) mod find;
    #[cfg(test)]
    mod tests {
        mod find;
    }
}
pub(crate) mod path;
pub(crate) mod error;
pub(crate) mod plugin;

pub(crate) use crate::{
    error::{
        GlobSnafu, InvalidPatternSnafu, MarkdownResult, NuPluginGrimoireResult, ReadFileSnafu,
        ReadSnafu, TreeResult, labeled_error, nu_plugin_error,
    },
    liquid::{
        render::render_template,
        schema::validate_fill,
    },
    md::find::find,
};

pub(crate) use snafu::ResultExt;

pub(crate) use nu_protocol::CompareTypes;

pub(crate) use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

pub(crate) mod nu {
    pub(crate) use nu_parser::parse;
    pub(crate) use nu_plugin::{
        EngineInterface,
        EvaluatedCall,
        Plugin,
        PluginCommand,
        SimplePluginCommand,
    };
    pub(crate) use nu_protocol::{
        Category,
        Example,
        LabeledError,
        Record,
        Signature,
        Span,
        SyntaxShape,
        Type,
        Value,
        ast::{
            Block,
            Expr,
        },
        engine::{
            EngineState,
            StateWorkingSet,
        },
    };
}

pub use crate::{
    plugin::GrimoirePlugin,
};
