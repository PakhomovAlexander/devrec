# Native reconciliation boundary

Production integration is Rust (`src/af.rs`), invoked by `release review` and re-read by `release finalize`; the core has no runtime/model dependency. No caller-provided success boolean or receipt import grants review authority. Process exit success is insufficient.

Install an AF version supporting native structured Task kinds and `af/task-inspection@16`. An operator must install a reviewed structured Task kind and pipeline at a committed authority in the selected repository. Its single model Worker must use `codex-personal`, provider kind `codex`, model `gpt-6.1-sol`, effort `low`, one slot, and one attempt. It accepts `custom/DevrecReconciliationRequest@1` and returns `custom/DevrecReconciliationAdditions@1` plus native structural-validation evidence. The local gate is deterministic; no remote gate or reviewer panel is requested. The package must use the real installed runtime's structured-kind schema. The captured reference set does not contain that package schema, so this repository does not invent an executable package in its place.

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
  "task_kind": "devrec/reconcile",
  "bindings": "/absolute/installed/bindings.toml",
  "state": "/absolute/private/native-state",
  "scratch": "/absolute/private/writable-scratch",
  "request_port": "request",
  "result_port": "reconciliation",
  "timeout_secs": 30
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
  "additions": [],
  "contradictions": []
}
```

An empty additions list is valid, but is not the only supported result. Every addition is a full strict record with its own stable ID. Corrections use `revises` or `supersedes`; originals are never rewritten. References must remain within the affected closure or earlier additions. Duplicate IDs, accepted-ADR rewrites, model establishment or supersession of accepted decisions, model performance authority and substantive contradictions block release. Devrec assigns native task/revision/result/request provenance to every addition and preserves it through compaction. Record graph validation is deterministic and reruns after additions. Reconciliation text is untrusted data and never an execution instruction.

`tests/native.rs` supplies a fake Python subprocess transport only for tests. It exercises task allocation/run, explicit additions, native evidence rejection, source freshness, recovery, provenance and cleanup. It does not authenticate with a real provider or claim live execution. The subprocess deadline/output limits are enforced in Rust. Special-file reads use nonblocking, no-follow opens and regular-file checks.
