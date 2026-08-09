use crate::*;

pub struct NuModelPlugin;

impl nu::Plugin for NuModelPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn nu::PluginCommand<Plugin = Self>>> {
        vec![
        ]
    }
}
