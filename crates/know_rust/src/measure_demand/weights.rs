use crate::*;

/// What: one parsed row of consumer_repos.txt - the panel target it
/// exercises, the pair-dir snake, the role (weight feeds the
/// consumer-demand significance blob; audit rides the zero-miss
/// gate), the consumer's provenance URL (`local:<path>` marks an
/// on-box consumer root), the pin ref, and an optional explicit
/// consumer-root override from a `# root: <path>` comment line.
///
/// Why: the batch form is roster-driven; the row is the unit the
/// resolver + the per-pair loop consume, and the role column is what
/// keeps weight-source pairs out of the independent audit.
///
/// Where: produced by `parse_consumer_repos`; consumed by
/// `measure_demand::run::measure_consumers`.
#[derive(Debug, Clone)]
pub struct ConsumerRow {
    pub target: String,
    pub snake: String,
    pub role: ConsumerRole,
    pub url: String,
    pub root_override: Option<PathBuf>,
}

/// What: the roster role - `Weight` rows feed the aggregated demand
/// blob and only report; `Audit` rows are held out of weights and
/// carry the zero-miss exit gate.
///
/// Why: a consumer whose demand feeds pick weights stops being an
/// independent audit by construction; the role column makes the
/// split explicit in the roster.
///
/// Where: parsed in `parse_consumer_repos`; branched on in
/// `measure_consumers` for gating + blob aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumerRole {
    Weight,
    Audit,
}

impl ConsumerRole {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Weight => "weight",
            Self::Audit => "audit",
        }
    }
}

/// What: parse consumer_repos.txt into rows. Data lines carry five
/// whitespace-separated columns (`<target> <snake> <role> <url>
/// <tag|sha>`); `#` lines are comments, except a `# root: <path>`
/// comment which attaches an explicit consumer-root override to the
/// most recent data row (the clone-reuse case, e.g. cosmic-files
/// living inside the cosmic-epoch submodule).
///
/// Why: the roster file is the single declaration surface for pairs;
/// the structured root-override comment keeps reuse machine-readable
/// without adding a column that is usually empty.
///
/// Where: called by `measure_consumers` on the CLI-supplied path.
pub fn parse_consumer_repos(path: &Path) -> Result<Vec<ConsumerRow>> {
    let text = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut rows: Vec<ConsumerRow> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            if let Some(root) = rest.trim().strip_prefix("root:") {
                if let Some(last) = rows.last_mut() {
                    last.root_override = Some(PathBuf::from(root.trim()));
                }
            }
            continue;
        }
        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        if cols.len() != 5 {
            eprintln!(
                "[measure consumers] skipping malformed row ({} columns): {}",
                cols.len(),
                trimmed
            );
            continue;
        }
        let role = match cols[2] {
            "weight" => ConsumerRole::Weight,
            "audit" => ConsumerRole::Audit,
            other => {
                eprintln!(
                    "[measure consumers] skipping row with unknown role `{}`: {}",
                    other, trimmed
                );
                continue;
            }
        };
        rows.push(ConsumerRow {
            target: cols[0].to_string(),
            snake: cols[1].to_string(),
            role,
            url: cols[3].to_string(),
            root_override: None,
        });
    }
    Ok(rows)
}

/// What: per-entry weight cell - how many weight consumers demanded
/// the key and across how many recorded demand sites.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WeightCell {
    pub consumers: usize,
    pub sites: usize,
}

/// What: one target's aggregated consumer-demand weights - the
/// contributing weight consumers (with the pass identity each was
/// traced against), demanded NAME weights, and demanded
/// `<outer>::<inner>` PAIR weights.
///
/// Why: the score side consumes names + pairs; the sources list
/// carries provenance so a weight derived against a fork identity
/// (halloy -> squidowl/iced) is auditable at application time.
///
/// Where: values of `WeightBlob::targets`; built by
/// `aggregate_weights`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TargetWeights {
    #[serde(default)]
    pub sources: Vec<WeightSource>,
    #[serde(default)]
    pub names: BTreeMap<String, WeightCell>,
    #[serde(default)]
    pub pairs: BTreeMap<String, WeightCell>,
}

/// What: one contributing weight consumer - its snake and the
/// target pass directory its trace ran against (the pass identity).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WeightSource {
    pub consumer: String,
    pub pass: String,
}

/// What: the consumer-demand weight blob, keyed by roster target.
/// Serialized deterministically (BTreeMaps) so the blob pins into a
/// sample's `input/` byte-reproducibly.
///
/// Why: the mechanical significance source for declared-but-unused
/// public API - regenerated whole from the roster per pass, never
/// accumulated.
///
/// Where: written by `measure_consumers --weights-out`; consumed by
/// the characterize/emit weight term.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WeightBlob {
    pub targets: BTreeMap<String, TargetWeights>,
}

/// What: load a weight blob JSON from disk.
///
/// Why: emit consumes the blob via the global `--weights` flag; the
/// loader keeps the read + parse error shapes uniform.
///
/// Where: called from `crate::run::run` when the flag is present.
pub fn load_weights(path: &Path) -> Result<WeightBlob> {
    let text = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| Error::Serialize { source })
}

/// What: resolve which blob target applies to the workspace being
/// emitted: the workspace root's basename first (panel + pair clone
/// layouts name the dir after the target), else the first blob
/// target that is one of the workspace's own crate names.
///
/// Why: the blob is keyed by roster target name; the workspace under
/// emit carries no roster identity of its own, so resolution rides
/// the two naming conventions that hold across the panel, pair dirs,
/// and fixtures.
///
/// Where: called by `emit::run::emit` before threading the target
/// weights into the picker.
pub fn target_weights<'a>(
    blob: &'a WeightBlob,
    workspace_root: &Path,
    crate_names: &[String],
) -> Option<&'a TargetWeights> {
    if let Some(base) = workspace_root.file_name().and_then(|n| n.to_str()) {
        if let Some(tw) = blob.targets.get(base) {
            return Some(tw);
        }
    }
    crate_names.iter().find_map(|c| blob.targets.get(c))
}

/// What: fold one weight pair's demand report into the blob under
/// `target`: every demanded name and pair contributes its site count
/// and one consumer vote.
///
/// Why: aggregation is additive across consumers per target; keeping
/// it report-shaped (names + pairs straight from the trace) means
/// the blob needs no new extraction pass.
///
/// Where: called by `measure_consumers` for each Weight-role row.
pub fn fold_weights(
    blob: &mut WeightBlob,
    target: &str,
    consumer: &str,
    pass: &Path,
    report: &DemandReport,
) {
    let tw = blob.targets.entry(target.to_string()).or_default();
    tw.sources.push(WeightSource {
        consumer: consumer.to_string(),
        pass: pass.display().to_string(),
    });
    for rec in report.hits.iter().chain(report.summary.misses.iter()) {
        let cell = tw.names.entry(rec.name.clone()).or_default();
        cell.consumers += 1;
        cell.sites += rec.sites.max(1);
    }
    for (pair, sites) in &report.pair_sites {
        let cell = tw.pairs.entry(pair.clone()).or_default();
        cell.consumers += 1;
        cell.sites += (*sites).max(1);
    }
}
