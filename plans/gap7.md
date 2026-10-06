SAVE/LOAD PERSISTENCE AUDIT — Mindustry-Godot vs Mindustry (2cd7aeec)
Scope: client/rust/mind-core/src/{game/saves.rs,io/**}, client/ui/dialogs/{save,load}_dialog.gd, mind-gdext glue. Ledger EV-0001..0034 read; none cover saves/load. No prior finding re-reported.
Ranked NEW gaps
GAP: No in-game save exists: pause menu absent, SaveDialog unreachable, save handler is a settings marker, autosave pump dead  
- severity: S1  
- port: client/ui/dialogs_manifest.json:20-21 (load menu-openable, save menu_openable:false; no paused entry at all though ui/manifest.rs:402-412 expects one); ui/fragments/hud_fragment.gd:1-83 (HUD has zero menu/save/quit controls); ui/dialogs/save_dialog.gd:11,57,62 (save_requested emitted, no receiver in tree or .tscn); mind-gdext/src/campaign.rs:440-446 (save_slot writes mcp-save-<name> marker only); game/saves.rs:217-264,310-336 (update/save_sector have no runtime caller — grep only tests); mind-gdext/src/campaign.rs/sim_host.rs: no runExitSave equivalent (absent)  
- upstream: PausedDialog.java:14-17,82-93,116,185-216 (Save Game + Save & Quit -> control.saves.getCurrent().save()); SaveDialog.java:24-32,43-58; Saves.java:253-263,330-341; Control.java:678 (saves.update() per frame)  
- symptom: after launching a campaign sector the only way out is closing the window; nothing is written, no autosave ever fires, the Save Interval slider is inert  
- campaign: blocks  
- confidence: high
GAP: Main-menu Load Game is always empty and selection cannot load  
- severity: S1  
- port: ui/fragments/menu_fragment.gd:38 opens load with ctx "{}"; mind-gdext/src/ui/ui_host.rs:174-201 passes that context through; ui/dialogs/load_dialog.gd:97 reads context().get("slots", []) (never populated anywhere — grep "slots" finds only the two dialogs); load_dialog.gd:219-221 emits slot_selected, unconnected; game/saves.rs:104-122,473-479 (Saves::load/load_slot never called)  
- upstream: MenuFragment.java:215 (ui.load::show); LoadDialog.java:85-99,224-263 (lists control.saves.getSaveSlots(), runLoadSave -> cautiousLoad + slot.load() + state.set(playing)); Saves.java:64-122,314-328  
- symptom: Load Game shows "@save.none" forever; clicking anything (if listed) does nothing; no sector or universe is ever restored  
- campaign: blocks  
- confidence: high
GAP: The only engine save path writes a non-campaign, tile-only save (no entities/rules/sector tags) and abortive load  
- severity: S2  
- port: mind-core/src/io/sim_io.rs:146-164 (save sets ctx.map only, tags = base_meta_tags(w,h,0,"sim")); versions/v1.rs:858-882 (rules {}, sectorPreset "", playtime/saved 0); sim_io.rs:167-196 (load restores grid only; buildings parked in pending_buildings); sim_host.rs:730-746 (request_save/request_load have no GDScript caller — grep finds only MCP comments)  
- upstream: SaveVersion.java:130-131,141-... (saved=Time.millis(), playtime=getTotalPlaytime(), rules/mod/entities regions); SaveIO.save/load  
- symptom: even if a button were wired to request_save, the file would not list as a save (Saves sector/rules logic sees no sector), lose all units/building state on reload, and read back with wave 0/playtime 0  
- campaign: blocks (resume impossible)  
- confidence: high
GAP: Campaign Universe state (research, planets, stats, turn, sector info) is never persisted or restored  
- severity: S2  
- port: mind-gdext/src/campaign.rs:61-120 (fresh Campaign::from_registry + MemoryUnlockStore each boot); campaign.rs:440-452 (markers); game/universe.rs:691-707 (save_all/load_all test-only); campaign.rs:180-198 writes sector info into self.settings, but that store is never force_saved/loaded (io/settings.rs:356,366 needs a caller; campaign.rs calls none); campaign.rs:238-272 research spends memory-only  
- upstream: Universe.java:29,82,293-300 (load() at boot, save() on turns); PausedDialog.java:43-46 (saveStats); Saves.java:318-328  
- symptom: quit and relaunch: research unlocks, captured/base sectors, campaign stats and turn all reset; Load Game has no progression to restore even conceptually  
- campaign: blocks  
- confidence: high
GAP: .msav/save import and export are dead ends (file chooser and desktop import only log)  
- severity: S2  
- port: load_dialog.gd:224-229 opens file_chooser; file_chooser_dialog.gd:96-98 emits file_chosen unconnected; platform/desktop.rs:59-87 logs "importing save ... plan-04/plan-14" and returns true without doing anything; game/saves.rs:357-377 (import_save) and io/save/slot.rs:179-195 (export_file) have no callers; export/rename/delete/autosave icons load_dialog.gd:152-155,189-196 have no handlers beyond toggle visuals  
- upstream: LoadDialog.java:139-145,196-222 (FileChooser.open("msav").submitMulti -> control.saves.importSave; export via FileChooser.export); Saves.java:273-282,461-485  
- symptom: Import Save opens a chooser, choosing a file does nothing; no export, rename or delete  
- campaign: degrades  
- confidence: high
GAP: Multiplayer campaign persistence is server-capable but the client never writes or reads it  
- severity: S2  
- port: server tables/reducers/views server/spacetimedb/src/campaign/mod.rs:25-101,206-269,552-605; client wrappers mind-stdb/src/connector.rs:699-785; no gdext caller of create_campaign/save_sector_info/add_campaign_stat and no reader of my_campaign_sectors/my_campaign_unlocks (grep across mind-gdext, mind-stdb minus bindings); RelayMatch.campaign_id is never populated client-side (grep campaign_id in mind-gdext = 0 hits)  
- upstream: single-player Universe.java:293-300 settings persistence; port-specific STDB is the MP store per server AGENTS  
- symptom: quit/relaunch a hosted campaign: no campaign row, sector state and unlocks are absent; late joiners read nothing  
- campaign: blocks (MP)  
- confidence: high
GAP: Map editor shell unreachable; editor-map import/export/open emit unconnected signals; editor writes a different data dir than Open Data Folder  
- severity: S3  
- port: ui/dialogs/editor_maps_dialog.gd:29-33,43-49 emits editor_map_action with no receiver; scenes/editor/map_editor_dialog.tscn:9 visible=false and scenes/editor/map_editor_dialog.gd:45-52 show_dialog() has no caller (grep MapEditorDialog outside its own scene = 0); scenes/editor/map_load_dialog.gd:71-72 orphan scene; working Rust mind-gdext/src/editor/mod.rs:573-604,944-1078 saves/maps via user://maps; mind-gdext/src/ui/ui_host.rs:651-654 opens native Paths::resolve(None) root (config.rs:62-76), while mods live in user://mods and selected-block settings in user://settings.json  
- upstream: EditorMapsDialog.java:44+ (import/export/open -> Maps.importMap/Maps.saveMap), Maps.java:192-206, MenuFragment Editor routing  
- symptom: menu > Editor lists fixture map names; import/export/open/rows do nothing; the editor (and its Ctrl+S map save) cannot be opened by a player; Open Data Folder shows no editor maps/mods  
- campaign: n-a (degraded tooling)  
- confidence: high
GAP: Schematic save/load/export/import are stubs; library shows the static fixture only  
- severity: S3  
- port: ui/dialogs/schematics_dialog.gd:51,74-93 (schematic_action emitted, unconnected; data from campaign_section("schematics") fixture); mind-gdext/src/campaign.rs:429-438 (write_schematic_selection returns "", place_schematic_base64 returns false); mind-stdb/src/connector.rs:790-798 save_schematic has no caller; list_schematics is MCP-only  
- upstream: SchematicsDialog.java:160,248,285,318-320 (export/import/file buttons -> Schematics.write/FileChooser); Schematics.java:454,553 (base64 codec)  
- symptom: Edit/Export/Delete buttons do nothing; a built base cannot be saved as .msch, and no schematic can be imported/placed  
- campaign: degrades  
- confidence: high
GAP: Save metadata is never stamped and previews/date/playtime never render  
- severity: S3  
- port: io/save/versions/v1.rs:858-882 hardcodes saved=0/playtime=0 and no writer overwrites them (grep "saved"/"playtime" only reads); io/save/slot.rs:22-206 lacks get_play_time/get_date/get_map/mode; slot.rs:197-206 queues a preview rerender but no renderer exists; load_dialog.gd:164-173 draws a static map glyph, _meta_lines would show empty fields  
- upstream: SaveVersion.java:130-131; Saves.java:343-367,385-399,433-435 (preview texture, getPlayTime, getDate, getMap, mode)  
- symptom: save cards could never show map, wave, date or playtime, and no screenshot preview  
- campaign: degrades  
- confidence: high
GAP: Corrupted-save and missing-mod handling exists in core but has no player-facing path  
- severity: S3  
- port: game/saves.rs:397-407 (cautious_load never called); io/save/mod.rs:118-198 falls back to backup with only log::error/warn; no @save.corrupted / mod.missing prompt anywhere (grep absent); SaveSlot.has_external_assets (slot.rs:106-116) is never consumed  
- upstream: Saves.java:401-410 (cautiousLoad shows the mod.missing confirm); LoadDialog.java:256-260 (@save.corrupted); LoadDialog.java:236-255 (missing-assets report)  
- symptom: a corrupt or mod-dependent save produces no UI feedback; the player cannot tell why it failed or see a recovery option  
- campaign: blocks (for affected saves)  
- confidence: high
GAP: Save-policy parity details: campaign playtime ignored, megabase/remap cleanup incomplete, interval default deviates  
- severity: S4  
- port: game/saves.rs:47,227-235 (is_campaign field unused; no planet stats playtime unlike upstream Saves.java:210-212); saves.rs:124-142 deletes old megabase files but never clears sector info (upstream clearInfo, Saves.java:56); saves.rs:182-203 copies source info to dest but never clears the source when it is not a remap target (upstream Saves.java:189-192); saves.rs:28 default 10 -> 600 s vs upstream absent-key behaviour (Saves.java:219)  
- upstream: Saves.java:50-62,116-193,204-235,249-263  
- symptom: campaign playtime never accrues to a planet; remapped legacy saves can leave stale sector info  
- campaign: degrades  
- confidence: medium
Coverage notes
- The file layer is genuinely solid: MGRS magic + deflate, named regions, temp+sync+rename, backup rotation and restore-on-error, meta/load backup fallback, corrupt listing skip, version-error text — tested (io/save/mod.rs:84-198, slot.rs:221-284). Gaps above are wiring/policy, not the container.
- Saves::update math, autosave-eligible check and tests mirror upstream (saves.rs:217-264,574-621); only the caller is missing (Control.java:678).
- MindUi.campaign_views() returns a frozen vanilla_fixture cached on first call (ui_host.rs:466-476; ui/campaign.rs:770-830), so planet/sector/research/schematics/loadout/editor-map dialogs all display static data; live MindCampaign mutations are never reflected and no save can refresh them.
- play_sector's from-save branch fires RulesLoadEvent{from_save:true} but reads no file (mind-core/src/game/play.rs:303-345); the UI calls start_sector -> play_new_sector (planet_dialog.gd:536-551), which always generates/loads the preset map.
- The dedicated server (mind-headless) has a working save/load incl. entities and autosave rotation (server/host.rs:226-243, server/commands.rs:787-815); the Godot client never bridges to it.
- msav-import is enabled in mind-gdext/Cargo.toml, so upstream designed preset maps load (io/legacy.rs:41-70); that is map import, not player save-file import.
- SpacetimeDB campaign tables/views and connector wrappers are complete and drift-gated (server/build.sh --check); consumption is the missing half.
- Settings persistence itself is verified in the ledger (EV-0001/0019/0021/0025 etc.); saveinterval is stored but inert because nothing reads it into a running autosave loop.
- No pause menu also removes upstream's Abandon/Objective/Custom Rules/Host/Quit actions (PausedDialog.java:51-159), adjacent to this audit.
- slots context is the only data channel for Load/Save dialogs (load_dialog.gd:97, save_dialog.gd:37); no endpoint in mind-gdext builds it, so even a future Saves::load needs a new bridge endpoint.
Explicit answer
Can a campaign player save, quit, and resume right now? No. The only reachable path is Play > Campaign > Serpulo/Erekir > OK > sector > Launch (planet_dialog.gd:536-551), which calls MindSimHost.load_sector (preset map/generator, sim_host.rs:884-933) and MindCampaign.start_sector (in-memory campaign, sets sector.save name only, campaign.rs:122-155). In-game there is no pause menu, no Save dialog reachable (menu_openable:false, no caller of open_dialog("save")), no autosave pump, and no quit-to-menu; MindCampaign.save_slot/load_slot are MCP settings markers, not save/load. The main-menu Load Game opens but is permanently empty (slots never supplied, slot_selected unconnected, Saves::load never called) and loading would restore nothing: no sector world, and no campaign Universe state (research/planets/stats/turn are memory-only, Campaign::save_all/load_all unused). Net: a campaign session is lost on exit and every Launch starts the sector fresh.
