//! Contract tests use a local subprocess transport, never a real AF authority or publication.
#![cfg(unix)]
use devrec::store::{digest, json as canonical};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const TRANSPORT: &str = r#"#!/usr/bin/env python3
import json, pathlib, sys
args = sys.argv[1:]
def arg(name): return args[args.index(name)+1]
state = pathlib.Path(arg('--state'))
state.mkdir(exist_ok=True)
with (state/'calls.jsonl').open('a') as f: f.write(json.dumps(args)+'\n')
control = json.loads((state/'control.json').read_text())
op = args[1]
path = state/'task.json'
def h(c): return 'sha256:'+c*64
if op == 'plan':
    task = json.loads(pathlib.Path(arg('--file')).read_text())
    captured = json.loads((pathlib.Path(arg('--file')).parent / arg('--input-json').split('=',1)[1]).read_text())
    task.update(captured)
    task['authority'] = arg('--authority')
    task['finished'] = False
    cas = state/'cas'/'objects'/('f'*2)/('f'*62)
    cas.parent.mkdir(parents=True,exist_ok=True)
    cas.write_text(json.dumps(dict(artifact_id=h('f'),type='custom/DevrecReconciliationRequest@1',payload=captured)))
    path.write_text(json.dumps(task))
    print('{}'); sys.exit(0)
if not path.exists(): sys.exit(3)
task = json.loads(path.read_text())
id = task['task_id']
# This fixture retains the newest task only; a different requested ID is absent.
if args[2] != id: sys.exit(3)
rev, planid = h('a'), h('b')
if op == 'explain':
    execution = dict(kind='model', provider='codex-personal', provider_kind='codex', model='gpt-6.1-sol', effort='low')
    execution.update(control.get('execution', {}))
    plan = dict(limits=task['limits'], task_revision_id=rev, bindings={'reconcile':{'execution':execution}}, authority={'allowed_effects':['read-source'], 'policy_id':h('1')}, inputs={'request':{'artifact_type':'custom/DevrecReconciliationRequest@1','cardinality':'one','artifact_ids':[h('f')]}})
    result = dict(schema='af/task-inspection@16', task_id=id, revision_id=rev, plan_id=planid, plan=plan, graph={'max_parallel':1,'slots':{'reconcile':{'max_attempts':1}},'nodes':{'input':{'operator':{'kind':'root_inputs'}},'admit':{'operator':{'kind':'provider_admission'}},'worker':{'operator':{'kind':'primitive','operator':{'op':'worker'}}},'accept':{'operator':{'kind':'primitive','operator':{'op':'structured_accept'}}}},'allowances':{'worker':{'max_attempts':1,'verification_attempts':0},'admit':{'max_attempts':1,'verification_attempts':0}}})
    for pointer, value in control.get('explain_edits', {}).items():
        keys=pointer.split('/'); current=result
        for key in keys[:-1]: current=current[key]
        current[keys[-1]]=value
elif op == 'run':
    assert arg('--confirm-plan') == planid
    task['finished'] = True
    path.write_text(json.dumps(task)); result={}
elif op == 'show':
    shown_rev = control.get('shown_revision', rev)
    result = dict(schema='af/task-inspection@16', task_id=id, revision_id=shown_rev, plan_id=control.get('shown_plan',planid), phase={'kind':'running'})
    if task['finished']:
        result['phase'] = {'kind':'finished', 'result_id':h('c')}
        result['result'] = dict(task_revision_id=control.get('result_revision',shown_rev), execution='completed', acceptance='satisfied', domain_conclusion='structurally_validated', evidence=control.get('evidence',[h('d')]), missing_obligations=[], outputs={'additions':{'artifact_type':'custom/DevrecReconciliationAdditions@1', 'artifact_ids':[h('e')]}})
elif op == 'output':
    payload = dict(source_artifact_ids=[h('f')], schema='devrec/reconciliation-additions@1', request_digest=control.get('request_digest',task['request_digest']), additions=control.get('additions',[]), contradictions=control.get('contradictions',[]))
    result=dict(artifact_id=control.get('artifact_id',h('e')), type='custom/DevrecReconciliationAdditions@1', payload=payload)
else: raise RuntimeError(op)
print(json.dumps(result))
"#;

struct Fixture {
    _owner: tempfile::TempDir,
    root: PathBuf,
    state: PathBuf,
    scratch: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let owner = tempfile::tempdir().unwrap();
        // Store intentionally refuses symlink path components, including macOS /var and /tmp.
        let base = fs::canonicalize(owner.path()).unwrap();
        let root = base.join("records");
        let authority = base.join("authority");
        let state = base.join("native-state");
        let scratch = base.join("scratch");
        for path in [&root, &authority, &state, &scratch] {
            fs::create_dir(path).unwrap();
        }
        fs::create_dir(root.join(".devrec")).unwrap();
        let executable = base.join("fake-native-af");
        fs::write(&executable, TRANSPORT).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let bindings = base.join("bindings.json");
        fs::write(&bindings, "{}").unwrap();
        write(
            &root,
            ".devrec/af.json",
            &json!({
                "schema":"devrec/af-integration@1", "executable":executable,
                "executable_digest":digest(TRANSPORT.as_bytes()), "repository":authority,
                "authority":"1111111111111111111111111111111111111111", "authority_policy_id":hash('1'),
                "pipeline":"reconciliation", "task_kind":"devrec-reconciliation", "bindings":bindings,
                "state":state, "scratch":scratch, "request_port":"request", "result_port":"additions", "timeout_secs":10
            }),
        );
        write(&state, "control.json", &json!({}));
        Self {
            _owner: owner,
            root,
            state,
            scratch,
        }
    }
    fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_devrec"));
        cmd.arg("--root").arg(&self.root).args(args);
        cmd
    }
    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn refused(&self, args: &[&str], message: &str) {
        let out = self.run(args);
        assert!(!out.status.success(), "unexpected success: {:?}", args);
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!self.root.join(".devrec/baseline.json").exists());
    }
    fn add(&self, id: &str) -> Value {
        write(&self.root, "input.json", &record(id));
        self.ok(&["new", "input.json"])["record"].clone()
    }
    fn prepared(&self) {
        self.add("original");
        self.ok(&["release", "prepare", "v1"]);
    }
    fn reviewed(&self) -> Value {
        self.prepared();
        self.ok(&["release", "review", "v1"])
    }
    fn control(&self, value: Value) {
        write(&self.state, "control.json", &value);
    }
    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.state.join("calls.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    fn baseline(&self) -> Value {
        read(&self.root, ".devrec/baseline.json")
    }
}
fn record(id: &str) -> Value {
    json!({"schema_version":1,"id":id,"kind":"changelog","title":format!("Title {id}"),"body":"Retained rationale"})
}
fn write(root: &Path, path: &str, v: &Value) {
    fs::write(root.join(path), serde_json::to_vec(v).unwrap()).unwrap();
}
fn read(root: &Path, path: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(path)).unwrap()).unwrap()
}
fn hash(c: char) -> String {
    format!("sha256:{}", c.to_string().repeat(64))
}

#[test]
fn native_review_allocates_runs_and_preserves_explicit_additions_and_provenance() {
    let f = Fixture::new();
    f.prepared();
    let original = read(&f.root, ".devrec/fragments/original.json");
    let mut addition = record("model-note");
    addition["depends_on"] = json!(["original"]);
    f.control(json!({"additions":[addition]}));
    let receipt = f.ok(&["release", "review", "v1"]);
    assert_eq!(receipt["additions"][0]["id"], "model-note");
    assert_eq!(receipt["revision_id"], hash('a'));
    assert_eq!(f.ok(&["release", "review", "v1"]), receipt);
    let calls = f.calls();
    assert_eq!(calls.iter().filter(|v| v[1] == "plan").count(), 1);
    assert_eq!(calls.iter().filter(|v| v[1] == "run").count(), 1);
    let task = read(&f.state, "task.json");
    assert_eq!(task["schema"], "af.task-file/1");
    assert_eq!(task["strategy"], "light");
    assert_eq!(task["limits"]["max_attempts"], 2);
    assert_eq!(task["limits"]["verification"]["attempts"], 0);
    assert_eq!(task["pipeline"]["fallback"], "refuse");
    assert_eq!(
        task["request"],
        read(&f.root, ".devrec/releases/v1/plan.json")["request"]
    );
    f.ok(&["release", "finalize", "v1"]);
    let baseline = f.baseline();
    assert_eq!(baseline["records"]["original"], original);
    let provenance = &baseline["records"]["model-note"]["provenance"];
    for key in ["task_id", "revision_id", "result_digest", "request_digest"] {
        assert_eq!(provenance[key], receipt[key]);
    }
    assert!(fs::read_dir(&f.scratch).unwrap().next().is_none());
    // A finished durable task is reused without another allocation or execution.
    assert_eq!(f.calls().iter().filter(|v| v[1] == "run").count(), 1);
}

#[test]
fn finalize_rolls_forward_every_failpoint_is_idempotent_and_keeps_late_records() {
    for point in ["baseline", "rendered", "consume", "done"] {
        let f = Fixture::new();
        f.reviewed();
        let late = f.add("late");
        let out = f
            .cmd(&["release", "finalize", "v1"])
            .env("DEVREC_TEST_FAILPOINT", point)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{point}");
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains(&format!("injected interruption after {point}"))
        );
        assert!(f.root.join(".devrec/transaction.json").exists());
        f.refused_after_journal();
        let done = f.ok(&["release", "finalize", "v1"]);
        let baseline_bytes = fs::read(f.root.join(".devrec/baseline.json")).unwrap();
        let rendered = fs::read(f.root.join(".devrec/rendered.md")).unwrap();
        assert_eq!(f.ok(&["release", "finalize", "v1"]), done);
        assert_eq!(
            fs::read(f.root.join(".devrec/baseline.json")).unwrap(),
            baseline_bytes
        );
        assert_eq!(
            fs::read(f.root.join(".devrec/rendered.md")).unwrap(),
            rendered
        );
        assert_eq!(read(&f.root, ".devrec/fragments/late.json"), late);
        assert!(!f.root.join(".devrec/fragments/original.json").exists());
        assert!(!f.root.join(".devrec/transaction.json").exists());
        assert!(f.baseline()["records"].get("late").is_none());
        assert_eq!(f.ok(&["check"])["records"], 2);
    }
}
impl Fixture {
    fn refused_after_journal(&self) {
        let out = self.run(&["release", "prepare", "v2"]);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("unfinished release"));
    }
}

#[test]
fn recovery_rejects_changed_consumed_record() {
    let f = Fixture::new();
    f.reviewed();
    let out = f
        .cmd(&["release", "finalize", "v1"])
        .env("DEVREC_TEST_FAILPOINT", "consume")
        .output()
        .unwrap();
    assert!(!out.status.success());
    write(
        &f.root,
        ".devrec/fragments/original.json",
        &record("original"),
    );
    let out = f.run(&["release", "finalize", "v1"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("recovery consumed record changed"));
    assert!(f.root.join(".devrec/transaction.json").exists());
}

#[test]
fn forged_stored_receipt_and_changed_native_evidence_cannot_finalize() {
    for native_change in [false, true] {
        let f = Fixture::new();
        let mut receipt = f.reviewed();
        if native_change {
            f.control(json!({"additions":[record("unexpected")]}));
        } else {
            receipt["additions"] = json!([record("forged")]);
            write(&f.root, ".devrec/releases/v1/review.json", &receipt);
        }
        f.refused(
            &["release", "finalize", "v1"],
            "differs from exact native AF evidence",
        );
        assert!(f.root.join(".devrec/fragments/original.json").exists());
    }
}

#[test]
fn invalid_model_policy_and_native_result_contracts_are_refused() {
    let cases = vec![
        (
            json!({"execution":{"model":"gpt-6-astra"}}),
            "model must be gpt-6.1-sol",
        ),
        (json!({"execution":{"effort":"high"}}), "effort must be low"),
        (
            json!({"execution":{"provider":"other"}}),
            "provider must be codex-personal",
        ),
        (
            json!({"explain_edits":{"plan/task_revision_id":hash('f')}}),
            "plan revision mismatch",
        ),
        (
            json!({"explain_edits":{"plan/authority/allowed_effects":["write-source"]}}),
            "beyond read-source",
        ),
        (
            json!({"explain_edits":{"graph/slots/reconcile/max_attempts":2}}),
            "single-pass",
        ),
        (
            json!({"shown_revision":hash('f')}),
            "revision/plan changed after validation",
        ),
        (
            json!({"shown_plan":hash('f')}),
            "revision/plan changed after validation",
        ),
        (
            json!({"explain_edits":{"plan/authority/policy_id":hash('f')}}),
            "authority differs from installed policy pin",
        ),
        (
            json!({"explain_edits":{"plan/inputs/source":{"artifact_type":"source"}}}),
            "unexpected ambient input",
        ),
        (
            json!({"explain_edits":{"plan/inputs/request/artifact_type":"custom/Wrong@1"}}),
            "input contract mismatch",
        ),
        (
            json!({"result_revision":hash('f')}),
            "structural validation",
        ),
        (json!({"evidence":[]}), "evidence incomplete"),
        (
            json!({"artifact_id":hash('f')}),
            "artifact identity mismatch",
        ),
        (json!({"request_digest":hash('f')}), "request mismatch"),
        (
            json!({"contradictions":["substantive contradiction"]}),
            "substantive contradictions",
        ),
    ];
    for (control, message) in cases {
        let f = Fixture::new();
        f.prepared();
        f.control(control);
        f.refused(&["release", "review", "v1"], message);
        assert!(!f.root.join(".devrec/releases/v1/review.json").exists());
        assert!(fs::read_dir(&f.scratch).unwrap().next().is_none());
    }
}

#[test]
fn additions_cannot_duplicate_rewrite_or_manufacture_performance_authority() {
    let mut perf = record("model-perf");
    perf["kind"] = json!("performance");
    perf["performance"] =
        json!({"type":"policy","source_inventory_digest":hash('a'),"rationale":"Model claim"});
    let mut outside = record("outside");
    outside["depends_on"] = json!(["unrelated"]);
    for (additions, message) in [
        (json!([record("original")]), "duplicates an existing ID"),
        (
            json!([record("duplicate"), record("duplicate")]),
            "duplicates an existing ID",
        ),
        (json!([perf]), "cannot manufacture performance authority"),
        (json!([outside]), "reference outside affected scope"),
    ] {
        let f = Fixture::new();
        f.prepared();
        f.control(json!({"additions":additions}));
        f.refused(&["release", "review", "v1"], message);
    }
}

#[test]
fn measurement_binds_inventory_command_result_and_remains_fresh_at_finalize() {
    for change in ["none", "source", "evidence", "command", "result"] {
        let f = Fixture::new();
        fs::write(f.root.join("source.rs"), "captured source").unwrap();
        write(
            &f.root,
            ".devrec/config.json",
            &json!({"schema_version":1,"protected_sources":["source.rs"]}),
        );
        let captured_inventory = f.ok(&["inventory"]);
        let inventory: std::collections::BTreeMap<String, devrec::release::Source> =
            serde_json::from_value(captured_inventory["sources"].clone()).unwrap();
        let inventory_digest = captured_inventory["source_inventory_digest"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(inventory_digest, digest(&canonical(&inventory).unwrap()));
        let evidence = json!({"schema":"devrec/measurement-evidence@1","source_inventory_digest":inventory_digest,"command":["bench","--json"],"result":{"elapsed":12}});
        write(&f.root, "measurement.json", &evidence);
        let mut measured = record("measured");
        measured["kind"] = json!("performance");
        measured["performance"] = json!({"type":"measurement","source_inventory_digest":inventory_digest,"evidence":{"path":"measurement.json","digest":digest(&canonical(&evidence).unwrap()),"command":["bench","--json"],"result_digest":digest(&canonical(&evidence["result"]).unwrap())}});
        if change == "command" {
            measured["performance"]["evidence"]["command"] = json!(["different"]);
        }
        if change == "result" {
            measured["performance"]["evidence"]["result_digest"] = json!(hash('f'));
        }
        write(&f.root, "input.json", &measured);
        f.ok(&["new", "input.json"]);
        if change == "command" || change == "result" {
            f.refused(
                &["release", "prepare", "v1"],
                "exact source, command and result",
            );
            continue;
        }
        f.ok(&["release", "prepare", "v1"]);
        f.ok(&["release", "review", "v1"]);
        match change {
            "source" => {
                fs::write(f.root.join("source.rs"), "changed").unwrap();
                f.refused(&["release", "finalize", "v1"], "stale");
            }
            "evidence" => {
                fs::write(f.root.join("measurement.json"), "{}").unwrap();
                f.refused(&["release", "finalize", "v1"], "evidence digest mismatch");
            }
            _ => {
                f.ok(&["release", "finalize", "v1"]);
                assert!(f.baseline()["records"]["measured"]["performance"].is_object());
            }
        }
    }
}

// TempDir owns every special file. Its Drop removes FIFOs even during unwinding;
// no cleanup code after a potentially panicking assertion is required.
fn fifo_fixture() -> (tempfile::TempDir, PathBuf) {
    let owner = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(owner.path()).unwrap().join("input.json");
    let status = Command::new("mkfifo").arg(&path).status().unwrap();
    assert!(status.success());
    (owner, path)
}
#[test]
fn fifo_negative_path_is_bounded_and_raii_cleanup_survives_success_and_panic() {
    use std::time::{Duration, Instant};
    let (owner, path) = fifo_fixture();
    let store = devrec::store::Store::open(&fs::canonicalize(owner.path()).unwrap()).unwrap();
    let started = Instant::now();
    let error = store.read("input.json").unwrap_err();
    assert!(error.contains("bounded regular file"));
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(store);
    drop(owner);
    assert!(!path.exists());
    let observed = std::cell::RefCell::new(None);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (_owner, path) = fifo_fixture();
        *observed.borrow_mut() = Some(path);
        panic!("intentional assertion failure exercises fixture cleanup");
    }));
    assert!(result.is_err());
    assert!(!observed.borrow().as_ref().unwrap().exists());
}

#[test]
fn review_resumes_an_existing_running_task_without_allocating_a_second_task() {
    let f = Fixture::new();
    let receipt = f.reviewed();
    let mut task = read(&f.state, "task.json");
    task["finished"] = json!(false);
    write(&f.state, "task.json", &task);
    assert_eq!(f.ok(&["release", "review", "v1"]), receipt);
    assert_eq!(f.calls().iter().filter(|v| v[1] == "plan").count(), 1);
    assert_eq!(f.calls().iter().filter(|v| v[1] == "run").count(), 2);
}

#[test]
fn stored_native_plan_cannot_point_at_a_different_captured_request() {
    let f = Fixture::new();
    f.reviewed();
    let object = format!("cas/objects/ff/{}", "f".repeat(62));
    let mut envelope = read(&f.state, &object);
    envelope["payload"]["request"]["release_id"] = json!("different");
    write(&f.state, &object, &envelope);
    f.refused(
        &["release", "finalize", "v1"],
        "different reconciliation request",
    );
}

#[test]
fn shared_snapshot_blocks_writers_and_writer_blocks_current_check_and_render() {
    use devrec::store::Store;
    let f = Fixture::new();
    f.add("original");
    let reader = Store::open(&f.root).unwrap();
    reader.read_lock().unwrap();
    let out = f.run(&["release", "prepare", "v1"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("repository writer lock"));
    assert!(!f.root.join(".devrec/releases/v1/plan.json").exists());
    drop(reader);

    // Separate roots avoid a concurrently forked test subprocess temporarily
    // retaining the first root's CLOEXEC descriptor before it reaches exec.
    let f = Fixture::new();
    f.add("original");
    let writer = Store::open(&f.root).unwrap();
    let writer_lock = writer.lock().unwrap();
    let snapshot = Store::open(&f.root).unwrap();
    let error = devrec::release::current(&snapshot).unwrap_err();
    assert!(error.contains("reader lock"));
    for args in [
        &["check"][..],
        &["render"][..],
        &["render", "--nightly"][..],
    ] {
        let out = f.run(args);
        assert!(!out.status.success(), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("reader lock"));
    }
    drop(snapshot);
    drop(writer_lock);
    drop(writer);
    let started = std::time::Instant::now();
    loop {
        let out = f.run(&["check"]);
        if out.status.success() {
            let checked: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(checked["records"], 1);
            break;
        }
        assert!(String::from_utf8_lossy(&out.stderr).contains("reader lock"));
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "writer lock was not released"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn nightly_snapshot_of_empty_repository_creates_no_state() {
    let owner = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(owner.path()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_devrec"))
        .arg("--root")
        .arg(&root)
        .args(["render", "--nightly"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rendered: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rendered["schema"], "devrec/render@1");
    assert!(!root.join(".devrec").exists());
    assert_eq!(fs::read_dir(root).unwrap().count(), 0);
}

#[test]
fn identifier_boundaries_match_core_for_records_and_release_names() {
    let f = Fixture::new();
    let max = "a".repeat(128);
    let too_long = "a".repeat(129);
    assert!(devrec::core::valid_id(&max));
    assert!(devrec::release::id(&max).is_ok());
    assert!(!devrec::core::valid_id(&too_long));
    assert!(devrec::release::id(&too_long).is_err());
    assert_eq!(f.add(&max)["id"], max);
    assert_eq!(f.ok(&["release", "prepare", &max])["release_id"], max);
    let out = f.run(&["release", "prepare", &too_long]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid identifier"));
}

#[test]
fn recovery_rechecks_external_late_records_against_journal_baseline() {
    let f = Fixture::new();
    f.prepared();
    f.control(json!({"additions":[record("model-note")]}));
    f.ok(&["release", "review", "v1"]);
    let out = f
        .cmd(&["release", "finalize", "v1"])
        .env("DEVREC_TEST_FAILPOINT", "baseline")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("injected interruption after baseline"));
    let journal = fs::read(f.root.join(".devrec/transaction.json")).unwrap();
    write(
        &f.root,
        ".devrec/fragments/model-note.json",
        &record("model-note"),
    );
    let out = f.run(&["release", "finalize", "v1"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("reuses an immutable historical ID"));
    assert_eq!(
        fs::read(f.root.join(".devrec/transaction.json")).unwrap(),
        journal
    );
    assert!(!f.root.join(".devrec/releases/v1/done.json").exists());
    fs::remove_file(f.root.join(".devrec/fragments/model-note.json")).unwrap();
    f.ok(&["release", "finalize", "v1"]);
    assert!(!f.root.join(".devrec/transaction.json").exists());
    assert!(f.baseline()["records"]["model-note"]["provenance"].is_object());
}

#[test]
fn model_cannot_accept_an_adr_or_supersede_a_captured_accepted_decision() {
    for supersede in [false, true] {
        let f = Fixture::new();
        let mut decision = record("decision");
        decision["kind"] = json!("adr");
        decision["accepted"] = json!(true);
        write(&f.root, "input.json", &decision);
        let captured = f.ok(&["new", "input.json"])["record"].clone();
        f.ok(&["release", "prepare", "v1"]);
        let mut addition = record("model-decision");
        addition["kind"] = json!("adr");
        if supersede {
            addition["supersedes"] = json!(["decision"]);
        } else {
            addition["accepted"] = json!(true);
        }
        f.control(json!({"additions":[addition]}));
        f.refused(&["release", "review", "v1"], "accepted ADR decision");
        assert_eq!(read(&f.root, ".devrec/fragments/decision.json"), captured);
        assert!(!f.root.join(".devrec/releases/v1/review.json").exists());
    }
}

#[test]
fn tampered_plan_record_is_rejected_before_native_dispatch_even_with_original_included_hash() {
    let f = Fixture::new();
    f.prepared();
    let mut plan = read(&f.root, ".devrec/releases/v1/plan.json");
    let included = plan["included"].clone();
    plan["records"]["original"]["body"] = json!("Forged plan body");
    assert_eq!(plan["included"], included);
    write(&f.root, ".devrec/releases/v1/plan.json", &plan);
    f.refused(
        &["release", "review", "v1"],
        "included record changed: original",
    );
    assert!(!f.state.join("calls.jsonl").exists());
    assert!(!f.root.join(".devrec/releases/v1/review.json").exists());
}

#[test]
fn tampered_request_rendering_is_rejected_before_native_dispatch() {
    let f = Fixture::new();
    f.prepared();
    let mut plan = read(&f.root, ".devrec/releases/v1/plan.json");
    plan["request"]["after"] = json!("Forged rendering unrelated to captured records");
    write(&f.root, ".devrec/releases/v1/plan.json", &plan);
    f.refused(
        &["release", "review", "v1"],
        "release request differs from exact captured records",
    );
    assert!(!f.state.join("calls.jsonl").exists());
    assert!(!f.root.join(".devrec/releases/v1/review.json").exists());
}

#[test]
fn model_additions_after_compaction_preserve_prior_evidence_and_consume_late_revision() {
    let f = Fixture::new();
    f.prepared();
    let mut first_note = record("first-note");
    first_note["depends_on"] = json!(["original"]);
    f.control(json!({"additions":[first_note]}));
    let first_receipt = f.ok(&["release", "review", "v1"]);
    f.ok(&["release", "finalize", "v1"]);
    let first_baseline = f.baseline();
    let first_evidence = fs::read(f.root.join(".devrec/releases/v1/review.json")).unwrap();
    let first_completion = fs::read(f.root.join(".devrec/releases/v1/done.json")).unwrap();
    assert!(!f.root.join(".devrec/fragments/original.json").exists());

    // A later branch revises the consumed record using its immutable baseline ID.
    let mut late_revision = record("late-revision");
    late_revision["revises"] = json!("original");
    late_revision["revision"] = json!(2);
    late_revision["depends_on"] = json!(["first-note"]);
    write(&f.root, "input.json", &late_revision);
    let captured_revision = f.ok(&["revise", "input.json"])["record"].clone();
    let plan = f.ok(&["release", "prepare", "v2"]);
    assert_eq!(
        plan["request"]["baseline"]["original"],
        first_baseline["records"]["original"]
    );
    assert_eq!(
        plan["request"]["baseline"]["first-note"],
        first_baseline["records"]["first-note"]
    );
    assert_eq!(plan["records"]["late-revision"], captured_revision);
    let mut second_note = record("second-note");
    second_note["depends_on"] = json!(["late-revision", "original"]);
    f.control(json!({"additions":[second_note]}));
    let second_receipt = f.ok(&["release", "review", "v2"]);
    assert_ne!(second_receipt["task_id"], first_receipt["task_id"]);
    assert_ne!(
        second_receipt["request_digest"],
        first_receipt["request_digest"]
    );
    f.ok(&["release", "finalize", "v2"]);

    let baseline = f.baseline();
    for id in ["original", "first-note"] {
        assert_eq!(baseline["records"][id], first_baseline["records"][id]);
    }
    assert_eq!(baseline["records"]["late-revision"], captured_revision);
    let second_provenance = &baseline["records"]["second-note"]["provenance"];
    for key in ["task_id", "revision_id", "result_digest", "request_digest"] {
        assert_eq!(second_provenance[key], second_receipt[key]);
        assert_eq!(
            baseline["records"]["first-note"]["provenance"][key],
            first_receipt[key]
        );
    }
    assert_eq!(
        baseline["consumed"]["late-revision"],
        plan["included"]["late-revision"]
    );
    assert_eq!(
        baseline["consumed"]["original"],
        first_baseline["consumed"]["original"]
    );
    assert!(!f.root.join(".devrec/fragments/late-revision.json").exists());
    assert_eq!(
        fs::read(f.root.join(".devrec/releases/v1/review.json")).unwrap(),
        first_evidence
    );
    assert_eq!(
        fs::read(f.root.join(".devrec/releases/v1/done.json")).unwrap(),
        first_completion
    );
    assert_eq!(f.calls().iter().filter(|v| v[1] == "plan").count(), 2);
    assert_eq!(f.calls().iter().filter(|v| v[1] == "run").count(), 2);
    assert_eq!(f.ok(&["check"])["records"], 4);
}

#[test]
fn additions_graph_is_independent_of_array_order_and_duplicates_are_refused() {
    let mut baselines = Vec::new();
    for reverse in [false, true] {
        let f = Fixture::new();
        f.prepared();
        let mut parent = record("parent");
        parent["depends_on"] = json!(["original"]);
        let mut child = record("child");
        child["depends_on"] = json!(["parent"]);
        let additions = if reverse {
            vec![parent, child]
        } else {
            vec![child, parent]
        };
        f.control(json!({"additions":additions}));
        f.ok(&["release", "review", "v1"]);
        f.ok(&["release", "finalize", "v1"]);
        let mut baseline = f.baseline();
        for record in baseline["records"].as_object_mut().unwrap().values_mut() {
            record["provenance"] = Value::Null;
        }
        baselines.push(baseline);
    }
    assert_eq!(baselines[0], baselines[1]);
    let f = Fixture::new();
    f.prepared();
    f.control(json!({"additions":[record("duplicate"), record("duplicate")]}));
    f.refused(&["release", "review", "v1"], "duplicates an existing ID");
}

#[test]
fn failed_bundle_validation_cleans_up_only_its_own_output() {
    let owner = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(owner.path()).unwrap();
    let executable = base.join("refusing-af");
    fs::write(&executable, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(devrec::bundle::build(&base, &executable, "new-bundle").is_err());
    assert!(!base.join("new-bundle").exists());
    fs::create_dir(base.join("reviewed-bundle")).unwrap();
    fs::write(base.join("reviewed-bundle/owned"), "preserve").unwrap();
    assert!(devrec::bundle::build(&base, &executable, "reviewed-bundle").is_err());
    assert_eq!(
        fs::read_to_string(base.join("reviewed-bundle/owned")).unwrap(),
        "preserve"
    );
}
