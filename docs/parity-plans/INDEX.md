<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Gameplay-parity plans (round G, written 2026-10-13) — index and schedule

Goal: close the in-campaign gameplay gaps between Mindustry-Godot and Java
Mindustry. The plans were written by area auditors against `main` + the four
unmerged session branches of 2026-10-12 (sessions 2-5), checked against the
Java sources, then cross-reconciled so every cross-area seam has exactly one
owner and one name.

Every executor reads, in this order: `00-environment.md` (shared setup, RAM
rules, gmcp recipes, commit rules), this file's §3 (dependencies), then its
own plan. Older files `audit-*.md` / `audit-*_summary.md` are the previous
round (history only; their unreached tasks were folded into the G-plans).
Do not open `archived_plans_do_not_read.zip`.

## 1. Plans

| Plan | Area | Tasks (commits) | Top gaps |
|---|---|---|---|
| `G00-integrate-sessions.md` | merge sessions 2-5 into `main`, fix goldens, baseline | 7 | 58 unmerged commits; stale golden hashes; base-builder golden |
| `G01-units-ai-combat.md` | unit/AI/combat sim | T1-T19 (+T6a/b, T10a-c, T18a/c) | base builder queues 0 plans (150 px vs 150 tiles); line weapons never hit units; heal bullets damage friends; ground AI ignores the core flowfield; no player respawn; mono/poly/mega idle |
| `G02-blocks.md` | block behavior + config | T1-T18 | default item/liquid intake rejects everything (drill boost, menders, reactors, heat crafters, unloaders); placement rules; power nodes reset to defaults; force projector; tap-to-link |
| `G03-rendering-hud.md` | world drawing + HUD | T1-T22 | no placement preview/plans/build beam; bullets never drawn; mining beam/item stack/engines; building bars; node lasers; boss bar/core-attack; command panel; light pass |
| `G04-saves-campaign-loop.md` | saves + campaign loop | T1-T12, T3a/b, T3w, V1 | every per-kind building body lost on load (PAR-0161); sector resume drops units/fires/wave timer; wave countdown 60x slow; tutorial objectives stall; capture leaves the enemy base live; launch/landing pads |
| `G05-campaign-ui.md` | campaign UI / meta screens | 27 commits (T1a-T13) | research node graph + Java partial spending; dialog stacking/dim/confirm; planet sector panel; hints (timeouts, 22 predicates); content info stats model; generated sectors (PAR-0112); globe shader |
| `G06-logic.md` | logic | T1-T15 (+T13a-d, T14a/b) | Ground Zero's own world processors are inert (setrule/flags/cutscene); sensors; executor bugs; frozen globals; lookup; logic editor; displays |
| `G07-audio-weather.md` | audio + weather | T1-T14 (+T10b) | client likely silent (asset root unresolvable); weapon/turret/block sounds; loops/ambient; music context; UI sounds; weather sim residuals + rendering |
| `G08-input-controls.md` | in-game input | T1-T15 (+T2a/b, T6a/b) | player unit cannot shoot; item transfer fake; schematic copy (PAR-0099); rebuild select; line placement bridges; hotkeys on the menu; even-size offset |

Each plan has: §1 inventory (Java file -> port -> status), §2 tasks (Symptom /
Java (transcribed) / Port today / Changes / Tests / Verify in-engine /
Pitfalls / Commit), §3 order, §4 ledger changes, §5 done criteria, §6
cross-area findings (already reconciled into the owning plans).

## 2. Schedule (max 4 parallel sessions on this host)

```
G00  (single executor, main checkout, nothing else running)
  │
  ├─ Wave 1 (parallel):  session 2: G01   session 3: G03   session 4: G02   session 5: G04
  │      coordinator merge into main after wave 1 (procedure §4), at least every ~8 commits per branch
  │
  └─ Wave 2 (parallel):  session 2: G08   session 3: G07   session 4: G06   session 5: G05
         final coordinator merge, then a re-run of every task skipped for a dependency
```

Why this order: wave 1 holds the plans the others depend on (G02 T1/T8/T10,
G01 T8/T9/T11/T19, G04 T4/T5/T10/T11). G03 is in wave 1 because it is the
largest plan; its tasks that need G01/G04 seams (T9 boss bar, T10 reticle,
T19, T20) are ordered last inside G03 and simply wait (00-environment §1.0).

Inside a wave, sessions working on hot shared files (`core/sim/mod.rs`,
`core/sim/unit_runtime.rs`, `core/game/runtime.rs`, `gdext/sim_host.rs`,
`gdext/input/mod.rs`, `gdext/input/desktop.rs`) must keep commits small; the
coordinator merges often to keep conflicts small.

## 3. Cross-plan dependencies (the exact seams)

| Consumer task | Needs | Seam / name |
|---|---|---|
| G02 T8 step 8 | G07 T9 | `GeneratorEnv` computation from weather |
| G02 T1 -> G08 T2a | G02 T1 first | trait-default `accept_stack`; G08 T2a adds Core/Turret stack overrides + `transfer::dispatch_accept_stack` |
| G03 T9 boss bar, G05 T4b guardian hint | G01 T19 | `#[func] SimHost.boss_json()` |
| G03 T10 command panel | G01 T16 | G03 T10 DEFINES `MindInput.set_unit_command(command: GString) -> bool` + `set_unit_stance_named(stance, enabled) -> bool` (G08 T11 reuses them); reads G01's `command_targets_json` |
| G03 T19 / minimap indicators | G01 T15 | `drain_build_damage_json` |
| G03 T11 border darkness | G04 T5 | `grid.limit_area` set by `Sim::apply_rules_to_world(&Rules)` |
| G03 T20 pods | G04 T10/T11 | `launch_payloads_json()`, `LandingPadState` |
| G03 T3 rebuild ghosts | — | reads `TeamData.plans` through its own Rust seam `team_block_plans`; G08 T7's `team_ghost_plans_json` is a playtest readback only |
| G01 T13 unit bounds | G04 T5 | `grid.limit_area` |
| G02 T8 build gate | G04 T5 | `grid.limit_area` + border darkness |
| G04 T1 per-kind loader | G02 T17, G06 (logic codecs) | behavior `read`/`write` bodies |
| G05 T10b in-game schematic use | G08 T6b change 13 | `#[func] MindInput.use_schematic(index: i32) -> bool` |
| G05 T4b hints | G08 T15, G01 T19 | `#[func] MindInput.take_hint_keys() -> PackedStringArray`; `boss_json` |
| G04 T4 command-mode objective | G08 T15 | `Sim::set_command_mode_satisfied` / `#[func] MindSimHost::set_command_mode_satisfied(bool)` (defined by G04 T4, called by G08 T15) |
| G06 T4 setrule | G04 T5, G02 T8 | `CampaignRuntime.rules_world_dirty = true` -> `Sim::apply_rules_to_world`; `world::limits::sync_build_rules(world, &rules)` |
| G06 T9 LogicAI | (owner) | G01 adds no `AiKind::Logic` branch |
| G06 T11 processor tap-to-link | G02 T10 | trait `on_configure_build_tapped(&self, world, grid, e, other) -> ConfigTapResult`; entry `world::config_tap::config_tap` |
| G06 T12 overlays | G03 T14 | `ConfigOverlay`/`SelectOverlay` (`Logic` variant), `MindInput::config_selected()` |
| G06 T6 announce | G03 T9 | G03 owns the announce/info-toast visuals; G06 only calls the MindUi entry point |
| G05 dialogs, G06 playsound, G07 | G07 T1/T13 | `MindUi.play_ui_sound(name)` is emit-only outside G07; sfx volume applied in the audio sink |
| G07 T14 | — | payload/configure sounds are emitted sim-side by G07 (G08 adds none) |
| G01 T10c / G08 T7 | G02 T8 | derelict repair branches in `Build.validPlace` |

## 4. Coordinator merge procedure (between and during waves)

Run in the main checkout, one branch at a time, with no session building:
```bash
git merge --no-ff parity/session-N        # conflicts: keep both sides' additions; never drop a hunk you do not understand — abort and ask the session
cd client/rust && flock /tmp/opencode/mg-build.lock bash -c 'CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib --features msav-import && CARGO_BUILD_JOBS=6 cargo test -p mind-headless --lib; CARGO_BUILD_JOBS=6 cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings'
cargo run -q -p mind-headless -- parity goldens   # golden hash conflicts: regenerate per 00-environment §2, never hand-pick a hash
```
After a merge, the sessions pick it up with `git merge --no-edit main`.
Apply each summary's "ledger changes" table to `chains.json` yourself
(executors never edit it).

## 5. Status

| Plan | Session | State |
|---|---|---|
| G00 | main | not started |
| G01-G04 | 2-5 | waiting for G00 |
| G05-G08 | 2-5 | waiting for wave 1 |
