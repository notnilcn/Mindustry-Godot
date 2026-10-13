<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Round-H gameplay-parity plans (written 2026-10-14) — index, schedule, seams

Goal: finish the in-campaign gameplay port (everything after entering a sector,
Serpulo and Erekir) so Mindustry-Godot plays like Java Mindustry 1:1.

Round G (`docs/parity-plans/`, being archived) was executed on 2026-10-13 and
fully merged into `main` (`ffbe84dd`). Round H was written by auditing every
round-G summary against the code (several "done" claims were false or partial;
each plan's §1 lists what was checked), carrying forward everything that was
not reached, and sweeping the Java gameplay packages for gaps no plan covered
(H09).

Every executor reads, in this order: `docs/H-environment.md` (setup, RAM
rules, gmcp recipes, commit rules), this file's §3 (seams + overrides), then
its own plan. Do not open `docs/parity-plans/` or any `*.zip`.

## 1. Plans

| Plan | Area | Tasks | Gameplay-critical first tasks |
|---|---|---|---|
| `H00-blockers.md` | cross-cutting blockers + baseline | T1, T2, T2b, T3, T4 | Planet `Env` bits wrong (Serpulo `rules.env` 31, Java 233) -> the player mech never spawns; player plans apply instantly with no builder; wave numbering off by one on resumed saves (wave 1 spawned wave 2's flares) |
| `H01-units-ai-combat.md` | units, AI, combat sim | T1-T15 | building/bullet tile coordinates half a tile off; pathfinder has no floor data or Java costs (wave daggers drown in deep water, T15); per-mount weapon targeting; respawn; Java velocity/drag model (wind, knockback); Ground Zero wave-1 acceptance |
| `H02-blocks.md` | block behavior + config + placement | T1-T16 | campaign cores never incinerate overflow (belts back up); turrets aim at stale positions and never lead (T16); `sync_build_rules` + `validBreak`/`validPlace`; core upgrade; configure values mangled on the wire (PAR-0190); tap-to-link; force projector |
| `H03-rendering-hud.md` | world rendering + HUD | T1-T14 | command panel; minimap attack indicators; overlay top; light composite (stale packed shader); fires/puddles invisible; crafter animations; fog rendering; pods |
| `H04-saves-campaign-loop.md` | campaign loop sim + saves | T1-T12, V1 (T5 withdrawn) | stale campaign copy overwrites the live one; sectors produce nothing offline; resume ignores `SectorInfo`; invasions of the played sector; launch/landing pads |
| `H05-campaign-ui.md` | generated sectors + campaign UI | T1-T22 (T18 withdrawn) | Serpulo attack sectors never enemy bases; generated sectors identical 128x128 maps without core/base/ruins (PAR-0112); toasts; research; content stats; custom rules |
| `H06-logic.md` | logic | T1-T19 | real processor seams; `spawnwave` dropped (Ground Zero waves/objectives stall); cutscenes; messages; `setrule`; early-sector program conformance |
| `H07-audio-weather.md` | audio + weather | T1-T13 | voice policy; block/step/ability sounds; fresh-sector weather; weather rendering + noise; menu music |
| `H08-input-controls.md` | in-game desktop input | T1-T13, V1 | dragged belt lines point the wrong way vertically; line junction/bridge replacement; tap gates; block hotkeys; RTS keys/orders; mouse bindings; camera |
| `H09-gameplay-sweep.md` | systems no other plan owned | T1-T11 | fog of war never simulated; `checkAllowUpdate`; fires; puddles; building explosions; `fillItems`; Erekir core incineration; game stats; fog persistence |

Each plan: §1 inventory + "done — do not redo", §2 tasks (Symptom/acceptance,
Java transcribed, Port today, Changes, Tests, Verify in-engine, Pitfalls,
Commit), §3 order, §4 ledger changes, §5 done criteria, §6 cross-area seams.
Blockquotes starting with **Coordinator override/note (2026-10-14)** at the top
of a task take precedence over the task text below them.

## 2. Schedule (max 4 parallel sessions on this 14 GB host)

```
H00  (main checkout, session 1, NOTHING else running) -> marker commit "H00: ..."
  │
  ├─ Wave 1 (providers):  s2: H01   s3: H09   s4: H02   s5: H04
  │      coordinator merges into main at least every ~6 commits per branch (§4)
  │
  ├─ Wave 2 (consumers):  s2: H08   s3: H03   s4: H06   s5: H05
  │
  └─ Wave 3:              s2: H07   s3-s5: re-run every task listed as "skipped: waiting for X"
                          in the wave-1/2 summaries (same plan, same session number as before)
```

Why this order: wave 1 holds the seams everything else consumes —
`sync_build_rules`/`config_tap`/`validPlace` (H02), building coordinates and
the velocity model (H01), live fog/`allowUpdate`/stats/explosions (H09), the
single live campaign + unlock pipeline + launch pads (H04). Wave 2 plans read
those seams. H07 goes last because its only hard dependency is H01 T4 and it
touches nothing the others need.

Hot shared files (keep commits small, coordinator merges often):
`core/sim/mod.rs`, `core/sim/unit_runtime.rs`, `core/game/runtime.rs`,
`core/world/update.rs`, `core/world/build.rs`, `gdext/sim_host.rs`,
`gdext/campaign.rs`, `gdext/input/mod.rs`, `gdext/input/desktop.rs`.
Wave 1 has three plans in `unit_runtime.rs` (H01, H09, H04 one-liners): merge
after every task that touches it.

## 3. Cross-plan seams and ownership decisions

### 3.1 Ownership decisions (where two plans covered the same thing)

| Topic | Owner | The other plan |
|---|---|---|
| `dynamic_explosion` lightning, fireballs, wave stagger, building explosion terms | H09 T7 | H01 T7 reduced to `explosion_absorbed` semantics only |
| `TeamRule.fillItems` | H09 T8 (adds `ItemModule::set`) | H01 T10 keeps BaseBuilderAI core fill, `aiCoreSpawn`, `prebuildAi`, turret DPS |
| `Logic` stat listeners (GameStats + planet CampaignStats) | H09 T10 | H04 T5 **withdrawn**; `cores_destroyed` hint value stays H05 T12 |
| `allowUpdate` / `checkAllowUpdate` / `World.checkMapArea` | H09 T4 (`core/world/allow_update.rs`) | H06 T5 keeps `setrule` tracking and calls H09's entry |
| Sector invasion / capture / loss toasts | H05 T7 | H04 T4 only changes rules + emits `CampaignEvent`s |
| PAR-0164 settings | H04 T11 (`gdext/settings_hub.rs`) | H05 T18 **withdrawn** |
| Core intake order + campaign incineration | H02 T1 | H09 T9 adds Erekir `incinerateNonBuildable` + `handle_stack` only |
| Unlock pipeline (items/liquids/units) | H04 T2 (`UnlockRef`, `CampaignRuntime.pending_unlocks`, `#[func] MindCampaign.take_unlocked_content`) | H01 T3 pushes `UnlockRef::Unit` into it |
| `set_unit_command` / `set_unit_stance_named` | H03 T3 | H08 T6a defines them with the same signatures only if H03 T3 is not on `main` |
| `control_block_unit_id` | H08 T8 (defines it) | — |
| `run_wave_campaign(session) -> (PlayEvent, i32 spawn index)`, sessions start at wave 1 | H00 T2b | H06 T2 `run_wave_now`, H04 wave paths |
| `create_weather` | H07 T5 | H06 T8 calls it (defines it only if H07 T5 is absent) |
| Building centre = `tile*8 + offset` | H01 T1 (edits the position line in `core/world/update.rs`) | H02 must not change that line |
| Fog seams for rendering | H09 T1 adds `fog_static_bytes` / `fog_sources` | H03 T12 consumes |

### 3.2 Seam table (provider -> consumer; exact names)

| Seam | Provider | Consumers |
|---|---|---|
| `rules.env` Java bits, real player spawn, no instant player plans | H00 T1/T2 | everyone's in-engine checks |
| `world::limits::sync_build_rules(world, &rules, is_campaign: bool)` | H02 T2 | H04, H06 T5 |
| `trait on_configure_build_tapped(&self, world, grid, e, other) -> ConfigTapResult`, `world::config_tap::config_tap`, `SimCommand::ConfigTap` | H02 T6 | H06 T15, H08 T4 (tap routing in `left_press`) |
| `Sim::valid_place_player`, `#[func] MindSimHost.valid_place_preview(x,y,block,rot)`, `#[func] MindInput.close_block_config` | H02 T3/T6 | H08, H01 (`valid_place_query` + `PlaceFlags::PLAYER`) |
| Derelict repair branches in `validPlace` | H02 T3 | H08 T11, H01 T12 |
| `LiquidModule::current_liquid()`, `force_*` fields in `building_debug` | H02 T10/T7 | H03 |
| `building_unit_on` hook (`unitOn`) | H02 T11 | H01 |
| `transfer::record_produced` | H04 T2 | H02 behaviors producing without `offload` |
| `launch_payloads_json()` `[{id,x,y,time,lifetime,block,team}]`, `landing_pads_json()`, `LandingPadState` | H04 T6/T7 | H03 T13 |
| `set_launch_destination(planet, sector)`, `"origin"` key in `start_sector_with_loadout` JSON | H04 T3/T6 | H05 |
| `MindSimHost.set_sector_gen_extras`, `MindCampaign.prime_sector_load`, `MindSimHost.set_pending_launch`, `generate_sector(planet, sector, content, SectorGenExtras)` | H05 T2/T4 | H04 (call before any `load_sector` of a numbered sector) |
| `world::darkness::get_darkness`, `WorldGrid.sector_polygon` | H05 T3 | H03 (renderer + minimap darkness), H02/H04 `valid_place` darkness |
| `Sim::set_player_team`, `Sim::debug_kill_unit`, `#[func] SimHost.debug_kill_unit(seq)` | H01 T3 | H03 T2, H07 T12 checks |
| `entities::comp::unit::velocity::{move_at, move_pref, impulse, vel_update}` | H01 T4 | H07 T13 (wind writes velocity), H08 (player input writes velocity only) |
| `world::status::java_block_status` | H01 T8 | H03 status markers |
| FogControl resource, `fog_visible_tile`, `fog_discovered`, `#[func] fog_debug_json`, `debug_set_fog`, `fog_static_bytes`, `fog_sources` | H09 T1/T2 | H03 T12, H07 (fog-suppressed sounds), H08 (fog-hidden taps) |
| `crate::world::allow_update::{allow_update, check_allow_update}`, `Sim::check_all_allow_update` | H09 T4 | H06 T5, H06 control enabled |
| `ItemModule::set` | H09 T8 | H01 T10 |
| `BlockBrokenEvent.team`, name-keyed `destroyed_block_count` | H09 T10 | H05 T12, H04 planet stats |
| `MindInput.set_unit_command(GString)->bool`, `set_unit_stance_named(GString,bool)->bool`, `select_only_type(i64)`, `deselect_type(i64)` | H03 T3 | H08 T6a |
| `MindInput::config_selected()`, `#[func] config_selected_json`, `ConfigOverlay::Logic`, `SelectOverlay::{MessageText, LogicTag}` (`render/overlay_top.rs`) | H03 T5 | H06 T16 |
| `MindUi.show_info_toast` | H03 | H06 T4 (falls back to `announce`) |
| `res://shaders` as the shader source of truth | H03 T6 | everyone editing shaders (see H-environment §3) |
| Real `logic_get_code/logic_set_code/logic_get_state`, `logic_last_error` | H06 T1 | all logic in-engine checks |
| `game::runtime::run_wave_now` | H06 T2 | — |
| `MindInput.set_logic_cutscene_lock`, `logic_cutscene_json` | H06 T3 | H03 HUD hide |
| `emit_world_sound`, `AudioViewRng`, `steps::*`, `env_attr`, `raw_noise_1d`, `active_voice_counts` | H07 | H01 (leg steps), H02 (block sounds, `AttributeCrafter` env term) |
| `MindUi.toggle_power_lines()`, `MindUi.toggle_fullscreen()` | H08 T9 | — |

If a consumer task starts before its provider is on `main`: follow the task's
own instruction (skip the dependent half and record it, or define the seam
with exactly the name above). Never invent a different name.

## 4. Coordinator merge procedure (between and during waves)

Run in the main checkout, one branch at a time, with no session building:
```bash
git merge --no-ff parity/session-N        # conflicts: keep both sides' additions; never drop a hunk you do not understand — abort and ask the session
cd client/rust && flock /tmp/opencode/mg-build.lock bash -c 'CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib --features msav-import && CARGO_BUILD_JOBS=6 cargo test -p mind-headless --lib && CARGO_BUILD_JOBS=6 cargo test -p mind-gdext --lib; CARGO_BUILD_JOBS=6 cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings'
cargo fmt -p mind-core -p mind-gdext -- --check   # run cargo fmt + commit "Wave N merge: cargo fmt" if it prints anything
cargo run -q -p mind-headless -- parity goldens   # golden hash conflicts: regenerate per H-environment §2, never hand-pick a hash
```
After a merge, the sessions pick it up with `git merge --no-edit main`.
Apply each summary's "ledger changes" table to `chains.json` yourself
(executors never edit it); new items get the next free `PAR-` id
(`jq -r '.ledger | keys[]' chains.json | sort | tail -1`).

Between waves, update §5 below and re-read the wave's summaries for
"skipped: waiting for X" lines; collect them into the wave-3 re-run list.

## 4b. In-engine verification of the plans (2026-10-14)

Before execution every plan was checked against the running Godot client (and the Java
client where the expected behaviour was in doubt) on `main` @ `ffbe84dd`. Each task carries
an `> **In-engine verification (2026-10-14, session N):**` blockquote with the evidence
(CONFIRMED / PARTLY / NOT CHECKABLE / SKIP); wrong facts were corrected in place. Reports:
`verify-s2-round1.md` (H00, H01, H09), `verify-s3-round1.md` (H02, H08), and the round-2
reports for H03-H07. Tasks added from the findings: H00 T2b, H01 T15, H02 T16, H03 T15-T18, H04 T13, H05 T23, H06 T20, H08 T14.
Coordinator decisions from round 2 (session 3, H04/H06): H06 T20 (map/save processors compiled
before the logic constants exist -> `@sharded`/`@duo`/`@scatter` are empty variables; Ground Zero's
duoAmmo/scatterAmmo flags never fire) is owned by H06 and runs right after H06 T1; if it needs a
one-line reorder in the load path (`load_sector_world` vs `install_sim_seams`), H06 makes it and H04
leaves it alone. H06 T1's `logic_set_code` seam writes code directly (bypassing the campaign
configure gate) — accepted as a debug/test seam. PAR-0193 (wave countdown) closes as verified on the
round-2 evidence (H04 T8 SKIP); `debug_set_wave_rules` is not needed.
Round-2 (session 2, H03/H07): H03 gained T15-T18 (permanent unit shield bubble, light discs
with lighting off, raw markup in toasts, static block cache never redraws door/sorter/cracks)
and a fog stopgap in T12 step 0b. Open item for H01 (not investigated): a factory-built dagger
vanished shortly after a move order on Onset — the H01 executor should try to reproduce it
during T3/T9 and file it if real.
Round-3 (session 3, H05 + H08 leftovers): new H05 T23 (palette keeps the previous planet's
blocks after a sector switch), new H08 T14 (Escape leaves config/inventory panels drawn over the
pause menu; moves the fragments to the HUD group — node path changes), new H04 T13 (resumed
player-team units come back as GroundAI instead of CommandAI). H05 T5 (BaseGenerator) has no
vanilla trigger: kept last, headless-only. Out of round-H scope: custom games never load their
map (PAR-0133 family) — H05 T11b verifies through the campaign pause-menu rules instead.
Notable: PAR-0129's "force projector stays at efficiency 0" did NOT reproduce (a powered
projector reaches 1.0 and draws its dome) — only live bullet absorption remains (H02 T7).

## 5. Status

| Plan | Session | State |
|---|---|---|
| H00 | 1 (main) | not started |
| H01, H09, H02, H04 | 2, 3, 4, 5 | waiting for H00 |
| H08, H03, H06, H05 | 2, 3, 4, 5 | waiting for wave 1 |
| H07 + re-runs | 2 (+3-5) | waiting for wave 2 |

## 6. Ledger at the start of round H (chains.json, 2026-10-14)

169 verified, 24 fixing, 30 seeded, 22 reference, 2 blocked (247 items).
Open in-campaign items and their owners: PAR-0047/0145 (H05), PAR-0055 (H03 T2),
PAR-0072 (H07 T10), PAR-0089 (H02 Erekir check), PAR-0091 (H09 T5/T6 + H01 T7),
PAR-0096 (H03 T4), PAR-0098 (H08 T3), PAR-0104 (H06), PAR-0105 (H06 T18),
PAR-0106 (H06 T1 in-engine), PAR-0112 (H05 T1-T6), PAR-0127 (H07 T6/T7),
PAR-0129 (H02 T7 + H03 T11), PAR-0132 (H05 T11), PAR-0137 (H08 T7),
PAR-0138 (H08 T8), PAR-0142/0144/0143 (H05), PAR-0156 (H01 T10),
PAR-0157/0095 (H03 T3), PAR-0164 (H04 T11), PAR-0174 (H00 + H01 T3),
PAR-0188/0190 (H02 T5 + H03 T2), PAR-0189/0215 (H00 T2), PAR-0191 (H03),
PAR-0193 (H04 T8), PAR-0198 (H04 T6/T7), PAR-0202 (H01 T4 + H07 T13),
PAR-0203 (H07 T5), PAR-0209 (H08 V1), PAR-0213 (H08 T9), PAR-0214 (H08 T5),
PAR-0218/0220-0225 (H06), PAR-0244 (H03 T2), PAR-0246 (H03 T6).
Out of scope for round H (not in-campaign single-player): PAR-0109-0111,
0113-0125, 0133-0136, 0151 (low priority in H05 T22).
