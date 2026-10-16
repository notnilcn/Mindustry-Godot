<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Round-I parity plans (written 2026-10-15): index, schedule, seams

Goal: Mindustry-Godot becomes a full 1:1 port of Java Mindustry. Round H finished most of the
in-campaign gameplay (H00, H01, H02, H09 complete; H03, H04, H06, H07, H08 mostly; H05 about a third).
Round I carries every unreached H task, fixes the deviations and bugs the round-H executors recorded
(plus two the plan writer found live on 2026-10-15), and covers the areas round H scoped out: custom
games and the map editor, mods and exports, multiplayer.

How these plans were written: every round-H summary was read and checked against `main` @ `b560e9b1`;
the Java source was read for every task; the live Godot client was run to probe the open bug reports
(three did not reproduce and are recorded as such at the top of I01; two new bugs were found). Fact sweeps of
the multiplayer, mods/export and custom-game/editor code were done with read-only agents and spot-checked.

Every executor reads, in this order: `docs/I-environment.md` (setup, RAM rules, gmcp recipes, commit
rules), this file's §3 (seams), then its own plan. Do not open `docs/parity-plans/` (round G/H history)
unless a task names a section, and never open any `*.zip`.

## 1. Plans

| Plan | Area | Tasks | Ledger items |
|---|---|---|---|
| `I00-coordinator.md` | merge session 4 (6 unmerged H03 commits), park the H03 T13 WIP, bring all session branches to `main`, apply round H's ledger changes (never applied), baseline | T1-T6 | ~35 updates, ~80 new items |
| `I01-sim-residuals.md` | sim bugs and deviations left by H01/H02/H04/H09 + probe findings: CoreItem objective text, doors not solid, collideLine rounding, pathfinder health/costs, ammo `rangeChange`, air/ground splash, Erekir core zone, ignoreBuildDarkness, current-liquid coolant, unit ids across saves + cargo loader, unit assembler, turret block units (PAR-0138), queued orders, power relink, fog debug stall, verification sweep | T1-T16 | PAR-0138, PAR-0168 + new |
| `I02-rendering-hud.md` | launch/landing pods (finish the WIP), mesh-null errors, status markers (noOutput + pulse), wall flash, beam-node lasers, power overlay rings, DrawBlock layer order, toasts, unit sprite scale (PAR-0055), walk effects, re-verification | T1-T11 | PAR-0198, PAR-0055, PAR-0191, PAR-0244, PAR-0129, PAR-0188 |
| `I03-logic.md` | ubind/ucontrol/LogicAI, completionLogicCode, sensor/fetch tables, turret control, radar, processor links + pencil, displays (CPU raster + render), canvas, the logic editor (dialog, field layouts, jumps/drag/add, variables), overlays | T1-T14 | PAR-0221, 0224, 0218, 0223, 0222, 0220, 0105, 0225, 0103 |
| `I04-campaign-ui.md` | palette rebuild, unlock toast, hints, content stats (logistics, units, turrets/weapons), schematics, planet globe interaction/mesh/glow/bloom, rename/icon, sector stats, database tabs, dialog fade, enemy-base generator | T1-T17 | PAR-0143, 0144, 0142, 0112 |
| `I05-settings-language-input.md` | one shared settings store, language persistence + bundle at boot, block hotkeys, settings rows take effect (audit), input re-verification | T1-T5 | PAR-0164, 0135, 0214, 0209 |
| `I06-custom-games-editor.md` | custom maps load + gamemodes, Map Play dialog, Custom Rules dialog, pause-menu rule edits, map previews, save import + card actions, editor rendering, editor menu + sub-dialogs, New Map, exit confirm, map view input, sub-dialog fidelity | T1-T12 | PAR-0133, 0132, 0114, 0134, 0109, 0110, 0111, 0113 |
| `I07-mods-export.md` | mod state persistence, crash restore, script mods unsupported, mod content in the live registries, mod sprites/sounds, end-to-end content mod, exports ship assets, CI export smoke | T1-T8 | PAR-0115, 0116, 0117, 0118, 0119 |
| `I08-multiplayer.md` | SpacetimeDB relay: local harness + online on demand, player identity, saved servers/remote list, host current world, join/late join via snapshot, full command relay, remote players, chat, player list/admin, dedicated server, per-match chat filters, host dialog, desync resync | T1-T13 | PAR-0120..0125, 0136, 0151 |

Each plan: state on `main` with file:line facts, tasks (Symptom/evidence, Java transcribed with
file:line, Port today, ordered Changes, Tests, Verify in-engine, Pitfalls, Commit), hot files, ledger
changes to report, done criteria.

## 2. Schedule (max 4 parallel sessions on this 14 GB host)

```
I00 (main checkout, session 1, NOTHING else running) -> marker commit "I00: round-I baseline ..."
  │
  ├─ Wave 1 (providers first):  s2: I01   s3: I03   s4: I05   s5: I06
  │     coordinator merges into main at least every ~6 commits per branch (§4)
  │
  ├─ Wave 2:                    s2: I02   s3: I03 (continue) or I04   s4: I07   s5: I04 or I06 (continue)
  │
  └─ Wave 3:                    I08 on s2 + s3 (a two-client plan uses TWO session slots),
                                s4/s5: continue whatever is unfinished, then re-run every task listed as
                                "skipped: waiting for X" in the wave-1/2 summaries
```

Why this order: I05 T1 (the settings hub) is consumed by I07 T1 and I08 T2; I06 T1
(`load_map_world` + `start_map_mode`) is consumed by I08 T4/T5 and I07 T6; I01 T5/T10 feed I03 T3 and
I01 T11. I03 and I04 are the largest plans and have no hard providers, so they start early and continue.
I02 waits for I00's session-4 merge only. I08 needs two game processes, so it runs when two slots are free.

When a session finishes its plan early, it takes the next unstarted plan from this order:
I02, I04, I07, I08 (needs two slots), never one already assigned to a running session.

Hot shared files (keep commits small, coordinator merges often): `core/sim/mod.rs`,
`core/sim/unit_runtime.rs`, `core/world/tile.rs`, `core/world/blocks/defense/turrets/mod.rs`,
`gdext/sim_host.rs`, `gdext/campaign.rs`, `gdext/input/desktop.rs`, `gdext/render.rs`,
`ui/fragments/placement_fragment.gd`, `scenes/ui/ui_root.tscn`, `ui/dialogs_manifest.json`,
`core/ui/manifest.rs`.

## 3. Cross-plan seams (provider -> consumer; exact names)

| Seam | Provider | Consumers |
|---|---|---|
| `gdext/settings_hub.rs`: `init(paths)`, `with_settings(f)`, `reload(paths)`, `save(paths)` | I05 T1 | I07 T1 (MindMods store), I05 T2 (locale), I08 T2 (name/colour) |
| `#[func] MindSimHost.load_map_world(path) -> bool` (+ shared `finish_world_load`) | I06 T1 | I08 T5 (join loads the snapshot save), I07 T6 (sandbox check), I04 T1 (emit `world_loaded` there once) |
| `#[func] MindCampaign.start_map_mode(map, mode, playtest, rules_json) -> bool` and a factored `start_loaded_world(rules)` | I06 T1 | I08 T4/T5, I06 T8 (playtest) |
| `Map::apply_rules(mode, hooks)`, `#[func] MindPreview.map_modes(name)`, `map_rules_json(name, mode)` | I06 T1/T2 | I06 T2/T3 |
| `custom_rules` dialog signal `rules_edited(json)` | I06 T3 | I06 T2, I06 T4, I06 T8 (map info) |
| `#[signal] MindSimHost.world_loaded`, `#[signal] MindCampaign.block_unlocked` | I04 T1 | I05 T3 (hotkey state reset may use it) |
| `MindHud.push_unlock(name)` producer | I04 T2 | — |
| `BulletDef.range_change`, `fn turret_range(world, e)` | I01 T5 | I03 T3 (`@range`), I02 range circle |
| `BuildingBehavior::check_solid`, `Tile` dynamic-solid flag | I01 T2 | pathfinder, collisions |
| unit ids persisted, `unit_by_id(world, id)` | I01 T10 | I01 T11, I08 T5/T13 (snapshot entity ids) |
| `MindSimHost::control_block_unit_id(x, y)`, turret block units | I01 T12 | desktop possession (I01 T12 itself) |
| `SimCommand::UnitCommand { .., queue }` | I01 T13 | I08 T6 (`sim_to_kind` must carry it) |
| `LogicAi` Java fields, `AiKind::Logic` | I03 T1 | I03 T3 (`@controller`), I03 T11 (LogicAI unit squares) |
| `core/logic/display_raster.rs`, `LogicDisplayRasters`, `process_display(world, root)` | I03 T12 | I03 T13, I03 T14 (canvas shares the render pass) |
| `#[func] MindLogic.write_statements`, `default_statement`, `sanitize`; `#[func] SimHost.logic_configure_code(x, y, code)` | I03 T7/T8 | I03 T8-T10 |
| `gdext/content_boot.rs::build_registry(bundle, unlocks)` | I07 T4 | every registry construction (`sim_host.rs`, `campaign.rs`, `editor/mod.rs`; also `MindPreview` if it builds one) |
| `resolve_assets_dir()` candidate order (incl. `--assets=` and exe-dir `assets/`) | I07 T7 | everything reading assets |
| `MindSimHost::submit_player_command(cmd)` (single outbound seam for player actions) | I08 T6 | all input call sites that enqueue player commands |
| `#[func] MindNet.ensure_online()`, `host_current_world(name)`, `ping_server(address)` | I08 T1/T3/T4 | `ui_root.gd`, host/join dialogs |

If a consumer task starts before its provider is on `main`, follow the task's own instruction (skip the
dependent half and record "skipped: waiting for <plan task>", or define the seam with exactly the name
above). Never invent a different name.

## 4. Coordinator merge procedure (between and during waves)

Run in the main checkout, one branch at a time, with no session building:
```bash
git merge --no-ff parity/session-N        # conflicts: keep both sides' additions; never drop a hunk you do not understand — abort and ask the session
cd client/rust && flock /tmp/opencode/mg-build.lock bash -c 'CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib --features msav-import && CARGO_BUILD_JOBS=6 cargo test -p mind-headless --lib && CARGO_BUILD_JOBS=6 cargo test -p mind-gdext --lib; CARGO_BUILD_JOBS=6 cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings'
cargo fmt -p mind-core -p mind-gdext -- --check   # run cargo fmt + commit "Wave N merge: cargo fmt" if it prints anything
cargo run -q -p mind-headless -- parity goldens   # golden hash conflicts: regenerate per I-environment §2, never hand-pick a hash
cd ../.. && bash tools/godot.sh --headless --editor --quit --path client 2>&1 | grep -E "SCRIPT ERROR|Parse Error"
```
After a merge, sessions pick it up with `git merge --no-edit main`. Apply each summary's "ledger
changes" table to `chains.json` yourself with I00 T5's script (executors never edit it); new items get
the next free `PAR-` id. If a session ends with uncommitted work, park it on a `wip/<plan>-<task>` branch
exactly like I00 T2 and note it in this file's §5.

Between waves, update §5 and collect every "skipped: waiting for X" line from the summaries into the
wave-3 re-run list.

## 5. Status

| Plan | Session | State |
|---|---|---|
| I00 | 1 (main) | done 2026-10-15: merge a4578878, ledger 91492792, marker c2ba1f69; tests 2551/133/47, goldens 97 PASS |
| I01 | 2 | DONE (T1-T16), merged through 6f4530cd |
| I02 | 4 | pass 1-2 merged (T1-T10; T9 verified no-change); pass 3 running (T11 residuals) |
| I03 | 5 | DONE (T1-T14); merged 51a55a23; PAR-0103 verified |
| I04 | 2 | pass 2 partial: T8/T9 committed (parity/session-2) and merging; T10 mid-task left uncommitted — pass 3 relaunched to finish it + T10-T17 |
| I05 | 4 | DONE (T1-T5), merged 546ceaab |
| I06 | 5 | pass 1-7 merged (872a7430); pass 8 running (T12.4/T12.5 residuals + T12.9 investigation) |
| I07 | 4 | DONE (T1-T8) + T6 sandbox half (d60dfee3); PAR-0115 fully verified |
| I08 | 3+4 (two slots) | running its CURRENT pass only — **DEFERRED by operator 2026-10-16**: no continuation pass is launched after this one; any commits it lands are merged, gated and documented |

Coordinator notes: user's `046ab375 "manual commit"` (mid-run) tracked `.opencode` caches + `parity-ledger.json` + docs plans; worktrees' ledger symlinks are `skip-worktree`-protected so `git merge main` works. **Operator deferred I08 (multiplayer) on 2026-10-16**: the in-flight pass finishes, its output is merged/gated/documented, and no further multiplayer pass is launched. Coordinator fix `4f95c4e0` (I06 settings-hub call sites found by the merge gate). Main gate after the I04-p1 merge (2026-10-16): mind-core 2671 / headless 133 / gdext 51, goldens 97 PASS.

## 6. Known non-issues (do not re-investigate)

Checked live on 2026-10-15 by the plan writer (details at the top of I01): the mining click works; a drill click
queues and builds; spawned units do not vanish; the duo range (160) matches Java. Java never shows
off-play `sector.capture`/`sector.lost` toasts (`SectorLoseEvent` is never fired; I04 header).
