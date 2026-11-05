//! Immutable plans and a roll-forward journal; no externally visible state is read mid-transaction.
use crate::{
    core::{self, Config, Record, State},
    store::{Store, digest, json},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json as value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub size: u64,
    pub digest: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: String,
    pub release_id: String,
    pub baseline_digest: String,
    pub config_digest: String,
    pub af_config_digest: String,
    pub inventory: BTreeMap<String, Source>,
    pub included: BTreeMap<String, String>,
    pub records: BTreeMap<String, Record>,
    pub request: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transaction {
    schema: String,
    release_id: String,
    plan_digest: String,
    baseline: State,
    rendered: String,
    included: BTreeMap<String, String>,
}
pub fn id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 100
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        Err("invalid identifier".into())
    } else {
        Ok(())
    }
}
pub fn config(s: &Store) -> Result<Config, String> {
    let c = s.load::<Config>(".devrec/config.json")?.unwrap_or_default();
    core::validate_config(&c)?;
    Ok(c)
}
pub fn state(s: &Store) -> Result<State, String> {
    let b = s
        .load::<State>(".devrec/baseline.json")?
        .unwrap_or_default();
    if b.schema_version != 1 {
        return Err("unsupported baseline version".into());
    }
    for (id, r) in &b.records {
        if id != &r.id {
            return Err("baseline key mismatch".into());
        }
    }
    core::validate(&b.records)?;
    Ok(b)
}
pub fn ready(s: &Store) -> Result<(), String> {
    if s.read(".devrec/transaction.json")?.is_some() {
        Err("unfinished release; rerun release finalize for the journal release ID".into())
    } else {
        Ok(())
    }
}
pub fn pending(s: &Store) -> Result<(BTreeMap<String, Record>, BTreeMap<String, String>), String> {
    let mut records = BTreeMap::new();
    let mut hashes = BTreeMap::new();
    let mut total = 0usize;
    for name in s.list(".devrec/fragments")? {
        if !name.ends_with(".json") {
            return Err(format!("unexpected pending entry {name}"));
        }
        let bytes = s
            .read(&format!(".devrec/fragments/{name}"))?
            .ok_or("pending file vanished")?;
        total += bytes.len();
        if total > 32 * 1024 * 1024 {
            return Err("pending byte limit".into());
        }
        let r: Record = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        id(&r.id)?;
        if name != format!("{}.json", r.id) {
            return Err("pending filename does not match ID".into());
        }
        hashes.insert(r.id.clone(), digest(&bytes));
        if records.insert(r.id.clone(), r).is_some() {
            return Err("duplicate ID".into());
        }
    }
    Ok((records, hashes))
}
pub fn inventory(s: &Store, c: &Config) -> Result<BTreeMap<String, Source>, String> {
    let mut inv = BTreeMap::new();
    let mut total = 0u64;
    for path in &c.protected_sources {
        if path.starts_with(".devrec/") {
            return Err("protected source cannot be devrec state".into());
        }
        let bytes = s
            .read(path)?
            .ok_or_else(|| format!("missing protected source {path}"))?;
        total += bytes.len() as u64;
        if total > 64 * 1024 * 1024 {
            return Err("source byte limit".into());
        }
        if inv
            .insert(
                path.clone(),
                Source {
                    size: bytes.len() as u64,
                    digest: digest(&bytes),
                },
            )
            .is_some()
        {
            return Err("duplicate protected source".into());
        }
    }
    Ok(inv)
}
fn raw_digest(s: &Store, path: &str) -> Result<String, String> {
    Ok(digest(&s.read(path)?.unwrap_or_default()))
}
pub fn current(s: &Store) -> Result<BTreeMap<String, Record>, String> {
    ready(s)?;
    let b = state(s)?;
    let (p, _) = pending(s)?;
    core::compose(&b, &p)
}
fn closure(
    records: &BTreeMap<String, Record>,
    roots: impl Iterator<Item = String>,
) -> BTreeMap<String, Record> {
    let mut visit: Vec<_> = roots.collect();
    let mut seen = BTreeSet::new();
    while let Some(id) = visit.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(r) = records.get(&id) {
            visit.extend(r.depends_on.iter().cloned());
            visit.extend(r.conflicts_with.iter().cloned());
            visit.extend(r.supersedes.iter().cloned());
            visit.extend(r.revises.iter().cloned());
            for (other, succ) in records {
                if succ.revises.as_ref() == Some(&id) || succ.supersedes.contains(&id) {
                    visit.push(other.clone());
                }
            }
        }
    }
    seen.into_iter()
        .filter_map(|id| records.get(&id).map(|r| (id, r.clone())))
        .collect()
}
pub fn prepare(s: &Store, release: &str) -> Result<Plan, String> {
    id(release)?;
    ready(s)?;
    let path = format!(".devrec/releases/{release}/plan.json");
    if let Some(p) = s.load::<Plan>(&path)? {
        fresh(s, &p)?;
        return Ok(p);
    }
    let base = state(s)?;
    let (pending, included) = pending(s)?;
    let all = core::compose(&base, &pending)?;
    let cfg = config(s)?;
    let inv = inventory(s, &cfg)?;
    validate_perf(s, &pending, &inv)?;
    let affected = closure(&all, pending.keys().cloned());
    let prior = base
        .records
        .iter()
        .filter(|(id, _)| affected.contains_key(*id))
        .map(|(id, r)| (id.clone(), r.clone()))
        .collect::<BTreeMap<_, _>>();
    let baseline_digest = raw_digest(s, ".devrec/baseline.json")?;
    let config_digest = raw_digest(s, ".devrec/config.json")?;
    let af_config_digest = raw_digest(s, ".devrec/af.json")?;
    let binding = value!({"release_id":release,"baseline_digest":baseline_digest,"config_digest":config_digest,"af_config_digest":af_config_digest,"inventory":inv,"included":included});
    let request = value!({"schema":"devrec/reconciliation-request@1","release_id":release,"stage_digest":digest(&json(&binding)?),"baseline":prior,"records":pending,"before":core::render(&prior)?,"after":core::render(&affected)?,"inventory":inv});
    let plan = Plan {
        schema: "devrec/release-plan@1".into(),
        release_id: release.into(),
        baseline_digest,
        config_digest,
        af_config_digest,
        inventory: inv,
        included,
        records: pending,
        request,
    };
    s.write(&path, &plan)?;
    Ok(plan)
}
pub fn plan(s: &Store, release: &str) -> Result<Plan, String> {
    id(release)?;
    let p = s
        .load::<Plan>(&format!(".devrec/releases/{release}/plan.json"))?
        .ok_or("release is not prepared")?;
    if p.schema != "devrec/release-plan@1" || p.release_id != release {
        return Err("invalid release plan".into());
    }
    Ok(p)
}
fn fresh(s: &Store, p: &Plan) -> Result<(), String> {
    if raw_digest(s, ".devrec/baseline.json")? != p.baseline_digest
        || raw_digest(s, ".devrec/config.json")? != p.config_digest
        || raw_digest(s, ".devrec/af.json")? != p.af_config_digest
        || inventory(s, &config(s)?)? != p.inventory
    {
        return Err("release source, config or baseline is stale".into());
    }
    for (id, hash) in &p.included {
        let bytes = s
            .read(&format!(".devrec/fragments/{id}.json"))?
            .ok_or("included record disappeared")?;
        if digest(&bytes) != *hash {
            return Err(format!("included record changed: {id}"));
        }
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MeasurementEvidence {
    schema: String,
    source_inventory_digest: String,
    command: Vec<String>,
    result: Value,
}
fn validate_perf(
    s: &Store,
    records: &BTreeMap<String, Record>,
    inv: &BTreeMap<String, Source>,
) -> Result<(), String> {
    let expected = digest(&json(inv)?);
    for r in records.values() {
        if let Some(p) = &r.performance {
            let v = serde_json::to_value(p).map_err(|e| e.to_string())?;
            if v["source_inventory_digest"].as_str() != Some(&expected) {
                return Err(format!("performance {} source inventory mismatch", r.id));
            }
            if v["type"] == "measurement" {
                let evidence = &v["evidence"];
                let path = evidence["path"]
                    .as_str()
                    .ok_or("measurement evidence path missing")?;
                let bytes = s.read(path)?.ok_or("measurement evidence missing")?;
                if evidence["digest"].as_str() != Some(&digest(&bytes)) {
                    return Err("measurement evidence digest mismatch".into());
                }
                let captured: MeasurementEvidence = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("measurement evidence: {e}"))?;
                if captured.schema != "devrec/measurement-evidence@1"
                    || captured.source_inventory_digest != expected
                    || serde_json::to_value(&captured.command).map_err(|e| e.to_string())?
                        != evidence["command"]
                    || evidence["result_digest"].as_str() != Some(&digest(&json(&captured.result)?))
                {
                    return Err(
                        "measurement evidence does not bind exact source, command and result"
                            .into(),
                    );
                }
            }
        }
    }
    Ok(())
}
fn native(s: &Store, request: &Value) -> Result<Value, String> {
    let c = s
        .load::<crate::af::IntegrationConfig>(".devrec/af.json")?
        .ok_or("native AF integration not installed: .devrec/af.json")?;
    crate::af::reconcile(&c, request)
}
pub fn review(s: &Store, release: &str) -> Result<Value, String> {
    ready(s)?;
    let p = plan(s, release)?;
    fresh(s, &p)?;
    let result = native(s, &p.request)?;
    fresh(s, &p)?;
    reconciled(s, &p, &result)?;
    s.write(&format!(".devrec/releases/{release}/review.json"), &result)?;
    Ok(result)
}
fn reconciled(s: &Store, p: &Plan, receipt: &Value) -> Result<State, String> {
    if receipt["request_digest"].as_str() != Some(&digest(&json(&p.request)?)) {
        return Err("reconciliation request mismatch".into());
    }
    let adds = receipt["additions"]
        .as_array()
        .ok_or("missing reconciliation additions")?;
    let mut b = state(s)?;
    let mut proposed = p.records.clone();
    let mut seen = BTreeSet::new();
    for v in adds {
        let mut r: Record = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
        id(&r.id)?;
        if !seen.insert(r.id.clone())
            || b.records.contains_key(&r.id)
            || proposed.contains_key(&r.id)
            || b.consumed.contains_key(&r.id)
        {
            return Err("reconciliation duplicates an existing ID".into());
        }
        // Model changes are explicit successors within the affected closure, never a rewrite of accepted state.
        if r.revises
            .iter()
            .chain(r.supersedes.iter())
            .chain(r.depends_on.iter())
            .any(|id| {
                p.request["baseline"].get(id).is_none()
                    && !p.records.contains_key(id)
                    && !proposed.contains_key(id)
            })
        {
            return Err("reconciliation reference outside affected scope".into());
        }
        r.provenance=Some(serde_json::from_value(value!({"task_id":receipt["task_id"],"revision_id":receipt["revision_id"],"result_digest":receipt["result_digest"],"request_digest":receipt["request_digest"]})).map_err(|e|e.to_string())?);
        if r.performance.is_some() {
            return Err("model cannot manufacture performance authority".into());
        }
        proposed.insert(r.id.clone(), r);
    }
    b.records = core::compose(&b, &proposed)?;
    for (id, hash) in &p.included {
        b.consumed.insert(id.clone(), hash.clone());
    }
    validate_perf(s, &p.records, &p.inventory)?;
    Ok(b)
}
pub fn finalize(s: &Store, release: &str) -> Result<Value, String> {
    id(release)?;
    let done = format!(".devrec/releases/{release}/done.json");
    if let Some(v) = s.load::<Value>(&done)? {
        if let Some(tx) = s.load::<Transaction>(".devrec/transaction.json")? {
            if tx.release_id != release {
                return Err("different unfinished transaction".into());
            }
            if v["baseline_digest"].as_str() != Some(&digest(&json(&tx.baseline)?)) {
                return Err("completion journal mismatch".into());
            }
            s.remove(".devrec/transaction.json")?;
        }
        return Ok(v);
    }
    let p = plan(s, release)?;
    let tx = if let Some(tx) = s.load::<Transaction>(".devrec/transaction.json")? {
        if tx.schema != "devrec/transaction@1"
            || tx.release_id != release
            || tx.plan_digest != digest(&json(&p)?)
        {
            return Err("different or corrupt unfinished transaction".into());
        }
        recover_fresh(s, &p, &tx)?;
        tx
    } else {
        fresh(s, &p)?;
        let receipt = s
            .load::<Value>(&format!(".devrec/releases/{release}/review.json"))?
            .ok_or("release is not reviewed")?;
        let c = s
            .load::<crate::af::IntegrationConfig>(".devrec/af.json")?
            .ok_or("native AF integration not installed")?;
        crate::af::verify(&c, &p.request, &receipt)?;
        fresh(s, &p)?;
        let baseline = reconciled(s, &p, &receipt)?;
        // Late additions remain pending, but must still compose with the reconciled baseline.
        let (mut late, _) = pending(s)?;
        for id in p.included.keys() {
            late.remove(id);
        }
        core::compose(&baseline, &late)?;
        let rendered = core::render(&baseline.records)?;
        let tx = Transaction {
            schema: "devrec/transaction@1".into(),
            release_id: release.into(),
            plan_digest: digest(&json(&p)?),
            baseline,
            rendered,
            included: p.included.clone(),
        };
        s.write(".devrec/transaction.json", &tx)?;
        tx
    };
    s.write(".devrec/baseline.json", &tx.baseline)?;
    failpoint("baseline")?;
    s.write_bytes(".devrec/rendered.md", tx.rendered.as_bytes())?;
    failpoint("rendered")?;
    for id in tx.included.keys() {
        s.remove(&format!(".devrec/fragments/{id}.json"))?;
        failpoint("consume")?;
    }
    let done_value = value!({"schema":"devrec/release-complete@1","release_id":release,"baseline_digest":digest(&json(&tx.baseline)?),"rendered_digest":digest(tx.rendered.as_bytes())});
    s.write(&done, &done_value)?;
    failpoint("done")?;
    s.remove(".devrec/transaction.json")?;
    Ok(done_value)
}
fn recover_fresh(s: &Store, p: &Plan, tx: &Transaction) -> Result<(), String> {
    if raw_digest(s, ".devrec/config.json")? != p.config_digest
        || raw_digest(s, ".devrec/af.json")? != p.af_config_digest
        || inventory(s, &config(s)?)? != p.inventory
    {
        return Err("recovery source/config is stale".into());
    }
    let bd = raw_digest(s, ".devrec/baseline.json")?;
    if bd != p.baseline_digest && bd != digest(&json(&tx.baseline)?) {
        return Err("recovery baseline changed".into());
    }
    validate_perf(s, &p.records, &p.inventory)?;
    core::validate(&tx.baseline.records)?;
    if core::render(&tx.baseline.records)? != tx.rendered {
        return Err("recovery output checksum mismatch".into());
    }
    for (id, hash) in &tx.included {
        if let Some(bytes) = s.read(&format!(".devrec/fragments/{id}.json"))? {
            if digest(&bytes) != *hash {
                return Err(format!("recovery consumed record changed: {id}"));
            }
        }
    }
    Ok(())
}
fn failpoint(point: &str) -> Result<(), String> {
    if std::env::var("DEVREC_TEST_FAILPOINT").ok().as_deref() == Some(point) {
        return Err(format!("injected interruption after {point}"));
    }
    Ok(())
}
