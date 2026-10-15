<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H04 — Saves + the campaign loop: session-5 summary (2026-10-14)

Worktree `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session5`, branch
`parity/session-5`, Godot bridge 7010, display :14. Started from H00 marker
`4fbfcf54`; merged `main` twice mid-session (now `80b104e8`; main carries H02 T2
and the wave-1 commits).

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 one authoritative campaign | done | `573d418b` |
| T2 statistics 1 s refresh, export counters, produced() | done | `6a195eae` |
| T3 resume/launch bookkeeping | done | `79e82f81` |
| T4 turns/invasions | done (toast/signal half left to H05 T7) | `24371ebb` |
| T5 stat listeners | skipped: moved to H09 T10 (withdrawn) | — |
| T6 launch pads export (PAR-0198 1/2) | done | `fb96eca1` |
| T7 landing pads import (PAR-0198 2/2) | done in engine for the fake half; the two-based-sector import half is headless-only (no fixture in the imported profile) | `9ac1f922` |
| T8 PAR-0193 countdown | skipped: pre-decided (verified 2026-10-14, session 3) | — |
| T9 unit chunk count + revision 3 | done | `86d20b0f` |
| T13 loaded-unit controllers | not reached | — |
| T10 Java save import units/fires/puddles | not reached | — |
| T11 shared settings store (PAR-0164) | not reached | — |
| T12 ignoreBuildDarkness / BeamDrillState / markers | not reached | — |
| V1 PAR-0084 + loss keeps rules.waves | not reached | — |

Merge commits on the branch: `51103158` (empty except `chains.json`), `80b104e8`
(wave-1: H02 T2 etc.; includes a one-line semantic merge fix —
`CampaignHookQueues::default` was missing the new `unlocks`/`imports` fields).

## `git log --oneline main..HEAD`

```
80b104e8 Merge branch 'main' into parity/session-5
86d20b0f Gameplay H04 T9: unit chunk count matches written chunks; unit save revision 3
9ac1f922 Fixes PAR-0198: landing pads import items (H04 T7)
fb96eca1 PAR-0198: launch pads launch items and record exports (H04 T6)
24371ebb Gameplay H04 T4: played-sector invasions hit live rules, Java campaign-rule defaults, countSpawns, invasion toast
79e82f81 Gameplay H04 T3: SectorInfo.write on resume, prepare lastPresetName/export clear, lost-sector conditions, quietUnlock, launch origin
51103158 Merge branch 'main' into parity/session-5
6a195eae Gameplay H04 T2: sector statistics refresh per second, export counters, produced() feeds production and unlocks
573d418b Gameplay H04 T1: one authoritative campaign while a sector runs (turn clock, being_played, rule edits)
```

## Tests (final state, post-merge)

From `client/rust` (`CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock ...`):

* `cargo test -p mind-core --lib --features msav-import` -> **2256 passed / 0 failed / 4 ignored**
  (baseline 2202 + wave-1 + this session's new tests; per-task counts in the commit bodies:
  2213/2216/2219/2222/2227/2230/2233 during the session).
* `cargo test -p mind-gdext --lib` -> **34 passed**.
* `cargo test -p mind-headless --lib` -> **132 passed**.
* `cargo clippy -p mind-core -p mind-gdext -p mind-headless --all-targets -- -D warnings` -> clean.
* `cargo run -q -p mind-headless -- parity goldens` -> **97 PASS**.
* GDScript gate `bash tools/godot.sh --headless --editor --quit --path client` -> no
  `SCRIPT ERROR`/`Parse Error`.
* New tests: `save_all_after_run_turn_keeps_turn`, `left_sector_produces_on_next_turn`,
  `export_counter_decays`, `refresh_every_60_ticks`, `drill_offload_records_production`,
  `write_to_state_applies_record_and_win_wave`, `prepare_sets_last_preset_and_clears_export_without_pads`,
  `launch_sets_origin_and_destination`, `serpulo_campaign_rule_defaults_match_java`,
  `invasion_of_played_sector_targets_live_rules`, `invasion_of_off_play_sector_writes_the_record`,
  `launch_pad_accepts_single_item_type`, `launch_pad_launches_full_stack`, `launch_pad_io_v1_roundtrip`,
  `payload_exports_on_removal`, `payload_without_destination_expires_silently`,
  `landing_pad_lands_when_timer_full`, `fake_landing_pad_is_an_item_source`,
  `landing_pad_io_v1_roundtrip`, `entity_count_matches_written_chunks`, `unit_rev3_round_trip`,
  `apply_decoded_entities_skips_spawned_by_core` (+ updates to `apply_decoded_entities_skips_core_units`,
  `prepare_and_write_populate_fields`, `info_windows_update_and_cap`).

## In-engine evidence (session 5, bridge 7010)

All runs live under `REPO/runs/`; every step ended with `grep -c panic` = 0.

* **T1** `runs/20261014-s5-H04-T1/` (+ `menu-after-restart.png`): GZ resume, turn 2140
  -> 2142 after two `run_turn()` calls, still 2142 after `run_exit_save()` and after an
  editor restart; `sector_record_json("serpulo",170)` -> `beingPlayed false`, `hasSave true`.
* **T2** `runs/20261014-s5-H04-T2/` (+ `drill-into-core.png`): drill (130,48) + conveyors
  (130,50..53) into the core -> `core_items_json` `{"copper":9}` after 1200 ticks, 12 after
  1800; `rawProduction.copper.mean` 0.0 -> 0.15; `take_unlocked_content()` `[]` (copper is
  already unlocked in the 282-unlock profile).
* **T3** `runs/20261014-s5-H04-T3/` (+ `resume-core-777.png`): exit-save, record patched to
  `{copper:777, lead:55}`, reload + Go -> core `{"copper":777,"lead":55}`, rules winWave 10,
  record `lastPresetName "groundZero"` (before: null).
* **T4** `runs/20261014-s5-H04-T4/` (+ `fresh-gz-rules.png`): after a clean re-import
  (`{"nodes":375,"unlocked":282}`) + GZ resume, `get_campaign_rules_json()` ->
  `{sectorInvasion:false, legacyLaunchPads:false, fog:false, hideSpawns:false, rtsAi:false}`
  (before: invasions/pads true). The invasion roll itself is headless (1 % per turn).
* **T6** `runs/20261014-s5-H04-T6/` (+ `pad-launch.png`, `pad-export.png`): fresh advanced
  pad full at 1790 ticks, empty + one payload `{"id":2,"time":4,"lifetime":120,"block":"advanced-launch-pad","team":1}`
  at 1805; with the T2 drill recording production, `export.copper.mean` 0.15 > 0 after the pod lands.
* **T7** `runs/20261014-s5-H04-T7/` (+ `fake-pad-landed.png`): team-2 fake pad (136,66) +
  water source -> `liquid_debug` `{"current":3000}` and `building_items_json` `{"copper":100}`;
  `landing_pads_json` lists the pad (`config copper`). The real-campaign import half has no
  fixture (needs two based exporting sectors) — covered headless.
* **T9** `runs/20261014-s5-H04-T9/` (+ `resumed-dagger-wet.png`): team-2 dagger +
  `debug_apply_status("wet",6000)` -> `unit_debug_json` status `["wet"]`; `run_exit_save()` +
  restart + Go -> the dagger is back with `["wet"]` (before T9 it came back empty).

## Deviations from Java

* T2: the 1-s statistics refresh keeps the seconds accumulator with a **half-tick
  comparison tolerance** because 60 f32 `1/60` additions round to 0.9999997; Java's
  integer-tick `Interval.get(60)` fires exactly on tick 60 (`SectorInfo.java:23,265`).
* T3: `sectorDataMatches`' map-size half passes the record's own `lastWidth/lastHeight`
  (the preset def carries no map dimensions), so the effective test is the preset name
  (`SectorInfo.java:98-103`). `play_new_sector` formats the origin key with the
  destination planet's name (`play.rs`), harmless for same-planet launches.
* T4: per the coordinator override the invasion toast + `sector_invaded` signal stay
  H05 T7; only the rules half is implemented. `CampaignEvent::SectorInvasion { played }`
  is emitted by `Campaign::run_turn` but not queued host-side — H05 T7's seam (list below).
* T6: launch payloads are not serialized (Java saves the `LaunchPayload` entity); with a
  120-tick life this is accepted (in the commit body). `LaunchPayloadSeq` ids are host-local.
* T7: landing-pad `priority` uses the packed tile instead of Java's
  `Mathf.rand.nextInt()` (`LandingPad.java:139-140`) — deterministic ties only. Arrival
  uses Java's exact f32 arithmetic, so a pod completes on tick 151 (not 150).
* T9: the command move target is not saved (the port has no matching ECS component; the
  `ControllerSlot.target` is a tile, not Java's `targetPos`). The plan's `has_target/x/y`
  pair is replaced by an append-only **controller kind** byte, which H04 T13 needs.
* T2 coordinator note: `UnlockRef::Unit` carries `UnitTypeId` (the codebase's unit id
  type), not `UnitId`; H01 T3 should use that variant.

## Blockers / notes for the next session

* Wave-1 landed on `main` mid-session; the merge needed one semantic fix
  (`CampaignHookQueues::default` missing `unlocks`/`imports`) — fixed in `80b104e8`.
  Re-check merge results after every future `main` merge (the struct is hot with H02/H09).
* H05: keep the `"origin": launcher_id` key when editing `planet_dialog.gd::_launch`
  (T3 seam). H03 consumes `launch_payloads_json()` / `landing_pads_json()` (T6/T7).
* H05 T7: `CampaignEvent::SectorInvasion { played }` is produced by `run_turn` in both
  `pump_runtime` and the `MindCampaign.run_turn` seam, but neither keeps a host-side
  queue of turn events — H05 T7 needs a small seam (e.g. `take_turn_events`) to toast.
* No new live Java evidence was captured (the Java client was reserved for the
  coordinator twin verification).

## Ledger changes to apply (coordinator)

| Item | Change |
|---|---|
| PAR-0198 | `fixing` after `fb96eca1` (T6), `verified` after `9ac1f922` (T7): pad launch + export in engine; landing pad lands the fake/queue halves headless + fake in engine |
| PAR-0193 | `verified` (T8 skipped pre-decided; the session-3 countdown evidence stands; no new run this session) |
| PAR-0164 | unchanged — T11 not reached |
| PAR-0084 | unchanged — V1 not reached (no re-verification after T3 this session) |
| new | "Campaign turn clock and offline production lost: facade campaign overwrites the live runtime copy; left sectors stay being_played" -> `verified` (T1; in-engine turn 2140 -> 2142 survives exit+restart) |
| new | "Sectors never produce while away: produced() is a no-op, statistics refresh every 60 s, exports never decay" -> `verified` (T2; drill raises rawProduction 0.0 -> 0.15) |
| new | "Resume ignores the sector record (items, winWave/attack), no lastPresetName/quietUnlock/setLastSector, launches have no origin" -> `verified` (T3; record items reach the core, lastPresetName `groundZero`) |
| new | "Invasions of the played sector never restart waves; campaign rules default invasions/legacy pads on" -> `verified` headless (T4); in-engine defaults checked after a clean re-import |
| new | "Unit save: entity count can exceed written chunks; statuses/items/shield/commands lost" -> `verified` (T9; `wet` status survives exit + restart) |
| new | "Java save import drops units/fires/puddles" -> **not reached** (T10) |
| new | "Loaded units come back as GroundAI instead of CommandAI" -> **not reached** (T13) |
| new | "Interplanetary Accelerator is a stub" -> `seeded` (deferred, unchanged) |
| existing | T12's three leftovers (`ignoreBuildDarkness`, dead `BeamDrillState`, markers decision) not reached |
| existing | H05-side note: `sector_invaded` seam missing for turn events (see Blockers) |
