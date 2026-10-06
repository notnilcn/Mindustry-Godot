Campaign progression audit — new findings
GAP-1: No client-side wave/wave-timer/objective/game-state loop, so a sector can never be won or lost
- severity: S2
- port: logic exists in Rust but no wiring. The whole play-flow state machine exists (client/rust/mind-core/src/game/play.rs:385 check_game_state, :488 sector_capture, :373 run_wave_campaign), but the only in-process caller is mind-headless (client/rust/mind-headless/src/campaign_scenarios.rs:963). The live schedule has one gameplay system total: build_p0_schedule registers only stub_system and update_buildings (client/rust/mind-core/src/sim/schedule.rs:405-408); TickSet::Campaign/Objectives/WaveTimer/RunWave/GameStateCheck (schedule.rs:37-58) have zero systems. WaveSpawner is never instantiated outside its own tests.
- upstream: core/src/mindustry/core/Logic.java:327-360 (checkGameState), :614-617 (called every tick), :394-420 (sectorCapture)
- symptom: Player builds in Ground Zero forever; no wave counter, no capture popup, no loss, no "sector captured" state. There is no gameplay action that can end the sector.
- campaign: blocks
- confidence: high
GAP-2: Launch never applies the sector preset's rules or win condition (captureWave, waves, difficulty) to the session or sim
- severity: S2
- port: logic exists in Rust but no wiring. MindCampaign.start_sector calls play_new_sector with the pre-existing default Rules (client/rust/mind-gdext/src/campaign.rs:124-155); play_new_sector only sets rules.sector (play.rs:269), never reads sector.preset_capture_wave (stored at planet.rs:156) or the SectorPreset rule callback. Rules::default() is waves=false, win_wave=0 (client/rust/mind-core/src/io/json/rules.rs:663,723), and Planet::apply_rules/CampaignRules::apply (planet.rs:339, campaign_rules.rs:222) are only called from headless/tests (mind-headless/src/campaign_scenarios.rs:388). MindSimHost.load_sector builds a raw Sim::new grid with no rules bridge (client/rust/mind-gdext/src/sim_host.rs:920-925).
- upstream: core/src/mindustry/core/World.java:265-330 (setSectorRules, sector.preset.rules.get, sector.planet.applyRules), Control.java:574-590
- symptom: Waves never start, no winWave, no campaign fog/RTS/health scaling; even a working GameStateCheck would never see a groundZero capture wave (10).
- campaign: blocks
- confidence: high
GAP-3: No in-game return to the planet map (no pause menu / "Back to planet")
- severity: S2
- port: logic not implemented at all. planet_dialog._enter_game hides the menu (client/ui/dialogs/planet_dialog.gd:554-558) and nothing shows it again; dialogs_manifest.json has no pause dialog, and open_dialog("planet") is only reachable from the main menu (client/ui/fragments/menu_fragment.gd:35,53). The restart dialog exists (dialogs_manifest.json:22, game_over_dialog.gd) but is never opened. MindUi/ui_root has no ESC/pause handling.
- upstream: core/src/mindustry/ui/dialogs/PausedDialog.java:146-149 (@planetmap -> ui.planet.show()), GameOverDialog.java:113 (@continue -> ui.planet.show())
- symptom: After launching any sector the player is trapped in the world; the only exit is quitting the process. Cannot reach the planet map to launch a second sector even manually.
- campaign: blocks
- confidence: high
GAP-4: Campaign progress is never persisted or resumed (fresh in-memory state every boot)
- severity: S2
- port: logic exists in Rust but no wiring. MindCampaign owns SettingsStore::new()/MemoryUnlockStore::new() (in-memory; gdext/src/campaign.rs:66,82), never calls Universe::load, Campaign::load_all/save_all (mind-core/src/game/universe.rs:701,691). The Saves policy exists (mind-core/src/game/saves.rs:310 save_sector) but is never instantiated in the client (no Saves::new in mind-gdext). start_sector overwrites sector.save and forces has_core=true (campaign.rs:138-141), then always takes the fresh-launch branch; play_sector (the save-load branch) has zero callers (play.rs:303). save_slot/load_slot write a settings marker "1", not a save (campaign.rs:442-452). sim_host.request_save (sim_host.rs:732) has no GDScript caller.
- upstream: core/src/mindustry/core/Control.java:436-560 (load existing sector.save), :569-601 (saves.saveSector after launch); game/Saves.java:253-263; Logic.java:416-419
- symptom: Every relaunch of a sector regenerates a fresh world (no Resume); campaign/universe counters, captured flags, difficulty, resources and unlocks disappear when the process exits. EV-0010's "Resume" label is cosmetic: there is no save to resume.
- campaign: blocks
- confidence: high
GAP-5: Planet map UI reads a frozen static fixture, not the live campaign
- severity: S2
- port: logic exists in Rust but no wiring. MindUi.campaign_views() builds CampaignViews::vanilla_fixture() once and caches it forever (client/rust/mind-gdext/src/ui/ui_host.rs:45,79,465-476). The fixture itself pre-owns Ground Zero and Salt Flats (mind-core/src/ui/campaign.rs:792-810). planet_dialog takes sectors, lock flags and launch disabled state from campaign_section("sectors") (planet_dialog.gd:236-239,330-372,502-515); CampaignViews::from_campaign (the live projection) is only called by the fixture (ui/campaign.rs:728,820).
- upstream: PlanetDialog.java reads Sector runtime state (hasBase, isCaptured, preset.unlocked()) at :429-462,1304,1328
- symptom: Captured sectors never turn captured; locked/unlocked and threat values never change; the panel always describes the baked fixture (Ground Zero always "owned"). The live MindCampaign.get_sector_state().wasCaptured exists but no GDScript calls it.
- campaign: blocks
- confidence: high
GAP-6: Capture bookkeeping is incomplete even where it exists
- severity: S2
- port: logic partially implemented, headless-only. sector_capture only sets was_captured, clears markers/objectives and disables waves (play.rs:488-513). It never increments CampaignStats.sectors_captured, never runs tech auto-unlocks/SectorComplete objectives, never saves, never fires the last-sector campaign-complete condition, never applies planet.sector_capture_replacements (data present at content/registries/planets.rs:278), never updates base coverage (planet.rs:374). The Solver-side mirror Teams::destroyToDerelict and Generator.onSectorCaptured event handlers are absent. sector_lose (play.rs:530) has no caller.
- upstream: Logic.java:394-420 (saveSector), :139-166 (capture replacements, stats.sectorsCaptured), Control.java:178-185 (CampaignComplete on isLastSector + initialCapture), PlanetDialog.java:1164 (loss clears save)
- symptom: Even via the MCP endpoint, capturing changes one bool; no completion rewards, no stats, no campaign-victory dialog, no world retheme, and defeat never books.
- campaign: blocks
- confidence: high
GAP-7: Tech-tree research is unreachable from the UI and cannot succeed or persist
- severity: S2
- port: logic exists in Rust but no UI wiring plus broken spend path. research_dialog.gd emits research_requested (:13,103-104) and nothing in the repo connects it (grep: only the declaration/emit). The dialog's data is the static fixture, not get_tech_state (gdext/src/campaign.rs:276). MindCampaign.research spends from a brand-new empty ItemModule on every call (campaign.rs:262-271), so any node with a nonzero cost can never complete; spend returns Ok even when nothing was spent/unlocked (tech_tree.rs:243-248), so research() reports success. Objective gating is bypassed by AllObjectivesMet (campaign.rs:44-56), unlock state goes to MemoryUnlockStore (in-memory, content/settings_store.rs:29), and check_auto_unlocks (tech_tree.rs:284) has no production caller. There is also no runtime placement check on locked content (content_unlocked has no input/placement caller) — distinct from EV-0013's catalog display.
- upstream: ui/dialogs/ResearchDialog.java:567 (spend), :403 (selectable); core/Control.java:350-358 (checkAutoUnlocks); Saves-backed unlock keys
- symptom: Tech tree opens but clicking a node does nothing; the whole "play -> capture -> unlock -> build" loop is absent; research state resets on restart.
- campaign: blocks
- confidence: high
GAP-8: Campaign turns (production/import/invasion) are never driven, so resources never carry over and sectors are never attacked
- severity: S2
- port: logic exists in Rust but no host wiring. Campaign::run_turn (universe.rs:451) is exposed only as MindCampaign.run_turn (campaign.rs:180-199) with no GDScript caller; Campaign::advance_time (universe.rs:168), update_global (:348) and apply_lighting (:187) have zero callers outside tests/headless. The schedule's TickSet::Campaign/UniverseGlobal slots are empty (GAP-1). Launch-pad APIs get_launch_resources/update_launch_resources/get_loadout/update_loadout (universe.rs:116,124,160,144) also have no client caller; logic_play's loadout branch is an empty if with a comment (play.rs:366-369).
- upstream: game/Universe.java:64-77 (auto runTurn when turnCounter>=turnDuration), :151-... production/export/import passes; Control.java:569-601 launcher params/loadout
- symptom: Owned sectors never accrue items, launch pads never ship resources to the next sector, no invasions, no turn counter; sector info.items stays empty forever.
- campaign: blocks
- confidence: high
GAP-9: The campaign session and the simulated world are separate, unsynchronized objects
- severity: S2
- port: logic exists in Rust but no bridge. MindCampaign.start_sector and MindSimHost.load_sector are called independently by planet_dialog.gd:542-548 and share no state: campaign knows nothing about the sim's cores/units, and the sim knows nothing about session.rules. PlaySession.teams/objectives/enemies/spawn_count are never populated by the host (headless does it by hand at campaign_scenarios.rs:934-961). Because player_core_count()==0 on a fresh PlaySession (play.rs:179-183), a naive hookup of check_game_state would immediately declare GameOver (play.rs:397-402). session.objectives is never seeded from rules.objectives (map tags never read).
- upstream: GameState/Logic read live state.teams/spawner; map objectives load with the map
- symptom: no campaign win/lose is possible and any future trigger would be wrong; objectives panel empty.
- campaign: blocks
- confidence: high
GAP-10: Launch resources / loadout pickers are fixture-only, signals unconnected, and loadouts are never applied
- severity: S3
- port: logic exists in Rust but no UI wiring. launch_loadout_dialog.gd lists fixture loadouts and emits loadout_chosen (:12,48) with no listener; Universe.get_loadout/update_loadout/get_launch_resources/update_launch_resources have no caller; logic_play does not apply rules.loadout (play.rs:366-369); Schematics::place_loadout has no campaign caller.
- upstream: Control.playNewSector(..., WorldParams params, ...) (Control.java:573) and LaunchLoadoutDialog/Universe.getLastLoadout
- symptom: Cannot choose starting resources or a core loadout before launch; every launch uses defaults/whatever the map ships.
- campaign: degrades
- confidence: high
GAP-11: Campaign difficulty/rules dialog writes nowhere and there is no API to change per-planet rules
- severity: S3
- port: logic exists in Rust but no wiring. campaign_rules_dialog.gd emits rule_changed (:12,55-60), unconnected; it is opened with an empty context (planet_dialog.gd:567-570), so it always displays Normal/off. MindCampaign.set_rules_json rejects campaign edits (rules_event guard; test at play.rs:680-693), and no facade method touches planet.campaign_rules or calls apply_rules. Persistence needs save_all, which is never called (GAP-4).
- upstream: CampaignRulesDialog.java:70-85 writes state.getPlanet().campaignRules; Logic.runWave applies difficulty.waveTimeMultiplier
- symptom: Difficulty buttons and rule toggles have no effect and are forgotten.
- campaign: degrades
- confidence: high
GAP-12: Sector eligibility/lock checks are bypassed; start_sector fakes ownership before play
- severity: S3
- port: logic partially implemented, no wiring. MindCampaign.start_sector accepts any name/sector: unknown planet falls back to PlanetId::new(0) (campaign.rs:133-135); no unlocked()/canSelect/tech gate is consulted; it sets sector.save/has_core=true before play (:138-141), which makes has_base()/unlocked()/is_captured() (sector.rs:485-531) true without any capture. Upstream PlanetDialog.canSelect additionally requires the preset's tech-tree unlock.
- upstream: PlanetDialog.java:429-462 (canSelect/preset.unlocked()), Sector.java:89-92
- symptom: Any sector (or a bogus sun sector) can be launched; a launched sector instantly looks captured even if the player abandons it.
- campaign: degrades
- confidence: high
GAP-13: Campaign victory and Erekir-specific flow are absent
- severity: S3
- port: logic partially implemented, no wiring. CampaignCompleteView/campaign_complete_dialog.tscn exist (manifest line 25) but nothing opens them; no is_last_sector runtime check (data at content/registries/sectors.rs, view at ui/campaign.rs:487), and sector_capture never consults it. Erekir-specific per-planet tech tree, shield sectors, and capture-replacement rules are dead; the campaign is built with EmptyNeighborhood (campaign.rs:110), so planet.neighbors is empty and update_base_coverage/invasion adjacency (planet.rs:374, universe.rs:652-664) can never work. planet_dialog._planet_sectors only synthesizes Erekir's start sector from the fixture (planet_dialog.gd:502-515).
- upstream: Control.java:178-185 (last-sector victory), SectorPresets.java:158,238, Planets.java:158 (capture replacements), Planet.java:386 (updateBaseCoverage)
- symptom: Beating a planet never shows campaign complete; Erekir is just a second sector generator with no unique rules; threat/invasion never account for neighbors.
- campaign: degrades
- confidence: high
GAP-14: HUD campaign feedback is dead (wave/enemies pinned to 0; wave/sector event signals never emitted)
- severity: S3
- port: logic exists but no emitter. MindHud.wave/enemies are constants never refreshed (mind-gdext/src/ui/hud.rs:39-44,175-190); the wave_event/sector_event signals (hud.rs:126,130) have no emit method (unlike push_unlock), and no GDScript connects them. MindCampaign.get_sector_state (with wasCaptured) and get_objective_state have no GDScript callers.
- upstream: HudFragment binds state.wave/state.enemies and the WaveEvent/SectorCaptureEvent/SectorLoseEvent toasts
- symptom: HUD shows no wave, no objective progress, and never tells the player a sector was captured/lost/invaded.
- campaign: degrades
- confidence: high
GAP-15: Map objectives never activate in the live game
- severity: S3
- port: logic exists in Rust but no host wiring. MapObjectivesRuntime (map_objectives.rs) and ObjectiveEnv are implemented/used only by headless (campaign_scenarios.rs:736,1257); play_new_sector never seeds session.objectives from rules.objectives/map tags; MindCampaign.complete_objective (campaign.rs:312) has no GDScript caller; TickSet::Objectives has no system.
- upstream: Logic.update -> state.objectives.update(); map objectives loaded with the map
- symptom: Scripted map objectives (copper counts, destroy units, timers) never show progress or complete.
- campaign: degrades
- confidence: high
GAP-16: Sector.being_played is never set, so attacked/frozen/captured predicates are wrong in any live path
- severity: S4
- port: logic exists in Rust but no wiring. The field defaults false (sector.rs:433,454) and is never assigned in production; Sector::is_being_played (:505) therefore always false, which mis-evaluates is_attacked/is_frozen/is_captured (:511-531) and would make Campaign::run_turn double-process the sector the player is in (universe.rs:583-593).
- upstream: Sector.isBeingPlayed() derives from live state.rules.sector (Sector.java:148-151)
- symptom: If turns or sector panels are ever driven, the current sector is misclassified (e.g. production while playing, "captured" while under attack).
- campaign: degrades
- confidence: high
Coverage notes (what is genuinely implemented/wired)
- mind-core campaign model is complete and unit-tested: Campaign/Planet/Sector/CampaignStats (universe.rs, planet.rs, sector.rs), CampaignRules/Difficulty with correct Java tables (campaign_rules.rs), play.rs play/capture/game-over state machine, tech_tree.rs spend/auto-unlock, objectives.rs/map_objectives.rs, saves.rs policy, waves.rs/spawn_group.rs. cargo test -p mind-core game covers them.
- mind-headless campaign scenarios exercise the intended loop end-to-end by hand: launch -> run_wave_campaign -> check_game_state -> was_captured + marker/objective clear, plus lose variant, rules guard, tech spend/auto-unlock/requirements, and run_turn production/export/import (campaign_scenarios.rs:878-1000,388-540). So the mechanics exist; only the client drive is missing.
- MindCampaign is instantiated in client/scenes/game.tscn:38 and bootstraps content/campaign; start_sector mechanically creates the session, sets rules.sector, increments attempts, and emits/discards the ordered events.
- MindSimHost.load_sector really loads groundZero.msav/planet-generated grids and logs (sim_host.rs:884-933); the renderer draws them (EV-0012/0014) with the camera fix (EV-0016).
- The planet globe/sector chooser UI is live for layout/topology: planet_view returns the grid mesh, start sector and preset remap (campaign.rs:462-538), matching EV-0010/EV-0011.
- MindUi.campaign_views() provides deterministic fixture JSON that drives all M5 dialogs (planet, research, loadouts, schematics, campaign_rules, campaign_complete) — everything renders, it is just not live state.
- MindCampaign.planet_view, get_tech_state, get_objective_state, get_sector_state, list_schematics, run_wave, run_turn, capture_sector are GDScript-callable; only planet_view/start_sector are actually called by shipped GDScript.
- The save container itself is implemented (MGRS writer/reader, sector slot naming/remap in saves.rs), and SimHost.request_save can serialize the live sim; campaign never calls it.
- UI event/signal plumbing seams exist but are disconnected: research_requested, rule_changed, loadout_chosen, sector_activated, play_requested.
Can a player go fresh -> capture Ground Zero -> launch a second sector?
No.
1. Fresh start -> launch works: Play > Campaign > Serpulo > Ground Zero > Launch loads the preset map, hides the menu, runs the sim, and renders terrain/HUD (the EV-0010/0012/0014/0016 fixes are real).
2. The chain breaks at the very next link: launch never installs the sector's rules/win condition (captureWave=10 -> rules.winWave, rules.waves) into either PlaySession or the sim (GAP-2), and the client has no wave/objective/game-state systems and never calls check_game_state (GAP-1). The session's teams/enemies are empty anyway (GAP-9). First missing link: "apply the preset/campaign rules at launch, then tick Objectives/WaveTimer/GameStateCheck". As shipped, there is literally no action that can capture Ground Zero.
3. Even if capture were forced through the MCP-only capture_sector, the planet map UI is a cached static fixture that will never show it (GAP-5), there is no in-game route back to the planet map at all (GAP-3), and restarting the process discards the in-memory campaign because nothing is saved or loaded (GAP-4). A second sector can only be launched by quitting to the menu and launching again, which starts a brand-new, empty campaign.
