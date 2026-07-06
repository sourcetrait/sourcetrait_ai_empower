use crate::*;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RigToml {
    pub name: String,
    pub title: String,
    pub version: String,
    pub author: TomlAuthor,
    pub provider: TomlProvider,
    pub description: TomlDescription,
    pub license: TomlLicense,
    pub exports: RigTomlExports,
    pub nushell: RigTomlNushell,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RigTomlExports {
    pub libraries: Vec<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RigTomlNushell {
    pub version: String,
}

impl RigToml {
    pub const RIG_TOML: &'static str = "rig.toml";
    
    pub fn read<P: AsRef<Path> + Into<PathBuf>>(path: P) -> EquipmentResult<Self> {
        let path = if path.as_ref().is_dir() {
            path.as_ref().join(Self::RIG_TOML)
        } else {
            path.into()
        };
        
        let txt = fs::read_to_string(&path)
            .map_err(|source| EquipmentError::File { source, path: path.to_path_buf(), op: IoErr::Read })?;
        let rig = toml::from_str(&txt)
            .map_err(|source| EquipmentError::TomlFileRead { source, path: path.to_path_buf() })?;
        
        Ok(rig)
    }
}