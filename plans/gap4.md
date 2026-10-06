DIALOG/MENU INVENTORY & REACHABILITY AUDIT (NEW GAPS ONLY)
Paths: PORT= /home/clintonnguyen/g/code_examples/Mindustry-Godot/, UP= /home/clintonnguyen/g/code_examples/Mindustry/. Opener sweep: every open_dialog call in the port (19 hits) + [connection] in every .tscn; MindUi.open_dialog (PORT:client/rust/mind-gdext/src/ui/ui_host.rs:174) has no Rust caller, so GDScript caller = only reachability path. EV-0001..0034 not re-reported.
Missing/extra inventory table (upstream class -> port -> reachable)
Upstream (UP:core/src/mindustry/…)	Port (PORT:client/ui/dialogs/…)	Reachable?
AboutDialog.java	about_dialog.gd	yes (menu_fragment.gd:43)
AdminsDialog.java	admins_dialog.gd	no – caller opens server.admins (player_list_fragment.gd:100)
BansDialog.java	bans_dialog.gd	no – caller opens server.bans (player_list_fragment.gd:100)
CampaignCompleteDialog.java	campaign_complete_dialog.gd	no – no opener
CampaignRulesDialog.java	campaign_rules_dialog.gd	yes (planet_dialog.gd:570)
CanvasEditDialog.java	MISSING	no (absent)
ColorPicker (dialogs/ColorPicker.java)	color_picker.gd (picker)	yes (join_dialog.gd:211)
ContentInfoDialog.java	content_info_dialog.gd	no – no opener
CustomGameDialog.java (+MapListDialog)	custom_game_dialog.gd	partial (menu; hardcoded maps, no Rules)
CustomRulesDialog.java	custom_rules_dialog.gd	no – no opener
DatabaseDialog.java	database_dialog.gd	yes (menu; empty, EV-0028)
DiscordDialog.java	discord_dialog.gd	yes (menu_fragment.gd:185)
EditorMapsDialog.java	editor_maps_dialog.gd	partial (menu; all actions dead)
EffectsDialog.java	MISSING	no (absent)
FileChooserDialog.java	file_chooser_dialog.gd	yes (load_dialog.gd:229)
FullTextDialog.java	full_text_dialog.gd	no – no opener
GameOverDialog.java (restart)	game_over_dialog.gd	no – no opener
HostDialog.java	host_dialog.gd	no – no opener
IconSelectDialog.java	icon_select_dialog.gd	no – no opener
JoinDialog.java	join_dialog.gd	partial (menu; connect dead, no Host)
KeybindDialog.java (controls)	keybind_dialog.gd	yes (settings_menu_dialog.gd:117)
LanguageDialog.java	language_dialog.gd	yes (settings_menu_dialog.gd:117)
LaunchLoadoutDialog.java	launch_loadout_dialog.gd	no – no opener
LoadDialog.java	load_dialog.gd	partial (menu; always empty)
LoadoutDialog.java	loadout_dialog.gd	no – no opener
MapListDialog.java (abstract)	merged into custom/editor maps	n/a
MapPlayDialog.java	map_play_dialog.gd	partial (no rules/preview/highscore)
ModBrowserDialog.java	mod_browser_dialog.gd	yes (mods_dialog.gd:354)
ModsDialog.java	mods_dialog.gd	yes (menu)
PaletteDialog.java	palette_dialog.gd	no – no opener
PausedDialog.java	MISSING	no (absent)
PlanetDialog.java	planet_dialog.gd	yes (menu Play>Campaign)
ResearchDialog.java	research_dialog.gd	yes (planet_dialog.gd:564)
SaveDialog.java	save_dialog.gd	no – no opener
SchematicsDialog.java	schematics_dialog.gd	partial (list; actions dead)
SectorSelectDialog.java	sector_select_dialog.gd	no – no opener
SettingsMenuDialog.java	settings_menu_dialog.gd	yes (menu)
TraceDialog.java (traces)	trace_dialog.gd	no – no opener
logic/LogicDialog.java	MISSING	no (absent)
editor/MapEditorDialog.java	scenes/editor/map_editor_dialog.gd	no – show_dialog() never called
—	credits_dialog.gd (extra split)	yes (about_dialog.gd:62)
—	widget_gallery.gd (extra debug)	no (no opener)
Fragments (all 14 present): reachable = menu, hud, performance, placement, fade_in. No opener = loading (set_progress loading_fragment.gd:21), hints (:31), minimap (:38), block_config (:23), block_inventory (:22), plan_config (:23), chat (:123), console (:105), player_list (:103).
Ranked NEW gaps
GAP: No in-game pause menu — PausedDialog missing entirely
- severity: S1
- port: absent (no paused_dialog.gd, no paused in PORT:client/ui/dialogs_manifest.json:3-40; PORT:client/ui/fragments/hud_fragment.gd:28-83 and hud_fragment.tscn:106 have only a "Paused" banner, no menu control)
- upstream: UP:core/src/mindustry/core/UI.java:214; UP:core/src/mindustry/core/Control.java:750 (ui.paused.show()); UP:core/src/mindustry/ui/dialogs/PausedDialog.java:20-118
- symptom: in-game there is no Back to menu, Save, Load, Settings, Host, Abandon sector, objective full-text or Quit confirmation; the only settings dialog lives in the main menu
- campaign: blocks (cannot save, exit a campaign or change rules in-game)
- confidence: high
GAP: Load Game and Save Game are not plumbed; save dialog unreachable
- severity: S2
- port: menu_fragment.gd:38 opens load with "{}"; load_dialog.gd:97 reads context().get("slots",[]) and no producer of "slots" exists (grep across client: none); slot_selected (load_dialog.gd:13,220) has no listener; card action buttons save/trash/pencil/export (load_dialog.gd:152-155) have no pressed handlers; save_dialog.gd has no caller
- upstream: UP:.../dialogs/PausedDialog.java:91-92 (save::show, load::show); UP:.../dialogs/SaveDialog.java:11; LoadDialog lists control.saves
- symptom: Load Game always renders "@save.none"; there is no in-game Save; even a hypothetical slot click would do nothing
- campaign: blocks (no save/load of campaign state)
- confidence: high
GAP: Join Game never submits a connection; Host dialog unreachable
- severity: S2
- port: join_dialog.gd:12,240,245 emits connect_requested; no listener anywhere (grep). No Host button in join_dialog.gd:33-41; host_dialog.gd has no caller. MindNet.join_match (PORT:client/rust/mind-gdext/src/net/mod.rs:261) has no GDScript caller; MindUi exposes no server-list/connect endpoint (ui_host.rs #[func] list)
- upstream: UP:.../dialogs/JoinDialog.java (connect/host), PausedDialog.java:98-105; HostDialog.java:15
- symptom: clicking a remembered server or OK in Add Server does nothing; hosting is impossible
- campaign: n-a (multiplayer)
- confidence: high
GAP: Game-over and campaign-complete dialogs exist but are never shown
- severity: S2
- port: restart/campaign_complete in dialogs_manifest.json:22,25, files game_over_dialog.gd / campaign_complete_dialog.gd; no open_dialog/show_dialog caller (grep)
- upstream: UP:core/src/mindustry/core/Logic.java:438 (ui.restart.show(winner) after game over); Control.java:182 (ui.campaignComplete.show(...))
- symptom: on defeat/completion the player gets no result dialog and no Continue/Restart path
- campaign: degrades (completion unreachable)
- confidence: high
GAP: Editor flow dead-ends — EditorMapsDialog actions unconnected and EditorDialog never shown
- severity: S2
- port: editor_maps_dialog.gd:12 emits editor_map_action at :32,:48; no listener (grep). map_editor_dialog.gd:45 show_dialog() has no caller (grep); editor node is statically in game.tscn:65 but hidden and never shown; editor dialogs are not in dialogs_manifest.json
- upstream: UP:.../dialogs/EditorMapsDialog.java:19; ClientLauncher.java:346-355; UI.java:219 (ui.maps)
- symptom: menu > Editor lists maps, but clicking a map, Import or Export does nothing and the map editor never opens
- campaign: n-a
- confidence: high
GAP: In-game overlay fragments have no opener (block config, inventory, plan config, chat, console, player list, minimap, hints, loading)
- severity: S2
- port: entry points exist but zero callers — block_config_fragment.gd:23 configure, block_inventory_fragment.gd:22 set_items, plan_config_fragment.gd:23 configure, hints_fragment.gd:31 show_hint, chat_fragment.gd:123 toggle, console_fragment.gd:105/113, player_list_fragment.gd:103, minimap_fragment.gd:38, loading_fragment.gd:21 set_progress; no GDScript/Rust references to these nodes (grep)
- upstream: UP:.../input/DesktopInput.java:234,450; fragments/HudFragment.java:389,408,427; ui/Minimap.java:108
- symptom: buildings cannot be configured, no inventory withdraw, no fullscreen map, no chat/console/player list, no loading overlay/hints
- campaign: degrades (block configuration/withdraw is core interaction)
- confidence: high
GAP: Custom Game map chooser is hardcoded and MapPlay ignores gamemode/rules
- severity: S3
- port: custom_game_dialog.gd:219-223 only opens map_play; no Rules/Customize button in _build (:45-68); map_play_dialog.gd:37-44 shows mode buttons but _play (:49-59) calls MindCampaign.start_map(name) ignoring mode/playtest and no rules dialog; campaign_section("maps") comes from default_map_entries() (PORT:client/rust/mind-gdext/src/ui/ui_host.rs:468-471; mind-core/src/ui/campaign.rs:711-724 -> maps/mod.rs:44) with 0x0 sizes and no custom/mod maps
- upstream: UP:.../dialogs/MapPlayDialog.java:17,52,75 (CustomRulesDialog),89 (control.playMap(map, rules, ...)); CustomGameDialog.java:5-16
- symptom: custom maps stored in the maps registry never appear; starting a custom map always uses whatever rules are already loaded; no difficulty/rule selection
- campaign: degrades
- confidence: high (maps fixture) / medium (mode application side)
GAP: Sector launch skips LaunchLoadout/Loadout dialogs (unreachable)
- severity: S3
- port: planet_dialog.gd:536-551 _launch calls start_sector directly; launch_loadout_dialog.gd and loadout_dialog.gd have no caller (grep); loadout_chosen/loadout_changed have no listeners
- upstream: UP:.../dialogs/PlanetDialog.java:53,1407 (loadouts.show(block, from, sector, ...)); LaunchLoadoutDialog.java:21-32,138; LoadoutDialog.java:15
- symptom: launching with a core schematic never shows the loadout picker, so sector launch composition cannot be chosen
- campaign: degrades
- confidence: high
GAP: Schematics dialog actions are dead; no import/export/icon selection
- severity: S3
- port: schematics_dialog.gd:88-94 emits schematic_action (edit/export/delete) with no listener (grep); no Import button; icon_select_dialog.gd has no caller
- upstream: UP:.../dialogs/SchematicsDialog.java import/export/edit flows; :435 (icon select TODO); IconSelectDialog.java:16
- symptom: Edit/Export/Delete do nothing and schematics cannot be imported or exported
- campaign: degrades
- confidence: high
GAP: Player-list Bans/Admins open the wrong dialog keys; Trace never reachable; no per-player actions
- severity: S3
- port: player_list_fragment.gd:45,97-100 passes "server.bans"/"server.admins" after trimming @, but manifest keys are bans/admins (dialogs_manifest.json:26-27); traces has no reference anywhere; _on_player (:75-77) only sends spectate, no ban/kick/trace menu
- upstream: UP:.../fragments/PlayerListFragment.java:149-183 (ban/kick/trace/team); UI.java:216-218
- symptom: Bans/Admins buttons log an unknown dialog and do nothing; traces dialog unreachable; no player moderation
- campaign: n-a
- confidence: high
GAP: Orphan dialogs instanced but never opened; missing Effects/CanvasEdit/Logic dialogs
- severity: S4
- port: content_info_dialog.gd, full_text_dialog.gd, icon_select_dialog.gd, palette_dialog.gd, sector_select_dialog.gd all have no open_dialog caller (database_dialog.gd:7 comment claims content opens but code stops at :70); EffectsDialog/CanvasEditDialog/LogicDialog absent
- upstream: UP:.../dialogs/DatabaseDialog.java:212, ResearchDialog.java:668 (content info); PausedDialog.java:79 (fullText); JoinDialog.java:348, HostDialog.java:32 (palette); PlanetDialog.java:75,657 (sector select); LStatements.java:1806 (effects); CanvasBlock.java:311 (canvas edit); LogicBlock.java:746 (logic)
- symptom: dead UI surface / missing info popups, objective full text, map-processor icon picker, host color palette, effects picker and logic editor
- campaign: n-a (logic editor affects optional processors)
- confidence: high
Coverage notes
- Reachable verified by callers: about, credits, settings, language, controls, database, discord, mods, mod_browser, join(open only), load(open only), picker, custom, map_play, editor_maps(open only), file_chooser, planet, research, schematics(open only), campaign_rules, widget_gallery(debug only).
- Every manifest dialog is statically instanced (ui_root.tscn:111-146) and cross-checked by ui_root.gd:195-199 + manifest.rs tests; no existing port dialog is missing from the ABI. menu_openable is read by nothing in GDScript (grep) – dead ABI field.
- Campaign/map read models are fixtures: campaign_views() uses CampaignViews::vanilla_fixture() + default_map_entries() (ui_host.rs:466-475; mind-core/src/ui/campaign.rs:770,711); a live MindPreview.maps_list exists (mind-gdext/src/editor/preview.rs:137) but is never fed to the Custom Game/Editor map grids.
- Save infrastructure exists (mind-core/src/game/saves.rs, io/save/slot.rs) but there is no UI endpoint for slot listing; MindCampaign.save_slot is only a settings marker (mind-gdext/src/campaign.rs:440-442).
- Keybinds for chat/console/minimap exist (mind-core/src/input/binding.rs:721,775,808) but nothing dispatches them to the fragments (grep), matching the GAP above.
- Not filed separately: mobile menu omits About (menu_fragment.gd:52-61 vs upstream MenuFragment.java:96 info banner, :151 about); checkPlay mod-error gate missing (menu_fragment.gd:264-280 vs MenuFragment.java:244-250); mod addButton custom menu entries unsupported; Steam Workshop correctly absent per EV-0002.
- All "unreachable" calls are grep-negative across client/**/*.gd, client/**/*.tscn (only [connection] entries are SimHost->StateInspector/TileGrid), and client/rust (open_dialog( single definition; no Rust caller).
