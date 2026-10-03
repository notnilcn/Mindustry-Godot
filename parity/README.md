# `parity/` — plan 23 verification oracle

This directory is owned by [`23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md`](../23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md):
it holds the machine-readable cross-cutting registries, catalogs and goldens, and
the `mind-headless parity` harness that validates them.

The harness never drives the editor and never mutates a registry. It answers one
question per file: *is the committed parity state internally consistent and
drift-free?*

## Files

| File | Format | Owns | Validate with |
|---|---|---|---|
| `upstream.lock` | JSON | Mindustry commit + JDK the JVM goldens were captured from | read-only provenance |
| `matrix.toml` | TOML subset | every upstream JUnit test → its primary Rust test | `parity matrix` |
| `checksum_registry.json` | JSON | `CHECKSUM_VERSION` + contributor ownership | `parity registry` |
| `scenario_catalog.json` | JSON | golden-scenario catalog (files + embedded suites) | `parity scenarios` |
| `golden_manifest.json` | JSON | committed oracle/scenario goldens + sha256 | `parity goldens` |
| `bench_budgets.json` | JSON | aggregated performance budgets across plans | `parity budgets` |
| `mcp_catalog.json` | JSON | in-engine playtest queue (documentation only) | `parity mcp` |
| `soak.toml` | TOML subset | soak profiles | `parity soak --profile <p>` |
| `golden/` | JSON | source/JVM-derived oracle goldens (`logic`, `ui`, `io`, `fx`) | `parity goldens` / `tools/regen_goldens.sh --check` |
| `screenshots/` | JSON + PNG | fixed-pose MCP baseline manifest | `parity screenshots` |
| `reports/` | JSON (generated) | gate reports (`gate_P*.json` committed) | `parity gate <phase> --out ...` |
| `../bench/baselines.json` | JSON | recorded baseline + every plan budget row | `parity bench-gate --baseline bench/baselines.json --canonical` |

Run everything at once:

```
cargo run -p mind-headless -- parity check                 # structural
cargo run -p mind-headless -- parity check --tests out/tests.txt   # + landed-row resolution
```

Exit codes match the harness contract: `0` pass, `1` mismatch, `2` usage/IO.

## Adding an upstream-test row (`matrix.toml`)

1. Add exactly one `[[row]]` with `primary = true` for the upstream method.
2. Set `owner` to a plan file that exists in the repo root.
3. `status` is one of `landed`, `planned`, `planned-ignored`, `no-equivalent`, `tbd`.
   - `landed` requires a `rust` name that resolves in `cargo test -- --list`.
   - `no-equivalent` requires an `oracle` naming the substitute (and may leave `rust` empty).
   - `tbd` requires `tbd_by = "<plan file>"`.
4. Put the exact plan-§7a name in `oracle` when it differs from the real `rust` name.
5. Run `parity matrix` (or `check`) — it fails on duplicate primaries, missing owner
   files, unknown statuses, or (with `--tests`) unresolved landed names.

CI proposes:

```
cargo test -p mind-core    -- --list > out/tests-core.txt
cargo test -p mind-headless -- --list > out/tests-headless.txt
cat out/tests-core.txt out/tests-headless.txt > out/tests.txt
mind-headless parity matrix check --tests out/tests.txt
```

## Adding a golden

- **Oracle golden** (JVM-derived): regenerate with `parity/java/*` + JDK 17, then
  update `golden_manifest.json` (`sha256`) in the *same commit*. CI never needs a JVM.
- **Scenario golden**: commit `scenarios/<name>.json`, add a `kind = "file"` entry to
  `scenario_catalog.json`, and record `expect_checksum` from the file.
- **Harness golden** (Rust self-recorded): every file under
  `client/rust/*/tests/golden(s)/**` is pinned in `golden_manifest.json` with
  `kind = "harness"` and the owning plan. `parity goldens` sha256-checks it in
  addition to the owning scenario's byte comparison.
- Any checksum change that touches a `checksum_registry.json` contributor must bump
  `CHECKSUM_VERSION` and re-record every golden in the same commit (R-02).

`parity check` fails if a committed `scenarios/*.json` is not catalogued, if a catalog
`expect_checksum` disagrees with the file, or if a manifest hash drifts.

## Adding a scenario

1. Register the scenario in `client/rust/mind-headless/src/registry.rs` (name =
   `{system}_{case}`) or add a committed `scenarios/<name>.json`.
2. Add a `scenario_catalog.json` entry with `plan`, `phase`, `tier` (`T0`–`T3`) and
   `kind` (`file` | `embedded` | `planned`). `embedded` entries name the producing
   command (e.g. `blocks scenario place_construct_destroy --json`).
3. `parity scenarios` cross-checks files ↔ catalog.

## MCP catalog + recipe vocabulary

`mcp_catalog.json` documents the queued in-engine scenarios. `steps` use either
`godot_exec call` methods or this shared vocabulary so `tools/mcp-smoke.sh` can run a
subset mechanically:

`load_scenario`, `set_paused <bool>`, `step <n>`, `eval <expr>`, `place_block <x> <y> <block>`,
`break_block <x> <y>`, `mouse place <x> <y>`, `inspector label`, `camera <pose>`,
`screenshot`, `godot_log errors`.

Node paths and pid-stamp rules are authoritative in
`00_FOUNDATION_IMPLEMENTATION_PLAN.md` §3.5 and `.opencode/skills/playtest/SKILL.md`.

## Determinism (M1)

```
parity checksums --suite smoke            # 2 in-process + 2 replay + 1 cross-process per scenario
parity checksums --suite gate --workers 1,4 --json
parity replay-fuzz --seed 7 --mutations reorder,dup,truncate
```

`checksums` compares the canonical FNV-1a checksum across worker counts and a fresh
child `mind-headless replay`; `replay-fuzz` mutates a known `CommandLog` to prove the
plan-05 ordering/dedup/truncation contract (reorder/truncate change the checksum;
duplicates are applied faithfully). The cross-OS `determinism-compare` job is nightly.

## Oracle goldens (`parity/golden/`)

The `parity/java/Dump*.java` one-off JVM tools are the primary oracle; the committed
`parity/golden/{logic,ui,io,fx}/` files are pinned in `golden_manifest.json` and verified
by `parity goldens`. `tools/regen_goldens.sh --check` verifies every hash without a JVM;
`--only <id>` regenerates one (JVM when `MIND_JAVA_PARITY=1`, else the reproducible
source-derived `parity/java/extract_source_goldens.py`). See `golden/README.md`.

## Performance budgets

`bench_budgets.json` is the cross-crate coverage registry; per-plan
`bench/baselines.json` files remain the recording source of truth. The canonical
gate metric is `sim_core mid` p95 tick time; a **>10% regression blocks a phase gate**
(NUD-39/A), while subsystem micro-benches use absolute budgets (+ plan 00's
warn/fail policy until a budget is met). `parity bench-gate` validates coverage in CI;
values are recorded on the baseline machine / self-hosted `perf` runner only.

## Phase gates

`parity gate Pn --out parity/reports/gate_Pn.json` runs the steps that are safe in CI
(matrix, checksum registry, scenario/golden/budget/MCP catalogs) and marks the
nightly-owned steps (`T1` suite, checksum matrix, MCP parity, bench-gate) as
`deferred`. Commit `gate_P*.json` as the phase evidence of record.

## Local suite driver

`tools/parity.sh` (`.ps1` twin) mirrors the runnable subset of the nightly runner on
the dev host without faking the deferred steps:

```
tools/parity.sh                      # default: structural check --tests + smoke suite
tools/parity.sh --suite gate --phase P5
tools/parity.sh --gate P8            # writes parity/reports/gate_P8.json
tools/parity.sh --check              # registries + landed-row resolution + golden sha256
tools/parity.sh --mcp                # headless half of mcp-parity (in-engine is editor-gated)
tools/parity.sh --bench              # bench-budget coverage
tools/parity.sh --soak mid           # bounded soak slice (full duration nightly-owned)
```

The T1/T2 suites, checksum matrix, in-engine MCP capture and full soak durations stay
on the self-hosted nightly runners (NUD-40/A).

See [`system_checklist.md`](system_checklist.md) for the per-system status roll-up.
