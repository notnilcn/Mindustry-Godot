# AGENTS.md — parity/ (verification oracle)

`parity/` is the cross-cutting verification oracle for Mindustry-Godot. It holds the
machine-readable registries (`matrix.toml`, `checksum_registry.json`,
`scenario_catalog.json`, `golden_manifest.json`, `bench_budgets.json`, `mcp_catalog.json`,
`soak.toml`), the sha256-pinned goldens and their JVM/source provenance, the fixed-pose
screenshot manifest, the generated gate reports and the source-derived audit ledgers. The
`mind-headless parity` harness (`client/rust/mind-headless/src/parity/`) validates those
files. It never drives the Godot editor and never mutates a registry; it answers one
question per file: *is the committed parity state internally consistent and drift-free?*

Read the root [`AGENTS.md`](../AGENTS.md) first.

## Layout

| Path | Responsibility |
|---|---|
| `upstream.lock` | Provenance: upstream repo, commit `2cd7aeec…`, capture date and the JDK the JVM goldens came from; read-only. |
| `matrix.toml` | Upstream-JUnit → primary Rust test mapping (`[[row]]` records). |
| `checksum_registry.json` | `CHECKSUM_VERSION` mirror, contributor ownership and the active `fold_order`. |
| `scenario_catalog.json` | Golden-scenario catalog: `file`, `embedded` and `planned` entries. |
| `golden_manifest.json` | sha256-pinned oracle, scenario and harness goldens plus upstream provenance. |
| `bench_budgets.json` | Cross-crate performance-budget registry and the canonical gate metric. |
| `mcp_catalog.json` | In-engine playtest queue; documentation and validation only. |
| `soak.toml` | Soak profiles (`mid`, `stress`, `windowed`, `multiplayer`). |
| `golden/{logic,ui,io,fx}/`; `golden_content.json`, `asset_manifest.json`, `bundle_keys.json`, `fx_order.txt` | Source/JVM-derived oracle goldens plus the extra pinned oracle artifacts. |
| `java/`; `parity/tools/` | One-off `Dump*.java` (`Content`, `Waves`, `LogicIO`, `Ui`, `JsonIO`, `Fx`), `run.sh`, `extract_source_goldens.py` and the `gen_*.py`/`jvm_golden_diff.py` generators/audits. |
| `screenshots/` | `manifest.json` fixed-pose baselines; `baseline/` holds any committed PNG. |
| `reports/` | Generated gate reports (`gate_P*.json`) plus audit markdown (`jvm_golden_diff.md`, `content_audit.md`, `addon_eval.md`). |
| `ledgers/` | Source-derived audit ledgers (`blocks.md`, `units.md`, `combat.md`, `fx.md`, `audio.md`, `mod_fields.md`). |
| `mod_fixtures/`; `mod_classmap.json` | Mod-loader fixtures (`basic`, `allkinds`, `assets`, `deps`, `patch`, `zip`, `error`), `expected_list.json` and the class-mapping golden. |
| `system_checklist.md`; root `bench/baselines.json` | Per-system status roll-up, and the recorded baseline rows that remain the source of truth for `bench_budgets.json`. |
| `client/rust/mind-headless/src/parity/` | Harness implementation (`matrix`, `registry`, `golden`, `scenario`, `budgets`, `mcp`, `soak`, `checksum`, `determinism`, `screenshot`, `report`, `desync`, `minitoml`). |

## Responsibilities

- Validate and roll up every committed registry; `parity check` and `parity report`
  aggregate the per-file checks into one outcome.
- Pin the simulation port to upstream: the test matrix, the canonical checksum contributors
  and the source/JVM goldens.
- Record gate evidence: `parity gate P<n> --out parity/reports/gate_P<n>.json` writes the
  CI-safe steps and marks editor/nightly-owned steps `deferred`.
- Meter performance coverage: `bench_budgets.json` covers every owning system and the
  canonical `sim_core mid` p95 tick rule; values are recorded on the baseline machine.

## Invariants

- The harness never drives the editor and never mutates a registry; the only files it writes
  are gate reports (`--out`) and the explicit `parity mirror --write` engine mirror.
- Registry files are read-only inputs. `parity/` goldens are frozen; CI never needs a JVM.
- `checksum_registry.json.checksum_version` equals `mind_core::constants::CHECKSUM_VERSION`;
  `algorithm` is `fnv1a64`; `fold_order` is a permutation of the `active` contributor ids.
- Any change touching a `checksum_registry.json` contributor bumps `CHECKSUM_VERSION` and
  re-records every golden in the same commit.
- Every committed `scenarios/*.json` is catalogued, and the catalog `expect_checksum` matches
  the file's `expect.checksum`.
- Every `golden_manifest.json` entry is sha256-verified; scenario goldens are also
  byte-compared by their owning scenario.
- Exit-code contract: `0` pass, `1` mismatch, `2` usage/IO.

## Harness subcommands (`mind-headless parity`)

| Subcommand | Validates |
|---|---|
| `check` | Runs every structural check; `--tests <list>` additionally resolves `landed` matrix rows. |
| `matrix` | Duplicate/missing primaries, missing owner files, bad statuses, unresolved `landed` names, `tbd_by`. |
| `registry` | `checksum_version` vs `CHECKSUM_VERSION`, algorithm, owner, contributor statuses and `fold_order`. |
| `scenarios` | `scenarios/*.json` ↔ `scenario_catalog.json`, names, tiers, kinds and `expect_checksum`. |
| `goldens` | sha256 of every `golden_manifest.json` entry. |
| `budgets` | `bench_budgets.json` coverage per owning system, unique ids, canonical metric. |
| `mcp` | `mcp_catalog.json` ids, scenes, steps, unique screenshots and scenario references. |
| `soak` | `soak.toml` plus a bounded profile run with RSS/alloc/checksum tracking. |
| `checksums` | Each tier scenario twice in-process, twice via replay and once cross-process. |
| `replay-fuzz` | `CommandLog` ordering/dedup/truncation semantics under seeded mutations. |
| `gate` | The CI-safe steps; writes `reports/gate_P<n>.json`; nightly steps are `deferred`. |
| `bench-gate` | Budget coverage and, with `--baseline`/`--canonical`, the canonical regression rule. |

Additional subcommands: `run`, `screenshots`, `mcp-parity`, `report`, `mirror`,
`desync-inject`. Each returns `0` pass, `1` mismatch, `2` usage/IO.

## Adding a matrix row / golden / scenario

- **Matrix row.** Add exactly one `[[row]]` with `primary = true`; `owner` names an existing
  repo-root file. `status` is `landed` (needs a resolving `rust` name), `planned`,
  `planned-ignored`, `no-equivalent` (needs an `oracle` substitute) or `tbd` (needs `tbd_by`).
  Put the aspirational name in `oracle` when it differs from the real `rust` name.
- **Golden.** Regenerate an oracle golden with `parity/java/*` under a JDK and update its
  `sha256` in `golden_manifest.json` in the same commit. A scenario golden is a committed
  `scenarios/<name>.json` with a `kind = "file"` catalog entry and an `expect_checksum`.
  Harness goldens under `client/rust/*/tests/golden(s)/**` are pinned `kind = "harness"`.
- **Scenario.** Register it in `client/rust/mind-headless/src/registry.rs` (name
  `{system}_{case}`) or commit `scenarios/<name>.json`, then add a `scenario_catalog.json`
  entry with the owner, P0–P8 key, `tier` (`T0`–`T3`) and `kind` (`file`/`embedded`/`planned`).

## Verification

```bash
cargo run -p mind-headless -- parity check
cargo run -p mind-headless -- parity check --tests out/tests.txt
cargo test -p mind-headless
tools/parity.sh
tools/parity.sh --gate P8
tools/regen_goldens.sh --check
```

Run from the repo root (WSL2 Ubuntu); `tools/parity.ps1` forwards the same arguments from a
Windows shell into WSL.
