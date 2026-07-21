use crate::*;

/// The namespace-level meta directory. NEW with purviews - the store carried
/// only `keypair/`, `libraries/` and `host.lock` before it.
pub(crate) const META_DIR: &str = ".meta";

const PURVIEWS_FILE: &str = "purviews.nuon";

/// What is in view when nothing says otherwise. CONFIGURABLE, unlike the other
/// two built-ins.
pub(crate) const PURVIEW_DEFAULT: &str = "default";

/// What is in view right now. DERIVED from the session's current ids, so it is
/// never a row in the file.
pub(crate) const PURVIEW_CURRENT: &str = ".";

/// Everything. Both a built-in purview id and a namepath pattern, and the same
/// thing either way.
pub(crate) const PURVIEW_ALL: &str = "*";

pub(crate) fn purviews_path() -> PathBuf {
    data_base_dir().join(META_DIR).join(PURVIEWS_FILE)
}

/// One configured purview: an arbitrary path-like label bound to the selectors
/// it puts in view.
///
/// `namepath_patterns` keeps the design's column name verbatim, so the persisted
/// table reads as it was specified rather than as it was implemented.
#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct PurviewRow {
    pub id: String,
    pub namepath_patterns: Vec<String>,
}

/// Render the purview table as NUON, through the serde Value bridge the library
/// index already uses - one bridge for the whole shape, and it cannot drift from
/// the struct.
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

/// Every configured purview, or None when the namespace is UNCONFIGURED.
///
/// ABSENT IS NOT EMPTY, and the distinction is the whole contract: a missing
/// file means nothing has been configured, which resolves to EVERYTHING, while a
/// present-but-empty table means someone deliberately configured nothing. The
/// library index made the opposite mistake once - a missing meta file reads as
/// "no libraries" rather than as an error - and this file must not repeat the
/// shape.
pub(crate) fn load_purviews() -> Result<Option<Vec<PurviewRow>>, Error> {
    let path = purviews_path();
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    purviews_from_nuon(&text)
        .map(Some)
        .map_err(|reason| Error::Internal {
            phase: "purview::decode".to_string(),
            reason,
        })
}

pub(crate) fn save_purviews(rows: &[PurviewRow]) -> Result<(), Error> {
    let path = purviews_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let nuon = purviews_to_nuon(rows).map_err(|reason| Error::Internal {
        phase: "purview::encode".to_string(),
        reason,
    })?;
    fs::write(&path, nuon.as_bytes())?;
    Ok(())
}

/// Is `id` a legal purview label?
///
/// Path-LIKE but never a path: bare relative, slash-separated snake components.
/// A leading `/` or `./` is rejected outright, because a label that looks like a
/// filesystem path invites being read as one - and a purview id is arbitrary,
/// unrelated to any namepath or file on disk.
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

/// A built-in that is DERIVED rather than stored, so it can never be configured.
pub(crate) fn is_derived_purview(id: &str) -> bool {
    id == PURVIEW_CURRENT || id == PURVIEW_ALL
}

/// The selectors a set of purview ids puts in view, in order and de-duplicated.
///
/// `*` is everything, and so is an UNCONFIGURED `default` - the design's "a new
/// namespace sees the whole library", and the reason absent and empty stay
/// distinguishable above. An id with no row contributes NOTHING rather than
/// everything, because a typo must narrow the view rather than silently open it.
pub(crate) fn resolve_selectors(
    ids: &[String],
    rows: Option<&Vec<PurviewRow>>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        let selectors: Vec<String> = if id == PURVIEW_ALL {
            vec![PURVIEW_ALL.to_string()]
        } else {
            match rows.and_then(|r| r.iter().find(|row| &row.id == id)) {
                Some(row) => row.namepath_patterns.clone(),
                // Unconfigured `default` is everything; any other absent id is
                // nothing.
                None if id == PURVIEW_DEFAULT => vec![PURVIEW_ALL.to_string()],
                None => Vec::new(),
            }
        };
        for s in selectors {
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

/// Parse selector strings into the matcher form, dropping any that do not parse.
///
/// A stored selector that no longer parses is treated as absent rather than
/// fatal: a purview file is agent-authored data, and one bad row must not take
/// `info()` down with it.
pub(crate) fn parse_selectors(selectors: &[String]) -> Vec<NamepathStr> {
    selectors
        .iter()
        .filter_map(|s| NamepathStr::parse(s).ok())
        .collect()
}

/// Which registered library, if any, a selector needs in order to mean anything.
///
/// None means it needs no particular library (`*`), so it can never dangle.
fn selector_requires(selector: &str) -> Option<SelectorNeed> {
    match NamepathStr::parse(selector).ok()? {
        NamepathStr::Pattern(NamepathPattern::All) => None,
        NamepathStr::Pattern(NamepathPattern::Current) => Some(SelectorNeed::Nothing),
        NamepathStr::Pattern(NamepathPattern::Author { author }) => {
            Some(SelectorNeed::Author(author))
        }
        NamepathStr::Pattern(
            NamepathPattern::Library { library }
            | NamepathPattern::ModuleTree { library, .. }
            | NamepathPattern::ModuleCalls { library, .. },
        ) => Some(SelectorNeed::Library(library)),
        NamepathStr::Namepath(n) => n.validate().ok().map(|r| SelectorNeed::Library(r.library().to_string())),
    }
}

enum SelectorNeed {
    Author(String),
    Library(String),
    /// Names no library at all - a stored `.` is meaningless in a row.
    Nothing,
}

/// Drop selectors no registered library can satisfy, returning what was pruned.
///
/// Pruning is by REGISTRATION, not by emptiness: a library with no calls yet
/// still satisfies its own pattern, and a purview that pointed at it should
/// survive until the library actually goes away.
///
/// A row pruned down to NOTHING is dropped rather than kept as an empty purview,
/// because an empty selector list is already the DELETE operation on the
/// configure tool - so a surviving empty row would be a state the tool surface
/// cannot otherwise produce, and it would resolve to "sees nothing" while
/// looking configured.
pub(crate) fn prune_dangling(rows: &mut Vec<PurviewRow>) -> Vec<String> {
    let names = registered_library_names();
    let mut pruned: Vec<String> = Vec::new();
    for row in rows.iter_mut() {
        row.namepath_patterns.retain(|selector| {
            let live = match selector_requires(selector) {
                None => true,
                Some(SelectorNeed::Nothing) => false,
                Some(SelectorNeed::Author(author)) => names
                    .iter()
                    .any(|n| n.split_once('/').is_some_and(|(a, _)| a == author)),
                Some(SelectorNeed::Library(library)) => names.iter().any(|n| n == &library),
            };
            if !live && !pruned.contains(selector) {
                pruned.push(selector.clone());
            }
            live
        });
    }
    rows.retain(|row| !row.namepath_patterns.is_empty());
    pruned
}

/// What changed between two selector sets - the delta the extend and reset
/// tools report, as `(added, removed)`.
pub(crate) fn selector_delta(
    before: &[String],
    after: &[String],
) -> (Vec<String>, Vec<String>) {
    let added: Vec<String> = after
        .iter()
        .filter(|s| !before.contains(s))
        .cloned()
        .collect();
    let removed: Vec<String> = before
        .iter()
        .filter(|s| !after.contains(s))
        .cloned()
        .collect();
    (added, removed)
}

/// May `id` be named as something to bring into view?
///
/// The built-ins always may; anything else must actually be configured. An
/// unknown id would otherwise contribute nothing in silence, which turns a typo
/// into a view that simply does not widen.
pub(crate) fn is_nameable_purview(
    id: &str,
    rows: Option<&Vec<PurviewRow>>,
) -> bool {
    id == PURVIEW_ALL
        || id == PURVIEW_DEFAULT
        || rows.is_some_and(|r| r.iter().any(|row| row.id == id))
}

/// The CURRENT purview: which purview ids this HOST PROCESS has in view.
///
/// Session-resident by design - it is a view rather than a configuration, so it
/// belongs in memory and dies with the host. It starts at `default`, so a host
/// nobody tells otherwise shows exactly what `default` shows.
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

    /// Add ids to the current view. Additive and order-preserving; an id already
    /// in view is not repeated, since duplicates would only widen the render's
    /// work without widening the view.
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

    /// Back to `default` - the startup state.
    pub(crate) fn reset(&self) -> Vec<String> {
        let mut ids = self.lock();
        *ids = vec![PURVIEW_DEFAULT.to_string()];
        ids.clone()
    }

    /// Drop ids that no longer exist, so an uninstall or a deletion cannot leave
    /// the session pointing at something gone.
    pub(crate) fn retain_known(
        &self,
        rows: Option<&Vec<PurviewRow>>,
    ) {
        let mut ids = self.lock();
        ids.retain(|id| {
            id == PURVIEW_DEFAULT
                || id == PURVIEW_ALL
                || rows.is_some_and(|r| r.iter().any(|row| &row.id == id))
        });
        if ids.is_empty() {
            *ids = vec![PURVIEW_DEFAULT.to_string()];
        }
    }
}

/// One `[id, patterns]` pair as `info()` and `purview_list()` report it.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub struct PurviewView(pub String, pub Vec<String>);

/// The reported form of a set of purview ids: each id beside what it resolves
/// to, in the order they came into view.
pub(crate) fn purview_views(
    ids: &[String],
    rows: Option<&Vec<PurviewRow>>,
) -> Vec<PurviewView> {
    ids.iter()
        .map(|id| PurviewView(id.clone(), resolve_selectors(std::slice::from_ref(id), rows)))
        .collect()
}
