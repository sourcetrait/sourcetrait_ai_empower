use crate::*;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RigToml {
    pub name: String,
    pub title: String,
    pub version: String,
    pub provider: RigTomlProvider,
    pub description: RigTomlDescription,
    pub license: RigTomlLicense,
    pub libraries: Vec<String>,
    pub nushell: RigTomlNushell,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RigTomlAuthor {
    pub name: String,
    pub title: String,
    pub email: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RigTomlDescription {
    pub summary: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RigTomlLicense {
    pub spdx: Option<String>,
    pub file: PathBuf,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RigTomlProvider {
    pub name: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RigTomlNushell {
    pub version: String,
}
