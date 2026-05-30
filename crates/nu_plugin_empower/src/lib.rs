pub(crate) mod commands;
pub(crate) mod error;

pub use crate::error::{
    NuPluginEmpowerError,
    NuPluginEmpowerResult,
};

use nu_plugin::{Plugin, PluginCommand};

pub struct EmpowerPlugin;

impl Plugin for EmpowerPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![]
    }
}
