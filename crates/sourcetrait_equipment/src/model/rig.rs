use crate::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Rig {
    pub name: Key,
    pub title: Title,
    pub version: Version,
    pub provider: Provider,
    pub author: Author,
    pub description: Description,
    pub license: License,
    pub exports: RigExports,
    pub nushell: RigNushell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigExports {
    pub libraries: Vec<LibraryName>,
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigNushell {
    pub version: Version,
}

impl TryFrom<RigToml> for Rig {
    type Error = EquipmentError;
    fn try_from(v: RigToml) -> EquipmentResult<Self> {
        Ok(Self {
            name: Key::try_from(v.name)?,
            title: Title::try_from(v.title)?,
            version: Version::try_from(v.version)?,
            provider: Provider::try_from(v.provider)?,
            author: Author::try_from(v.author)?,
            description: Description::try_from(v.description)?,
            license: License::try_from(v.license)?,
            exports: RigExports::try_from(v.exports)?,
            nushell: RigNushell::try_from(v.nushell)?,
        })
    }
}

impl TryFrom<RigTomlNushell> for RigNushell {
    type Error = EquipmentError;
    fn try_from(v: RigTomlNushell) -> EquipmentResult<Self> {
        Ok(Self {
            version: Version::try_from(v.version)?,
        })
    }
}

impl TryFrom<RigTomlExports> for RigExports {
    type Error = EquipmentError;
    fn try_from(v: RigTomlExports) -> EquipmentResult<Self> {
        Ok(Self {
            libraries: v.libraries.into_iter()
                .map(|s| LibraryName::try_from(s))
                .collect::<EquipmentResult<_>>()?
        })
    }
}

impl Rig {
    pub fn read<P: AsRef<Path> + Into<PathBuf>>(path: P) -> EquipmentResult<Self> {
        Self::try_from(RigToml::read(path)?)
    }
}
