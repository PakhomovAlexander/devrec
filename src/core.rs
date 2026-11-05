use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: u32 = 1;
fn version() -> u32 {
    SCHEMA_VERSION
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default)]
    pub protected_sources: Vec<String>,
    #[serde(default)]
    pub review_adapter: Option<String>,
    #[serde(default)]
    pub review_adapter_digest: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: version(),
            protected_sources: vec![],
            review_adapter: None,
            review_adapter_digest: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub schema_version: u32,
    #[serde(default)]
    pub records: BTreeMap<String, Record>,
    #[serde(default)]
    pub consumed: BTreeMap<String, String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            schema_version: version(),
            records: BTreeMap::new(),
            consumed: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Changelog,
    Adr,
    Docs,
    Performance,
}
impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Changelog => "changelog",
            Self::Adr => "adr",
            Self::Docs => "docs",
            Self::Performance => "performance",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub schema_version: u32,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub kind: Kind,
    pub title: String,
    pub body: String,
    #[serde(default = "version")]
    pub revision: u32,
    #[serde(default)]
    pub revises: Option<String>,
    #[serde(default)]
    pub retired: bool,
    #[serde(default)]
    pub supersedes: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub accepted: bool,
    #[serde(default)]
    pub performance: Option<Performance>,
    #[serde(default)]
    pub provenance: Option<Provenance>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub after: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Performance {
    Policy {
        source_inventory_digest: String,
        rationale: String,
    },
    Measurement {
        source_inventory_digest: String,
        evidence: EvidenceProvenance,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceProvenance {
    pub path: String,
    pub digest: String,
    pub command: Vec<String>,
    pub result_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub task_id: String,
    pub revision_id: String,
    pub result_digest: String,
    pub request_digest: String,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
}
pub fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub fn valid_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && !value.contains('\0')
        && value
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}

pub fn validate_config(config: &Config) -> Result<(), String> {
    if config.schema_version != SCHEMA_VERSION {
        return Err("unsupported config schema_version".into());
    }
    let mut seen = BTreeSet::new();
    for path in &config.protected_sources {
        if !valid_relative_path(path) || !seen.insert(path) {
            return Err(format!("invalid or duplicate protected source: {path}"));
        }
    }
    match (&config.review_adapter, &config.review_adapter_digest) {
        (None, None) => (),
        (Some(path), Some(digest))
            if std::path::Path::new(path).is_absolute() && valid_digest(digest) =>
        {
            ()
        }
        _ => return Err("review adapter requires an absolute path and sha256 digest".into()),
    }
    Ok(())
}

pub fn compose(
    state: &State,
    pending: &BTreeMap<String, Record>,
) -> Result<BTreeMap<String, Record>, String> {
    if state.schema_version != SCHEMA_VERSION {
        return Err("unsupported state schema_version".into());
    }
    let mut records = state.records.clone();
    for (id, record) in pending {
        if records.contains_key(id) || state.consumed.contains_key(id) {
            return Err(format!("record {id} reuses an immutable historical ID"));
        }
        records.insert(id.clone(), record.clone());
    }
    validate(&records)?;
    Ok(records)
}

// Kahn's algorithm with a sorted ready set makes independent additions commute.
fn ordered(
    edges: &BTreeMap<String, BTreeSet<String>>,
    context: &str,
) -> Result<Vec<String>, String> {
    let mut remaining = edges.clone();
    let mut result = Vec::with_capacity(edges.len());
    while !remaining.is_empty() {
        let Some(id) = remaining
            .iter()
            .find(|(_, refs)| refs.is_empty())
            .map(|(id, _)| id.clone())
        else {
            return Err(format!("{context} cycle"));
        };
        remaining.remove(&id);
        for refs in remaining.values_mut() {
            refs.remove(&id);
        }
        result.push(id);
    }
    Ok(result)
}

fn section_order(record: &Record) -> Result<Vec<String>, String> {
    let mut edges = BTreeMap::new();
    for section in &record.sections {
        if !valid_id(&section.id)
            || section.title.trim().is_empty()
            || edges.insert(section.id.clone(), BTreeSet::new()).is_some()
        {
            return Err(format!("invalid or duplicate section in {}", record.id));
        }
    }
    for section in &record.sections {
        let mut refs = BTreeSet::new();
        for after in &section.after {
            if !edges.contains_key(after) || !refs.insert(after.clone()) {
                return Err(format!(
                    "invalid section reference {after} in {}",
                    record.id
                ));
            }
        }
        edges.insert(section.id.clone(), refs);
    }
    ordered(&edges, &format!("sections of {}", record.id))
}

pub fn validate(records: &BTreeMap<String, Record>) -> Result<(), String> {
    let mut edges = BTreeMap::new();
    let mut revisions: BTreeMap<&str, Vec<&Record>> = BTreeMap::new();
    let mut replaced = BTreeSet::new();
    for (id, r) in records {
        if r.schema_version != SCHEMA_VERSION {
            return Err(format!("unsupported record schema_version for {id}"));
        }
        if id != &r.id || !valid_id(id) || r.title.trim().is_empty() || r.revision == 0 {
            return Err(format!("invalid record identity or title: {id}"));
        }
        if r.accepted && r.kind != Kind::Adr {
            return Err(format!("only ADRs can be accepted: {id}"));
        }
        if r.retired && (r.revises.is_none() || r.body.trim().is_empty()) {
            return Err(format!(
                "retirement {id} requires a predecessor and rationale"
            ));
        }
        if r.kind != Kind::Docs && !r.sections.is_empty() {
            return Err(format!("sections require docs kind: {id}"));
        }
        section_order(r)?;
        match (&r.performance, r.kind) {
            (
                Some(Performance::Policy {
                    source_inventory_digest,
                    rationale,
                }),
                Kind::Performance,
            ) => {
                if !valid_digest(source_inventory_digest) || rationale.trim().is_empty() {
                    return Err(format!("invalid performance policy: {id}"));
                }
            }
            (
                Some(Performance::Measurement {
                    source_inventory_digest,
                    evidence,
                }),
                Kind::Performance,
            ) => {
                if !valid_digest(source_inventory_digest)
                    || !valid_relative_path(&evidence.path)
                    || !valid_digest(&evidence.digest)
                    || !valid_digest(&evidence.result_digest)
                    || evidence.command.is_empty()
                    || evidence
                        .command
                        .iter()
                        .any(|v| v.is_empty() || v.contains('\0'))
                {
                    return Err(format!("invalid measurement evidence: {id}"));
                }
            }
            (None, Kind::Performance) => {
                return Err(format!(
                    "performance record {id} requires typed policy or measurement"
                ));
            }
            (Some(_), _) => {
                return Err(format!("performance data requires performance kind: {id}"));
            }
            (None, _) => (),
        }
        if let Some(p) = &r.provenance {
            if p.task_id.trim().is_empty()
                || p.revision_id.trim().is_empty()
                || !valid_digest(&p.result_digest)
                || !valid_digest(&p.request_digest)
            {
                return Err(format!("invalid provenance: {id}"));
            }
        }
        let mut refs = BTreeSet::new();
        for list in [&r.depends_on, &r.supersedes, &r.conflicts_with] {
            let mut unique = BTreeSet::new();
            for target in list {
                if target == id || !records.contains_key(target) || !unique.insert(target) {
                    return Err(format!("invalid reference {target} from {id}"));
                }
            }
        }
        refs.extend(r.depends_on.iter().cloned());
        for target in &r.supersedes {
            if records[target].kind != r.kind {
                return Err(format!("supersession must preserve kind: {id} -> {target}"));
            }
            refs.insert(target.clone());
            replaced.insert(target.clone());
        }
        if let Some(target) = &r.revises {
            let prior = records
                .get(target)
                .ok_or_else(|| format!("missing revision predecessor {target}"))?;
            if prior.accepted {
                return Err(format!(
                    "accepted ADR {target} is immutable; create a successor decision"
                ));
            }
            if prior.kind != r.kind || prior.revision.checked_add(1) != Some(r.revision) {
                return Err(format!("invalid revision sequence: {id}"));
            }
            refs.insert(target.clone());
            replaced.insert(target.clone());
            revisions.entry(target).or_default().push(r);
        } else if r.revision != 1 {
            return Err(format!("initial record {id} must have revision 1"));
        }
        edges.insert(id.clone(), refs);
    }
    ordered(&edges, "record relationships")?;
    for (prior, branches) in revisions {
        for (i, left) in branches.iter().enumerate() {
            for right in &branches[i + 1..] {
                if !records
                    .values()
                    .any(|r| r.supersedes.contains(&left.id) && r.supersedes.contains(&right.id))
                {
                    return Err(format!(
                        "competing revisions {} and {} of {prior} require an explicit common successor",
                        left.id, right.id
                    ));
                }
            }
        }
    }
    for r in records.values().filter(|r| !replaced.contains(&r.id)) {
        for target in &r.conflicts_with {
            if !replaced.contains(target) {
                return Err(format!("unresolved conflict: {} and {target}", r.id));
            }
        }
    }
    Ok(())
}

pub fn render(records: &BTreeMap<String, Record>) -> Result<String, String> {
    validate(records)?;
    let edges = records
        .iter()
        .map(|(id, r)| {
            (
                id.clone(),
                r.depends_on
                    .iter()
                    .chain(r.supersedes.iter())
                    .chain(r.revises.iter())
                    .cloned()
                    .collect(),
            )
        })
        .collect();
    let order = ordered(&edges, "record relationships")?;
    let mut successors: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for r in records.values() {
        for prior in r.supersedes.iter().chain(r.revises.iter()) {
            successors.entry(prior).or_default().insert(&r.id);
        }
    }
    let mut out = String::from("# Development records\n");
    let mut last_kind = None;
    for id in &order {
        let r = &records[id];
        if last_kind != Some(r.kind) {
            out.push_str(&format!("\n## {}\n", r.kind.as_str()));
            last_kind = Some(r.kind);
        }
        out.push_str(&format!(
            "\n<a id=\"record:{}\"></a>\n\n### {}\n\n",
            r.id, r.title
        ));
        out.push_str(&format!("ID: `{}` · revision {}", r.id, r.revision));
        if r.accepted {
            out.push_str(" · accepted");
        }
        if r.retired {
            out.push_str(" · retired");
        }
        if let Some(next) = successors.get(r.id.as_str()) {
            out.push_str(" · superseded by ");
            out.push_str(
                &next
                    .iter()
                    .map(|id| format!("[{}](#record:{})", id, id))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        out.push_str("\n\n");
        out.push_str(&r.body);
        out.push('\n');
        if let Some(performance) = &r.performance {
            match performance {
                Performance::Policy {
                    source_inventory_digest,
                    rationale,
                } => {
                    out.push_str(&format!("\nPerformance policy · source inventory `{source_inventory_digest}`\n\n{rationale}\n"));
                }
                Performance::Measurement {
                    source_inventory_digest,
                    evidence,
                } => {
                    out.push_str(&format!("\nPerformance measurement · source inventory `{source_inventory_digest}`\n\nEvidence: `{}` · `{}`\n\nResult: `{}`\n\nCommand: `{}`\n", evidence.path, evidence.digest, evidence.result_digest, serde_json::to_string(&evidence.command).map_err(|e| e.to_string())?));
                }
            }
        }
        if let Some(prior) = &r.revises {
            out.push_str(&format!("\nRevises [{}](#record:{}).\n", prior, prior));
        }
        if !r.supersedes.is_empty() {
            let mut ids = r.supersedes.clone();
            ids.sort();
            out.push_str(&format!(
                "\nSupersedes {}.\n",
                ids.iter()
                    .map(|id| format!("[{}](#record:{})", id, id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for sid in section_order(r)? {
            let section = r
                .sections
                .iter()
                .find(|s| s.id == sid)
                .expect("validated section");
            out.push_str(&format!(
                "\n<a id=\"section:{}:{}\"></a>\n\n#### {}\n\n{}\n",
                r.id, section.id, section.title, section.body
            ));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(id: &str) -> Record {
        serde_json::from_value(serde_json::json!({"schema_version":1,"id":id,"kind":"docs","title":id,"body":"original"})).unwrap()
    }
    fn map(records: Vec<Record>) -> BTreeMap<String, Record> {
        records.into_iter().map(|r| (r.id.clone(), r)).collect()
    }
    #[test]
    fn strict_contract() {
        assert!(serde_json::from_value::<Record>(serde_json::json!({"schema_version":1,"id":"a","kind":"docs","title":"a","body":"b","unknown":1})).is_err());
        let mut r = record("a");
        r.schema_version = 2;
        assert!(validate(&map(vec![r])).is_err());
    }
    #[test]
    fn deterministic_history_and_sections() {
        let a = record("a");
        let mut b = record("b");
        b.revises = Some("a".into());
        b.revision = 2;
        b.sections = vec![
            Section {
                id: "last".into(),
                title: "Last".into(),
                body: "end".into(),
                after: vec!["first".into()],
            },
            Section {
                id: "first".into(),
                title: "First".into(),
                body: "start".into(),
                after: vec![],
            },
        ];
        let text = render(&map(vec![b.clone(), a.clone()])).unwrap();
        assert_eq!(text, render(&map(vec![a, b])).unwrap());
        assert!(
            text.contains("record:a")
                && text.contains("original")
                && text.contains("superseded by [b]")
        );
        assert!(text.find("section:b:first").unwrap() < text.find("section:b:last").unwrap());
    }
    #[test]
    fn accepted_decisions_require_successors() {
        let mut a = record("a");
        a.kind = Kind::Adr;
        a.accepted = true;
        let mut b = record("b");
        b.kind = Kind::Adr;
        b.revises = Some("a".into());
        b.revision = 2;
        assert!(validate(&map(vec![a.clone(), b.clone()])).is_err());
        b.revises = None;
        b.revision = 1;
        b.supersedes = vec!["a".into()];
        assert!(validate(&map(vec![a, b])).is_ok());
    }
    #[test]
    fn competing_revisions_require_resolution() {
        let a = record("a");
        let mut b = record("b");
        b.revises = Some("a".into());
        b.revision = 2;
        let mut c = b.clone();
        c.id = "c".into();
        assert!(validate(&map(vec![a.clone(), b.clone(), c.clone()])).is_err());
        let mut d = record("d");
        d.supersedes = vec!["b".into(), "c".into()];
        assert!(validate(&map(vec![a, b, c, d])).is_ok());
    }
    #[test]
    fn cycles_and_unknown_sections_fail() {
        let mut a = record("a");
        let mut b = record("b");
        a.depends_on = vec!["b".into()];
        b.depends_on = vec!["a".into()];
        assert!(validate(&map(vec![a, b])).is_err());
        let mut a = record("a");
        a.sections = vec![Section {
            id: "x".into(),
            title: "x".into(),
            body: "x".into(),
            after: vec!["missing".into()],
        }];
        assert!(validate(&map(vec![a])).is_err());
    }
    #[test]
    fn dependencies_order_across_collections() {
        let mut a = record("a");
        a.kind = Kind::Changelog;
        a.depends_on = vec!["z".into()];
        let z = record("z");
        let rendered = render(&map(vec![a, z])).unwrap();
        assert!(
            rendered.find("id=\"record:z\"").unwrap() < rendered.find("id=\"record:a\"").unwrap()
        );
    }
    #[test]
    fn section_cycles_fail_and_pinned_policy_renders() {
        let mut a = record("a");
        a.sections = vec![
            Section {
                id: "x".into(),
                title: "x".into(),
                body: "x".into(),
                after: vec!["y".into()],
            },
            Section {
                id: "y".into(),
                title: "y".into(),
                body: "y".into(),
                after: vec!["x".into()],
            },
        ];
        assert!(validate(&map(vec![a])).is_err());
        let mut p = record("p");
        p.kind = Kind::Performance;
        let digest = format!("sha256:{}", "a".repeat(64));
        p.performance = Some(Performance::Policy {
            source_inventory_digest: digest.clone(),
            rationale: "No measurement required".into(),
        });
        assert!(render(&map(vec![p])).unwrap().contains(&digest));
    }
    #[test]
    fn immutable_baseline_and_measurement_validation() {
        let a = record("a");
        let state = State {
            records: map(vec![a.clone()]),
            ..State::default()
        };
        assert!(compose(&state, &map(vec![a.clone()])).is_err());
        let consumed_only = State {
            consumed: [("a".into(), format!("sha256:{}", "a".repeat(64)))].into(),
            ..State::default()
        };
        assert!(compose(&consumed_only, &map(vec![a.clone()])).is_err());
        let mut changed = a;
        changed.body = "changed".into();
        assert!(compose(&state, &map(vec![changed])).is_err());
        let mut p = record("p");
        p.kind = Kind::Performance;
        assert!(validate(&map(vec![p])).is_err());
    }
}
