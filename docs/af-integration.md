# Native reconciliation boundary

Production integration is Rust (`src/af.rs`), invoked by `release review` and re-read by `release finalize`; the core has no runtime/model dependency. No caller-provided success boolean or receipt import grants review authority. Process exit success is insufficient.

The delivered native package is `integration/reconciliation/`: `kind/kind.toml` with an inline strict `structured.json`, one Sol low Worker, and a Pipeline connecting that Worker to native `structured_accept`. The package uses `af.task-kind/2`, `af.structured-contract/1`, `af.worker/1` and `af.pipeline/4`. It is validated against installed AF 0.14.0-rc.7 (commit e2898cd7d7c5), which supplies `af/task-inspection@16`.

Build an installable copy using the installed Rust CLI (the destination is an absent direct child of the selected repository):

```sh
devrec --root /absolute/authority-repository af bundle /absolute/installed/af reconciliation
```

The Rust builder embeds all package bytes, calls that exact AF executable's `catalog digest` on each package (which validates the real manifests and kind contract), and emits `catalog-snippet.toml`, empty versioned `bindings.toml`, and `bundle.json` with the executable hash and computed package pins. It refuses existing destinations and removes incomplete output on failure. It never edits `.af/`, Git authority, accounts or provider registrations. Review the package and merge the snippet's mappings and pins into the existing `.af/task-catalog.toml`, preserving other entries, then commit the reviewed authority. Pins cover paths relative to that repository, so keep the chosen destination name. No credentials or executable placeholder pins ship.

The repository's native catalog must reserve bounded provider admission. The tested `af.task-catalog/2` configuration adds these settings alongside its existing independence policy and generated package entries:

```toml
[provider_admission]
tokens = 4096
wall_ms = 5000
```

Keep `codex-personal` as the Worker provider; authentication remains a machine-local operator responsibility. The Worker reserves 8,192 tokens and 20 seconds, uses `gpt-6.1-sol` with `low` effort, declares no effects, and has precisely one semantic attempt. The native Task and Pipeline require **two total counted attempts**: one provider-admission capability probe and one semantic Worker pass. The actual planner rejects Task max_attempts=1 as infeasible, reporting mandatory attempts=2; Pipeline max_attempts=1 is also infeasible. Both totals are therefore 2, with Worker max_attempts=1 and every verification/recovery allowance 0. The adapter inspects the compiled nodes, parallelism, Task limits and allowances before dispatch. Failed semantic execution receives no retry. Larger pre-existing admission reservations may make the fixed 16,384-token Task budget infeasible; review the catalog policy rather than increasing semantic attempts or silently fitting limits. There is no reviewer panel, remote gate or separate verification Task.

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
  "timeout_secs": 120
}
```

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

An empty additions list is valid, but is not the only supported result. Every addition is a full strict record with its own stable ID. Corrections use `revises` or `supersedes`; originals are never rewritten. References must remain within the affected closure or the complete addition set, regardless of array order. Duplicate IDs, accepted-ADR rewrites, model establishment or supersession of accepted decisions, model performance authority and substantive contradictions block release. Devrec assigns native task/revision/result/request provenance to every addition and preserves it through compaction. Record graph validation is deterministic and reruns after additions. Reconciliation text is untrusted data and never an execution instruction.

`tests/native.rs` supplies a fake Python subprocess transport only for tests. It exercises task allocation/run, explicit additions, native evidence rejection, source freshness, recovery, provenance and cleanup. It does not authenticate with a real provider or claim live execution. The subprocess deadline/output limits are enforced in Rust. Special-file reads use nonblocking, no-follow opens and regular-file checks.

`tests/native_package.rs` builds the delivered bundle through the CLI, installs it into a private Git authority, compares all computed pins, proves the one-attempt planner refusal, and runs the unchanged production package through the real installed AF kernel. An inert Codex protocol fixture supplies responses; AF performs actual capture, CAS writes, admission, schema/provenance validation and native result construction. The test then runs CLI release review/finalize with forward-referencing additions, verifies provenance and Task reuse, and checks that incorrect external-input provenance is rejected after one Worker pass. No fake native transcript, credentials or live model call is involved. Run `make check-native` (or set `DEVREC_REQUIRE_NATIVE_AF=1`) to require the test; `DEVREC_AF_EXECUTABLE` selects an installed binary. Without AF on PATH the standalone core gate skips that optional runtime test explicitly. Run it against a supported installed AF before integration acceptance.
