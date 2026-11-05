# Devrec coding authority

`.af/task-catalog.toml` installs the coordinator-generated universal `impl` packages.
`[task.defaults]` in `.af/af.toml` selects `impl/implementation`. These are consuming
repository packages, not temporary AF runtime bootstrap packages. Their Worker schema
copies omit only inadmissible `$id`/`$schema` metadata. The unreferenced `onboard`
coding packages have been removed; the separate legacy diff-review authority remains
under `workers/`, `pipelines/`, `code-policy.toml` and `af.lock`.

Implementation uses Sol medium, required correctness review uses Sol high, required
bug review uses Terra high, and evaluation uses Astra medium. All provider bindings
are `codex-personal`. The local required Gate is `make check`; sealed execution
requires the native runtime and privately installed CLI replay. There is no remote
Gate or verifier-only Task. Correctness independently builds, privately installs and
executes the sealed candidate inside the implementation Task and returns both the
review result and typed `af/ImplementationTestEvidence@1` bound to that exact Snapshot.
The evaluator requires that testing artifact. `verification=review` matches the public
`af/ReviewedImplementation@1` guarantee; evaluation does not imply publication or
Scenario completion.

Review catalog paths and native digest pins together. Models, credentials, machine
provider registrations and publication remain explicit integration boundaries.
The reconciliation package and its separate fixed budgets are documented in
`docs/af-integration.md`; the coding catalog's larger admission reservation must not
be copied into a reconciliation-only authority.
