use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(
        std::env::var_os("DEVREC_TEST_EXECUTABLE")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_devrec").into()),
    )
    .arg("--root")
    .arg(root)
    .args(args)
    .output()
    .unwrap()
}
fn ok(root: &Path, args: &[&str]) -> Value {
    let out = cli(root, args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn write(root: &Path, name: &str, v: &Value) {
    fs::write(root.join(name), serde_json::to_vec(v).unwrap()).unwrap()
}
fn input(id: &str, title: &str) -> Value {
    json!({"schema_version":1,"id":id,"kind":"changelog","title":title,"body":"Reason retained"})
}
#[test]
fn real_cli_allocation_commutes_and_nightly_is_read_only() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    for root in [a.path(), b.path()] {
        write(root, "one.json", &input("one", "One"));
        write(root, "two.json", &input("two", "Two"));
    }
    for n in ["one.json", "two.json"] {
        ok(a.path(), &["new", n]);
    }
    for n in ["two.json", "one.json"] {
        ok(b.path(), &["new", n]);
    }
    let rendered = ok(a.path(), &["render", "--nightly"]);
    assert_eq!(rendered, ok(b.path(), &["render", "--nightly"]));
    assert!(!a.path().join(".devrec/baseline.json").exists());
    assert!(a.path().join(".devrec/fragments/one.json").exists());
    write(a.path(), "allocate.json", &input("", "Readable Title"));
    let first = ok(a.path(), &["new", "allocate.json"]);
    let second = ok(a.path(), &["new", "allocate.json"]);
    assert_ne!(first["record"]["id"], second["record"]["id"]);
    assert!(
        first["record"]["name"]
            .as_str()
            .unwrap()
            .starts_with("readable-title-")
    );
}
#[test]
fn conflicts_and_stale_source_are_refused() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(r, "a.json", &input("a", "A"));
    ok(r, &["new", "a.json"]);
    let mut rev = input("r1", "R1");
    rev["revises"] = json!("a");
    rev["revision"] = json!(2);
    write(r, "r1.json", &rev);
    ok(r, &["revise", "r1.json"]);
    rev["id"] = json!("r2");
    write(r, "r2.json", &rev);
    assert!(!cli(r, &["revise", "r2.json"]).status.success());
    fs::write(r.join("source.rs"), "original").unwrap();
    write(
        r,
        ".devrec/config.json",
        &json!({"schema_version":1,"protected_sources":["source.rs"]}),
    );
    let p = ok(r, &["release", "prepare", "v1"]);
    assert_eq!(p["inventory"]["source.rs"]["size"], 8);
    fs::write(r.join("source.rs"), "changed").unwrap();
    assert!(!cli(r, &["release", "prepare", "v1"]).status.success());
    assert!(!cli(r, &["release", "review", "v1"]).status.success());
    assert!(r.join(".devrec/fragments/a.json").exists());
}
#[test]
fn traversal_unknown_fields_and_symlinks_refused() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(r, "input.json", &input("safe", "Safe"));
    assert!(!cli(r, &["new", "../input.json"]).status.success());
    let mut v = input("bad", "Bad");
    v["typo"] = json!(true);
    write(r, "bad.json", &v);
    assert!(!cli(r, &["new", "bad.json"]).status.success());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(r.join("input.json"), r.join("link.json")).unwrap();
        assert!(!cli(r, &["new", "link.json"]).status.success());
    }
}
#[test]
fn accepted_adr_sections_and_typed_perf() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let adr = json!({"schema_version":1,"id":"decision","kind":"adr","title":"Decision","body":"Rationale","accepted":true});
    write(r, "adr.json", &adr);
    ok(r, &["new", "adr.json"]);
    let mut rev = adr.clone();
    rev["id"] = json!("rewrite");
    rev["revises"] = json!("decision");
    rev["revision"] = json!(2);
    write(r, "rev.json", &rev);
    assert!(!cli(r, &["revise", "rev.json"]).status.success());
    let docs = json!({"schema_version":1,"id":"guide","kind":"docs","title":"Guide","body":"Intro","sections":[{"id":"later","title":"Later","body":"B","after":["first"]},{"id":"first","title":"First","body":"A"}]});
    write(r, "docs.json", &docs);
    ok(r, &["new", "docs.json"]);
    let text = ok(r, &["render"])["text"].as_str().unwrap().to_string();
    assert!(text.find("First").unwrap() < text.find("Later").unwrap());
    assert!(text.contains("Rationale"));
    let perf = json!({"schema_version":1,"id":"perf","kind":"performance","title":"Policy","body":"Prose is not a measurement"});
    write(r, "perf.json", &perf);
    assert!(!cli(r, &["new", "perf.json"]).status.success());
}
