use crate::*;

/// Supports Nushell MCP run/call/interact source-code with common utilities.
/// 
/// Each plugin Command's name and description are listed via the MCP's `info()`.
pub struct GrimoirePlugin;

impl nu::Plugin for GrimoirePlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn nu::PluginCommand<Plugin = Self>>> {
        vec![
            Box::new(crate::eye::md::find::Command),
            Box::new(crate::eye::tree::Command),
            Box::new(crate::liquid::from::Command),
            Box::new(crate::liquid::soak::Command),
        ]
    }
}
