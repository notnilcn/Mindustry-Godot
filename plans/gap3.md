Audit: in-game HUD (Mindustry-Godot vs upstream 2cd7aeec)
Roots: PORT = /home/clintonnguyen/g/code_examples/Mindustry-Godot · UP = /home/clintonnguyen/g/code_examples/Mindustry. Read the ledger first; EV-0013/0017/0018/0012/0014 re-checked and not re-reported. All findings below are new.
Explicit answers: There is no in-game pause menu — no PausedDialog equivalent, no ESC handler, no pause/save/load/settings/quit panel (confirmed by absence in dialogs_manifest.json, scenes/ui/dialogs, ui_root.gd, input/mod.rs:84-89). Campaign objectives are NOT shown to the player — MindCampaign.get_objective_state() exists but has zero callers and the HUD status path that would render them is never fed.
GAP: In-game pause menu does not exist; Escape does nothing
- severity: S1
- port: absent — client/ui/dialogs_manifest.json has no paused dialog; client/rust/mind-gdext/src/input/mod.rs:84-89 drains/clears key events without dispatch; no ESC/ui_cancel handling in client/ui/ui_root.gd
- upstream: core/src/mindustry/core/Control.java:746-750 (Binding.menu -> ui.paused.show()), ui/dialogs/PausedDialog.java:14-217
- symptom: In-game you cannot pause, resume, open settings, save, load, host, or quit; the only exit is killing the app. The paused banner exists but no input can reach a paused state.
- campaign: blocks
- confidence: high
GAP: Game-over / win-lose dialog is unreachable; campaign continue/next-sector flow missing
- severity: S2
- port: client/ui/dialogs/game_over_dialog.gd:20 (only a @menu button, no @continue); client/ui/dialogs_manifest.json:22 (restart) has no caller; check_game_state (mind-core/src/game/play.rs:385) is never invoked by mind-gdext/src/campaign.rs; campaign_complete also never opened
- upstream: ui/dialogs/GameOverDialog.java:43-52 (show(winner)), 101-140 (campaign @continue -> planet, else @menu), stats at 54-99
- symptom: Losing your last core or winning never shows a result; play continues in a dead/PvE-decided sector; no return-to-planet or next-sector loop; stats/win/lose text never displayed.
- campaign: blocks
- confidence: high
GAP: Wave/enemy/countdown HUD permanently empty; no Skip Wave button or key
- severity: S2
- port: client/ui/fragments/hud_fragment.gd:50-51 reads status_text; client/rust/mind-gdext/src/ui/hud.rs:39-44 declares wave/enemies and 175-190 never assigns wave, enemies, waiting, ping, tps, position_text or status_text; mind-core/src/ui/hud_text.rs:53-134 composes the exact upstream text but is only exercised by the headless oracle
- upstream: ui/fragments/HudFragment.java:879-1184 (wave cap, enemy count, next-wave timer, health/ammo bars), 1234-1236 canSkipWave, 502-510 skip button, 461-467 skip keybind
- symptom: "Wave N", "Enemies N", "Next wave in s" and the wave button never appear; you cannot see or skip an incoming wave. Banner texts are also hardcoded English (Paused, Waiting for players) instead of @paused/@waiting.players.
- campaign: degrades
- confidence: high
GAP: Campaign objectives never shown
- severity: S2
- port: no caller of MindCampaign.get_objective_state() (client/rust/mind-gdext/src/campaign.rs:322-338); hud_text.rs:63-78 objectives branch has no producer; hud_fragment.gd binds only status_text
- upstream: HudFragment.java:1083-1107 (qualified, non-hidden objectives rendered into the status label)
- symptom: At Ground Zero and any objective sector the player sees no objective text; the campaign goal is undiscoverable.
- campaign: blocks
- confidence: high
GAP: Configurable buildings cannot be opened/configured; no in-world block info
- severity: S2
- port: client/ui/fragments/block_config_fragment.gd:23 configure() has zero callers; client/rust/mind-gdext/src/sim_host.rs:145-179 makes left-click always Place and right-click always Break; only the harness can issue Configure (world/harness.rs:412, determinism/command.rs:490)
- upstream: input/InputHandler.java:1989-1994 (config.showConfig), ui/fragments/BlockConfigFragment.java:39-67; info key PlacementFragment.java:260-272
- symptom: Sorters, unloaders, processors, routers, logic blocks cannot be configured; right-clicking a building destroys it instead; no info dialog on the hovered/selected block.
- campaign: blocks
- confidence: high
GAP: Block inventory fragment is dead code
- severity: S3
- port: client/ui/fragments/block_inventory_fragment.gd:22-38 set_items/take_requested have no callers; no client path emits SimCommand::Inventory
- upstream: ui/fragments/BlockInventoryFragment.java:48-59 showFor, 73-87 takeItem; InputHandler.java:2015-2019
- symptom: Cannot inspect or withdraw items from containers/vaults (or tap cores) near the cursor.
- campaign: degrades
- confidence: high
GAP: Plan-config popup is a no-op stub
- severity: S3
- port: client/ui/fragments/plan_config_fragment.gd:29-35 _rebuild() contains only a comment (no widgets); configure() never called
- upstream: ui/fragments/PlanConfigFragment.java:34-72 (getPlanConfigs -> ItemSelection table before placement)
- symptom: A queued plan with a configurable target cannot be re-configured before it is built.
- campaign: degrades
- confidence: high
GAP: Placement input is single-click only — no rotation, drag/line, block replacement, or deconstruction drag
- severity: S2
- port: client/rust/mind-gdext/src/input/mod.rs:84-89 explicitly logs/counts and clears every translated event ("desktop controller is M2"); the only world input is sim_host.rs:145-179 place/break with rot: 0 (mind-core/src/sim/mod.rs:259-261, checksum note 402-403); core DesktopController/InputState/update_line/break_rect are unused by mind-gdext; Command::Place on an occupied tile is a deterministic no-op (sim/mod.rs:261-273)
- upstream: input/DesktopInput.java:753-826 (drag line, rotate-while-dragging, break rect), InputHandler.rotateBlock/flushPlans; PlacementFragment.java:186-258 block-select hotkeys
- symptom: R/rotate does nothing; you cannot drag a conveyor line, rotate valid multi-tiles, replace/upgrade occupied tiles, or hold-right drag-deconstruct.
- campaign: degrades
- confidence: high
GAP: Core/sector item display never fed
- severity: S3
- port: client/scenes/ui/fragments/hud_fragment.tscn:71 is an empty CoreItems Control; client/ui/widgets/core_items_display.gd:20 update_items has no caller (only widget_gallery.gd:35); hud_fragment.gd reads no core-item model
- upstream: HudFragment.java:592-624 (coreItems collapser), ui/CoreItemsDisplay.java
- symptom: No copper/lead/graphite/etc. counts at the bottom of the HUD in campaign or attack mode.
- campaign: degrades
- confidence: high
GAP: Minimap is a static placeholder; fullscreen minimap unreachable
- severity: S3
- port: client/ui/widgets/mind_minimap.gd:38-50 draws only a dark grid ("Placeholder until plan 16 binds the minimap texture"); hud_fragment.gd never connects the widget's pan_requested/zoom_changed/tapped; the fullscreen fragment is hidden:true with no toggle (minimap_fragment.gd, dialogs_manifest.json:49); native get_texture (mind-gdext/src/render/minimap.rs:272) has no GDScript consumer
- upstream: ui/fragments/MinimapFragment.java:39-152 (texture + drawEntities + click-to-pan + ping), HudFragment.java:336-351 embeds the interactive Minimap
- symptom: The 140px minimap shows no terrain/units/attack indicators and does not pan/zoom the camera; the fullscreen map (M) cannot be opened.
- campaign: degrades
- confidence: high
GAP: Hints never displayed; no skip; catalogue reduced
- severity: S3
- port: client/ui/fragments/hints_fragment.gd:12-21 lists 8 hints, show_hint/complete_hint have no callers, manifest entry is hidden:true, no @hint.skip button; HintsFragment data/event logic absent
- upstream: ui/fragments/HintsFragment.java:42-160 (auto-advance, world/derelict events, playtime gate) and 118-137 (text + Skip button)
- symptom: No onboarding hints (drills, conveyors, turrets, waves, core items, research) appear anywhere; no skip.
- campaign: degrades
- confidence: high
GAP: Content-info dialog unreachable from the world
- severity: S3
- port: client/ui/dialogs_manifest.json:10 registers content, but no GDScript/Rust opens it (database_dialog.gd:70 is an @none placeholder); no blockInfo key handling in mind-gdext; placement_fragment.gd has no info box/hover display
- upstream: PlacementFragment.java:260-272 (ui.content.show) and 347-438 (hover/selected info box with requirements, unplaceable reason, "?" button)
- symptom: No way to read block/unit stats, costs or descriptions from the world.
- campaign: degrades
- confidence: high
GAP: Loading overlay never shown for loads; Cancel button is inert
- severity: S4
- port: client/ui/fragments/loading_fragment.gd:11-13 declares _button but never connects pressed; set_progress has no callers; only boot-time hide_loading() from ui_root.gd:134-136; Loading.../Cancel hardcoded
- upstream: ui/fragments/LoadingFragment.java:44-55 (progress bar + cancel + ESC), 92-96 setButton, 128-138 fade-out hide
- symptom: Map/save/mod loads show no progress or cancel UI.
- campaign: degrades
- confidence: high
GAP: Console fragment unreachable
- severity: S4
- port: client/ui/fragments/console_fragment.gd:105-124 toggle/toggle_mobile have no callers (manifest hidden:true); key events are discarded (input/mod.rs:84-89)
- upstream: console registered in core/UI.java:183 and toggled by binding/settings; ConsoleFragment.java
- symptom: In-game console cannot be opened from the client (only MindUi.console_execute via MCP).
- campaign: n-a
- confidence: high
GAP: MindHud emitter surface has no producers (hudText/announce/unlock/wave/sector)
- severity: S4
- port: client/rust/mind-gdext/src/ui/hud.rs:108-160 signals hud_text/toast/unlock/announce/wave_event/sector_event and push_* methods have zero callers; relay handlers in net/relay.rs never route HudText/announce; position_text/ping/tps/waiting never assigned (hud.rs:175-190)
- upstream: HudFragment.java:240-284 (WaveEvent/Sector capture/lose/invasion), 720-873 (setHudText, toasts, unlock popups)
- symptom: No guardian warnings, sector capture/loss toasts, unlock notifications or server HUD text; the position/mouse-tile label stays empty.
- campaign: degrades
- confidence: high
Coverage notes (genuinely working)
- Block/category picker loads the real catalogue, renders 46px icon buttons, category toggle group, search filter and block selection into SimHost.selected_block (placement_fragment.gd:40-143); residual filtering/EV-0013 issues aside, the widget itself works.
- MindHud refresh loop correctly supplies FPS, static memory, pause/waiting flags from the sim host (hud.rs:175-190, hud_fragment.gd:46-64), and the paused/waiting banners toggle visibility.
- Pause governor (mind-core/src/ui/pause.rs) reference-counts pause:true dialogs and correctly restores wasPaused; it is wired to MindUi.show_dialog/close_dialog (ui_host.rs:191-260) — only in-game entry points are missing.
- Toast/announce/show_text/show_confirm/text_input prompts from MindUi render and resolve back into Rust (ui_root.gd:219-344).
- FadeInFragment.start is invoked at boot and fades the black cover (ui_root.gd:118-122, fade_in_fragment.gd:13-16).
- Console commands execute through the Rust registry with history/scroll and mobile buttons once toggle() is invoked (console_fragment.gd:71-103, ui_host.rs:524-541).
- Fullscreen minimap widget implements wheel zoom clamped 0.25-10, right-drag pan, left-tap and forwards to Camera2D.pan_by/zoom_by (minimap_fragment.gd:24-38) — the wiring is correct but the panel is unreachable and texture-less.
- HintsFragment has 8 localized hint.* names and show/hide entry points; only the display driver, skip button and remaining catalogue are missing.
- GameOverDialog renders a bundle title, headline and stat rows and hides the HUD while shown (game_over_dialog.gd:15-49); it is simply never opened.
- Campaign planet/sector dialogs and the 3D sector view (EV-0010/0011) launch sectors into MindSimHost correctly; HUD follow-up state is what is missing above.
