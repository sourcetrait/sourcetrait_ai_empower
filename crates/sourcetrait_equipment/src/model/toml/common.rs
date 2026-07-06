use crate::*;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TomlAuthor {
    pub name: String,
    pub title: String,
    pub email: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TomlDescription {
    pub summary: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TomlLicense {
    pub spdx: Option<String>,
    pub file: PathBuf,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TomlProvider {
    pub name: String,
}
