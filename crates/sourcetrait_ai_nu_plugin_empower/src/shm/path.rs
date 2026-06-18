use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = EmpowerPlugin;

    fn name(&self) -> &str {
        "shm path"
    }

    fn description(&self) -> &str {
        "Retrieves the IPC directory or child file path within /dev/shm for a given ident, unique token, and (optionally) filename"
    }

    fn signature(&self) -> nu::Signature {
        todo!("ai")
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        todo!("ai")
        // `shm path <shm_ident:string> <unique: string> <filename?: string>` -> path (throws error)
    }

    fn run(
        &self,
        _plugin: &EmpowerPlugin,
        _engine: &nu::EngineInterface,
        call: &nu::EvaluatedCall,
        _input: &nu::Value,
    ) -> Result<nu::Value, nu::LabeledError> {
        todo!("ai")
    }
}
