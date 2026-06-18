use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = EmpowerPlugin;

    fn name(&self) -> &str {
        "shm dir"
    }

    fn description(&self) -> &str {
        "Creates a unique tmpfs directory within /dev/shm for use with IPC"
    }

    fn signature(&self) -> nu::Signature {
        todo!("ai")
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        todo!("ai")
        // `shm dir <shm_ident:string>`: nothing -> record<ident: string, unique: string, dir: directory>
        // creates the empty directory. uses base62 to generate the unique
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
