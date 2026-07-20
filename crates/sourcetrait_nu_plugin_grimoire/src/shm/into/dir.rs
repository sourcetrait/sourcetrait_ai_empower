use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = GrimoirePlugin;

    fn name(&self) -> &str {
        "into shm dir"
    }

    fn description(&self) -> &str {
        ""
    }

    fn signature(&self) -> nu::Signature {
        todo!("ai")
        // `into shm dir: string -> record<author: string, name: string, path: path>`
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        todo!("ai")
        // ```nu
        // let my_shm = "fae_name/message.txt"
        // let the_shm_file = $my_shm | into shm file
        // 
        // let my_shm_dir = "fae_name"
        // let my_shm_name = "message.txt"
        // let the_shm_file = $my_shm_name | into shm --dir $my_shm_dir
        // ```
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
