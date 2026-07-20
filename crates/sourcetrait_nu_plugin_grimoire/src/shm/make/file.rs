//! ## Nonce
//! IF `$env.NONCE` exists THEN we use that ELSE we generate a base62

use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = GrimoirePlugin;

    fn name(&self) -> &str {
        "shm make file"
    }

    fn description(&self) -> &str {
        "Stubs a unique tmpfs file within $env.XDGX_SHM_DIR for use with IPC."
    }

    fn signature(&self) -> nu::Signature {
        todo!("ai")
        // `shm make file [author: path, --dir?: string, --file?: string]`: nothing -> record<dir: record<author: path, name: string, path: directory>, name: string, path: path>, 
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        todo!("ai")
    }

    fn run(
        &self,
        _plugin: &GrimoirePlugin,
        _engine: &nu::EngineInterface,
        call: &nu::EvaluatedCall,
        _input: &nu::Value,
    ) -> Result<nu::Value, nu::LabeledError> {
        todo!("ai")
    }
}
