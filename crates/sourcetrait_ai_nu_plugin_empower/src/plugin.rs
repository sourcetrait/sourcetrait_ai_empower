use crate::*;

pub struct EmpowerPlugin;

impl nu::Plugin for EmpowerPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn nu::PluginCommand<Plugin = Self>>> {
        vec![
            Box::new(MdFind),
        ]
    }
}
