use crate::*;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LibraryToml {
    pub name: String,
    pub title: String,
    pub version: String,
    pub provider: TomlProvider,
    pub description: TomlDescription,
    pub license: TomlLicense,
    pub dependencies: HashMap<String, LibraryTomlDependency>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LibraryTomlDependency {
    pub author: String,
    pub version: String,
    pub provider: String,
    pub path: Option<PathBuf>,
}
