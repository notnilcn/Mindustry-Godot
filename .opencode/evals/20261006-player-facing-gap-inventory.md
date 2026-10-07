# Player-facing parity gap inventory — code-reading pass

- Date: 2026-10-06
- Port base: Mindustry-Godot `4e24e40` (main)
- Upstream reference: Mindustry `2cd7aeec` (per `parity/upstream.lock`)
- Method: 8 read-only explorer agents, code inspection only — no MCP, no Godot,
  no cargo, no screenshots. Every item is a **candidate gap**, not an
  engine-reproduced finding.
- Status: **not in `findings.json`**. This file is the prioritization queue.
  Promote an item to `EV-####` through
  `.opencode/skills/parity-eval/scripts/record_finding.py add --source code
  --status open`; the writer/evaluator pipeline reproduces it in-engine later.

## Why this list exists

The Rust simulation modules are implemented, but the **shipping client path
drives almost none of them**: the behavior stack is reached only by
`mind-headless` harnesses. Most campaign/player-facing gaps are wiring, not
missing simulation code. The 34 existing ledger findings are all former
menu/settings/planet-view/render issues; nothing below duplicates them.

## First break point (answer to "can a campaign be played end-to-end?")

No. Launching Ground Zero works (menu → campaign → sector → launch loads the
map, renders terrain, HUD appears). Then:

1. launch never installs the sector preset's rules/win condition (`C-1`);
2. the client never ticks objectives/waves/game-state, so no action can capture
   the sector (`R0-1`, `R0-3`, `C-10`);
3. even a forced capture would never be shown (fixture-backed planet map,
   `C-4`) and there is no in-game route back to the planet map (`C-2`);
4. relaunching the process discards all campaign state because nothing is saved
   or loaded (`C-3`, `S-1`).

# 0. Root causes (why nothing in-game works)

### R0-1 · Live tick schedule is stub-only — S1
- Live schedule registers only `update_buildings` + a no-op stub; `TickSet`
  slots for Campaign, Objectives, WaveTimer, RunWave, GameStateCheck, Power,
  Units and Bullets have zero systems.
- Evidence: `client/rust/mind-core/src/sim/schedule.rs:403-409`
- Campaign: blocks · Confidence: high (spot-checked)

### R0-2 · Buildings placed in the live path are inert — S1
- `Sim::apply(Place)` spawns only `BuildingComp` with `rot: 0`; no block
  runtime/`BlockTable` is ever inserted, so drills, conveyors, factories,
  turrets and cores set a grid cell and do nothing.
- Evidence: `client/rust/mind-core/src/sim/mod.rs:249-274`;
  `client/rust/mind-gdext/src/sim_host.rs:918-925`
- Campaign: blocks · Confidence: high (spot-checked)

### R0-3 · No unit/AI/wave/bullet/fire runtime in the live path — S1
- `WaveSpawner` is never instantiated outside tests; `check_game_state` is
  called only by `mind-headless`; combat/weapons/AI drivers exist only in
  harnesses.
- Evidence: `client/rust/mind-core/src/game/play.rs:385`;
  `client/rust/mind-headless/src/campaign_scenarios.rs:963`;
  `client/rust/mind-core/src/combat/harness.rs`, `src/ai/harness.rs`
- Campaign: blocks · Confidence: high

# 1. Campaign critical path

### C-1 · Launch never applies the sector preset's rules or win condition — S2
- `start_sector` calls `play_new_sector` with default `Rules`;
  `capture_wave`/`win_wave`/`waves` are never read into the session or sim;
  `Planet::apply_rules`/`CampaignRules::apply` are headless-only.
- Evidence: `client/rust/mind-gdext/src/campaign.rs:124-155`;
  `client/rust/mind-core/src/game/play.rs:269`;
  `client/rust/mind-core/src/game/planet.rs:156,339`
- Upstream: `core/src/mindustry/core/World.java:265-330`,
  `Control.java:574-590` · Campaign: blocks · Confidence: high

### C-2 · No in-game pause menu / no way back to the planet map — S1
- `PausedDialog` has no equivalent: no dialog in the manifest, ESC keys are
  drained without dispatch; `open_dialog("planet")` is only reachable from the
  main menu. Blocks save, quit, abandon, host and "back to planet".
- Evidence: `client/ui/dialogs_manifest.json`;
  `client/rust/mind-gdext/src/input/mod.rs:84-89`;
  `client/ui/fragments/hud_fragment.gd` (Paused banner only)
- Upstream: `core/src/mindustry/ui/dialogs/PausedDialog.java:14-217`,
  `Control.java:746-750` · Campaign: blocks · Confidence: high
  (independently found by 4 audits)

### C-3 · Campaign progress is never persisted or resumed — S1
- `MindCampaign` uses in-memory stores, never calls `Universe::load` /
  `Campaign::save_all` / `Saves::new`; `save_slot` writes a settings marker;
  `play_sector` (resume branch) has zero callers; every launch starts fresh.
- Evidence: `client/rust/mind-gdext/src/campaign.rs:66,82,440-452`;
  `client/rust/mind-core/src/game/universe.rs:691-707`;
  `client/rust/mind-core/src/game/play.rs:303`
- Upstream: `core/src/mindustry/core/Control.java:436-560`; `game/Saves.java`
- Campaign: blocks · Confidence: high

### C-4 · Planet map reads a frozen fixture, not the live campaign — S2
- `MindUi.campaign_views()` caches `CampaignViews::vanilla_fixture()` forever;
  the fixture pre-owns Ground Zero and Salt Flats. Captured/locked/threat state
  never updates; `MindCampaign.get_sector_state()` has no GDScript caller.
- Evidence: `client/rust/mind-gdext/src/ui/ui_host.rs:465-476`;
  `client/rust/mind-core/src/ui/campaign.rs:792-810`
- Campaign: blocks · Confidence: high (spot-checked)

### C-5 · Capture bookkeeping incomplete — S2
- `sector_capture` sets `was_captured`, clears markers and disables waves, but
  never increments `CampaignStats`, runs auto-unlocks/`SectorComplete`
  objectives, saves, checks last-sector victory, applies
  `sector_capture_replacements`, or updates base coverage; `sector_lose` has no
  caller.
- Evidence: `client/rust/mind-core/src/game/play.rs:488-530`;
  `content/registries/planets.rs:278`; `planet.rs:374`
- Upstream: `Logic.java:394-420`, `Control.java:178-185` · Campaign: blocks ·
  Confidence: high

### C-6 · Tech tree cannot be advanced — S1/S2
- `research_dialog.gd` emits `research_requested` with no listener; the dialog
  shows fixture data, not `get_tech_state`; `MindCampaign.research` spends from
  a brand-new empty inventory (nonzero-cost nodes can never complete) and
  reports success without unlocking; unlocks are memory-only; placement is not
  gated by `content_unlocked`.
- Evidence: `client/ui/dialogs/research_dialog.gd:13,103-104`;
  `client/rust/mind-gdext/src/campaign.rs:238-271`;
  `client/rust/mind-core/src/game/tech_tree.rs:243-284`
- Upstream: `ui/dialogs/ResearchDialog.java:567`, `Control.java:350-358` ·
  Campaign: blocks · Confidence: high

### C-7 · Campaign turns and launch logistics are never driven — S2
- `Campaign::run_turn`, `advance_time`, `update_global`, launch-resource and
  loadout APIs have no client callers; the loadout branch in `logic_play` is an
  empty `if`; owned sectors never produce/import/export and are never invaded.
- Evidence: `client/rust/mind-core/src/game/universe.rs:116-199`;
  `play.rs:366-369`; `client/rust/mind-gdext/src/campaign.rs:180-199`
- Upstream: `game/Universe.java:64-77,151+`; `Control.java:569-601` ·
  Campaign: blocks · Confidence: high

### C-8 · Campaign-complete / Erekir flow absent — S3
- `campaign_complete_dialog` is never opened; `sector_capture` never checks
  last-sector; the campaign is built with `EmptyNeighborhood`, so neighbor
  adjacency, base coverage and invasions cannot work.
- Evidence: `client/rust/mind-gdext/src/campaign.rs:110`;
  `play.rs:488-513`; `planet.rs:374`
- Upstream: `Control.java:177-185`, `Planet.java:386` · Campaign: degrades ·
  Confidence: high

### C-9 · Difficulty/rules dialog inert; sector eligibility bypassed — S3
- `campaign_rules_dialog.rule_changed` has no listener, opens with empty
  context, uses stale key names and 3 of 5 difficulties; `start_sector` accepts
  any name and fakes `has_core=true` before play.
- Evidence: `client/ui/dialogs/campaign_rules_dialog.gd:12,15,47-52`;
  `campaign.rs:133-141`; `ui/campaign.rs:630-662`
- Upstream: `CampaignRulesDialog.java:22-85`, `PlanetDialog.java:429-462` ·
  Campaign: degrades · Confidence: high

### C-10 · HUD campaign feedback is dead — S2
- `wave`/`enemies` are constants never refreshed; no objective text; no
  capture/lose toasts; no skip-wave button or key; `hud_text.rs` composer has no
  producer.
- Evidence: `client/rust/mind-gdext/src/ui/hud.rs:39-53,175-190`;
  `client/ui/fragments/hud_fragment.gd:50-51`;
  `client/rust/mind-core/src/ui/hud_text.rs:53-134`
- Upstream: `HudFragment.java:879-1184,502-510` · Campaign: blocks (goal is
  undiscoverable) · Confidence: high

# 2. Save / load

### S-1 · No in-game save; autosave pump dead — S1
- `SaveDialog` is `menu_openable:false` with no caller; no `runExitSave`
  equivalent; `Saves::update`/`save_sector` have no runtime caller; the Save
  Interval slider is inert.
- Evidence: `client/ui/dialogs/save_dialog.gd:11,57,62`;
  `client/rust/mind-core/src/game/saves.rs:217-264,310-336`
- Upstream: `PausedDialog.java:82-93,185-216`; `Control.java:678` ·
  Campaign: blocks · Confidence: high

### S-2 · Load Game is always empty and cannot load — S1
- The `slots` context is never produced, `slot_selected` is unconnected, and
  `Saves::load`/`load_slot` are never called.
- Evidence: `client/ui/fragments/menu_fragment.gd:38`;
  `client/ui/dialogs/load_dialog.gd:97,219-221`;
  `client/rust/mind-gdext/src/ui/ui_host.rs:174-201`
- Upstream: `ui/dialogs/LoadDialog.java:85-99,224-263` · Campaign: blocks ·
  Confidence: high

### S-3 · The only engine save path writes a tile-only save — S2
- No entities/rules/sector tags; `saved=0`, `playtime=0`; load restores the
  grid only and parks buildings in `pending_buildings`.
- Evidence: `client/rust/mind-core/src/io/sim_io.rs:146-196`;
  `io/save/versions/v1.rs:858-882`
- Upstream: `SaveVersion.java:130+`; `SaveIO.save/load` · Campaign: blocks
  (resume impossible) · Confidence: high

### S-4 · `.msav` import/export and load-card actions are dead — S2
- File chooser `file_chosen` is unconnected; desktop import only logs; export,
  rename and delete card buttons have no handlers.
- Evidence: `client/ui/dialogs/file_chooser_dialog.gd:96-98`;
  `client/rust/mind-gdext/src/platform/desktop.rs:59-87`;
  `load_dialog.gd:152-155,224-229`
- Upstream: `LoadDialog.java:139-145,196-222`; `Saves.java:273-282` ·
  Campaign: degrades · Confidence: high

### S-5 · Corrupted-save / missing-mod handling has no UI — S3
- `cautious_load` is never called; backup fallback only logs; no
  `@save.corrupted` or `mod.missing` prompt.
- Evidence: `client/rust/mind-core/src/game/saves.rs:397-407`;
  `io/save/mod.rs:118-198`
- Upstream: `Saves.java:401-410`; `LoadDialog.java:236-260` · Campaign: blocks
  for affected saves · Confidence: high

### S-6 · Save metadata and previews are never stamped/rendered — S3
- No writer for date/playtime; slot has no `get_play_time`/`get_date`/`get_map`
  and no preview renderer; cards would show empty fields.
- Evidence: `client/rust/mind-core/src/io/save/slot.rs:22-206`;
  `io/save/versions/v1.rs:858-882`
- Upstream: `Saves.java:343-399`; `SaveVersion.java:130-131` · Campaign:
  degrades · Confidence: high

# 3. Core player interaction

### I-1 · RTS unit commanding/selection is absent from the client — S2
- No command mode, box/shift select, orders, stances or control groups;
  `get_input_state_json` hardcodes empty state; `rts.rs`/`command_emit.rs` are
  headless-only.
- Evidence: `client/rust/mind-gdext/src/input/mod.rs:299-307`;
  `client/rust/mind-core/src/input/rts.rs:68-203`
- Upstream: `DesktopInput.java:300-421,905-935`; `InputHandler.java:1091-1200` ·
  Campaign: blocks · Confidence: high

### I-2 · No rotation; placement is single-click only — S2
- `Place` always uses `rot: 0` (the relay path drops rotation too); no
  drag/line/rect placement, no replace/upgrade, no drag-deconstruct.
- Evidence: `client/rust/mind-gdext/src/sim_host.rs:168-173`;
  `sim/mod.rs:261`; `determinism/command.rs:230-240`;
  `input/desktop.rs:86,106` (no live callers)
- Upstream: `DesktopInput.java:753-826,834-846` · Campaign: blocks ·
  Confidence: high

### I-3 · Block config is unreachable and unsupported — S2
- `block_config_fragment.configure()` has no caller; `SimCommand::Configure` is
  `Unsupported` in `Sim::apply`; right-click breaks instead of opening config.
- Evidence: `client/ui/fragments/block_config_fragment.gd:23`;
  `sim/mod.rs:577-583`; `sim_host.rs:145-179`
- Upstream: `InputHandler.java:1968-2010`;
  `BlockConfigFragment.java:39-67` · Campaign: blocks · Confidence: high

### I-4 · Keybinds are cosmetic — S2
- No `[input]` actions exist in `project.godot`; rebinding stores into
  `BindingState` which no gameplay path consumes; pan/zoom are hardcoded.
- Evidence: `client/rust/mind-gdext/src/input/mod.rs:102-109`; `camera.rs:91-108`
- Upstream: `DesktopInput.java:239-295`; `Binding.java` · Campaign: degrades ·
  Confidence: high

### I-5 · Schematic copy/save/place are hard stubs — S3
- `write_schematic_selection` returns `""`, `place_schematic_base64` returns
  `false`; dialog action buttons emit an unconnected signal.
- Evidence: `client/rust/mind-gdext/src/campaign.rs:428-438`;
  `schematics_dialog.gd:88-94`
- Upstream: `InputHandler.java:1528-1556`; `Schematics.java:454,553` ·
  Campaign: degrades · Confidence: high

### I-6 · Item withdrawal/deposit and payload interaction absent — S3
- `block_inventory_fragment` has no caller; `SimCommand::Payload` is
  unsupported; no right-click item transfer.
- Evidence: `block_inventory_fragment.gd:12`;
  `sim/mod.rs:511-519`; `input/desktop.rs` (`tryDropItems` unported)
- Upstream: `DesktopInput.java:798,1019-1039` · Campaign: degrades ·
  Confidence: high

### I-7 · No camera recenter/follow and no build hotkeys — S3
- No detach/recenter-on-core; none of R/Q/E/F1/M/J/N/1-0/pick-block work;
  `spectate`/`pan_to` have no callers.
- Evidence: `client/rust/mind-gdext/src/camera.rs:192-215,313-319`;
  `mind-core/src/input/binding.rs` (consumers absent)
- Upstream: `DesktopInput.java:253-295,449-467` · Campaign: degrades ·
  Confidence: high

### I-8 · Minimap is a placeholder; fullscreen map unreachable — S3
- Widget draws a dark grid only; `pan_requested`/`zoom_changed`/`tapped` are
  unconnected; fullscreen fragment is hidden with no toggle.
- Evidence: `client/ui/widgets/mind_minimap.gd:38-50`;
  `minimap_fragment.gd`; `ui/host.rs` minimap texture has no consumer
- Upstream: `MinimapFragment.java:39-152`; `HudFragment.java:336-351` ·
  Campaign: degrades · Confidence: high

### I-9 · Hints/onboarding never displayed — S3
- `show_hint`/`complete_hint` have no callers; no skip button; reduced hint
  catalogue.
- Evidence: `client/ui/fragments/hints_fragment.gd:12-21`
- Upstream: `HintsFragment.java:42-160` · Campaign: degrades · Confidence: high

### I-10 · Mobile/touch controls absent — S3
- Bridge exists (`input/mobile.rs`) but zero GDScript/tscn callers; no touch
  HUD or gestures.
- Evidence: `client/rust/mind-gdext/src/input/mobile.rs`; `project.godot`
- Upstream: `MobileInput.java` · Campaign: n-a on desktop; blocks on touch ·
  Confidence: high

# 4. Content behavior gaps (surface once R0 is wired)

### K-1 · 16 of 27 factory recipes missing — S2
- `apply_vanilla_knobs` is the only source of crafter outputs; graphite-press,
  silicon-crucible, mixers and all Erekir heat crafters emit nothing; bases
  stall.
- Evidence: `client/rust/mind-core/src/world/block_kind_data.rs:640-782`;
  `world/behavior/production.rs:237-251`
- Upstream: `Blocks.java:1041-1655` · Campaign: blocks · Confidence: high

### K-2 · Turrets/projectors/shields absent from the behavior registry — S2
- `default_registry` omits defense; 28 turrets + projectors/mines resolve to
  `NoopBehavior`; overdrive/regen/shockwave have no implementation at all.
- Evidence: `world/blocks/mod.rs:24-33`; `world/behavior/mod.rs:489-529`;
  `combat/harness.rs:59-104`
- Upstream: `Blocks.java:3276-6430,1934-2058` · Campaign: blocks ·
  Confidence: high

### K-3 · Power grid never updated — S2
- `update_power_graph` has no production caller; `PowerNodeConfig` is never
  inserted; powered machines run at efficiency 0.
- Evidence: `world/blocks/power/module.rs:209`;
  `sim/schedule.rs:143-146` (slot empty)
- Upstream: `Logic.updateEntities` power graph update · Campaign: degrades ·
  Confidence: high

### K-4 · Erekir unit assemblers built with empty plans — S2
- `UnitAssembler::new(Vec::new(), 240)` ignores `def.assembler_plans`; 6 T4/T5
  units unbuildable.
- Evidence: `world/blocks/units/behavior.rs:285-296,394-397`;
  `content/registries/blocks/units_erekir.rs:206,240,274`
- Upstream: `UnitAssembler.java:52,433` · Campaign: blocks · Confidence: high

### K-5 · Pumps produce 0; eruption-drill mines instantly — S3
- `PumpDef.pump_amount` never set for 4 pumps; eruption-drill missing from the
  knob overlay leaves drill time clamped to 0.0001.
- Evidence: `world/block_kind_data.rs:94-125,640-782`;
  `world/behavior/production.rs:308,478-496`
- Upstream: `Blocks.java:2299,2305,3112` · Campaign: degrades · Confidence:
  high

### K-6 · Liquid bridges unregistered/inert — S3
- Conduits/routers/junctions are registered; all 3 bridges' update fns have no
  callers.
- Evidence: `world/blocks/liquid/behavior.rs:369-400`;
  `world/blocks/liquid/bridge.rs`
- Upstream: `Blocks.java:2375,2387,2435` · Campaign: blocks (fluid chains) ·
  Confidence: high

### K-7 · Unit abilities, status effects and weather are metadata-only — S3
- `AbilityKind` has no runtime consumers (15 unit types); `StatusApply` has no
  production impl (23 statuses); weather is view-only.
- Evidence: `content/registries/units/ability.rs:20-45`;
  `combat/damage/status.rs:20-43`; `fx/weather_fx.rs`
- Upstream: `entities/abilities/*`; `type/StatusEffect.java` ·
  Campaign: n-a (degrades combat) · Confidence: high

### K-8 · `unit-cargo-loader` spawns the wrong unit and never updates — S4
- Configured with `UnitTypeId::new(0)` (dagger) instead of manifold;
  `CargoLoaderBehavior` has no `update_tile`.
- Evidence: `world/blocks/units/behavior.rs:404-409`
- Upstream: `Blocks.java` unit-cargo-loader; `UnitCargoLoader.java` ·
  Campaign: n-a · Confidence: high

### K-9 · Tech/BuildVisibility never gates the palette or placement — S2
- Catalog filters only `build_time > 0`; sandbox/debug/editor-only blocks are
  buildable from start; research has no effect on the live match.
- Evidence: `client/rust/mind-core/src/ui/campaign.rs:284-337`;
  `sim/mod.rs:249-268`; `world/block.rs:446-451`
- Upstream: `Block.unlockedNow()`; `BuildVisibility` filtering · Campaign:
  blocks · Confidence: high

# 5. Menus & reachability

### M-1 · Editor dead-ends — S2
- `map_editor_dialog.show_dialog()` has no caller; editor-maps rows and
  import/export emit unconnected `editor_map_action`.
- Evidence: `client/scenes/editor/map_editor_dialog.gd:45-52`;
  `client/ui/dialogs/editor_maps_dialog.gd:29-49`
- Upstream: `EditorMapsDialog.java`; `ClientLauncher.java:346-355` · Campaign:
  n-a · Confidence: high

### M-2 · Custom Game hardcoded; rules/gamemode ignored — S3
- Map list is a fixture, no Customize/Rules button; `map_play._play` calls
  `start_map(name)` ignoring mode; `play_requested` unconnected.
- Evidence: `client/ui/dialogs/custom_game_dialog.gd:45-68,219-223`;
  `map_play_dialog.gd:12,49-59`; `ui_host.rs:468-475`
- Upstream: `MapPlayDialog.java:52,75,89`; `CustomGameDialog.java` · Campaign:
  degrades · Confidence: high/medium

### M-3 · Orphan dialogs and missing shells — S3
- Unreachable: content info, full text, palette, sector select, icon select,
  traces. Missing: EffectsDialog, CanvasEditDialog, LogicDialog. Admins/Bans
  are opened with wrong registry keys (`server.bans` vs `bans`). Database
  search is a no-op.
- Evidence: `client/ui/dialogs_manifest.json`;
  `player_list_fragment.gd:97-100`; `database_dialog.gd:65-74`
- Upstream: `DatabaseDialog.java:212`; `PausedDialog.java:79`;
  `UI.java:216-219` · Campaign: n-a · Confidence: high

### M-4 · Orphan overlay fragments — S4
- Loading overlay never shown (`set_progress` no caller), console unreachable,
  player-list/chat/minimap toggles unbound, `MindHud` toast/announce/unlock
  emitters have no producers.
- Evidence: `loading_fragment.gd:11-21`; `console_fragment.gd:105-124`;
  `ui/hud.rs:108-160`
- Upstream: `LoadingFragment.java:44-55`; `UI.java:183` · Campaign: degrades ·
  Confidence: high

# 6. Multiplayer & social

### P-1 · Hosting impossible — S1
- `HostDialog` is unreachable and `host_requested` has no listener;
  `MindNet.create_match` never passes `campaign_id`; no pause-menu host entry.
- Evidence: `client/ui/dialogs/host_dialog.gd:11,56-60`;
  `client/rust/mind-gdext/src/net/mod.rs:224-243`; `session.rs:97`
- Upstream: `PausedDialog.java:98-105,152`; `HostDialog.java:51-73` ·
  Campaign: blocks (co-op) · Confidence: high

### P-2 · Join never connects — S1
- `connect_requested` is unconnected; server lists are static/empty; no
  address→match_id resolution; networking offline unless launched with
  `--db`/`--pN`.
- Evidence: `join_dialog.gd:12,99-115,173-204,235-245`;
  `client/rust/mind-gdext/src/net/mod.rs:54-56,174`
- Upstream: `JoinDialog.java:403-412,626-652` · Campaign: blocks (co-op) ·
  Confidence: high

### P-3 · In-match commands are never relayed — S1
- Mouse place/break apply locally only; `send_command_json` has no caller and
  supports only place/break; no outbound bridge for config/plans/units/rules.
- Evidence: `sim_host.rs:168-173`; `net/mod.rs:368-397`; `relay.rs`
- Upstream: `NetServer.java` command handling · Campaign: blocks (MP) ·
  Confidence: high

### P-4 · Campaign co-op progression not synchronized — S1
- Server campaign tables exist but are in no subscription wave, the
  `my_campaign_*` views are host-only, and the client never reads/writes them;
  campaign is local `MindCampaign` only.
- Evidence: `client/rust/mind-stdb/src/waves.rs:12-50`;
  `server/spacetimedb/src/campaign/mod.rs:162-168,542-605`;
  `mind-gdext/src/campaign.rs:122-147`
- Upstream: `NetworkIO.java:33-64` · Campaign: blocks (co-op) · Confidence:
  high

### P-5 · Chat is a dead local loop — S2
- Send emits into the void (no listener), receive path is unbound, and the
  fragment cannot be opened (keybind not dispatched).
- Evidence: `chat_fragment.gd:72-78,123-129`; `ui_host.rs:494-503`;
  `mind-core/src/input/binding.rs:775`
- Upstream: `ChatFragment.java:65,221-236` · Campaign: degrades ·
  Confidence: high

### P-6 · Player list empty, unopenable, no moderation — S2
- `player_list_json()` returns `"[]"`; `toggle()` has no caller; every row
  click hardcodes spectate; no kick/ban/vote/team UI.
- Evidence: `ui_host.rs:508-522`; `player_list_fragment.gd:75-77,103-106`
- Upstream: `PlayerListFragment.java:149-247` · Campaign: degrades ·
  Confidence: high

### P-7 · No disconnect/kick/version UI; ping hardcoded 0 — S3
- `disconnected`/`connect_error`/`resync` signals have no GDScript consumers;
  protocol compatibility is never checked client-side; `ping` has no writer.
- Evidence: `stdb.rs:237-259`; `net/mod.rs:137`;
  `protocol.rs:40`; `ui/hud.rs:51-53,79`
- Upstream: `NetClient.java:138-145,420-443`;
  `JoinDialog.java:654-682` · Campaign: degrades · Confidence: high

### P-8 · No team select / vote-kick / admin command surface — S3
- No team-select UI; console registers only `help`; server wrappers
  (kick/ban/unban/switch_team) are not exposed by `MindNet`.
- Evidence: `mind-core/src/ui/console.rs:74`; `ui_host.rs:49,81,526`;
  `mind-stdb/src/connector.rs:824-940`
- Upstream: `PlayerListFragment.java:187-213,240-246`; `ServerControl.java` ·
  Campaign: degrades · Confidence: high

# Ground truth (verified working, for calibration)

- Planet chooser and 3D sector view: EV-0010/EV-0011 fixes are real; launch
  loads the preset map and renders terrain (EV-0012/0014/0016).
- Block picker renders the real catalogue with 46px icons and category toggles
  (EV-0013 residual is filtering only).
- Campaign/mission model in `mind-core` is complete and unit-tested; headless
  scenarios exercise launch → wave → capture/lose → turn production end-to-end.
- Save container (`MGRS`), slot policy, backup/rotation and autosave math are
  implemented and tested; only the client wiring is missing.
- Pause governor, prompt/toast pipeline, console command registry, editor
  rotation/undo, and the headless input/RTS/placement layers are functional.
- Server (SpacetimeDB) match/relay/campaign tables are built and integration
  tested; the client half is unwired.

# Suggested prioritization

- Minimal "campaign is playable" spine: **R0-1..3, C-1..C-7, S-1..S-3,
  I-1..I-3** (wiring spine plus build/rotate/config interactions).
- Content gaps (**K-1..K-9**) only surface once R0 is closed; treat as a second
  wave.
- Quick, independent wins with high player visibility: **C-2** (pause menu),
  **S-2** (load list), **I-5/I-8/I-9** (schematic/minimap/hints),
  **M-3/M-4** (orphan UI).
- Multiplayer (**P-1..P-8**) is a separate track; co-op campaign also needs
  **R0**, **C-3** and **P-3/P-4**.

# Provenance

Compiled from 8 parallel read-only code audits (campaign progression; campaign
UI; in-game HUD; dialog inventory/reachability; content breadth; input/controls;
save/load; multiplayer/social). Root-cause items R0-1/R0-2 and the fixture
caching in C-4 were spot-checked by direct file reads on 2026-10-06. No MCP
tool, engine run, build or file in the game source tree was touched by the
audits that produced this file.
