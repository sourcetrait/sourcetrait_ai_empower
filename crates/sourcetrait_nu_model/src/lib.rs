pub(crate) mod model;
pub(crate) mod plugin;
#[cfg(test)]
mod tests;

pub use crate::{
    model::{
        NuModel, NuModelMeta, NuModelSummary, NuModelDetails, NuModelNamepath,
        NuModelVersion,
    },
    plugin::NuModelPlugin,
};

#[allow(unused)]
pub(crate) use std::{
    fs,
};

pub(crate) mod nu {
    pub(crate) use nu_protocol::{
        SyntaxShape, Span,
        engine::{
            EngineState, StateWorkingSet,
        },
    };
    pub(crate) use nu_parser::{
        parse_shape_name,
    };
    pub(crate) use nu_plugin::{
        Plugin,
        PluginCommand,
    };
}

