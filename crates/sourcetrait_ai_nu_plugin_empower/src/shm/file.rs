use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = EmpowerPlugin;

    fn name(&self) -> &str {
        "shm file"
    }

    fn description(&self) -> &str {
        "Creates a unique tmpfs file within a given `shm dir` for use with IPC"
    }

    fn signature(&self) -> nu::Signature {
        todo!("ai")
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        todo!("ai")
        // `shm file <shm_dir: record<ident: string, unique: string, dir: directory>`: nothing -> path
        // creates the empty file. uses a base62 to generate a unique
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
