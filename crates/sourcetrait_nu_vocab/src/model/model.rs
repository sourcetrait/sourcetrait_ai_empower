use crate::*;

#[derive(Debug, Clone)]
pub struct NuModel {
    pub meta: NuModelMeta,
    pub shape: nu::SyntaxShape,
}

#[derive(Debug, Clone)]
pub struct NuModelMeta {
    pub namepath: Option<NuModelNamepath>,
    pub version: Option<NuModelVersion>,
    pub summary: Option<NuModelSummary>,
    pub details: Option<NuModelDetails>,
}

#[derive(Debug, Clone)]
pub struct NuModelNamepath(pub String);
#[derive(Debug, Clone)]
pub struct NuModelVersion(pub semver::Version);
#[derive(Debug, Clone)]
pub struct NuModelSummary(pub String);
#[derive(Debug, Clone)]
pub struct NuModelDetails(pub String);

impl NuModelNamepath {
    pub const MAX: usize = 120;
    
    pub fn parse(s: &str) -> Result<Self, String> {
        Ok(Self(s.to_string())) // todo: validate
    }
}
impl NuModelVersion {
    pub fn parse(s: &str) -> Result<Self, String> {
        let ver = semver::Version::parse(s)
            .map_err(|e| e.to_string())?;
        Ok(Self(ver))
    }
}
impl NuModelSummary {
    pub const MAX: usize = 80;
    
    pub fn parse<S: Into<String>>(s: S) -> Result<Self, String> {
        Ok(Self(s.into())) // todo: validate
    }
}
impl NuModelDetails {
    pub const MAX: usize = 1024;
    
    pub fn parse<S: Into<String>>(s: S) -> Result<Self, String> {
        Ok(Self(s.into())) // todo: validate
    }
}

impl NuModel {
    //todo: handle errors using Error types, properly, spans etc.
    pub fn parse(s: &str) -> Result<Self, String> {
        let (s, meta) = Self::parse_meta(s)?;
        let engine = nu::EngineState::new();          // empty is fine — see below
        let mut ws = nu::StateWorkingSet::new(&engine);
        
        //let text = b"record<foo: int, bar: string>";
        let text = s.trim().as_bytes();
        let start = ws.next_span_start();
        let _ = ws.add_file("nu.model".into(), text);
        let span = nu::Span::new(start, start + text.len());
        
        let errs_before = ws.parse_errors.len();
        //let ty: Type = parse_type(&mut ws, text, span);
        let shape = nu::parse_shape_name(&mut ws, text, span);
        if ws.parse_errors.len() > errs_before {
            panic!("parse failed: ty is Type::Any, ws.parse_errors has the reason");
        }

        Ok(Self {
            meta,
            shape,
        })
    }

    fn parse_meta(s: &str) -> Result<(&str, NuModelMeta), String> {
        let s = s.trim();

        // handle any meta at top
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Phase { Summary, Details, Attributes }
        
        let mut lines = s.lines().peekable();
        let mut summary = String::new();
        let mut details = String::new();
        let mut namepath: Option<&str> = None;
        let mut version: Option<&str> = None;
        let mut phase = Phase::Summary;
        while let Some(line) = lines.peek() {
            let line = line.trim();
            let mut chars = line.chars();
            let Some(first_char) = chars.next() else {
                return Err(format!("Blank model meta line: {line}"));
            };
            match (phase, first_char) {
                (Phase::Summary, '#') => {
                    let line = chars.as_str().trim_start();
                    if line.is_empty() {
                        phase = Phase::Details;
                        let _ = lines.next();
                        continue;
                    }
                    dbg!(&line);
                    
                    let max = std::cmp::max(80 - summary.len() as isize, 0) as usize;
                    if max > 0 {
                        let len = std::cmp::min(line.len(), max);
                        summary.push_str(&line[0..len]);
                    }
                },
                (Phase::Details, '#') => {
                    let line = chars.as_str().trim_start();
                    dbg!(&line);
                    let max = std::cmp::max(1024 - details.len() as isize, 0) as usize;
                    if max > 0 {
                        let len = std::cmp::min(line.len(), max);
                        details.push_str(&line[0..len]);
                    }
                },
                (_, '@') => {
                    if phase != Phase::Attributes {
                        phase = Phase::Attributes;
                    }
                    
                    let Some((attrib, value)) = chars.as_str().split_once(' ') else {
                        let _ = lines.next();
                        continue; //todo: ignore for now, support custom attribs later
                    };
                    
                    match attrib {
                        "namepath" => {
                            match namepath {
                                None => { namepath = Some(value.trim()) },
                                Some(_) => return Err(format!("Duplicate attribute {attrib}")),
                            }
                        },
                        "version" => {
                            match version {
                                None => { version = Some(value.trim()) },
                                Some(_) => return Err(format!("Duplicate attribute {attrib}")),
                            }
                        },
                        _ => return Err(format!("Unrecognized attribute: {attrib}")),
                    }
                },
                _ => break,
            }

            let _ = lines.next();
        }

        let s = if let Some(next_line) = lines.next() {
            let offset = next_line.as_ptr() as usize - s.as_ptr() as usize;
            &s[offset..]
        } else {
            return Err(format!("Empty model"));
        };

        let summary = if !summary.is_empty() {
            Some(NuModelSummary::parse(summary)?)
        } else { None };
        let details = if !details.is_empty() {
            Some(NuModelDetails::parse(details)?)
        } else { None };
        let namepath = if let Some(namepath) = namepath {
            Some(NuModelNamepath::parse(namepath)?)
        } else { None };
        let version = if let Some(version) = version {
            Some(NuModelVersion::parse(version)?)
        } else { None };

        Ok((
            s,
            NuModelMeta {
                summary,
                details,
                namepath,
                version,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_shape() {
        dbg!(NuModel::parse("record<file: path, dir: directory, name: string>").unwrap());
        dbg!(NuModel::parse(r#"
            # This is a model summary line.
            #
            # This is a detailed description.
            # And this is more detail.
            @namepath vocab/test/basic/Shape
            @version 0.0.1-test
            record<
                file: path,  # this is a file
                dir: directory,  # this is a directory
                num: float,  # this is a number
                tab: table<  # this is a tab
                    key: string,  # this is a key
                    value: oneof<  # this is a value
                        int,  # this is a value integer
                        record<  # this is a value record
                            stuff: string,  # this is stuff
                            intg: int,  # this is another integer
                            subtab: table<  # this is a subtab
                                k: string,  # this is a subtab key
                                v: directory,  # this is a subtab value
                            >,
                        >,
                    >,
                >,
            >"#).unwrap()
        );
    }
}