pub mod commands;
pub(crate) mod error;

pub(crate) use std::path::PathBuf;

use nu_plugin::{Plugin, PluginCommand};

pub(crate) use nu_plugin::{
    EngineInterface,
    EvaluatedCall,
    SimplePluginCommand,
};

pub(crate) use nu_protocol::{
    Category,
    Example,
    LabeledError,
    Signature,
    SyntaxShape,
    Value,
};

pub(crate) use sourcetrait_cmdlib_empower::markdown;

pub use crate::error::{
    NuPluginEmpowerError,
    NuPluginEmpowerResult,
};

pub struct EmpowerPlugin;

impl Plugin for EmpowerPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![
            Box::new(commands::peek::MdFind),
        ]
    }
}
