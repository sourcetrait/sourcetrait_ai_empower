use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = EmpowerPlugin;

    fn name(&self) -> &str {
        "shm result"
    }

    fn description(&self) -> &str {
        "Merges a `shm dir` record and a list of `shm file` records for use within a result"
    }

    fn signature(&self) -> nu::Signature {
        todo!("ai")
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        todo!("ai")
        // `shm result <shm_poke_dir: record<unique: string, dir: directory>> list<path>`: nothing -> record<dir: directory, filenames: list<string>>
        // - gaurantees dir and files exist before returning or throws error
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
