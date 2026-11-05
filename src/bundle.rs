//! Stage reviewed native packages; never edit catalog authority or machine accounts.
use crate::{
    af::run_process,
    store::{Store, digest},
};
use serde_json::{Value, json};
use std::os::unix::fs::DirBuilderExt;
use std::{
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

const FILES: &[(&str, &str)] = &[
    (
        "kind/kind.toml",
        include_str!("../integration/reconciliation/kind/kind.toml"),
    ),
    (
        "kind/structured.json",
        include_str!("../integration/reconciliation/kind/structured.json"),
    ),
    (
        "worker/worker.toml",
        include_str!("../integration/reconciliation/worker/worker.toml"),
    ),
    (
        "worker/instructions.md",
        include_str!("../integration/reconciliation/worker/instructions.md"),
    ),
    (
        "worker/input.schema.json",
        include_str!("../integration/reconciliation/worker/input.schema.json"),
    ),
    (
        "worker/outputs/reconciliation.schema.json",
        include_str!("../integration/reconciliation/worker/outputs/reconciliation.schema.json"),
    ),
    (
        "pipeline/pipeline.toml",
        include_str!("../integration/reconciliation/pipeline/pipeline.toml"),
    ),
];

/// Build into one absent child of an existing directory. On error remove only our new directory.
/// The resulting catalog is a reviewable snippet, not installed authority.
pub fn build(root: &Path, executable: &Path, destination: &str) -> Result<Value, String> {
    crate::release::id(destination)?;
    Store::open(root)?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let executable = fs::canonicalize(executable).map_err(|e| e.to_string())?;
    let executable_digest = crate::af::hash_file(&executable)?;
    let output = root.join(destination);
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&output)
        .map_err(|e| format!("bundle destination must be absent: {e}"))?;
    let result = (|| {
        let store = Store::open(&output)?;
        for (path, bytes) in FILES {
            store.write_bytes(path, bytes.as_bytes())?;
        }
        let scratch = crate::af::Scratch::new(&output)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut catalog = String::from(
            "[kinds]\ndevrec-reconciliation = \"devrec/reconcile-kind\"\n\n[providers]\n\"devrec/reconcile-worker\" = \"codex-personal\"\n",
        );
        let mut pins = serde_json::Map::new();
        for (index, (directory, name)) in [
            ("kind", "devrec/reconcile-kind"),
            ("worker", "devrec/reconcile-worker"),
            ("pipeline", "devrec/reconcile"),
        ]
        .iter()
        .enumerate()
        {
            if crate::af::hash_file(&executable)? != executable_digest {
                return Err("AF executable changed while building package".into());
            }
            let mut command = Command::new(&executable);
            command
                .args(["catalog", "digest"])
                .arg(output.join(directory));
            let bytes = run_process(command, &scratch.0, index as u32, deadline)?;
            let pin = std::str::from_utf8(&bytes)
                .map_err(|e| e.to_string())?
                .trim();
            crate::af::require_digest(pin)?;
            catalog.push_str(&format!("\n[packages.\"{name}\"]\nversion = \"1.0.0\"\ndigest = \"{pin}\"\npath = \"{destination}/{directory}\"\n"));
            pins.insert((*name).into(), json!(pin));
        }
        store.write_bytes("catalog-snippet.toml", catalog.as_bytes())?;
        store.write_bytes("bindings.toml", b"schema = \"af.task-bindings/1\"\n")?;
        let receipt = json!({"schema":"devrec/native-bundle@1","executable":executable,"executable_digest":executable_digest,"packages":pins,"catalog_digest":digest(catalog.as_bytes())});
        store.write("bundle.json", &receipt)?;
        Ok(receipt)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&output);
    }
    result
}
