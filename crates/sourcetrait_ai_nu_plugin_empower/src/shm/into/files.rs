use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = EmpowerPlugin;

    fn name(&self) -> &str {
        "into shm files"
    }

    fn description(&self) -> &str {
        ""
    }

    fn signature(&self) -> nu::Signature {
        todo!("ai")
        // `into shm files [--dir?: path]: string -> record<name: string, path: path, dir: record<name: string, path: directory>>'
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        todo!("ai")
        // ```nu
        // let my_shm = "ai/fae_name/message.txt"
        // let the_shm_file = $my_shm | into shm file
        // 
        // let my_shm_dir = "ai/fae_name"
        // let my_shm_filename = "message.txt"
        // let the_shm_file = $my_shm_name | into shm --dir $my_shm_dir
        // ```
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
