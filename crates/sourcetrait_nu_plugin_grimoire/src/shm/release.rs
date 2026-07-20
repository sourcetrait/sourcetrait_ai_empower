use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = GrimoirePlugin;

    fn name(&self) -> &str {
        "shm release"
    }

    fn description(&self) -> &str {
        "Deletes a unique dir from /dev/shm, typically upon success"
    }

    fn signature(&self) -> nu::Signature {
        todo!("ai")
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        todo!("ai")
        // `shm release <shm_ident: string> <shm_unique: string>`: nothing ->  nothing (error)
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
