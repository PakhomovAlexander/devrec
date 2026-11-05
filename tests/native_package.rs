//! Real installed AF kernel and Store with an inert Codex protocol fixture.
//! No credentials, models, network, publication or machine registry changes.
#![cfg(unix)]
use devrec::store::digest;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn write(path: impl AsRef<Path>, value: Value) {
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}
struct Fixture {
    _owner: tempfile::TempDir,
    base: PathBuf,
    af: PathBuf,
    repo: PathBuf,
    state: PathBuf,
}
impl Fixture {
    fn new() -> Option<Self> {
        let af = std::env::var_os("DEVREC_AF_EXECUTABLE")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                    .map(|p| p.join("af"))
                    .find(|p| p.is_file())
            });
        let Some(af) = af else {
            assert!(
                std::env::var_os("DEVREC_REQUIRE_NATIVE_AF").is_none(),
                "installed AF required"
            );
            eprintln!("native package test skipped: install AF or set DEVREC_AF_EXECUTABLE");
            return None;
        };
        let af = fs::canonicalize(af).unwrap();
        let owner = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(owner.path()).unwrap();
        for p in [
            "authority",
            "state-parent",
            "auth",
            "bin",
            "scratch",
            "records",
        ] {
            fs::create_dir(base.join(p)).unwrap();
        }
        fs::set_permissions(base.join("auth"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(base.join("auth/auth.json"), "{}").unwrap();
        fs::write(
            base.join("bin/codex"),
            include_str!("fixtures/codex-protocol.py"),
        )
        .unwrap();
        fs::set_permissions(base.join("bin/codex"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(base.join("providers.toml"), format!("version = 3\n[[providers]]\nid = \"codex-personal\"\nkind = \"codex\"\nauth_dir = {:?}\n", base.join("auth"))).unwrap();
        let repo = base.join("authority");
        let state = base.join("state-parent/store");
        Some(Self {
            _owner: owner,
            base,
            af,
            repo,
            state,
        })
    }
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env("TMPDIR", self.base.join("scratch"))
            .env("XDG_STATE_HOME", self.base.join("xdg-state"))
            .env("XDG_CACHE_HOME", self.base.join("xdg-cache"))
            .env("XDG_CONFIG_HOME", self.base.join("xdg-config"))
            .env("XDG_DATA_HOME", self.base.join("xdg-data"))
            .env("AF_SELF_OFFLINE", "1")
            .env("AF_PROVIDERS_FILE", self.base.join("providers.toml"))
            .env(
                "PATH",
                std::env::join_paths(
                    std::iter::once(self.base.join("bin"))
                        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                )
                .unwrap(),
            );
        command
    }
    fn native(&self, args: &[&str]) -> Output {
        self.command(&self.af)
            .args(args)
            .arg("--repo")
            .arg(&self.repo)
            .arg("--state")
            .arg(&self.state)
            .output()
            .unwrap()
    }
    fn cli(&self, args: &[&str]) -> Output {
        self.command(
            std::env::var_os("DEVREC_TEST_EXECUTABLE")
                .unwrap_or_else(|| env!("CARGO_BIN_EXE_devrec").into()),
        )
        .arg("--root")
        .arg(self.base.join("records"))
        .args(args)
        .output()
        .unwrap()
    }
    fn install(&self) -> Value {
        let receipt = success(
            self.command(
                std::env::var_os("DEVREC_TEST_EXECUTABLE")
                    .unwrap_or_else(|| env!("CARGO_BIN_EXE_devrec").into()),
            )
            .arg("--root")
            .arg(&self.repo)
            .args(["af", "bundle"])
            .arg(&self.af)
            .arg("reconciliation")
            .output()
            .unwrap(),
        );
        fs::create_dir(self.repo.join(".af")).unwrap();
        let catalog = format!(
            "schema = \"af.task-catalog/2\"\nno_match = \"refuse\"\n[independence]\ncommand_workers_by_package = true\ndistinct_principals = false\ndistinct_providers = false\ndistinct_models = false\n[provider_admission]\ntokens = 4096\nwall_ms = 45000\n\n{}",
            fs::read_to_string(self.repo.join("reconciliation/catalog-snippet.toml")).unwrap()
        );
        fs::write(self.repo.join(".af/task-catalog.toml"), catalog).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "Private native package fixture",
            ],
        ] {
            let out = self
                .command("git")
                .arg("-C")
                .arg(&self.repo)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        receipt
    }
    fn plan(&self, attempts: u64) -> Output {
        write(
            self.base.join("task.json"),
            json!({"schema":"af.task-file/1","task_id":"install-probe","kind":"devrec-reconciliation","goal":"Validate installed reconciliation package","strategy":"light","pipeline":{"name":"devrec/reconcile","fallback":"refuse"},"limits":{"tokens":16384,"max_attempts":attempts,"wall_ms":180000,"verification":{"tokens":0,"attempts":0,"wall_ms":0}}}),
        );
        self.native(&[
            "task",
            "plan",
            "--file",
            self.base.join("task.json").to_str().unwrap(),
            "--bindings",
            self.repo
                .join("reconciliation/bindings.toml")
                .to_str()
                .unwrap(),
            "--input-json",
            "request=input.json",
            "--json",
        ])
    }
}

#[test]
fn reconciliation_source_contracts_have_singleton_parity() {
    let public: Value =
        serde_json::from_str(include_str!("../schemas/reconciliation-additions-v1.json")).unwrap();
    let worker: Value = serde_json::from_str(include_str!(
        "../integration/reconciliation/worker/outputs/reconciliation.schema.json"
    ))
    .unwrap();
    let kind: Value = serde_json::from_str(include_str!(
        "../integration/reconciliation/kind/structured.json"
    ))
    .unwrap();
    assert_eq!(worker, kind["outputs"]["reconciliation"]["schema"]);
    let source = &public["properties"]["source_artifact_ids"];
    assert_eq!(source["minItems"], 1);
    assert_eq!(source["maxItems"], 1);
    assert_eq!(source, &worker["properties"]["source_artifact_ids"]);
}

fn fixture_sources(f: &Fixture, mode: &str) {
    let input: Value =
        serde_json::from_slice(&fs::read(f.base.join("auth/last-worker-input.json")).unwrap())
            .unwrap();
    let output: Value =
        serde_json::from_slice(&fs::read(f.base.join("auth/last-reconciliation.json")).unwrap())
            .unwrap();
    let request = &input["inputs"]["request"][0]["artifact_id"];
    let requirements = &input["inputs"]["requirements"][0]["artifact_id"];
    assert!(request.is_string() && requirements.is_string());
    assert_ne!(request, requirements);
    let expected = match mode {
        "bad-source" => json!([format!("sha256:{}", "0".repeat(64))]),
        "two-sources" => json!([request, requirements]),
        _ => json!([request]),
    };
    assert_eq!(output["source_artifact_ids"], expected);
    assert_ne!(output["request_digest"], *request);
}

#[test]
fn delivered_package_plans_and_runs_native_kernel_and_installs_for_release_review() {
    let Some(f) = Fixture::new() else { return };
    let bundle = f.install();
    for (directory, name) in [
        ("kind", "devrec/reconcile-kind"),
        ("worker", "devrec/reconcile-worker"),
        ("pipeline", "devrec/reconcile"),
    ] {
        let output = f
            .command(&f.af)
            .args(["catalog", "digest"])
            .arg(f.repo.join("reconciliation").join(directory))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            bundle["packages"][name].as_str().unwrap()
        );
    }
    // Builder refuses overwriting a reviewed package.
    assert!(
        !f.command(
            std::env::var_os("DEVREC_TEST_EXECUTABLE")
                .unwrap_or_else(|| env!("CARGO_BIN_EXE_devrec").into())
        )
        .arg("--root")
        .arg(&f.repo)
        .args(["af", "bundle"])
        .arg(&f.af)
        .arg("reconciliation")
        .output()
        .unwrap()
        .status
        .success()
    );
    write(
        f.base.join("input.json"),
        json!({"request_digest":digest(b"probe"),"request":{"schema":"devrec/reconciliation-request@1","release_id":"probe","stage_digest":digest(b"stage"),"baseline":{},"records":{},"before":"","after":"","inventory":{}}}),
    );
    let refused = f.plan(1);
    assert!(!refused.status.success());
    let diagnostic: Value = serde_json::from_slice(&refused.stdout).unwrap();
    assert_eq!(
        diagnostic["planning_diagnostic"]["mandatory"]["attempts"],
        2
    );
    let plan = success(f.plan(2));
    assert_eq!(plan["schema"], "af/task-inspection@16");
    assert_eq!(plan["plan"]["limits"]["max_attempts"], 2);
    // Native planning compiles the relative Task wall budget to an absolute deadline.
    let task: Value = serde_json::from_slice(&fs::read(f.base.join("task.json")).unwrap()).unwrap();
    assert_eq!(task["limits"]["wall_ms"], 180000);
    assert_eq!(
        diagnostic["planning_diagnostic"]["mandatory"]["wall_ms"],
        165000
    );
    assert_eq!(plan["graph"]["slots"].as_object().unwrap().len(), 1);
    assert_eq!(
        plan["graph"]["slots"]["root.slots.reconcile"]["max_attempts"],
        1
    );
    assert_eq!(
        plan["graph"]["allowances"]["root.nodes.work"]["max_attempts"],
        1
    );
    assert_eq!(
        plan["graph"]["allowances"]["root.providers.admit0"]["max_attempts"],
        1
    );
    assert_eq!(
        plan["graph"]["allowances"]["root.nodes.work"]["verification_attempts"],
        0
    );
    assert!(
        !f.base.join("auth/calls").exists(),
        "planner invoked model protocol"
    );
    let run = success(f.native(&[
        "task",
        "run",
        "install-probe",
        "--confirm-plan",
        plan["plan_id"].as_str().unwrap(),
        "--json",
    ]));
    assert_eq!(run["result"]["domain_conclusion"], "structurally_validated");
    assert_eq!(run["attempts"], 2);
    fixture_sources(&f, "exact");
    assert_eq!(
        fs::read_to_string(f.base.join("auth/calls")).unwrap(),
        "admission\nworker\n"
    );
    let head = f
        .command("git")
        .arg("-C")
        .arg(&f.repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    fs::create_dir(f.base.join("records/.devrec")).unwrap();
    write(
        f.base.join("records/.devrec/af.json"),
        json!({"schema":"devrec/af-integration@1","executable":f.af,"executable_digest":bundle["executable_digest"],"repository":f.repo,"authority":String::from_utf8(head.stdout).unwrap().trim(),"authority_policy_id":plan["plan"]["authority"]["policy_id"],"pipeline":"devrec/reconcile","task_kind":"devrec-reconciliation","bindings":f.repo.join("reconciliation/bindings.toml"),"state":f.state,"scratch":f.base.join("scratch"),"request_port":"request","result_port":"reconciliation","timeout_secs":180}),
    );
    write(
        f.base.join("records/input.json"),
        json!({"schema_version":1,"id":"original","kind":"changelog","title":"Original","body":"Captured evidence"}),
    );
    success(f.cli(&["new", "input.json"]));
    success(f.cli(&["release", "prepare", "v1"]));
    let receipt = success(f.cli(&["release", "review", "v1"]));
    assert_eq!(receipt["additions"].as_array().unwrap().len(), 2);
    fixture_sources(&f, "exact");
    assert_eq!(success(f.cli(&["release", "review", "v1"])), receipt);
    success(f.cli(&["release", "finalize", "v1"]));
    let baseline: Value =
        serde_json::from_slice(&fs::read(f.base.join("records/.devrec/baseline.json")).unwrap())
            .unwrap();
    assert_eq!(
        baseline["records"]["child"]["depends_on"],
        json!(["parent"])
    );
    assert_eq!(
        baseline["records"]["parent"]["provenance"]["result_digest"],
        receipt["result_digest"]
    );
    assert_eq!(
        fs::read_to_string(f.base.join("auth/calls")).unwrap(),
        "admission\nworker\nadmission\nworker\n"
    );
    // Exercise both native rejection and the release CLI using the delivered package.
    // Each failure must consume just admission plus one semantic Worker pass.
    let mut extra_calls = Vec::new();
    for mode in ["bad-source", "two-sources"] {
        fs::write(f.base.join("auth").join(mode), "").unwrap();
        let calls_before = fs::read_to_string(f.base.join("auth/calls")).unwrap();
        let mut task: Value =
            serde_json::from_slice(&fs::read(f.base.join("task.json")).unwrap()).unwrap();
        task["task_id"] = json!(mode);
        write(f.base.join("task.json"), task);
        let bad_plan = success(f.native(&[
            "task",
            "plan",
            "--file",
            f.base.join("task.json").to_str().unwrap(),
            "--input-json",
            "request=input.json",
            "--json",
        ]));
        let bad = f.native(&[
            "task",
            "run",
            mode,
            "--confirm-plan",
            bad_plan["plan_id"].as_str().unwrap(),
            "--json",
        ]);
        assert!(!bad.status.success());
        let bad: Value = serde_json::from_slice(&bad.stdout).unwrap();
        assert_eq!(bad["attempts"], 2);
        assert_ne!(bad["result"]["domain_conclusion"], "structurally_validated");
        fixture_sources(&f, mode);
        let calls_after_native = fs::read_to_string(f.base.join("auth/calls")).unwrap();
        if calls_after_native != format!("{calls_before}admission\nworker\n") {
            extra_calls.push(format!("{mode} native: {calls_after_native:?}"));
        }

        success(f.cli(&["release", "prepare", mode]));
        let rejected = f.cli(&["release", "review", mode]);
        assert!(!rejected.status.success(), "CLI accepted {mode}");
        fixture_sources(&f, mode);
        let calls_after_cli = fs::read_to_string(f.base.join("auth/calls")).unwrap();
        if calls_after_cli != format!("{calls_after_native}admission\nworker\n") {
            extra_calls.push(format!("{mode} CLI: {calls_after_cli:?}"));
        }
        assert!(!f.cli(&["release", "finalize", mode]).status.success());
        assert_eq!(
            fs::read_to_string(f.base.join("auth/calls")).unwrap(),
            calls_after_cli
        );
        fs::remove_file(f.base.join("auth").join(mode)).unwrap();
    }
    assert!(
        extra_calls.is_empty(),
        "extra semantic calls: {extra_calls:?}"
    );
}
