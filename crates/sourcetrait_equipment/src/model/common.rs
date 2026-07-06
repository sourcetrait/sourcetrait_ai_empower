use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Title(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: Key,
    pub title: Title,
    pub email: Email,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyword(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description {
    pub summary: Summary,
    pub keywords: Vec<Keyword>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct License {
    pub spdx: Option<Spdx>,
    pub file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version(pub semver::Version);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    GitHub(GitHubProvider),
    Some(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubProvider;

#[derive(Debug, Clone, PartialEq)]
pub struct Spdx(pub spdx::Expression);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryName(pub String);

impl LibraryName {
    pub(crate) const CHECK: StrCheck = StrCheck::DEFAULT
        .case(StrCase::Snake)
        .max_len(16);
}

impl TryFrom<String> for LibraryName {
    type Error = EquipmentError;
    fn try_from(v: String) -> EquipmentResult<Self> {
        Self::CHECK.valid(v).map(|v| Self(v))
    }
}

impl From<&str> for LibraryName {
    fn from(v: &str) -> Self {
        Self::CHECK.valid(v.to_string()).map(|v| Self(v)).expect("valid")
    }
}

impl Key {
    pub(crate) const CHECK: StrCheck = StrCheck::DEFAULT
        .case(StrCase::Snake)
        .max_len(16);
}

impl From<&str> for Key { fn from(v: &str) -> Self { Self(v.to_string()) } }
impl From<&str> for Title { fn from(v: &str) -> Self { Self(v.to_string()) } }
impl From<&str> for Version {
    fn from(v: &str) -> Self {
        Self(semver::Version::parse(v).expect("semver"))
    }
}

impl TryFrom<String> for Key {
    type Error = EquipmentError;
    fn try_from(v: String) -> EquipmentResult<Self> {
        Self::CHECK.valid(v).map(|v| Self(v))
    }
}

impl Title {
    pub(crate) const CHECK: StrCheck = StrCheck::DEFAULT.max_len(16);
}

impl TryFrom<String> for Title {
    type Error = EquipmentError;
    fn try_from(v: String) -> EquipmentResult<Self> {
        Self::CHECK.valid(v).map(|v| Self(v))
    }
}

impl TryFrom<String> for Version {
    type Error = EquipmentError;
    fn try_from(ver: String) -> EquipmentResult<Self> {
        semver::Version::parse(&ver)
            .map(|v| Version(v))
            .map_err(|source| EquipmentError::SemVer { source, ver })
    }
}

impl TryFrom<TomlAuthor> for Author {
    type Error = EquipmentError;
    fn try_from(v: TomlAuthor) -> EquipmentResult<Self> {
        Ok(Self {
            name: Key::try_from(v.name)?,
            title: Title::try_from(v.title)?,
            email: Email::try_from(v.email)?,
        })
    }
}

impl Provider {
    pub const GITHUB: &'static str = "github";
}

impl TryFrom<TomlProvider> for Provider {
    type Error = EquipmentError;
    fn try_from(v: TomlProvider) -> EquipmentResult<Self> {
        let this = match v.name.as_str() {
            Self::GITHUB => Self::GitHub(GitHubProvider),
            _ => Self::Some(v.name),
        };
        
        Ok(this)
    }
}

impl Summary {
    pub(crate) const CHECK: StrCheck = StrCheck::DEFAULT.max_len(120);
}

impl TryFrom<String> for Summary {
    type Error = EquipmentError;
    fn try_from(v: String) -> EquipmentResult<Self> {
        Self::CHECK.valid(v).map(|v| Self(v))
    }
}

impl From<&str> for Summary {
    fn from(v: &str) -> Self {
        Self::CHECK.valid(v.to_string()).map(|v| Self(v)).expect("valid")
    }
}

impl Keyword {
    pub(crate) const CHECK: StrCheck = StrCheck::DEFAULT
        .max_len(8)
        .case(StrCase::Snake);
}

impl TryFrom<String> for Keyword {
    type Error = EquipmentError;
    fn try_from(v: String) -> EquipmentResult<Self> {
        Self::CHECK.valid(v).map(|v| Self(v))
    }
}

impl From<&str> for Keyword {
    fn from(v: &str) -> Self {
        Self::CHECK.valid(v.to_string()).map(|v| Self(v)).expect("valid")
    }
}

impl TryFrom<TomlDescription> for Description {
    type Error = EquipmentError;
    fn try_from(v: TomlDescription) -> EquipmentResult<Self> {
        Ok(Self {
            summary: Summary::try_from(v.summary)?,
            keywords: v.keywords.into_iter()
                .map(|k| Keyword::try_from(k))
                .collect::<EquipmentResult<_>>()?,
        })
    }
}

impl TryFrom<TomlLicense> for License {
    type Error = EquipmentError;
    fn try_from(v: TomlLicense) -> EquipmentResult<Self> {
        Ok(Self {
            spdx: v.spdx.map(|v| Spdx::try_from(v)).transpose()?,
            file: v.file,
        })
    }
}

impl TryFrom<String> for Spdx {
    type Error = EquipmentError;
    fn try_from(spdx: String) -> EquipmentResult<Self> {
        spdx::Expression::parse(&spdx)
            .map(|v| Self(v))
            .map_err(|source| EquipmentError::Spdx { source, spdx })
    }
}

impl From<&str> for Spdx {
    fn from(spdx: &str) -> Self {
        spdx::Expression::parse(&spdx)
            .map(|v| Self(v))
            .expect("valid")
    }
}

impl Email {
    pub(crate) const CHECK: StrCheck = StrCheck::DEFAULT
        .min_len(3)
        .max_len(255);

    pub fn check(s: &str) -> EquipmentResult<()> {
        Self::CHECK.check(s)?;
        
        let (local, domain) = {
            let mut parts = s.split('@');
            let local = parts.next()
                .ok_or_else(|| EquipmentError::Email { address: s.to_string() })?;
            let domain = parts.next()
                .ok_or_else(|| EquipmentError::Email { address: s.to_string() })?;
            if parts.count() != 0 {
                return Err(EquipmentError::Email { address: s.to_string() });
            }
            
            (local, domain)
        };
        
        if local.len() < 1 || domain.len() < 1 {
            return Err(EquipmentError::Email { address: s.to_string() });
        }

        Ok(())
    }

    pub(crate) fn valid(s: String) -> EquipmentResult<Self> {
        Self::check(&s)?;
        Ok(Self(s))
    }
}

impl TryFrom<String> for Email {
    type Error = EquipmentError;
    fn try_from(v: String) -> EquipmentResult<Self> {
        Self::valid(v)
    }
}

impl From<&str> for Email {
    fn from(v: &str) -> Self {
        Self::valid(v.to_string()).expect("valid email")
    }
}
