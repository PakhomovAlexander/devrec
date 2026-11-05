#![forbid(unsafe_code)]
use devrec::{
    core::{self, Record},
    release,
    store::{Store, allocate},
};
use serde_json::{Value, json};
use std::path::PathBuf;
fn main() {
    match run() {
        Ok(v) => println!("{}", serde_json::to_string(&v).unwrap()),
        Err(e) => {
            eprintln!(
                "{}",
                json!({"schema":"devrec/error@1","code":"operation_refused","message":e})
            );
            std::process::exit(1)
        }
    }
}
fn run() -> Result<Value, String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut root = PathBuf::from(".");
    if args.first().is_some_and(|s| s == "--root") {
        if args.len() < 3 {
            return Err("--root requires DIR and command".into());
        }
        root = PathBuf::from(args.remove(1));
        args.remove(0);
    }
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        return Ok(
            json!({"schema":"devrec/help@1","usage":"devrec [--root DIR] new FILE | revise FILE | check | render [--nightly] | inventory | release prepare|request|review|finalize ID","new":"JSON record: id/name may be empty; allocates collision-resistant ID and readable name. revise requires revises predecessor ID."}),
        );
    }
    if args == ["--version"] {
        return Ok(json!({"version":env!("CARGO_PKG_VERSION")}));
    }
    let s = Store::open(&root)?;
    match args[0].as_str() {
        "new" | "revise" if args.len() == 2 => {
            let _lock = s.lock()?;
            release::ready(&s)?;
            let bytes = s
                .read(&args[1])?
                .ok_or("input file missing (must be repository relative)")?;
            let mut r: Record = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if args[0] == "new" && r.revises.is_some() {
                return Err("use revise with a predecessor".into());
            }
            if args[0] == "revise" && r.revises.is_none() {
                return Err("revise requires revises".into());
            }
            if r.id.is_empty() {
                r.id = format!("{}-{}", r.kind.as_str(), allocate()?)
            }
            release::id(&r.id)?;
            if r.name.is_empty() {
                let slug = r
                    .title
                    .to_ascii_lowercase()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                    .collect::<String>();
                r.name = format!(
                    "{}-{}",
                    slug.trim_matches('-').chars().take(48).collect::<String>(),
                    &r.id[r.id.len().saturating_sub(8)..]
                );
            }
            if r.provenance.is_some() {
                return Err("provenance is assigned only by native reconciliation".into());
            }
            release::id(&r.id)?;
            let b = release::state(&s)?;
            let (mut p, _) = release::pending(&s)?;
            if b.records.contains_key(&r.id)
                || b.consumed.contains_key(&r.id)
                || p.contains_key(&r.id)
            {
                return Err("record ID already exists".into());
            }
            p.insert(r.id.clone(), r.clone());
            core::compose(&b, &p)?;
            s.write(&format!(".devrec/fragments/{}.json", r.id), &r)?;
            Ok(json!({"schema":"devrec/created@1","record":r}))
        }
        "check" if args.len() == 1 => {
            let records = release::current(&s)?;
            core::validate_config(&release::config(&s)?)?;
            Ok(json!({"schema":"devrec/check@1","records":records.len()}))
        }
        "render" if args.len() == 1 || (args.len() == 2 && args[1] == "--nightly") => {
            let records = release::current(&s)?;
            Ok(json!({"schema":"devrec/render@1","text":core::render(&records)?}))
        }
        "inventory" if args.len() == 1 => {
            s.read_lock()?;
            release::ready(&s)?;
            let c = release::config(&s)?;
            let sources = release::inventory(&s, &c)?;
            let source_inventory_digest = devrec::store::digest(&devrec::store::json(&sources)?);
            Ok(
                json!({"schema":"devrec/inventory@1","sources":sources,"source_inventory_digest":source_inventory_digest}),
            )
        }
        "release" if args.len() == 3 => {
            let _lock = s.lock()?;
            let id = &args[2];
            match args[1].as_str() {
                "prepare" => {
                    serde_json::to_value(release::prepare(&s, id)?).map_err(|e| e.to_string())
                }
                "request" => Ok(release::plan(&s, id)?.request),
                "review" => release::review(&s, id),
                "finalize" => release::finalize(&s, id),
                _ => Err("unknown release operation".into()),
            }
        }
        _ => Err("invalid command; run devrec --help".into()),
    }
}
