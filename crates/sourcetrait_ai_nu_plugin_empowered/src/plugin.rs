use crate::*;

/// Supports Nushell MCP run/call/interact source-code with common utilities.
/// 
/// Each plugin Command's name and description are listed via the MCP's `info()`.
pub struct EmpowerPlugin;

impl nu::Plugin for EmpowerPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn nu::PluginCommand<Plugin = Self>>> {
        vec![
            Box::new(crate::eye::md::find::Command),
            //todo:ai: Box::new(crate::shm::dir::Command),
            //todo:ai: Box::new(crate::shm::file::Command),
            //todo:ai: Box::new(crate::shm::path::Command),
            //todo:ai: Box::new(crate::shm::release::Command),
            //todo:ai: Box::new(crate::shm::result::Command),
        ]
    }
}
