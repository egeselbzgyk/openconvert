# Versioning — what to bump, when, and what it costs

OpenConvert carries six version numbers besides the app's. Each one guards something a user or a
cache holds on to across releases, so each one has a rule for when it moves, and the mechanical
rules are enforced by `cargo run -p xtask -- bump-rules-check` (PHASE 15 detail 8, rows 15.16 and
15.17). The goal is that the next release is mechanical rather than remembered.

## The rules

| Version | Where it lives | Bump when | Consequence | Enforced by |
|---|---|---|---|---|
| App semver | `Cargo.toml` `[workspace.package] version`, `apps/desktop/src-tauri/tauri.conf.json` `version` | Any user-visible change | Release notes; an updater manifest (`latest.json`) entry | `bump-rules-check --tag` (the tag must be the tree's version) |
| `ir_version` | `oc_model::IR_VERSION` | An IR field is added, removed or renamed; block-id derivation changes; a `Reason` variant is added | Invalidates the LLM cache and every `overrides.json`; the app refuses a mismatched overrides file with a clear message (Phase 12) | `bump-rules-check` — the IR digest |
| `protocol` (NDJSON `v`) | `oc_core::events::PROTOCOL_VERSION` | An event type or any required field changes; the control channel changes | The GUI hard-errors on mismatch, so engine and app must ship in the same release | `bump-rules-check` — the protocol digest |
| `prompt_version` | `oc_ai::prompt::PROMPT_VERSION` | Any prompt text or grammar edit | Invalidates cache entries and cassettes; forces deliberate re-recording (Appendix B) | `bump-rules-check` — the prompt digest (and `prompts/v1.sha256`, which freezes a released prompt set) |
| job-spec schema | `schemas/job-spec.v<N>.json`, compiled into `oc_core::jobspec` | Any change at all | A **new file** `schemas/job-spec.v<N+1>.json`; the old one is never mutated | `bump-rules-check` — the released file's SHA-256 |
| `models.toml` `schema_version` | `models.toml`, `packs.toml` | Registry shape change | The app refuses an unknown schema version rather than guessing | Review; the registry parsers refuse an unknown version |
| `thresholds.toml` values | `thresholds.toml` | Any value change | A release-note line, because `PROVENANCE` is embedded in every report (D17) | Review; `thresholds-lint` holds provenance and expiry |

## How the check works

`docs/releases/baseline.toml` records the last release: its tag, the five versions above that are
numbers or the app's semver, and a SHA-256 of what each guards. `bump-rules-check` recomputes the
digests from the tree and fails when a guarded thing changed and its version did not move, or when a
version went backwards:

- **IR:** every type, alias and constant in `crates/oc-model/src` (less `IR_VERSION` itself), and
  all of `ids.rs` (block-id derivation) and `canonical.rs` (canonical serialisation).
- **Protocol:** the event constructors in `crates/oc-core/src/events.rs` (less `PROTOCOL_VERSION`),
  every `.emit("<type>", …)` elsewhere in `oc-core` and `openconvert`, and the control channel
  (`crates/openconvert/src/control.rs`).
- **Prompts:** every byte under `crates/oc-ai/prompts/`.
- **Job spec:** the released `schemas/job-spec.v<N>.json`, byte for byte — it must still exist and
  be unchanged, whatever the tree's current schema is.

The code digests are taken over the source *as the compiler sees it*: comments, doc comments,
formatting and `#[cfg(test)]` / `#[test]` items are ignored, so a reworded comment or a new test owes
nothing, and a renamed field, a new variant or an edited event payload does. The rehearsal tests
(`ir_version_bump_is_enforced`, `protocol_bump_is_enforced`,
`prompt_and_job_spec_changes_follow_their_rules`) edit a copy of the tree in each of these ways and
check the verdict.

A digest can change without a user-visible effect — a function in `canonical.rs` refactored with
identical output, say. The rule still asks for the bump: a version bumped once too often costs a
cache rebuild; a version not bumped when it should have been costs users corrupted overrides. When
in doubt, bump.

## When the baseline is written

Only at a release, and only by the release process:

```sh
cargo run -p xtask -- bump-rules-check --tag v1.2.0        # in the release job, before building
# … the release is tagged and published …
cargo run -p xtask -- bump-rules-check --record v1.2.0     # then commit docs/releases/baseline.toml
```

Between releases the baseline is not touched: two changes to the IR in two pull requests owe one
`ir_version` bump between them, because the comparison is always against what last shipped.

Before the first release the baseline says `released = "none"`. Nothing has shipped that a change
could break, so `bump-rules-check` reports drift as a note and passes; `v1.0.0`'s release records the
first real baseline. From then on it is strict.

## Other artefacts a release commits

`docs/releases/<version>/` holds what a release must be able to show later: the reproducibility hash
table (`xtask repro`), the SBOM (`xtask sbom`) and the release manifest with every artefact's SHA-256
(`xtask release manifest`). See `docs/RELEASE_CHECKLIST.md`.
