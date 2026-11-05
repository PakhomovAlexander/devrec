# devrec

`devrec` is a standalone Rust CLI for deterministic development records: changelog entries, ADRs, documentation, and performance notes. The core has no AF runtime or model dependency. Native AF review is an explicit subprocess integration described in [docs/af-integration.md](docs/af-integration.md).

Build and check on macOS or Linux with Rust stable and Python 3:

```sh
make check
make build
make install
```

Builds, dependency downloads, temporary test repositories, and the private installation stay under `target/`. The executable is `target/release/devrec`; `make install` also creates `target/install/bin/devrec`.

Write a repository-contained JSON input, for example `entry.json`:

```json
{
  "schema_version": 1,
  "id": "add-export",
  "revision": 1,
  "kind": "changelog",
  "title": "Add export support",
  "body": "Exports preserve stable record anchors.",
  "depends_on": [],
  "supersedes": [],
  "conflicts_with": [],
  "metadata": {}
}
```

Then run:

```sh
devrec new entry.json
devrec check
devrec render --nightly
devrec release prepare v1
devrec release request v1
# Configure and run native review using docs/af-integration.md.
devrec release review v1
devrec release finalize v1
```

`--root DIR` precedes the command. Failures exit nonzero and emit JSON on stderr with `schema: "devrec/error@1"`, `code: "operation_refused"`, and a diagnostic `message`. `new FILE` accepts an empty `id` to allocate a random independent ID without a shared counter or addition manifest. Identifiers use lowercase ASCII letters, digits, hyphens, and underscores so independent records remain distinct on case-insensitive filesystems. Each pending record gets its own `.devrec/fragments/ID.json`. Existing IDs never overwrite a different record. `check` validates baseline plus pending records; rendering uses that same complete effective set. Nightly rendering writes nothing and consumes nothing.

Revisions have independent IDs and an explicit `revises` predecessor, the same kind, and exactly predecessor revision + 1. Use `revise FILE`. A retirement is a revision with `retired: true` and a nonempty body explaining why. Historical records retain their anchors and content with an explicit superseded status. Competing revisions do not choose a winner: every competing pair needs an explicit common successor listing both in `supersedes`. Supersession preserves record kind; only an ADR can supersede another ADR. References must resolve; dependency, revision, and supersession cycles fail. Independent record additions render in the same order regardless of arrival order.

Accepted ADR records are immutable. To change a decision, create an ADR with a new ID, `supersedes: ["old-decision"]`, and the rationale in its body. The predecessor and links remain in the rendered record. Performance source evidence is pinned separately from fragment prose; review reconciliation cannot change source authority or accepted ADRs.

Documentation records may have named `sections`, each with `id`, `title`, `body`, and `after` section IDs. Sections render in a deterministic topological order and get `section:record-id:section-id` anchors. Unknown section relationships and cycles fail. Top-level `depends_on` expresses ordering between records. Relationships and sections are validated before writing an addition.

An optional `.devrec/config.json` protects exact repository-relative source files:

```json
{
  "schema_version": 1,
  "protected_sources": ["Cargo.toml", "src/lib.rs"],
  "review_adapter": null,
  "review_adapter_digest": null
}
```

Set `review_adapter` to an installed absolute executable and `review_adapter_digest` to its `sha256:` digest, following the integration guide. Review requires that pinned executable; the CLI has no command to import arbitrary review JSON. Every protected path is inventoried with its byte size and SHA-256 after version/source changes. Configuration itself is pinned. Changing the config, baseline, protected source, or any consumed record makes the stage stale. Additions arriving after prepare remain pending and can reference consumed predecessors, because the accepted baseline preserves those records.

Release preparation saves an immutable plan under `.devrec/releases/`; `release request` emits the exact native review request without executing it. The request includes only consumed records and their reference closure, affected successor decisions, the corresponding prior baseline records, and the protected source inventory. Unrelated accepted records remain outside model scope. Review stores verified native run/task/revision/result identities. Finalization validates the exact stage and evidence, writes a durable transaction journal, publishes baseline and rendered output, removes only the exact staged records, and records completion. It is idempotent. If interrupted, rerun `release finalize ID` to finish the same transaction. A changed consumed record or protected source blocks recovery while preserving the journal. Reads refuse unfinished release state rather than exposing a partially committed view.

Writers hold a nonblocking exclusive advisory lock on the retained `.devrec/lock` inode. The operating system releases the lock when the writer exits or crashes, so `release finalize ID` can retry directly. The file retains a diagnostic PID; leave the lock file in place to preserve mutual exclusion between writers. Storage is rooted in a directory descriptor with no-follow traversal, bounded regular-file reads (8 MiB), at most 10,000 entries per pending directory, 32 MiB aggregate pending input and 64 MiB protected source input, same-directory atomic replacement, and file/directory synchronization. The core forbids unsafe Rust. The review adapter is bounded to 30 seconds and 64 KiB output; its process group is cleaned up on every exit. The repository and the configured executable are trusted local state; review receipts are not a remote signature scheme.

Contracts live in [schemas/fragment-v1.json](schemas/fragment-v1.json) and [schemas/config-v1.json](schemas/config-v1.json). Native integration contracts and their implementation limitations are documented separately; this CLI never commits, pushes, opens a PR, merges, or publishes a remote release.

This implementation does not yet satisfy the full requested lifecycle and measurement protocol. Full `impl` orchestration and structured reconciliation correction records are absent. Performance fragments currently hold prose/string metadata and deterministic source inventory; they do not validate typed measurement provenance or infer fresh measurements from an evidence file. The native pipeline's exact model/provider policy requires operator inspection, and no live native model run was performed. See the integration guide for the explicit authority gap and no-change-only bridge. Subprocess/install contracts are in [schemas/review-boundary-v1.json](schemas/review-boundary-v1.json).
