# 24 — DEFERRED ITEMS SWEEP PLAN

> **Status:** ACTIVE — compiled 2026-10-05 from the current plan files, `HIGH_LEVEL_PLAN.md` §3/§13 (through F31), and a six-lane verification pass. Tracks every open/deferred item in plans `01`–`23` that blocks their archival. Plan `00_FOUNDATION_IMPLEMENTATION_PLAN.md` is complete and already archived at `plans/archived_plans_llms_do_not_read.zip`.

| Field | Value |
|---|---|
| **Phase** | Post-P8 verification/sweep — no new systems, only closures. |
| **Depends on** | plans `01`–`23` on disk; `HIGH_LEVEL_PLAN.md` §13 (F27–F31); `parity/**` registries; Godot 4.7.2 + MCP bridge; SpacetimeDB 2.10.2 for live gates. |
| **Blocks** | Archival of plans `01`–`23`. |
| **Sources** | Each `{01..23}_*_IMPLEMENTATION_PLAN.md` (status row, checklists, Changelog); `HIGH_LEVEL_PLAN.md` §3 + §13; `parity/reports/jvm_golden_diff.md`; six-lane verification pass 2026-10-05. |
| **Extends spine** | N/A — this is a tracking/sweep plan. |

## 1. Archival rule

A plan is archivable only when it has:

1. Every milestone complete.
2. No explicit `Deferred` / `Open` / `Remaining` item owned by it.
3. Every exit-criteria checkbox checked (or explicitly waived by the user).
4. Every §7c/§7d acceptance run executed (or explicitly waived).

Item classes used below:

| Class | Meaning |
|---|---|
| ENV | Blocked by this environment/session: single-editor MCP mutex, no-pixel compositor (in-engine checksum matches but screenshots are blank), no live STDB in older lanes. |
| PLATFORM | Needs per-OS export templates / per-platform GDExtension builds. |
| PERF | Budget or baseline not measured/recorded (or over budget). |
| FEAT | Code/parity residual. |
| DECIDE | Open `NEEDS USER DECISION` / OD item. |
| BOOK | Stale status/checkbox only; no code work. |

Most items are **ENV** — core implementation is essentially complete. The gate is green: `cargo test -p mind-core` 1727 passed / 4 ignored, `mind-headless`/`mind-stdb` green, `parity check --tests` 6/6 (100 matrix rows / 91 landed, 1888 names; catalog 156, golden_manifest 97, bench_budgets 59, mcp_catalog 26), `run-all --tier T0` 4/4, `tools/ci.sh == ci: OK ==`.

## 2. Environment facts (blockers to schedule around)

- **Single-editor mutex:** the MCP bridge binds `127.0.0.1:6970`; only one Godot editor may hold it. Nearly every plan's §7c run needs a serialized slot.
- **No-pixel compositor:** the bridge connects and reproduces the P0 golden (`spine_place_break` checksum `a1a7b96167c9718d`), but the headless/VM compositor emits no pixels, so no screenshot has been promoted (F31). Visual verification is blocked until a GPU/display-capable runner is used.
- **Live STDB:** `spacetime start` on 2.10.2 is available; the four env-gated `MIND_STDB_IT=1` tests pass (F31). Only the two-instance MCP/admin-UI gates and the long soak remain.
- **Exports:** only Windows templates are installed; Linux/macOS/Android/iOS need per-platform GDExtension builds (Android SDK + template, iOS on macOS).
- **`units bench --units 300`** release p50 ≈ 363 ms/tick vs the §7d 1.5 ms budget — pre-existing, filed in F31, not fixed, not baselined.

## 3. Aggregate open items by plan

| Plan | Code state | Open items | Classes |
|---|---|---|---|
| 01 STDB | M0–M6 merged; live ITs pass | §7.3 two-instance MCP; relay-throughput bench; §7.4 budget partial; 2 unchecked boxes | ENV, PERF, BOOK |
| 02 Content | Complete (447 blocks / 65 units) | in-engine MCP inspector; `BlockDef.kind_data` ownership (R2); 56 JVM block-field rows + plan-10/19 deltas; 1 unchecked box | ENV, DECIDE, FEAT, BOOK |
| 03 Assets | M0–M10 (5135 regions) | §7.1c MCP probe; `bundles sync`; WOFF→TTF; `assets-pack` CI wiring; R10/R12; 1 unchecked + 5 partial boxes | ENV, FEAT, DECIDE, BOOK |
| 04 IO | M0–M6 + M8 budgets | `MindIo` autoload/dev actions; §7c save→load MCP; groundZero size rows; loaded-building ECS rebuild; 8 stale boxes | ENV, FEAT, PERF, BOOK |
| 05 Sim core | M0–M9 (M9 in progress) | M9 in-engine MCP (pause/resume, `IoSet`); header stale; all 10 exit boxes unchecked | ENV, BOOK |
| 06 World | M0–M8, M9 partial | §7c MCP; release-machine §7d baseline; `BaseGenerator` ruins + `postGenerate` walls; Rules subset | ENV, PERF, FEAT |
| 07 Blocks | M0–M8 | §7c MCP; R2 `kind_data` ownership + overlay removal; release §7d sign-off; drill/beam live tile mutations; 3 verification handshakes; 9 unchecked boxes | ENV, DECIDE, PERF, FEAT, BOOK |
| 08 Logistics | M0–M8 headless | §7.3 MCP (two screenshots); `bench logistics` criterion missing; unit payloads; `instantDeconstruct`; R2/R3 | ENV, PERF, FEAT, DECIDE |
| 09 Power/liquids/heat | M0–M6 | M6 MCP + inspector tab; R4 exact quadtree; p99 over budget; 4 behavior gaps; module IO + determinism replay; 12 unchecked boxes | ENV, FEAT, PERF, BOOK |
| 10 Combat | M0–M9 | §7c MCP; `scathe.spawnUnit`; sublimate activation window; per-tick `update_tile`; Fire/Puddle save-registry entries; 13 unchecked boxes | ENV, FEAT, BOOK |
| 11 Units/AI/Waves | M0–M8 headless | §7c MCP + `bench-air`/`bench-path`; `floorSpeedMultiplier`→velocity; basepartnames golden; controller-codec/factory revisions; `Payload`; OD-11-A; 14 unchecked boxes | ENV, PERF, FEAT, DECIDE, BOOK |
| 12 Campaign | M0–M9 headless | ECS-schedule registration + `trace order`; `Rules` in `Sim::checksum` (R5); `sector_save_load_turn`; `schematic_place` golden; §7c MCP; auto-unlock residual; header stale | FEAT, ENV, BOOK |
| 13 Logic/mlog | M0–M8 | §7c MCP; §7d logic bench missing; tileable BFS/`commandImage`; entity/audio/FX sink application; live `logic_place` probes; radar cache | ENV, PERF, FEAT |
| 14 UI | M0–M8 partial | Java-oracle goldens; in-engine `ui_sweep` + prompts/HUD/minimap/mobile; two-client relay; file chooser; §7d budgets; theme build; M4 unchecked | ENV, PERF, FEAT |
| 15 Input/RTS | M0–M7 | §7c MCP + §7d; real `BuilderComp` call-through; handler/focus parity; snapshot/camera verification; `Payload`; U1/U3 | ENV, FEAT, DECIDE |
| 16 Render | M0–M10 | §7.1 inventory listing; in-engine stage trace; MCP 7c-1/2/3; fixed-pose visuals; GPU budgets; region/atlas execution; `world/draw` remainder; docs | ENV, PERF, FEAT |
| 17 FX | M0–M7 (267/267) | §7c MCP; §7d in-engine profiler; region/atlas binding; draw-call executor; 03/16/21 reconciliations; playtest docs | ENV, PERF, FEAT |
| 18 Audio | M0–M6 | §7c MCP (bus layout/pause/audition) + mcp-smoke audio steps; persistent loop voices; gdext poller wiring | ENV, FEAT |
| 19 Maps/editor | M0–M8 | §7c C1–C3 + dialog open; SubViewport render/world preservation; `Gd<ImageTexture>`; live config shift; async preview worker; body views | ENV, FEAT |
| 20 Mods | Data/service complete | MCP dialog smoke; release perf run; 1 unchecked + 3 partial boxes | ENV, PERF, BOOK |
| 21 Multiplayer | M0–M8 | large-row spike; §7c two-instance + SQL; 30-min live soak; admin console/UI reachability; late-join live timing; `--profile-json`; unticked milestones | ENV, FEAT, BOOK |
| 22 Platform/export | M0–M5 + Windows | Android/iOS + Linux/macOS/web exports; assets in exports; live two-client + association gates; §7c capture; 13 unchecked boxes | ENV, PLATFORM, FEAT, BOOK |
| 23 Parity | Continuous | windowed soak; in-engine capture/nightly; JVM re-dump; ~15 null baseline rows; registry CI; 13 unchecked + 4 partial boxes | ENV, PERF, BOOK |

## 4. W1 — Unblock the verification environment

Precondition for W2 and most of W4–W6.

- **W1.1** Stand up a GPU/display-capable Godot runner (or fix the VM compositor) so `godot_screenshot` produces pixels; re-run the F31 golden-pose check.
- **W1.2** Serialize the single-editor mutex: a documented MCP sweep driver that launches exactly one editor, runs plan §7c scripts in order, stores pid-stamped evidence, and tears down.
- **W1.3** Extend `tools/mcp_smoke.py` beyond steps 1–6/9: fix the mouse-input timeout (F28), add per-plan §7c recipes, add audio/UI/net steps.
- **W1.4** Keep a local SpacetimeDB 2.10.2 running for M0 spike, two-instance MCP, admin console, and soak work.
- **W1.5** Record on the baseline machine (perf rows + nightly ownership, NUD-40/A).

## 5. W2 — In-engine §7c MCP sweep (per plan)

The bulk of deferred items. Each plan needs its own scripted run; all require W1.1/W1.2.

- **01:** two-instance relay `stdb_relay_roundtrip_2p` + SQL outputs; relay-throughput bench (this one is PERF).
- **02:** content inspector tab open + screenshot; MCP inspector re-record after content changes.
- **03:** `§7.1c` assets probe (atlas/FileTree) using evals in `assets/parity/mcp_assets_scenario.md`.
- **04:** save→load digest equality + screenshot (via F30 `SimIoHandler`); `MindIo` autoload slot/list/preview UI; SaveDialog/LoadDialog round trip (also plan-14 step 12).
- **05:** M9 pause/resume + `IoSet` save/load proof.
- **06:** generated terrain visible + camera pan; checksum/log/screenshot evidence.
- **07:** block/building runtime evals (place → finish construct → config → footprint/edge-break → screenshot, clean logs).
- **08:** §7.3 logistics scenario with two screenshots.
- **09:** M6 MCP + `MindSim` network inspector tab (power/liquid/heat).
- **10:** §7c exact HP/damage numbers + screenshot.
- **11:** §7c logs, position deltas, screenshot; relay command path exercised.
- **12:** §7c inspector state + screenshot + logs.
- **13:** §7c display screenshot + command-queue evidence; live `logic_place`/`logic_set_code` probes.
- **14:** full `ui_sweep` (all 35 dialogs / 14 fragments), prompt/text-input desktop+mobile, HUD-vs-inspector diff, minimap pan/zoom, theme build from manifest; two-client plan-21 relay run.
- **15:** §7c MCP + `tools/mcp-smoke` input steps + screenshots.
- **16:** MCP 7c-1/2/3 per-layer screenshots; `stage_trace` order; fixed-pose shadows/darkness/light/fog/minimap/pixelator/bloom.
- **17:** §7c fixed frames with `sample_part`/`dump_effects` matching the headless golden; in-engine profiler.
- **18:** §7c bus layout, dialog-pause lowpass, audition; mcp-smoke audio steps.
- **19:** MCP C1/C2/C3 (`editor_line`/`editor_undo`/`editor_reload`/`editor_gen_preview`), dialog open/close + inspector Editor tab; editor SubViewport mount/unmount + world preservation.
- **20:** mods dialog/browser/inspector/error-case screenshots.
- **21:** two-instance MCP + SQL evidence; admin kick/ban/trace/wave/team/chat-filter reachability from console (22) and UI (14).
- **22:** §7c in-engine capture; M5 association/drag-drop gates; M3 live two-client gate.
- **23:** in-engine capture + screenshot baselines; `parity mcp-parity`/`mcp-soak` interactive half.

## 6. W3 — Platform exports

- **22 M6:** Android export + touch smoke (or documented NOT-RUN); iOS preset with NOT-RUN note (iOS was cut per OD-P; keep the documented posture).
- **22 M7:** Linux/macOS presets; web statement (`docs/platform/web.md`); per-platform GDExtension builds; §7d budget measurements.
- **22 assets:** ship `assets/` in exports (currently falls back to placeholder quads).
- **22 docs:** `docs/platform/{steam-parity,discord,store-packaging}.md`, `GameService` no-op proof (Steam/Discord are user-locked deferrals, not work).

## 7. W4 — Functional/parity residuals

### 02 Content
- Triage-close the 56 block-field rows and the plan-10 turret bullet/unit + plan-19 sector-preset deltas in `parity/reports/jvm_golden_diff.md` (or record user-accepted deviations).

### 04 IO
- Entity/building ECS rebuild on load (`Context::pending_buildings` needs the plan-07 sink).
- `groundZero`/Serpulo real-map save size ≤ 500 KB + timings (needs plan-06 map).

### 06 World
- Full `BaseGenerator` ruins placement + `postGenerate` wall fixes + `placeLaunchLoadout` (needs generation ECS context).
- Finish planet-generator `Rules` subset (Serpulo waves/spawns landed F28).

### 07 Blocks
- Adopt/remove the 07 interim `BlockKindData` overlay once plan 02 owns `BlockDef.kind_data` (R2).
- Drill/beam mining that needs live tile mutations from plans 11/16.

### 08 Logistics
- Unit payloads (`ai/types/cargo.rs` `tryDropPayload`) once plan 11's cargo runtime lands.
- `instantDeconstruct` remains Prop-only (plan-02 scope — decision needed or accepted).

### 09 Power/liquids/heat
- R4 exact quadtree (currently deterministic `(is-node, dst2, index)` scan).
- `dumpLiquid(outputLiquid)` call site; register `LiquidBridge`/`DirectionLiquidBridge` behaviors.
- `itemDurationMultipliers` (pyratite 3×, phase-fabric 15×); armored-conduit proximity `acceptLiquid`.
- Module IO parity (04/07) + `power_network_determinism` replay.

### 10 Combat
- `scathe.spawnUnit` (`MissileUnitType` + init path — plan 11).
- `sublimate` liquid activation window.
- Per-tick `BuildingBehavior::update_tile` + plan-11 `TargetQueries` integration (F24 rewired targeting; `update_tile` is still a no-op).
- Append `FireComp`/`PuddleComp` to plan-04 `io/entity/registry.rs` so their codecs are reachable from the save entity chunk.

### 11 Units/AI/Waves
- Multiply `floorSpeedMultiplier` into velocity (`UnitComp.update`).
- `unitMoveBreakable` deconstruct landed in F31 (`TankComp.apply_crush` + harness removal) — verify and close the plan note.
- Add `basepartnames`/`BaseRegistry` tier-sort headless golden.
- Thread `state.rules.unitDamage(team)` + `disarmed` status gating.
- `SimCommand::Payload` stays `Unsupported` (player `Payloadc` unported); `UnitCargoUnloadPoint` selection simplification.
- Controller codec / unit-factory save revisions are plan 04/21-owned — confirm handoff closed.

### 12 Campaign
- ECS-schedule registration of campaign systems + `trace order` golden (currently harness-driven).
- `Rules` in `Sim::checksum` (R5) — joint `CHECKSUM_VERSION` bump with 05/23 + global golden re-record.
- Real `sector_save_load_turn` (plan-06 world + plan-04 write context) and the `schematic_place` golden.
- `check_auto_unlocks` explicit-list residual.

### 13 Logic/mlog
- Tileable BFS linking + `commandImage` display-index resolution (render half in 16).
- Host-side application of entity/audio/FX event sinks.
- Live `logic_place`/`logic_set_code` state once plan 05/07 expose live logic buildings.

### 15 Input/RTS
- Real plan-11 `BuilderComp` call-through (the `input::queue::BuildQueue` seam stands in).
- Runtime reachability of desktop+mobile handlers, `locked()`/focus-guard branch parity.
- Plan-snapshot/mobile select-plan and camera-math/hook verification.
- `Payload` sim-apply (blocked on the `Payloadc` carrier).

### 16 Render / 17 FX
- In-engine region/atlas execution + remaining `world/draw/*` descriptor chains (headless `draw_desc`/layers landed F30).
- Full region render-list via plan-03 menu region-name resolution.
- 17: region/atlas binding + loose-texture lookup; in-engine profiler; reconcile 03 `--regions` / 16 ownership / 21 exclusion.
- 17/16: live draw-call executor gate (F31 models 4/200 via `fx::batch::batched_draw_call_count`).

### 18 Audio
- Persistent loop voices; gdext ambient-poller wiring once the sim/view snapshot exists.

### 19 Editor
- `Gd<ImageTexture>` preview binding; in-engine SubViewport mount/unmount + world preservation + `logic.reset()`-on-confirm.
- Live building-config shift (undone: append-only save ABI change) and async generation-preview worker (undone: `ContentRegistry` is not `Send`).
- `data/*_view.gd` body views + `assets()` plan-20 mount.

### 21 Multiplayer
- M0 large-row spike (16 KiB `Vec<u8>` + 100 KB `rules_json` over `spacetime start`).
- Live 30-minute soak (bounded `mp_soak` slice exists); live late-join ≤ 8 s / reconnect timing.
- `--profile-json` not-implemented warning in `mind-headless/src/server/mod.rs:306`.
- Per-match chat filters await the authoritative sim (accepted deviation).

## 8. W5 — Performance and baselines

- **09:** liquid/heat p99 over budget (power marginal) — optimize or record an accepted deviation with owner.
- **11:** `units bench --units 300` p50 ≈ 363 ms vs 1.5 ms budget (F31, discovered not fixed); record/regress or fix; add `bench-air`/`bench-path` runtime profiles.
- **16:** §7.4 GPU/full-frame/draw-call/triangle/texture-memory budgets.
- **20:** release mods perf run (debug numbers recorded).
- **23:** fill or explicitly waive null baseline rows: `units_{mid,stress}_p95`, all five `logic_*` (needs a logic bench), four `ui_*` (in-engine), `assets_pack_full`/`assets_ready`, `logistics_conveyor_throughput`, `power`/`liquid`/`heat_update_p95`.
- **04:** groundZero map-save rows (also W4); **01:** relay-throughput bench; **07:** release §7d sign-off.

## 9. W6 — Verification automation and registries

- **23:** JVM re-dump (`MIND_JAVA_PARITY=1`) of the four source-derived dumps in `parity/golden/**`.
- **23:** nightly runners (determinism-compare, golden-drift, bench, soak, T2 catalog, MCP-gated) on the baseline machine (NUD-40/A).
- **23:** `rules`/`logic_rng` checksum contributors pending the joint `CHECKSUM_VERSION` bump (with 12/05); matrix CI coverage; §7e phase-gate ledger rows still read `not-started` — truth them up.
- **03:** `assets-pack` CI job/cache wiring.
- **14/23:** confirm `ui/prompts.json`, `ui/relay_wire.hex`, `render_layers_full` goldens are registered in `parity/golden_manifest.json` (F30).

## 10. W7 — Open user decisions

| Ref | Plan | Decision |
|---|---|---|
| R2 | 02/07 | `BlockDef.kind_data` ownership + interim overlay removal. |
| R3 | 08 | Carried-payload entity model; core inventory sharing (R2/08). |
| R10, R12 | 03 | Asset vendoring; icon-font regeneration. |
| R4 | 09 | Exact quadtree vs deterministic scan. |
| OD-11-A | 11 | Pathfinder budget constants sign-off. |
| R5 | 12 | `Rules` in `Sim::checksum` golden-re-record policy. |
| U1/U3 | 15 | 14-push-default and plan-12 integration records. |
| NUD-40/A | 23 | Nightly/baseline-machine ownership. |
| R-10-9 | 10 | BlastBullets2D adoption (record closure either way). |

## 11. W8 — Plan bookkeeping

After each closure, update the owning plan so the archive gate is mechanical:

- Tick stale milestone/exit boxes: 05 (all 10), 07 (9), 09 (12), 10 (13), 11 (14), 14 (10), 15 (8), 16 (6), 17 (4), 18 (3), 19 (2+1 partial), 20 (1+3 partial), 21 (M0–M5), 22 (13), 23 (13+4 partial), 01 (2), 02 (1), 03 (1+5 partial), 04 (8), 06 (2), 08 (1), 12 (4), 13 (2).
- Refresh stale headers: 05 ("Draft v1 — not started"), 06/07/09/10/11/12/13/14/15 ("In progress"/"Ready"/"Draft" despite landed milestones), 16 ("Ready to execute"), 17 ("not started"), 21 ("not started"), 22 ("Originally Draft"), 23 ("Draft… plans 18–22 not on disk").
- Confirm F30/F31 closures in the plan text: `SimIoHandler` live `IoSet` (04/05), `unitMoveBreakable` + `BlockDef.commandable` + incremental flowfield (02/07/11), `SteamVent.attributes.steam` (02/06), mods browser (20), Windows export + templates (22), live STDB ITs (21/22/23), draw-call model (16/17), bench baselines (23).
- Mark `MindIo` `.msav` importer (04 M7) and Steam/Discord/iOS (22) as user-waived rather than open.

## 12. Suggested sweep milestones

| Milestone | Scope | Exit signal |
|---|---|---|
| S0 | W1 environment unblock + W8 truth-up | One editor sweeps serially; screenshots non-blank; plans' checkboxes match reality. |
| S1 | W2 in-engine §7c sweep (all plans) | Pid-stamped logs + screenshots committed; all §7c boxes ticked. |
| S2 | W4 functional residuals | Named residuals closed or user-waived; tests + goldens green. |
| S3 | W5/W6 perf + automation | Benchmark gates green on the baseline machine; nightly runs recorded. |
| S4 | W3 platform exports | Android + Linux/macOS presets built or documented NOT-RUN; assets packaged. |
| S5 | Archive pass | Re-run the archival rule; archive eligible plans; update this plan's Changelog. |

## 13. Exit criteria for this plan

- [ ] Every row in §3 is closed, user-waived, or moved to a named owner plan with a recorded decision.
- [ ] `parity check --tests`, `bench-gate`, `goldens`, `run-all --tier T0/T1` green; no null baseline rows without a waiver.
- [ ] Each §7c run has pid-stamped evidence (log excerpt + checksum + screenshot) linked from the owning plan.
- [ ] Eligible plans archived into `plans/archived_plans_llms_do_not_read.zip`; remaining plans carry only user-waived deferrals.

## 14. References

- Plans `01`–`23` in `plans/`; `plans/HIGH_LEVEL_PLAN.md` §3, §5, §13 (F27–F31).
- `parity/reports/jvm_golden_diff.md`, `parity/matrix.toml`, `parity/checksum_registry.json`, `parity/golden_manifest.json`, `parity/bench_budgets.json`, `parity/scenario_catalog.json`, `parity/mcp_catalog.json`.
- `tools/mcp-smoke.sh`, `tools/mcp_smoke.py`, `.opencode/skills/playtest/SKILL.md`.
- `bench/baselines.json`, `client/rust/mind-headless/bench/*.json`, `bench/editor_baseline.json`.

## Changelog

- **2026-10-05 — compiled.** Aggregated from the six-lane verification pass over plans `01`–`23` plus `HIGH_LEVEL_PLAN.md` §13 through F31. No item closed yet; plan `00` already archived.
