use crate::*;

/// The namespace-level meta directory.
pub(crate) const META_DIR: &str = ".meta";

const PURVIEWS_FILE: &str = "purviews.nuon";

/// What is in view when nothing says otherwise; CONFIGURABLE.
pub(crate) const PURVIEW_DEFAULT: &str = "default";

/// What is in view right now; DERIVED, never a row.
pub(crate) const PURVIEW_CURRENT: &str = ".";

/// Everything: both a built-in purview id and a namepath pattern.
pub(crate) const PURVIEW_ALL: &str = "*";

pub(crate) fn purviews_path() -> PathBuf {
    data_base_dir().join(META_DIR).join(PURVIEWS_FILE)
}

/// One configured purview: a label bound to what it puts in view.
#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct PurviewRow {
    pub id: String,
    pub namepath_patterns: Vec<String>,
}

/// Render the purview table as NUON through the serde Value bridge.
pub(crate) fn purviews_to_nuon(rows: &[PurviewRow]) -> Result<String, String> {
    let json = json::to_value(rows).map_err(|e| e.to_string())?;
    let value = json_value_to_nu_value(&json);
    nu::to_nuon(&nu::EngineState::new(), &value, nu::ToNuonConfig::default())
        .map_err(|e| e.to_string())
}

pub(crate) fn purviews_from_nuon(text: &str) -> Result<Vec<PurviewRow>, String> {
    let value = nu::from_nuon(text, None).map_err(|e| e.to_string())?;
    let json_compat = nu::JsonValue::from_value(value).map_err(|e| e.to_string())?;
    let json = json::to_value(&json_compat).map_err(|e| e.to_string())?;
    json::from_value(json).map_err(|e| e.to_string())
}

/// Every configured purview; None only before startup materialized it.
pub(crate) fn load_purviews() -> Result<Option<Vec<PurviewRow>>, GrammarMcpError> {
    let path = purviews_path();
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    purviews_from_nuon(&text)
        .map(Some)
        .map_err(|reason| GrammarMcpError::Internal {
            phase: "purview::decode".to_string(),
            reason,
        })
}

pub(crate) fn save_purviews(rows: &[PurviewRow]) -> Result<(), GrammarMcpError> {
    let path = purviews_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let nuon = purviews_to_nuon(rows).map_err(|reason| GrammarMcpError::Internal {
        phase: "purview::encode".to_string(),
        reason,
    })?;
    fs::write(&path, nuon.as_bytes())?;
    Ok(())
}

/// Is `id` a legal purview label? Path-LIKE, but never a path.
pub(crate) fn is_valid_purview_id(id: &str) -> bool {
    if id.is_empty() || id.starts_with('/') || id.starts_with("./") || id.ends_with('/') {
        return false;
    }
    if id.contains("//") {
        return false;
    }
    id.split('/').all(|seg| {
        let mut chars = seg.chars();
        match chars.next() {
            Some(first) if first.is_ascii_lowercase() || first == '_' => {
                chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            }
            _ => false,
        }
    })
}

/// A built-in that is DERIVED rather than stored.
pub(crate) fn is_derived_purview(id: &str) -> bool {
    id == PURVIEW_CURRENT || id == PURVIEW_ALL
}

/// The sigil marking a value as a REFERENCE to another purview.
pub(crate) const PURVIEW_REF: char = '@';

/// The purview id a value references, or None for a plain pattern.
pub(crate) fn purview_ref(value: &str) -> Option<&str> {
    value.strip_prefix(PURVIEW_REF)
}

/// May `id` be referenced as `@id`?
pub(crate) fn is_valid_purview_ref(id: &str) -> bool {
    !is_derived_purview(id) && is_valid_purview_id(id)
}

/// Expand purview REFERENCES into the patterns they stand for.
pub(crate) fn expand_values(
    values: &[String],
    rows: Option<&Vec<PurviewRow>>,
) -> Vec<String> {
    fn walk(
        values: &[String],
        rows: Option<&Vec<PurviewRow>>,
        seen: &mut Vec<String>,
        out: &mut Vec<String>,
    ) {
        for value in values {
            let Some(id) = purview_ref(value) else {
                if !out.contains(value) {
                    out.push(value.clone());
                }
                continue;
            };
            if seen.iter().any(|s| s == id) {
                continue;
            }
            seen.push(id.to_string());
            if let Some(row) = rows.and_then(|r| r.iter().find(|row| row.id == id)) {
                let nested = row.namepath_patterns.clone();
                walk(&nested, rows, seen, out);
            }
        }
    }
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    walk(values, rows, &mut seen, &mut out);
    out
}

/// Write `default` as `['*']` when it has no row. Runs at startup.
pub(crate) fn ensure_default_purview() -> Result<(), GrammarMcpError> {
    let mut rows = load_purviews()?.unwrap_or_default();
    if rows.iter().any(|row| row.id == PURVIEW_DEFAULT) {
        return Ok(());
    }
    rows.push(PurviewRow {
        id: PURVIEW_DEFAULT.to_string(),
        namepath_patterns: vec![PURVIEW_ALL.to_string()],
    });
    save_purviews(&rows)
}

/// The namepath patterns a set of ids puts in view, in order and deduplicated.
pub(crate) fn resolve_patterns(
    ids: &[String],
    rows: Option<&Vec<PurviewRow>>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        let patterns: Vec<String> = if id == PURVIEW_ALL {
            vec![PURVIEW_ALL.to_string()]
        } else {
            match rows.and_then(|r| r.iter().find(|row| &row.id == id)) {
                Some(row) => row.namepath_patterns.clone(),
                None => Vec::new(),
            }
        };
        for pattern in patterns {
            if !out.contains(&pattern) {
                out.push(pattern);
            }
        }
    }
    out
}

/// Parse pattern strings into the matcher form, dropping any that fail.
pub(crate) fn parse_patterns(patterns: &[String]) -> Vec<NamepathStr> {
    patterns
        .iter()
        .filter_map(|p| NamepathStr::parse(p).ok())
        .collect()
}

/// What a stored value needs in order to mean anything.
fn pattern_requires(pattern: &str) -> Option<PatternNeed> {
    if let Some(id) = purview_ref(pattern) {
        return Some(PatternNeed::Purview(id.to_string()));
    }
    match NamepathStr::parse(pattern).ok()? {
        NamepathStr::Pattern(NamepathPattern::All) => None,
        NamepathStr::Pattern(NamepathPattern::Current) => Some(PatternNeed::Nothing),
        NamepathStr::Pattern(NamepathPattern::Author { author }) => {
            Some(PatternNeed::Author(author))
        }
        NamepathStr::Pattern(
            NamepathPattern::Rig { rig }
            | NamepathPattern::ModuleTree { rig, .. }
            | NamepathPattern::ModuleCalls { rig, .. },
        ) => Some(PatternNeed::Rig(rig)),
        NamepathStr::Namepath(n) => n
            .validate()
            .ok()
            .map(|r| PatternNeed::Rig(r.rig().to_string())),
    }
}

enum PatternNeed {
    Author(String),
    Rig(String),
    /// A `@id` reference, satisfied by a PURVIEW rather than by a rig.
    Purview(String),
    /// Names nothing at all - a stored `.` is meaningless in a row.
    Nothing,
}

/// Drop patterns no registered rig can satisfy, returning what was pruned.
pub(crate) fn prune_dangling(rows: &mut Vec<PurviewRow>) -> Vec<String> {
    let names = registered_rig_names();
    let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    let mut pruned: Vec<String> = Vec::new();
    for row in rows.iter_mut() {
        row.namepath_patterns.retain(|pattern| {
            let live = match pattern_requires(pattern) {
                None => true,
                Some(PatternNeed::Nothing) => false,
                Some(PatternNeed::Author(author)) => names
                    .iter()
                    .any(|n| n.split_once('/').is_some_and(|(a, _)| a == author)),
                Some(PatternNeed::Rig(rig)) => names.iter().any(|n| n == &rig),
                Some(PatternNeed::Purview(id)) => ids.iter().any(|k| k == &id),
            };
            if !live && !pruned.contains(pattern) {
                pruned.push(pattern.clone());
            }
            live
        });
    }
    rows.retain(|row| !row.namepath_patterns.is_empty());
    pruned
}

/// What changed between two pattern sets, as `(added, removed)`.
pub(crate) fn pattern_delta(
    before: &[String],
    after: &[String],
) -> (Vec<String>, Vec<String>) {
    let added: Vec<String> = after
        .iter()
        .filter(|p| !before.contains(p))
        .cloned()
        .collect();
    let removed: Vec<String> = before
        .iter()
        .filter(|p| !after.contains(p))
        .cloned()
        .collect();
    (added, removed)
}

/// May `id` be named as something to bring into view?
pub(crate) fn is_nameable_purview(
    id: &str,
    rows: Option<&Vec<PurviewRow>>,
) -> bool {
    id == PURVIEW_ALL || rows.is_some_and(|r| r.iter().any(|row| row.id == id))
}

/// The CURRENT purview: which ids this HOST PROCESS has in view.
pub(crate) struct CurrentPurview {
    ids: std::sync::Mutex<Vec<String>>,
}

impl Default for CurrentPurview {
    fn default() -> Self {
        Self::new()
    }
}

impl CurrentPurview {
    pub(crate) fn new() -> Self {
        Self {
            ids: std::sync::Mutex::new(vec![PURVIEW_DEFAULT.to_string()]),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
        self.ids.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn ids(&self) -> Vec<String> {
        self.lock().clone()
    }

    /// Add ids to the current view; additive and order-preserving.
    pub(crate) fn extend(
        &self,
        add: &[String],
    ) -> Vec<String> {
        let mut ids = self.lock();
        for id in add {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        ids.clone()
    }

    /// Replace what is in view; an EMPTY list means `default`.
    pub(crate) fn set(
        &self,
        want: &[String],
    ) -> Vec<String> {
        let mut ids = self.lock();
        *ids = if want.is_empty() {
            vec![PURVIEW_DEFAULT.to_string()]
        } else {
            want.to_vec()
        };
        ids.clone()
    }

    /// Drop ids that no longer exist.
    pub(crate) fn retain_known(
        &self,
        rows: Option<&Vec<PurviewRow>>,
    ) {
        let mut ids = self.lock();
        ids.retain(|id| {
            id == PURVIEW_ALL || rows.is_some_and(|r| r.iter().any(|row| &row.id == id))
        });
        if ids.is_empty() {
            *ids = vec![PURVIEW_DEFAULT.to_string()];
        }
    }
}

/// One `[id, namepath_patterns]` pair as the tools report it.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub struct PurviewView(pub String, pub Vec<String>);

/// Each id beside the namepath patterns it resolves to.
pub(crate) fn purview_views(
    ids: &[String],
    rows: Option<&Vec<PurviewRow>>,
) -> Vec<PurviewView> {
    ids.iter()
        .map(|id| PurviewView(id.clone(), resolve_patterns(std::slice::from_ref(id), rows)))
        .collect()
}
