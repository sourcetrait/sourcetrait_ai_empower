use crate::*;

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EngineSysConfig;

impl EngineSysConfig {
    pub fn read(paths: &EngineSysPaths) -> green::GreenResult<Self> {
        let config_path = paths.config_toml(); 
        EngineSysConfig::from_toml_file(&config_path)
            .map_err(|e| green::GreenError::into_io(e))
    }
}

impl green::Config for EngineSysConfig {}
impl tomlx::FromToml for EngineSysConfig {}