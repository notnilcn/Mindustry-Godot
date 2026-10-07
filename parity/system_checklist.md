# System parity checklist

Roll-up of `parity report` at the current branch tip. Status vocabulary
(plan 23 §3.7): `parity-complete | in-progress | deviation | deferred(owner) | not-started`.

"Parity complete" for a system means: its `matrix.toml` rows are landed/green (or
explicitly `no-equivalent` with a substitute oracle), its promised scenarios exist,
its goldens verify, and its budget rows are recorded. MCP rows are deferred on the
single-editor mutex until the nightly `windows-gpu` runner is wired (NUD-40/A).

| System | Plan | Phase | Upstream rows | Scenarios | Budgets | Status | Evidence |
|---|---|---|---|---|---|---|---|
| Foundation | 00 | P0 | 5 (timers, createMap) | 4 file | 2 | in-progress | `matrix.toml`, `scenarios/spine_*`; P0 gate passed (HLP §13) |
| Platform/STDB | 01 | P1 | 0 upstream (behavior) | 3 file | 2 | in-progress | `stdb_*` scenarios; §7.3 two-instance MCP deferred |
| Content | 02 | P1 | 2 | 4 embedded | 1 | parity-complete | `content::parity::tests`; JVM golden `parity/golden_content.json` |
| Assets | 03 | P1 | 0 upstream | 4 embedded | 2 | in-progress | `assets regions` 3092/3092; pack byte-deterministic; MCP deferred |
| IO/serialization | 04 | P1 | 10 | 4 embedded | 2 | parity-complete | `io::*` tests; `.msav` import declined (OD2) → `no-equivalent` |
| Sim core | 05 | P2 | 10 | 4 file + 1 embedded | 3 | parity-complete | canonical FNV-1a checksum; `checksum_registry.json` v1 |
| World/terrain | 06 | P3 | 10 | 4 embedded | 1 | in-progress | planet/asteroid generators + `save_map` open |
| Blocks/build | 07 | P3 | 8 | 4 embedded | 4 | parity-complete | all 447 blocks; `blocks bench` P95 |
| Logistics | 08 | P3 | 5 | 1 embedded | 2 | parity-complete | alloc-free hot path; `logistics_golden.rs` |
| Power/liquids/heat | 09 | P3 | 9 | 4 embedded | 3 | parity-complete | `network_scenarios` goldens; full generator family |
| Combat/bullets | 10 | P4 | 0 upstream (substitute) | 2 embedded + 8 MCP | 1 | in-progress | ammo for 13 turrets; remaining Serpulo/Erekir ammo + MCP |
| Units/AI/waves | 11 | P4 | 4 | 2 embedded | 2 | in-progress | controllers/pathfinder/waves; plan-12 seams open |
| Campaign | 12 | P4 | 6 planned | 2 planned | 0 | not-started | `campaign::tests` names reserved in matrix |
| Logic/mlog | 13 | P5 | 15 planned | 2 planned | 0 | not-started | `logic::*` names reserved; 0 rows landed |
| UI | 14 | P5 | 0 upstream | 1 planned | 0 | in-progress | in-engine MCP eval at 1152x648: `boot_menu`/`settings_ui`/`ui_dialogs` (`.opencode/evals/runs/20261006-*`); 28 findings twin-verified via `.opencode/evals/findings.json`; `ui_text` scenario pending |
| Input/RTS | 15 | P5 | 3 placement-adjacent | 2 planned | 0 | in-progress | in-engine MCP: GUI-consumed input (EV-0018) and the Rebind Keys dialog (EV-0023) verified; `input_place_line_headless` pending |
| Render/world | 16 | P6 | 0 upstream | 4 embedded | 2 | in-progress | render-list goldens; MCP per-layer deferred |
| FX/parts | 17 | P6 | 0 upstream | 1 planned | 0 | not-started | `fx_lifecycle` promised |
| Audio | 18 | P6 | 0 upstream | 3 embedded | 2 | parity-complete | `audio_golden.rs`; `bench/baselines.json` audio rows |
| Maps/editor | 19 | P7 | 0 upstream | 1 planned | 0 | not-started | plan 19 not landed |
| Mods | 20 | P7 | 37 (DataAsset+Patcher) | 2 embedded | 1 | parity-complete | `mods::patch::tests::*`; `mod_fields.md` ledger |
| Multiplayer | 21 | P8 | 0 upstream | 1 planned | 0 | not-started | relay/desync plan not landed |
| Platform/export | 22 | P7 | 0 upstream | 1 planned | 0 | in-progress | `mind-headless server` landed (M2); export gated |

Counters at this tip: matrix **75 landed / 9 no-equivalent / 16 planned**;
scenarios **11 file / 38 embedded / 11 planned**; MCP **18 documented** (2 with a
backing scenario check pending the windowed runner); budget rows **30** across 15 plans.

## Known deviations carried here (see plan §8.3)

- `.msav` legacy import declined → `load*Save` rows are `no-equivalent` (OD2/NUD-02).
- Java power tests used `delta = 0.5`; Rust goldens use `delta = 1.0` (09 R6).
- Visual/terrain parity is structural + Rust-recorded goldens (HLP §9).
- MCP runs are local/self-hosted only; never required on shared CI (NUD-40/A).
- In-engine MCP evaluation runs locally via `.opencode/evals/` (one client per display); the UI/Input rows above are set from those `runs/` artifacts and `findings.json`, not code inspection.
- `tools/ci.sh`'s bench step enforces the baseline-machine `bench/baselines.json` values (`bench_baseline` p99 1 µs, +50%); on the current loaded Linux host the release bench measures ~9× that (p50 1188 ns / p99 2375 ns, `baseline_status: fail`), so the bench gate is red for environment reasons, not a sim regression. Every other `tools/ci.sh` step (fmt/clippy/tests, parity/goldens/T0, mirror, Godot import, boundary, mind-stdb/spacetimedb, STDB drift) passes at this tip.
