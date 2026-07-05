use crate::*;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LibraryToml {
    pub name: String,
    pub title: String,
    pub version: String,
    pub provider: LibraryTomlProvider,
    pub description: LibraryTomlDescription,
    pub license: LibraryTomlLicense,
    pub dependencies: HashMap<String, LibraryTomlDependency>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LibraryTomlAuthor {
    pub name: String,
    pub title: String,
    pub email: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LibraryTomlProvider {
    pub name: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LibraryTomlDescription {
    pub summary: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LibraryTomlLicense {
    pub spdx: Option<String>,
    pub file: PathBuf,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LibraryTomlDependency {
    pub author: String,
    pub version: String,
    pub provider: String,
    pub path: Option<PathBuf>,
}
