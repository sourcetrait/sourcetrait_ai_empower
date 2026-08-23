use crate::*;

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EngineSysConfig;

impl EngineSysConfig {
    pub fn read(paths: &EngineSysPaths) -> subsys::SubsysResult<Self> {
        let config_path = paths.config_toml(); 
        EngineSysConfig::from_toml_file(&config_path)
            .map_err(|e| subsys::SubsysError::into_io(e))
    }
}

impl subsys::Config for EngineSysConfig {}
impl tomlx::FromToml for EngineSysConfig {}