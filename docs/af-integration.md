# Native reconciliation boundary

Production integration is Rust (`src/af.rs`), invoked by `release review` and re-read by `release finalize`; the core has no runtime/model dependency. No caller-provided success boolean or receipt import grants review authority. Process exit success is insufficient.

The delivered native package is `integration/reconciliation/`: `kind/kind.toml` with an inline strict `structured.json`, one Sol low Worker, and a Pipeline connecting that Worker to native `structured_accept`. The package uses `af.task-kind/2`, `af.structured-contract/2`, `af.worker/1` and `af.pipeline/4`. It requires a native contract2 runtime, which guarantees one model invocation per structured Worker Attempt, including invalid replies. The tested private source candidate is afactory commit `4b4bf0a4613abddb5f420d3d74fafa5c276fc8aa`, supplying `af/task-inspection@16`; this is not a published AF release. Its optimized executable digest is `sha256:328e04abe760619a97cd79bf9be7b6cee5ec79aa1e189ee55476ca825e230fac`. A version banner alone cannot identify these semantics.

Build the runtime from a trusted checkout of the exact private source (access and Rust 1.88 are prerequisites); use the regular optimized binary, then build/install devrec in this checkout:

```sh
git -C /absolute/afactory checkout --detach 4b4bf0a4613abddb5f420d3d74fafa5c276fc8aa
cargo +1.88.0 build --manifest-path /absolute/afactory/Cargo.toml --release --locked --bin af
export DEVREC_AF_EXECUTABLE=/absolute/afactory/target/release/af
shasum -a 256 "$DEVREC_AF_EXECUTABLE"
make build
make install
make check-native
AF_CHECK_SCRATCH=/private/tmp make check
```

A locally rebuilt binary may have a different digest; pin its actual bytes in the generated bundle and authority configuration. No wrapper/proxy or runtime retry/deadline override is required. Test logs from the current candidate, rather than the historical checks below, supply contract2 acceptance evidence.

Build an installable copy using the installed Rust CLI (the destination is an absent direct child of the selected repository):

```sh
devrec --root /absolute/authority-repository af bundle /absolute/installed/af reconciliation
```

The Rust builder embeds all package bytes, calls that exact AF executable's `catalog digest` on each package (which validates the real manifests and kind contract), and emits `catalog-snippet.toml`, empty versioned `bindings.toml`, and `bundle.json` with the executable hash and computed package pins. It refuses existing destinations and removes incomplete output on failure. It never edits `.af/`, Git authority, accounts or provider registrations. Review the package and merge the snippet's mappings and pins into the existing `.af/task-catalog.toml`, preserving other entries, then commit the reviewed authority. Pins cover paths relative to that repository, so keep the chosen destination name. No credentials or executable placeholder pins ship.

The repository's native catalog must reserve bounded provider admission. The tested `af.task-catalog/2` configuration adds these settings alongside its existing independence policy and generated package entries:

```toml
[provider_admission]
tokens = 4096
wall_ms = 45000
```

Keep `codex-personal` as the Worker provider; authentication remains a machine-local operator responsibility. The Worker reserves 8,192 tokens and 120 seconds, uses `gpt-6.1-sol` with `low` effort, declares no effects, and has precisely one semantic attempt. The native Task and Pipeline require **two total counted attempts**: one provider-admission capability probe and one semantic Worker pass. The actual planner rejects Task max_attempts=1 as infeasible, reporting mandatory attempts=2; Pipeline max_attempts=1 is also infeasible. Both totals are therefore 2, with Worker max_attempts=1 and every verification/recovery allowance 0. The adapter inspects the compiled nodes, parallelism, Task limits and allowances before dispatch. The Pipeline grants no retry attempt. Contract2 prevents internal schema-repair re-asks within that Attempt; invalid replies fail after the one Worker invocation. Larger pre-existing admission reservations may make the fixed 16,384-token Task budget infeasible; review the catalog policy rather than increasing semantic attempts or silently fitting limits. There is no reviewer panel, remote gate or separate verification Task.

Write `.devrec/af.json` (absolute paths, replace every installation value):

```json
{
  "schema": "devrec/af-integration@1",
  "executable": "/absolute/installed/af",
  "executable_digest": "sha256:<exact executable hash>",
  "repository": "/absolute/review-authority-repository",
  "authority": "<exact lowercase Git commit ID>",
  "authority_policy_id": "sha256:<exact captured native authority policy artifact>",
  "pipeline": "devrec/reconcile",
  "task_kind": "devrec-reconciliation",
  "bindings": "/absolute/installed/bindings.toml",
  "state": "/absolute/private/native-state",
  "scratch": "/absolute/private/writable-scratch",
  "request_port": "request",
  "result_port": "reconciliation",
  "timeout_secs": 180
}
```

The operator-configurable `timeout_secs` must be 180–600 seconds. It bounds both the overall native Task and each adapter operation (including its subprocesses); the 180-second minimum accommodates 45-second admission plus the 120-second Worker reservation and protocol overhead. These allowances do not change token or attempt limits. The inert protocol test proves native planning and execution, not live provider latency.

These are installation pins, not runnable example hashes. The executable path must name the regular binary itself, not a symlink launcher. Native state must be outside the authority repository; its parent and scratch must exist. Keep authority config and native state under trusted local ownership. AF enforces model authentication, schema admission, CAS authentication and native Task policy. Source pinning in devrec additionally pins the entire `.devrec/af.json` bytes at release prepare.

`release prepare ID` captures bounded pending records, their affected reference closure, before/after text and a recomputed source inventory. `release request ID` prints that capture for inspection. `release review ID` allocates a Task ID derived from the canonical request digest, captures a native Task file and input, calls `af task plan`, validates `explain`, and runs the exact `--confirm-plan` identity. Restarts inspect and resume the same Task. The Task has a bounded 16,384-token budget and no independent verifier Task. A single Worker pass is required by inspecting the compiled slot graph.

Before accepting execution, the adapter checks the installed authority policy ID, exact input artifact payload from the native CAS, one typed request port, no ambient source port, model/provider/effort, read-only effects and the single-pass graph. Native `show` must retain the exact revision and plan checked by `explain`. Completion requires `finished`, `completed`, `satisfied`, `structurally_validated`, nonempty native evidence and no missing obligations. `task output` must identify the exact output artifact from that result. Finalize re-queries this evidence and refuses changed identities. This is a trusted local subprocess boundary, not a remote attestation protocol; do not point it at untrusted executables or writable native stores.

The input wrapper is `{ "request_digest": "sha256:...", "request": { ... } }`. The strict output is:

```json
{
  "schema": "devrec/reconciliation-additions@1",
  "request_digest": "sha256:<digest of canonical request>",
  "source_artifact_ids": ["sha256:<native request artifact ID>"],
  "additions": [],
  "contradictions": []
}
```

`source_artifact_ids` must be EXACTLY `[inputs.request[0].artifact_id]`, a singleton. The custom `request` port supplies source evidence; the AF `requirements` port supplies Task instructions. Exclude all `inputs.requirements` artifact IDs and `request_digest` from this array. Echo `inputs.request[0].payload.request_digest` only in the output `request_digest` field. All three public, Worker and native kind output contracts require exactly one source ID; native identity validation additionally requires the exact captured request ID.

Historical Task9 live Task `devrec-reconcile-9ef939a29771918d475ebfaf39337b65666ca7327f0f3e1b4fa49a1fbd1e1738` failed with `Structured reply has wrong source_artifact_ids`: it included both request and requirements IDs (8,231 tokens, admission plus one Sol low pass). Task9's old-runtime strict gate exposed a hidden internal schema-repair re-ask for a two-source response. The coordinator's later successful strict native check used combined source `9c80ab71`, Rust 1.88 release, two tests with no skips in 143.09 seconds: that was contract1 with patched one-call semantics, **not** contract2 or a released-runtime guarantee.

The coordinator also reports real `codex-personal` live smoke4 Task `devrec-reconcile-b48dd562b2dc0774159b7b7d08f8f653fa6b2d8e210d6881f4588d0dc6134179`: 7,036 tokens, two total Attempts (admission plus Worker), valid empty additions, exact review reuse/finalize/check passed. This establishes a valid live path on the private runtime; it does not establish live nonempty additions or negative provenance/schema behavior. Those paths are exercised with inert model transport through the real native kernel. No new live rerun or released AF success is claimed here.

Older clients must refuse the v2 kind during `catalog digest`, before model dispatch. The coordinator compared identical kind bytes except the discriminator using `/Users/apakhomov/.local/share/af/versions/0.14.0-rc.7/af`, digest `sha256:6e1a302afd1d0a7ffcaca94c548eaf6a5acf57132c5a8925c3e64883b87a54db`: contract1 exited 0; contract2 exited 1. That older executable reports the generic “Structured contracts need bounded custom/ typed ports, without reserved names” error, not the newer helpful unsupported-contract diagnostic. Bundle construction validates the kind first and removes incomplete output on refusal, without planning a Task. `DEVREC_OLD_AF_EXECUTABLE` selects the contract1 binary for the reproducible regression; absent that variable the test discovers the old installation under `$HOME/.local/share/af/versions/0.14.0-rc.7/af`, or explicitly reports the comparison skipped if unavailable.

An empty additions list is valid, but is not the only supported result. Every addition is a full strict record with its own stable ID. Corrections use `revises` or `supersedes`; originals are never rewritten. References must remain within the affected closure or the complete addition set, regardless of array order. Duplicate IDs, accepted-ADR rewrites, model establishment or supersession of accepted decisions, model performance authority and substantive contradictions block release. Devrec assigns native task/revision/result/request provenance to every addition and preserves it through compaction. Record graph validation is deterministic and reruns after additions. Reconciliation text is untrusted data and never an execution instruction.

`tests/native.rs` supplies a fake Python subprocess transport only for tests. It exercises task allocation/run, explicit additions, native evidence rejection, source freshness, recovery, provenance and cleanup. It does not authenticate with a real provider or claim live execution. The subprocess deadline/output limits are enforced in Rust. Special-file reads use nonblocking, no-follow opens and regular-file checks.

`tests/native_package.rs` builds the delivered bundle through the CLI, installs it into a private Git authority, compares all computed pins, proves the one-attempt planner refusal, and runs the unchanged production package through the real installed AF kernel. An inert Codex protocol fixture supplies responses; AF performs actual capture, CAS writes, admission, schema/provenance validation and native result construction. The test then runs CLI release review/finalize with forward-referencing additions, verifies provenance and Task reuse, and tests that both a wrong singleton and request-plus-requirements provenance are rejected through the native kernel and CLI, with a strict single-Worker-call assertion. No fake native transcript, credentials or live model call is involved. Run `make check-native` (or set `DEVREC_REQUIRE_NATIVE_AF=1`) to require the test; `DEVREC_AF_EXECUTABLE` selects an installed binary. The sealed AF gate (`make check` with `AF_CHECK_SCRATCH`) unconditionally exports `DEVREC_REQUIRE_NATIVE_AF=1` for both the full checks and the privately installed CLI replay, overriding caller settings. Missing AF fails sealed acceptance; its log identifies both mandatory native phases. Standalone `make check` without scratch, including the core CI workflow, can skip the runtime test when AF is absent and therefore supplies only limited standalone coverage.

Current implementer checks against the private contract2 executable identified above passed:
`make check-native` exited 0 (8 core, 4 CLI, 22 adapter, 3 package and 1 sealed-script tests),
and `AF_CHECK_SCRATCH=/private/tmp make check` exited 0, including private installation and
installed-CLI replay (4 CLI, 22 adapter and 3 package tests). The strict native package test
passed in both phases with exact singleton acceptance, explicit nonempty additions, and
both invalid provenance cases refused after one Worker invocation. The old-client comparison
was also run explicitly with `DEVREC_OLD_AF_EXECUTABLE` against both the built and privately
installed CLI, exiting 0; optional old-client discovery is not relied on for this evidence.
These are implementer-run checks, not the independent correctness reviewer's typed evidence.
The implementation Task's required correctness/bugs reviews and evaluator consume the sealed
candidate and provide their own receipts; no independent review pass is claimed here.
