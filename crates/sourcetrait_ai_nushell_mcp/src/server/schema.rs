// The item 21 grammar is a complete, deliberately-typed API: some
// `*Kind` discriminants and `kind()` accessors are surface for callers
// and future items (15/16) and are not all internally consumed yet.
#![allow(dead_code)]
use crate::*;

// Structured-schema grammar (item 21). A strict, well-defined,
// bidirectional grammar represented twice: as Copy `*Kind`
// discriminants and as fieldful `*Typedef` structural ASTs, one
// parallel hierarchy per representation (`Json*` and `Nu*`).
//
// Two directions, mirror images:
// - schema_to_typedef (input): a JSON schema -> Json*Typedef ->
//   map to Nu*Typedef -> render the nu positional-type string the
//   worker parses.
// - typedef_to_schema (emit): a nu positional-type string ->
//   Nu*Typedef -> map to Json*Typedef -> render a JSON value.
//
// No fallback: validity is confirmed at import/define, so the emit
// parser trusts grammar-conformant input; a failure on a validated
// schema is a bug. Every grammar denial is a single classify/parse
// error here, not scattered checks elsewhere.

// ============================================================================
// Scalar vocabulary -- the 14 named scalar types. `nothing` is its own
// kind (JSON null, not a string); `any` is deliberately absent.
// ============================================================================

macro_rules! scalar_enum {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum $name {
            Int, Float, String, Bool, Datetime, Duration, Filesize,
            Binary, Range, Number, Glob, CellPath, Path, Directory,
        }
        impl $name {
            pub(crate) fn name(self) -> &'static str {
                match self {
                    Self::Int => "int",
                    Self::Float => "float",
                    Self::String => "string",
                    Self::Bool => "bool",
                    Self::Datetime => "datetime",
                    Self::Duration => "duration",
                    Self::Filesize => "filesize",
                    Self::Binary => "binary",
                    Self::Range => "range",
                    Self::Number => "number",
                    Self::Glob => "glob",
                    Self::CellPath => "cell-path",
                    Self::Path => "path",
                    Self::Directory => "directory",
                }
            }
            fn from_name(s: &str) -> Result<Self, String> {
                Ok(match s {
                    "int" => Self::Int,
                    "float" => Self::Float,
                    "string" => Self::String,
                    "bool" => Self::Bool,
                    "datetime" => Self::Datetime,
                    "duration" => Self::Duration,
                    "filesize" => Self::Filesize,
                    "binary" => Self::Binary,
                    "range" => Self::Range,
                    "number" => Self::Number,
                    "glob" => Self::Glob,
                    "cell-path" => Self::CellPath,
                    "path" => Self::Path,
                    "directory" => Self::Directory,
                    "any" => return Err(
                        "`any` is not a grammar type".to_string()),
                    other => return Err(
                        format!("unknown scalar type `{other}`")),
                })
            }
        }
    };
}

scalar_enum!(JsonScalarTypedef);
scalar_enum!(NuScalarTypedef);

// ============================================================================
// Shared name newtypes -- a field/column name carries no representation
// difference, so these are shared (not Json/Nu-paired), but distinct
// from each other.
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldName(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnName(pub String);

// ============================================================================
// Kind discriminants (Copy) for the genuine multi-variant enums
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsonTypedefKind { Scalar, Nothing, Record, Oneof, Table, List }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NuTypedefKind { Scalar, Nothing, Record, Oneof, Table, List }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsonArgsTypedefKind { Void, Record }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NuArgsTypedefKind { Void, Record }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsonResultTypedefKind { Void, Record }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NuResultTypedefKind { Void, Record }

// ============================================================================
// JSON-side structural AST
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum JsonTypedef {
    Scalar(JsonScalarTypedef),
    Nothing,
    Record(JsonRecordTypedef),
    Oneof(JsonOneofTypedef),
    Table(JsonTableTypedef),
    List(JsonListTypedef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JsonRecordTypedef {
    pub fields: Vec<JsonRecordFieldTypedef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JsonRecordFieldTypedef {
    pub name: FieldName,
    pub typedef: JsonTypedef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JsonTableTypedef {
    pub columns: Vec<JsonTableColumnTypedef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JsonTableColumnTypedef {
    pub name: ColumnName,
    pub typedef: JsonTypedef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JsonOneofTypedef {
    pub members: Vec<JsonTypedef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JsonListTypedef {
    pub element: Box<JsonTypedef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum JsonArgsTypedef {
    Void,
    Record(JsonRecordTypedef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum JsonResultTypedef {
    Void,
    Record(JsonRecordTypedef),
}

impl JsonTypedef {
    pub(crate) fn kind(&self) -> JsonTypedefKind {
        match self {
            Self::Scalar(_) => JsonTypedefKind::Scalar,
            Self::Nothing => JsonTypedefKind::Nothing,
            Self::Record(_) => JsonTypedefKind::Record,
            Self::Oneof(_) => JsonTypedefKind::Oneof,
            Self::Table(_) => JsonTypedefKind::Table,
            Self::List(_) => JsonTypedefKind::List,
        }
    }
}

impl JsonArgsTypedef {
    pub(crate) fn kind(&self) -> JsonArgsTypedefKind {
        match self {
            Self::Void => JsonArgsTypedefKind::Void,
            Self::Record(_) => JsonArgsTypedefKind::Record,
        }
    }
}

impl JsonResultTypedef {
    pub(crate) fn kind(&self) -> JsonResultTypedefKind {
        match self {
            Self::Void => JsonResultTypedefKind::Void,
            Self::Record(_) => JsonResultTypedefKind::Record,
        }
    }
}

// ============================================================================
// Nu-side structural AST (mirror of the JSON side)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NuTypedef {
    Scalar(NuScalarTypedef),
    Nothing,
    Record(NuRecordTypedef),
    Oneof(NuOneofTypedef),
    Table(NuTableTypedef),
    List(NuListTypedef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NuRecordTypedef {
    pub fields: Vec<NuRecordFieldTypedef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NuRecordFieldTypedef {
    pub name: FieldName,
    pub typedef: NuTypedef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NuTableTypedef {
    pub columns: Vec<NuTableColumnTypedef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NuTableColumnTypedef {
    pub name: ColumnName,
    pub typedef: NuTypedef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NuOneofTypedef {
    pub members: Vec<NuTypedef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NuListTypedef {
    pub element: Box<NuTypedef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NuArgsTypedef {
    Void,
    Record(NuRecordTypedef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NuResultTypedef {
    Void,
    Record(NuRecordTypedef),
}

impl NuTypedef {
    pub(crate) fn kind(&self) -> NuTypedefKind {
        match self {
            Self::Scalar(_) => NuTypedefKind::Scalar,
            Self::Nothing => NuTypedefKind::Nothing,
            Self::Record(_) => NuTypedefKind::Record,
            Self::Oneof(_) => NuTypedefKind::Oneof,
            Self::Table(_) => NuTypedefKind::Table,
            Self::List(_) => NuTypedefKind::List,
        }
    }
}

impl NuArgsTypedef {
    pub(crate) fn kind(&self) -> NuArgsTypedefKind {
        match self {
            Self::Void => NuArgsTypedefKind::Void,
            Self::Record(_) => NuArgsTypedefKind::Record,
        }
    }
}

impl NuResultTypedef {
    pub(crate) fn kind(&self) -> NuResultTypedefKind {
        match self {
            Self::Void => NuResultTypedefKind::Void,
            Self::Record(_) => NuResultTypedefKind::Record,
        }
    }
}

// The reserved record-field / oneof key.
const ONEOF_KEY: &str = "oneof<>";

// ============================================================================
// JSON side: classify -> parse -> render
// ============================================================================

/// Classify a nested JSON schema node into its grammar kind (the one
/// place the object/array disambiguation lives). Nested only: a nested
/// empty object {} is denied (Void is top-level; see parse_json_args).
fn json_typedef_kind(v: &json::Value) -> Result<JsonTypedefKind, String> {
    match v {
        json::Value::Null => Ok(JsonTypedefKind::Nothing),
        json::Value::String(_) => Ok(JsonTypedefKind::Scalar),
        json::Value::Object(map) => {
            if map.contains_key(ONEOF_KEY) {
                Ok(JsonTypedefKind::Oneof)
            } else if map.is_empty() {
                Err("nested empty record {} is not allowed".to_string())
            } else {
                Ok(JsonTypedefKind::Record)
            }
        }
        json::Value::Array(arr) => {
            if arr.is_empty() {
                return Err(
                    "[] is ambiguous (list vs table) and not allowed"
                        .to_string());
            }
            if arr.len() != 1 {
                return Err(format!(
                    "array type must hold exactly one element, got {}",
                    arr.len()));
            }
            // Table iff the single element is a non-oneof object.
            match &arr[0] {
                json::Value::Object(m) if !m.contains_key(ONEOF_KEY) => {
                    Ok(JsonTypedefKind::Table)
                }
                _ => Ok(JsonTypedefKind::List),
            }
        }
        json::Value::Bool(_) | json::Value::Number(_) => Err(format!(
            "schema node must be a string/object/array/null, got {v}")),
    }
}

fn parse_json_record_fields(
    map: &mcp::JsonObject,
) -> Result<Vec<JsonRecordFieldTypedef>, String> {
    let mut fields = Vec::with_capacity(map.len());
    for (k, v) in map {
        fields.push(JsonRecordFieldTypedef {
            name: FieldName(k.clone()),
            typedef: parse_json_typedef(v)?,
        });
    }
    Ok(fields)
}

/// Parse a nested JSON schema node into a JsonTypedef.
fn parse_json_typedef(v: &json::Value) -> Result<JsonTypedef, String> {
    Ok(match json_typedef_kind(v)? {
        JsonTypedefKind::Nothing => JsonTypedef::Nothing,
        JsonTypedefKind::Scalar => {
            let s = v.as_str().expect("classified Scalar");
            JsonTypedef::Scalar(JsonScalarTypedef::from_name(s)?)
        }
        JsonTypedefKind::Oneof => {
            let map = v.as_object().expect("classified Oneof");
            let members_val = &map[ONEOF_KEY];
            let arr = members_val.as_array().ok_or_else(|| format!(
                "`{ONEOF_KEY}` value must be an array of types"))?;
            if arr.is_empty() {
                return Err("empty oneof<> is not allowed".to_string());
            }
            if map.len() != 1 {
                return Err(format!(
                    "a oneof object must have only the `{ONEOF_KEY}` key"));
            }
            let mut members = Vec::with_capacity(arr.len());
            for m in arr {
                members.push(parse_json_typedef(m)?);
            }
            JsonTypedef::Oneof(JsonOneofTypedef { members })
        }
        JsonTypedefKind::Record => {
            let map = v.as_object().expect("classified Record");
            JsonTypedef::Record(JsonRecordTypedef {
                fields: parse_json_record_fields(map)?,
            })
        }
        JsonTypedefKind::Table => {
            let arr = v.as_array().expect("classified Table");
            let map = arr[0].as_object().expect("classified Table elem");
            if map.is_empty() {
                return Err(
                    "[{}] (a table with no columns) is not allowed".to_string());
            }
            let mut columns = Vec::with_capacity(map.len());
            for (k, cv) in map {
                columns.push(JsonTableColumnTypedef {
                    name: ColumnName(k.clone()),
                    typedef: parse_json_typedef(cv)?,
                });
            }
            JsonTypedef::Table(JsonTableTypedef { columns })
        }
        JsonTypedefKind::List => {
            let arr = v.as_array().expect("classified List");
            JsonTypedef::List(JsonListTypedef {
                element: Box::new(parse_json_typedef(&arr[0])?),
            })
        }
    })
}

/// Parse a top-level args schema object: empty {} -> Void, else Record.
fn parse_json_args(
    map: &mcp::JsonObject,
) -> Result<JsonArgsTypedef, String> {
    if map.is_empty() {
        Ok(JsonArgsTypedef::Void)
    } else {
        Ok(JsonArgsTypedef::Record(JsonRecordTypedef {
            fields: parse_json_record_fields(map)?,
        }))
    }
}

fn parse_json_result(
    map: &mcp::JsonObject,
) -> Result<JsonResultTypedef, String> {
    if map.is_empty() {
        Ok(JsonResultTypedef::Void)
    } else {
        Ok(JsonResultTypedef::Record(JsonRecordTypedef {
            fields: parse_json_record_fields(map)?,
        }))
    }
}

fn render_json_typedef(t: &JsonTypedef) -> json::Value {
    match t {
        JsonTypedef::Scalar(s) => json::Value::String(s.name().to_string()),
        JsonTypedef::Nothing => json::Value::Null,
        JsonTypedef::Record(r) => render_json_record(&r.fields),
        JsonTypedef::Oneof(o) => {
            let arr: Vec<json::Value> =
                o.members.iter().map(render_json_typedef).collect();
            let mut m = serde_json::Map::new();
            m.insert(ONEOF_KEY.to_string(), json::Value::Array(arr));
            json::Value::Object(m)
        }
        JsonTypedef::Table(tab) => {
            let mut m = serde_json::Map::new();
            for c in &tab.columns {
                m.insert(c.name.0.clone(), render_json_typedef(&c.typedef));
            }
            json::Value::Array(vec![json::Value::Object(m)])
        }
        JsonTypedef::List(l) => {
            json::Value::Array(vec![render_json_typedef(&l.element)])
        }
    }
}

fn render_json_record(
    fields: &[JsonRecordFieldTypedef],
) -> json::Value {
    let mut m = serde_json::Map::new();
    for f in fields {
        m.insert(f.name.0.clone(), render_json_typedef(&f.typedef));
    }
    json::Value::Object(m)
}

// ============================================================================
// Nu side: classify -> parse -> render
// ============================================================================

/// Split `s` on top-level occurrences of `sep` (depth 0 w.r.t. `<>`).
fn split_top_level(s: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut cur = std::string::String::new();
    for c in s.chars() {
        match c {
            '<' => { depth += 1; cur.push(c); }
            '>' => { depth -= 1; cur.push(c); }
            _ if c == sep && depth == 0 => {
                parts.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    let last = cur.trim().to_string();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// Split a `name: type` field at its first top-level `:`.
fn split_field(field: &str) -> Result<(std::string::String, &str), String> {
    let mut depth = 0i32;
    for (i, c) in field.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ':' if depth == 0 => {
                return Ok((
                    field[..i].trim().to_string(),
                    field[i + 1..].trim(),
                ));
            }
            _ => {}
        }
    }
    Err(format!("field `{field}` is missing a `:` type separator"))
}

/// If `tok` is `prefix<...>`, return the balanced inner; else None.
fn bracketed<'a>(tok: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = tok.strip_prefix(prefix)?;
    let inner = rest.strip_suffix('>')?;
    Some(inner)
}

/// Classify a nu typedef token into its grammar kind (nested context).
fn nu_typedef_kind(tok: &str) -> Result<NuTypedefKind, String> {
    let tok = tok.trim();
    if tok == "nothing" {
        Ok(NuTypedefKind::Nothing)
    } else if tok.starts_with("record<") {
        Ok(NuTypedefKind::Record)
    } else if tok.starts_with("oneof<") {
        Ok(NuTypedefKind::Oneof)
    } else if tok.starts_with("table<") {
        Ok(NuTypedefKind::Table)
    } else if tok.starts_with("list<") {
        Ok(NuTypedefKind::List)
    } else if tok == "record" || tok == "table" || tok == "list" {
        Err(format!("bare `{tok}` is not allowed; specify its contents"))
    } else {
        // A scalar name (from_name rejects `any` and unknowns).
        Ok(NuTypedefKind::Scalar)
    }
}

fn parse_nu_record_fields(
    inner: &str,
) -> Result<Vec<NuRecordFieldTypedef>, String> {
    let mut fields = Vec::new();
    for field in split_top_level(inner, ',') {
        let (name, ty) = split_field(&field)?;
        fields.push(NuRecordFieldTypedef {
            name: FieldName(name),
            typedef: parse_nu_typedef(ty)?,
        });
    }
    Ok(fields)
}

/// Parse a nested nu typedef token into a NuTypedef.
fn parse_nu_typedef(tok: &str) -> Result<NuTypedef, String> {
    let tok = tok.trim();
    Ok(match nu_typedef_kind(tok)? {
        NuTypedefKind::Nothing => NuTypedef::Nothing,
        NuTypedefKind::Scalar => {
            NuTypedef::Scalar(NuScalarTypedef::from_name(tok)?)
        }
        NuTypedefKind::Record => {
            let inner = bracketed(tok, "record<")
                .ok_or_else(|| format!("malformed record type `{tok}`"))?;
            if inner.trim().is_empty() {
                return Err(
                    "nested empty record<> is not allowed".to_string());
            }
            NuTypedef::Record(NuRecordTypedef {
                fields: parse_nu_record_fields(inner)?,
            })
        }
        NuTypedefKind::Oneof => {
            let inner = bracketed(tok, "oneof<")
                .ok_or_else(|| format!("malformed oneof type `{tok}`"))?;
            let parts = split_top_level(inner, ',');
            if parts.is_empty() {
                return Err("empty oneof<> is not allowed".to_string());
            }
            let mut members = Vec::with_capacity(parts.len());
            for p in &parts {
                members.push(parse_nu_typedef(p)?);
            }
            NuTypedef::Oneof(NuOneofTypedef { members })
        }
        NuTypedefKind::Table => {
            let inner = bracketed(tok, "table<")
                .ok_or_else(|| format!("malformed table type `{tok}`"))?;
            if inner.trim().is_empty() {
                return Err(
                    "table<> with no columns is not allowed".to_string());
            }
            let mut columns = Vec::new();
            for col in split_top_level(inner, ',') {
                let (name, ty) = split_field(&col)?;
                columns.push(NuTableColumnTypedef {
                    name: ColumnName(name),
                    typedef: parse_nu_typedef(ty)?,
                });
            }
            NuTypedef::Table(NuTableTypedef { columns })
        }
        NuTypedefKind::List => {
            let inner = bracketed(tok, "list<")
                .ok_or_else(|| format!("malformed list type `{tok}`"))?;
            NuTypedef::List(NuListTypedef {
                element: Box::new(parse_nu_typedef(inner)?),
            })
        }
    })
}

/// Parse a top-level args positional type: `nothing` -> Void, else
/// `record<...>` -> Record. Anything else at the top level is rejected.
fn parse_nu_args(tok: &str) -> Result<NuArgsTypedef, String> {
    let tok = tok.trim();
    if tok == "nothing" {
        Ok(NuArgsTypedef::Void)
    } else if let Some(inner) = bracketed(tok, "record<") {
        if inner.trim().is_empty() {
            return Err(
                "top-level record<> is not allowed; use {} (void)"
                    .to_string());
        }
        Ok(NuArgsTypedef::Record(NuRecordTypedef {
            fields: parse_nu_record_fields(inner)?,
        }))
    } else {
        Err(format!(
            "top-level args type must be `nothing` or `record<...>`, got `{tok}`"))
    }
}

fn parse_nu_result(tok: &str) -> Result<NuResultTypedef, String> {
    let tok = tok.trim();
    if tok == "nothing" {
        Ok(NuResultTypedef::Void)
    } else if let Some(inner) = bracketed(tok, "record<") {
        if inner.trim().is_empty() {
            return Err(
                "top-level record<> is not allowed; use {} (void)"
                    .to_string());
        }
        Ok(NuResultTypedef::Record(NuRecordTypedef {
            fields: parse_nu_record_fields(inner)?,
        }))
    } else {
        Err(format!(
            "top-level result type must be `nothing` or `record<...>`, got `{tok}`"))
    }
}

fn render_nu_typedef(t: &NuTypedef) -> std::string::String {
    match t {
        NuTypedef::Scalar(s) => s.name().to_string(),
        NuTypedef::Nothing => "nothing".to_string(),
        NuTypedef::Record(r) => render_nu_record(&r.fields),
        NuTypedef::Oneof(o) => {
            let parts: Vec<std::string::String> =
                o.members.iter().map(render_nu_typedef).collect();
            format!("oneof<{}>", parts.join(", "))
        }
        NuTypedef::Table(tab) => {
            let cols: Vec<std::string::String> = tab
                .columns
                .iter()
                .map(|c| format!("{}: {}", c.name.0, render_nu_typedef(&c.typedef)))
                .collect();
            format!("table<{}>", cols.join(", "))
        }
        NuTypedef::List(l) => {
            format!("list<{}>", render_nu_typedef(&l.element))
        }
    }
}

fn render_nu_record(fields: &[NuRecordFieldTypedef]) -> std::string::String {
    let parts: Vec<std::string::String> = fields
        .iter()
        .map(|f| format!("{}: {}", f.name.0, render_nu_typedef(&f.typedef)))
        .collect();
    format!("record<{}>", parts.join(", "))
}

fn render_nu_args(a: &NuArgsTypedef) -> std::string::String {
    match a {
        NuArgsTypedef::Void => "nothing".to_string(),
        NuArgsTypedef::Record(r) => render_nu_record(&r.fields),
    }
}

fn render_nu_result(r: &NuResultTypedef) -> std::string::String {
    match r {
        NuResultTypedef::Void => "nothing".to_string(),
        NuResultTypedef::Record(rec) => render_nu_record(&rec.fields),
    }
}

// ============================================================================
// Structural maps Json* <-> Nu* (1:1; total)
// ============================================================================

fn json_to_nu_typedef(t: &JsonTypedef) -> NuTypedef {
    match t {
        JsonTypedef::Scalar(s) => NuTypedef::Scalar(scalar_j2n(*s)),
        JsonTypedef::Nothing => NuTypedef::Nothing,
        JsonTypedef::Record(r) => NuTypedef::Record(record_j2n(r)),
        JsonTypedef::Oneof(o) => NuTypedef::Oneof(NuOneofTypedef {
            members: o.members.iter().map(json_to_nu_typedef).collect(),
        }),
        JsonTypedef::Table(tab) => NuTypedef::Table(NuTableTypedef {
            columns: tab
                .columns
                .iter()
                .map(|c| NuTableColumnTypedef {
                    name: c.name.clone(),
                    typedef: json_to_nu_typedef(&c.typedef),
                })
                .collect(),
        }),
        JsonTypedef::List(l) => NuTypedef::List(NuListTypedef {
            element: Box::new(json_to_nu_typedef(&l.element)),
        }),
    }
}

fn nu_to_json_typedef(t: &NuTypedef) -> JsonTypedef {
    match t {
        NuTypedef::Scalar(s) => JsonTypedef::Scalar(scalar_n2j(*s)),
        NuTypedef::Nothing => JsonTypedef::Nothing,
        NuTypedef::Record(r) => JsonTypedef::Record(record_n2j(r)),
        NuTypedef::Oneof(o) => JsonTypedef::Oneof(JsonOneofTypedef {
            members: o.members.iter().map(nu_to_json_typedef).collect(),
        }),
        NuTypedef::Table(tab) => JsonTypedef::Table(JsonTableTypedef {
            columns: tab
                .columns
                .iter()
                .map(|c| JsonTableColumnTypedef {
                    name: c.name.clone(),
                    typedef: nu_to_json_typedef(&c.typedef),
                })
                .collect(),
        }),
        NuTypedef::List(l) => JsonTypedef::List(JsonListTypedef {
            element: Box::new(nu_to_json_typedef(&l.element)),
        }),
    }
}

fn record_j2n(r: &JsonRecordTypedef) -> NuRecordTypedef {
    NuRecordTypedef {
        fields: r
            .fields
            .iter()
            .map(|f| NuRecordFieldTypedef {
                name: f.name.clone(),
                typedef: json_to_nu_typedef(&f.typedef),
            })
            .collect(),
    }
}

fn record_n2j(r: &NuRecordTypedef) -> JsonRecordTypedef {
    JsonRecordTypedef {
        fields: r
            .fields
            .iter()
            .map(|f| JsonRecordFieldTypedef {
                name: f.name.clone(),
                typedef: nu_to_json_typedef(&f.typedef),
            })
            .collect(),
    }
}

fn scalar_j2n(s: JsonScalarTypedef) -> NuScalarTypedef {
    NuScalarTypedef::from_name(s.name()).expect("paired scalar")
}

fn scalar_n2j(s: NuScalarTypedef) -> JsonScalarTypedef {
    JsonScalarTypedef::from_name(s.name()).expect("paired scalar")
}

// ============================================================================
// Public entry points (the 4 the rest of the crate calls)
// ============================================================================

/// Input: a JSON args schema object -> the nu positional-type string
/// (`record<...>` or `nothing`).
pub(crate) fn args_schema_to_nu(
    schema: &mcp::JsonObject,
) -> Result<std::string::String, String> {
    let json = parse_json_args(schema)?;
    let nu = match json {
        JsonArgsTypedef::Void => NuArgsTypedef::Void,
        JsonArgsTypedef::Record(r) => NuArgsTypedef::Record(record_j2n(&r)),
    };
    Ok(render_nu_args(&nu))
}

/// Input: a JSON result schema object -> the nu positional-type string.
pub(crate) fn result_schema_to_nu(
    schema: &mcp::JsonObject,
) -> Result<std::string::String, String> {
    let json = parse_json_result(schema)?;
    let nu = match json {
        JsonResultTypedef::Void => NuResultTypedef::Void,
        JsonResultTypedef::Record(r) => NuResultTypedef::Record(record_j2n(&r)),
    };
    Ok(render_nu_result(&nu))
}

/// Emit: a nu args positional-type string -> the JSON schema object
/// ({} for void, {fields} for a record).
pub(crate) fn nu_to_args_schema(
    typedef: &str,
) -> Result<mcp::JsonObject, String> {
    let nu = parse_nu_args(typedef)?;
    let json = match nu {
        NuArgsTypedef::Void => JsonArgsTypedef::Void,
        NuArgsTypedef::Record(r) => JsonArgsTypedef::Record(record_n2j(&r)),
    };
    Ok(match json {
        JsonArgsTypedef::Void => mcp::JsonObject::new(),
        JsonArgsTypedef::Record(r) => match render_json_record(&r.fields) {
            json::Value::Object(m) => m,
            _ => unreachable!("record renders to an object"),
        },
    })
}

/// Emit: a nu result positional-type string -> the JSON schema object.
pub(crate) fn nu_to_result_schema(
    typedef: &str,
) -> Result<mcp::JsonObject, String> {
    let nu = parse_nu_result(typedef)?;
    let json = match nu {
        NuResultTypedef::Void => JsonResultTypedef::Void,
        NuResultTypedef::Record(r) => JsonResultTypedef::Record(record_n2j(&r)),
    };
    Ok(match json {
        JsonResultTypedef::Void => mcp::JsonObject::new(),
        JsonResultTypedef::Record(r) => match render_json_record(&r.fields) {
            json::Value::Object(m) => m,
            _ => unreachable!("record renders to an object"),
        },
    })
}

// ============================================================================
// Tests -- the grammar matrix both directions + the live shapes
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(s: &str) -> mcp::JsonObject {
        match serde_json::from_str::<json::Value>(s).unwrap() {
            json::Value::Object(m) => m,
            _ => panic!("not an object"),
        }
    }

    // ---- args_schema_to_nu (json -> nu) ----

    #[test]
    fn args_void() {
        assert_eq!(args_schema_to_nu(&obj("{}")).unwrap(), "nothing");
    }

    #[test]
    fn args_simple_record() {
        assert_eq!(
            args_schema_to_nu(&obj(r#"{"a":"int","b":"string"}"#)).unwrap(),
            "record<a: int, b: string>");
    }

    #[test]
    fn args_nested_record_list_table_oneof() {
        let got = args_schema_to_nu(&obj(
            r#"{"r":{"a":"string","b":["int"]},"t":[{"c":"int"}],"u":{"oneof<>":["int",null]}}"#,
        )).unwrap();
        assert_eq!(
            got,
            "record<r: record<a: string, b: list<int>>, t: table<c: int>, u: oneof<int, nothing>>");
    }

    #[test]
    fn args_list_of_oneof_with_record_member() {
        let got = args_schema_to_nu(&obj(
            r#"{"xs":[{"oneof<>":[{"a":"int"},"string"]}]}"#)).unwrap();
        assert_eq!(got, "record<xs: list<oneof<record<a: int>, string>>>");
    }

    #[test]
    fn args_all_scalars_incl_cell_path() {
        let got = args_schema_to_nu(&obj(
            r#"{"c":"cell-path","d":"directory","n":"number","p":"path"}"#)).unwrap();
        assert_eq!(got, "record<c: cell-path, d: directory, n: number, p: path>");
    }

    // ---- denials ----

    #[test]
    fn deny_any() {
        assert!(args_schema_to_nu(&obj(r#"{"x":"any"}"#)).is_err());
    }

    #[test]
    fn deny_nested_empty_record() {
        assert!(args_schema_to_nu(&obj(r#"{"x":{}}"#)).is_err());
    }

    #[test]
    fn deny_empty_array() {
        assert!(args_schema_to_nu(&obj(r#"{"x":[]}"#)).is_err());
    }

    #[test]
    fn deny_table_of_empty_record() {
        assert!(args_schema_to_nu(&obj(r#"{"x":[{}]}"#)).is_err());
    }

    #[test]
    fn deny_unknown_scalar() {
        assert!(args_schema_to_nu(&obj(r#"{"x":"bogus"}"#)).is_err());
    }

    #[test]
    fn deny_multi_element_array() {
        assert!(args_schema_to_nu(&obj(r#"{"x":["int","string"]}"#)).is_err());
    }

    #[test]
    fn deny_empty_oneof() {
        assert!(args_schema_to_nu(&obj(r#"{"x":{"oneof<>":[]}}"#)).is_err());
    }

    // ---- nu_to_args_schema (nu -> json) ----

    #[test]
    fn emit_void() {
        assert_eq!(nu_to_args_schema("nothing").unwrap(), obj("{}"));
    }

    #[test]
    fn emit_nested() {
        let got = nu_to_args_schema(
            "record<t: record<y: string, n: list<int>>, u: table<a: int>, v: oneof<int, nothing>>",
        ).unwrap();
        assert_eq!(got, obj(
            r#"{"t":{"y":"string","n":["int"]},"u":[{"a":"int"}],"v":{"oneof<>":["int",null]}}"#));
    }

    #[test]
    fn emit_rejects_top_level_record_empty() {
        assert!(nu_to_args_schema("record<>").is_err());
    }

    #[test]
    fn emit_rejects_bare_list() {
        assert!(nu_to_args_schema("record<x: list>").is_err());
    }

    // ---- round-trips ----

    #[test]
    fn roundtrip_json_nu_json() {
        for s in [
            "{}",
            r#"{"x":"int"}"#,
            r#"{"t":{"y":"string","n":["int"]}}"#,
            r#"{"xs":[{"oneof<>":[{"a":"int"},"string"]}]}"#,
            r#"{"tab":[{"set":"string","count":"int"}]}"#,
        ] {
            let nu = args_schema_to_nu(&obj(s)).unwrap();
            let back = nu_to_args_schema(&nu).unwrap();
            assert_eq!(back, obj(s), "roundtrip failed for {s}");
        }
    }

    #[test]
    fn result_void_and_record() {
        assert_eq!(result_schema_to_nu(&obj("{}")).unwrap(), "nothing");
        assert_eq!(
            result_schema_to_nu(&obj(r#"{"out":"int"}"#)).unwrap(),
            "record<out: int>");
        assert_eq!(nu_to_result_schema("nothing").unwrap(), obj("{}"));
    }

    // ---- nu -> json -> nu round-trips (the emit direction) ----

    #[test]
    fn roundtrip_nu_json_nu() {
        // Sorted field names so the BTreeMap-sorted render is
        // string-identical to the input.
        for nu in [
            "nothing",
            "record<a: int>",
            "record<a: int, b: string>",
            "record<r: record<x: cell-path, y: list<string>>>",
            "record<t: table<col: int, name: string>>",
            "record<u: oneof<int, nothing>>",
            "record<xs: list<oneof<int, string>>>",
            "record<n: oneof<record<a: int>, string>>",
        ] {
            let json = nu_to_args_schema(nu).unwrap();
            let back = args_schema_to_nu(&json).unwrap();
            assert_eq!(back, nu, "nu roundtrip failed for {nu}");
        }
    }

    // ---- emit normalizations + nested composites ----

    #[test]
    fn emit_list_of_record_normalizes_to_table_json() {
        // list<record<...>> and table<...> share the [{...}] JSON form:
        // the settled grammar maps [{record}] canonically to a table.
        assert_eq!(
            nu_to_args_schema("record<x: list<record<a: int>>>").unwrap(),
            obj(r#"{"x":[{"a":"int"}]}"#));
        assert_eq!(
            nu_to_args_schema("record<x: table<a: int>>").unwrap(),
            obj(r#"{"x":[{"a":"int"}]}"#));
    }

    #[test]
    fn emit_nested_list_and_list_of_table() {
        assert_eq!(
            nu_to_args_schema("record<x: list<list<int>>>").unwrap(),
            obj(r#"{"x":[["int"]]}"#));
        assert_eq!(
            nu_to_args_schema("record<x: list<table<a: int>>>").unwrap(),
            obj(r#"{"x":[[{"a":"int"}]]}"#));
    }

    #[test]
    fn emit_oneof_with_composite_members() {
        assert_eq!(
            nu_to_args_schema(
                "record<u: oneof<record<a: int>, table<b: string>, nothing>>",
            ).unwrap(),
            obj(r#"{"u":{"oneof<>":[{"a":"int"},[{"b":"string"}],null]}}"#));
    }

    // ---- emit denials (the nu parse side) ----

    #[test]
    fn emit_rejects_bare_table_and_record() {
        assert!(nu_to_args_schema("record<x: table>").is_err());
        assert!(nu_to_args_schema("record<x: record>").is_err());
    }

    #[test]
    fn emit_rejects_nested_empty_record_and_table() {
        assert!(nu_to_args_schema("record<x: record<>>").is_err());
        assert!(nu_to_args_schema("record<x: table<>>").is_err());
    }

    #[test]
    fn emit_rejects_any_and_unknown() {
        assert!(nu_to_args_schema("record<x: any>").is_err());
        assert!(nu_to_args_schema("record<x: frobnicate>").is_err());
    }

    #[test]
    fn emit_rejects_top_level_bare_forms() {
        assert!(nu_to_args_schema("int").is_err());
        assert!(nu_to_args_schema("list<int>").is_err());
        assert!(nu_to_args_schema("oneof<int, string>").is_err());
    }

    // ---- json-side denials not covered above ----

    #[test]
    fn deny_oneof_extra_key() {
        assert!(args_schema_to_nu(
            &obj(r#"{"x":{"oneof<>":["int"],"k":"string"}}"#)).is_err());
    }

    #[test]
    fn deny_oneof_value_not_array() {
        assert!(args_schema_to_nu(&obj(r#"{"x":{"oneof<>":"int"}}"#)).is_err());
    }

    #[test]
    fn deny_bool_and_number_nodes() {
        assert!(args_schema_to_nu(&obj(r#"{"x":true}"#)).is_err());
        assert!(args_schema_to_nu(&obj(r#"{"x":3}"#)).is_err());
    }

    // ---- all 14 scalars, both directions ----

    #[test]
    fn all_scalars_roundtrip_both_ways() {
        let names = [
            "int", "float", "string", "bool", "datetime", "duration",
            "filesize", "binary", "range", "number", "glob", "cell-path",
            "path", "directory",
        ];
        for n in names {
            let json = obj(&format!(r#"{{"f":"{n}"}}"#));
            let nu = format!("record<f: {n}>");
            assert_eq!(args_schema_to_nu(&json).unwrap(), nu, "j2n {n}");
            assert_eq!(nu_to_args_schema(&nu).unwrap(), json, "n2j {n}");
        }
    }

    // ---- the live migration shapes (regression locks) ----

    #[test]
    fn roundtrip_symbols_result_shape() {
        let s = r#"{"suspect_count":"int","suspects":[{"files":["string"],"id":"string"}]}"#;
        let nu = result_schema_to_nu(&obj(s)).unwrap();
        assert_eq!(
            nu,
            "record<suspect_count: int, suspects: table<files: list<string>, id: string>>");
        assert_eq!(nu_to_result_schema(&nu).unwrap(), obj(s));
    }

    #[test]
    fn roundtrip_measure_result_shape() {
        let s = r#"{"failed_picks":"int","per_group":[{"count":"int","group":"string","kp_chars":"int"}],"per_set":[{"count":"int","kp_chars":"int","set":"string"}],"total_kp_chars":"int","valid_picks":"int"}"#;
        let nu = result_schema_to_nu(&obj(s)).unwrap();
        assert_eq!(nu_to_result_schema(&nu).unwrap(), obj(s));
    }

    #[test]
    fn roundtrip_integrity_nested_record_shape() {
        let s = r#"{"counts":{"indexed":"int","memories":"int"},"p1_frontmatter":{"name_mismatch":[{"file":"string","name":"string","slug":"string"}]},"p5_sizes":{"memory_md_bytes":"int","over_cap":"bool"}}"#;
        let nu = result_schema_to_nu(&obj(s)).unwrap();
        assert_eq!(nu_to_result_schema(&nu).unwrap(), obj(s));
    }

    #[test]
    fn args_void_and_validate_result_shape() {
        // memories' migrated shape: void args + a list<string> result.
        assert_eq!(args_schema_to_nu(&obj("{}")).unwrap(), "nothing");
        let s = r#"{"failed_details":[{"bytes":"int","pattern":"string","reason":"string"}],"failed_patterns":["string"]}"#;
        let nu = result_schema_to_nu(&obj(s)).unwrap();
        assert_eq!(nu_to_result_schema(&nu).unwrap(), obj(s));
    }
}
