# AGENTS.md — parity/ (verification oracle)

`parity/` is the cross-cutting verification oracle for Mindustry-Godot: the machine-readable registries, sha256-pinned goldens and their JVM/source provenance, the fixed-pose screenshot manifest, generated gate reports and source-derived audit ledgers. The `mind-headless parity` harness (`client/rust/mind-headless/src/parity/`) validates that state; it never drives the Godot editor and never mutates a registry. It answers one question per file: *is the committed parity state internally consistent and drift-free?*

Read the root [`AGENTS.md`](../AGENTS.md) first. [`README.md`](README.md) owns the file-by-file format/validator table; this file owns the semantics an agent must preserve. The repo-root [`parity-ledger.json`](../parity-ledger.json) is the parity work queue: claim, reproduce and update items there, not here.

## Provenance

- [`upstream.lock`](upstream.lock) pins the upstream repo (`Anuken/Mindustry`), commit `2cd7aeec…`, the capture date and the JDK the JVM goldens came from; it is read-only.
- Oracle goldens under `golden/{logic,ui,io,fx}/` are JVM/source-derived; `golden_content.json`, `asset_manifest.json`, `bundle_keys.json` and `fx_order.txt` are the extra pinned artifacts. `golden_manifest.json` carries every sha256 and the upstream commit.
- `mod_fixtures/` holds 16 fixture dirs in three groups: loader cases (`basic`, `deps`, `zip`, `error`), content/class cases (`allkinds`, `classes`, `consumers`, `effects`, `drawer`, `drill`, `weapon_fx`, `unlock`, `plans`) and asset/patch/script cases (`assets`, `patch`, `scripts`), plus `expected_list.json` and `allkinds/expected.json`; `mod_classmap.json` is the class-mapping golden.
- `ledgers/` holds source-derived audit ledgers (`blocks`, `units`, `combat`, `fx`, `audio`, `mod_fields`); `system_checklist.md` is the per-system status roll-up. `reports/gate_P*.json` are the committed phase evidence.
- `mcp_catalog.json` is the in-engine playtest queue (documentation and validation only); `soak.toml` defines the `mid`, `stress`, `windowed` and `multiplayer` profiles.
- `parity/java/Dump*.java` with `run.sh` produce the JVM oracle; `extract_source_goldens.py` is the reproducible source-derived fallback. CI only reads committed goldens and never needs a JVM.

## Registry semantics

- `matrix.toml` is the upstream-test contract: every upstream method has exactly one `primary = true` row, `owner` is a provenance-only plan label (the `plans/` tree is retired), and every `landed` row's `rust` name resolves in a `cargo test -- --list` dump.
- `checksum_registry.json` is the fold contract. `fold_order` is a permutation of the `active` contributor ids; `planned` and `header` contributors stay out until a deliberate version bump, and `excluded` ids (`render`, `fx`) are view-only.
- `scenario_catalog.json` is the single index of runnable scenarios: every committed `scenarios/*.json` has one entry whose `expect_checksum` matches the file, and every entry is `file`, `embedded` or `planned`.
- `golden_manifest.json` is the sha256 authority for `oracle`, `scenario` and `harness` goldens. A mismatch is drift to repair, not a baseline to refresh.
- `bench_budgets.json` validates per-plan coverage, unique entry ids, non-empty measures and the canonical `sim_core mid` p95 metric; `bench/baselines.json` rows are the recording source referenced by those entries.
- `mcp_catalog.json` and `screenshots/manifest.json` describe editor-gated captures. The harness validates catalog ids/scenes/steps/screenshot references and manifest ids/hashes; it can sha256-verify and tolerance-diff existing PNGs headlessly, but capturing new baselines needs the editor.
- `soak.toml` profiles are bounded on the dev host and full-duration on the nightly perf runner; changing a profile changes the evidence, so record the run that justifies it.

## Invariants

- The harness never drives the editor and never mutates a registry; the only files it writes are gate reports (`--out`) and the explicit `mind-headless parity mirror --write` engine mirror.
- Registry files are read-only inputs. `parity/` goldens are frozen; CI never needs a JVM.
- `checksum_registry.json.checksum_version` equals `mind_core::constants::CHECKSUM_VERSION`; `algorithm` is `fnv1a64`; `fold_order` is a permutation of the `active` contributor ids.
- Any change touching a `checksum_registry.json` contributor bumps `CHECKSUM_VERSION` and re-records every golden in the same commit.
- Every committed `scenarios/*.json` is catalogued, and the catalog `expect_checksum` matches the file's `expect.checksum`.
- Every `golden_manifest.json` entry is sha256-verified; scenario goldens are also byte-compared by their owning scenario.
- Exit-code contract: `0` pass, `1` mismatch, `2` usage/IO.

## Recording and verification

- **Matrix row.** Add exactly one `[[row]]` with `primary = true`; `owner` is the historical plan label (provenance only). `status` is `landed` (needs a resolving `rust` name), `planned`, `planned-ignored`, `no-equivalent` (needs an `oracle` substitute) or `tbd` (needs `tbd_by`). Put the aspirational name in `oracle` when it differs from the real `rust` name.
- **Golden.** Regenerate an oracle golden with the JVM/source tooling in `parity/java/` and `bash tools/regen_goldens.sh`, then update its `sha256` in `golden_manifest.json` in the same commit. A scenario golden is a committed `scenarios/<name>.json` with a `kind = "file"` catalog entry and an `expect_checksum`. Harness goldens under `client/rust/*/tests/golden(s)/**` are pinned `kind = "harness"`.
- **Scenario.** Register it in `client/rust/mind-headless/src/registry.rs` (name `{system}_{case}`) or commit `scenarios/<name>.json`, then add a `scenario_catalog.json` entry with `plan`, `phase` (P0–P8), `tier` (`T0`–`T3`) and `kind` (`file`/`embedded`/`planned`).
- **Gate and bench.** `mind-headless parity gate P<n> --out parity/reports/gate_P<n>.json` runs the CI-safe steps and marks editor/nightly-owned steps `deferred`; commit the report. `bench_budgets.json` carries per-plan budgets and the canonical `sim_core mid` p95 rule; values are recorded on the baseline machine (`bench/baselines.json`).
- **Harness surface.** `mind-headless parity <subcommand>` validates and rolls up the registries; the subcommand inventory is `client/rust/mind-headless/src/cli.rs` (`--help` is authoritative). `check --tests <list>` also resolves `landed` matrix rows. Only `gate --out` and `mirror --write` write files.

## Failure triage

- **Golden hash mismatch** means the committed bytes changed: regenerate the owning golden and update `golden_manifest.json` in the same commit, or revert the bytes. Never edit a hash to match an unreviewed file.
- **Uncatalogued scenario** means a `scenarios/*.json` lacks a `scenario_catalog.json` entry, or its `expect_checksum` disagrees with the file; fix the catalog entry, not the harness.
- **Matrix failure** is a duplicate/missing primary, an unknown status, a `tbd` without `tbd_by`, or a `landed` name absent from the test dump.
- **Checksum-version failure** means `checksum_registry.json` and `mind_core::constants::CHECKSUM_VERSION` diverged; bump both and re-record every golden, or revert the contributor edit.
- **Budget failure** is a missing required plan, a duplicate id, an incomplete canonical metric, or a regression over the recorded baseline rule.
- **Screenshot/MCP failure** is a duplicate id, a missing baseline file, a hash/tolerance miss, or a step/scene reference that no catalogued scenario backs.
- The harness `check` subcommand is the aggregate gate; `report` adds the program roll-up. A green `check` means the committed state is internally consistent, not that the port matches upstream — behavioral evidence lives in the ledger.

## Editing rules

- Intent registries (`matrix.toml`, `checksum_registry.json`, `scenario_catalog.json`, `bench_budgets.json`) are authored by hand; goldens and their hashes come from regeneration.
- Run `bash tools/parity.sh --check` before committing; it covers the registry check and golden hashes, and CI runs the same checks without a JVM.
- Land paired changes together: scenario file + catalog entry, golden + manifest hash, contributor + `CHECKSUM_VERSION` bump.
- Point ledger evidence at its `runs/` directory; do not paste run logs into `parity/`.

## Verification

```bash
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- parity check
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- parity check --tests out/tests.txt
cargo test --manifest-path client/rust/Cargo.toml -p mind-headless
bash tools/parity.sh
bash tools/parity.sh --gate P8
bash tools/regen_goldens.sh --check
```

Run from the repo root (Linux).
