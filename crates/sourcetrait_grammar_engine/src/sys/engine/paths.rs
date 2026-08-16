use crate::*;

#[derive(Clone)]
pub struct EngineSysPaths(agnostic::AppPathRouter);

impl Default for EngineSysPaths {
    fn default() -> Self {
        Self(agnostic::DefaultAppPaths::Default(
            "sourcetrait/grammar/engine"
        ).into())
    }
}

impl HasAppPaths for EngineSysPaths {
    fn app_paths(&self) -> &impl AppPaths {
        &self.0
    }
}

impl EngineSysPaths {
    pub const CONFIG_TOML: &'static str = "config.toml";
    
    pub fn config_toml(&self) -> PathBuf {
        self.config_dir().join(Self::CONFIG_TOML)
    }
}
