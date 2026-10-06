Player Input & Controls Audit — Mindustry-Godot (upstream 2cd7aeec)
Roots: port = Mindustry-Godot/, upstream = Mindustry/. Ledger re-read: EV-0001..0034, input-adjacent EV-0018 (UI leak, fixed) and EV-0023 (Controls rows, fixed) are not re-reported. Findings below are new.
Headline: the pure input layer in mind-core/src/input/** (9,387 lines: DesktopController, InputState, ClientPlan/PlanTree, MobileController, rts.rs, command_emit.rs) has zero runtime consumers in the Godot client. Grep for its symbols outside the module returns only mind-headless scenarios and sim/command.rs. The shipped client is two hand-written paths: MindSimHost::unhandled_input (single-tile place/break) and MindCamera2D::process (hardcoded WASD/arrow pan + wheel zoom). No [input] actions exist in client/project.godot, and MindInput::process clears captured events unused (client/rust/mind-gdext/src/input/mod.rs:84-94).
RANKED NEW GAPS
GAP: RTS unit commanding / selection is entirely absent from the client
- severity: S2
- port: client/rust/mind-gdext/src/input/mod.rs:299-307 (get_input_state_json hardcodes command_mode:false, selected_units:[]); rts.rs:68-203 and desktop.rs:130-186 have no non-headless callers; no #[func] on MindSimHost for selection/commands
- upstream: core/src/mindustry/input/DesktopInput.java:300-421 (command mode, select-all, control groups), :499-501,905-935 (drag-rect/tap), core/src/mindustry/input/InputHandler.java:1091-1200 (selection, commandTap)
- symptom: no command-mode toggle, no box/shift/double-tap selection, no move/attack/patrol orders, no control groups, no unit stances, no possession/shooting. Units can only be watched.
- campaign: blocks
- confidence: high
GAP: No way to rotate blocks or placed buildings
- severity: S2
- port: client/rust/mind-gdext/src/sim_host.rs:168-173 places with no rotation; client/rust/mind-core/src/sim/mod.rs:261 always spawns rot: 0; client/rust/mind-core/src/determinism/command.rs:230-240 (to_p0) drops SimCommand::Place.rotation even for MCP/relay; no rotate key/UI (editor has one: client/scenes/editor/map_editor_dialog.gd:161)
- upstream: DesktopInput.java:834-846 (rotate key/axis, rotatePlans, selectPlans), :898-899 (rotateBlock on placed buildings)
- symptom: conveyors, turrets, drills, factories, bridges always face rotation 0; placed buildings cannot be quick-rotated.
- campaign: blocks
- confidence: high (handler exists at sim/mod.rs:493-510 but nothing can invoke it)
GAP: Keybinding application path is dead — the Controls dialog is purely cosmetic
- severity: S2
- port: client/rust/mind-gdext/src/input/mod.rs:102-109,263-285 load/rebind/persist BindingState; only keybind_dialog.gd:74-245 reads it. Pan/zoom/boost are hardcoded: camera.rs:91-108. No [input] section in client/project.godot; events.rs:109-114 action translation can never fire
- upstream: DesktopInput.java:239,267,282 reads Binding.boost/moveX/moveY; Binding.java:8-103
- symptom: rebinding Move X / Pan / Boost etc. in Settings changes the saved value but no gameplay key behavior at all; only the four hardcoded pan keys and wheel work.
- campaign: degrades
- confidence: high
GAP: Placement is single-click only — no drag line, area, queue, or A* conveyor modes
- severity: S2
- port: sim_host.rs:141-179 handles only mouse-button press; neither desktop.rs:86 drag_line nor :106 break_rect is called; MindInput clears MouseMove events unused
- upstream: DesktopInput.java:685-700 (isPlacing() + updateLine), :771-790 (removeSelection), InputHandler.java:1943-1965 (updateLine), placement.rs A*/bridges
- symptom: cannot drag-place lines/rectangles of conveyors/walls, cannot drag-break areas, cannot queue plans for the builder, no diagonal-placement modifier.
- campaign: degrades
- confidence: high
GAP: Block config UI is unreachable and configure is unsupported in sim
- severity: S2
- port: client/ui/fragments/block_config_fragment.gd:23 configure() has zero callers; sim/mod.rs:577-583 maps SimCommand::Configure to Unsupported (no arm at all); sim_host.rs exposes no open-config API; plan_config_fragment.gd also orphaned
- upstream: InputHandler.java:1968-2010 (checkConfigTap/tileTapped), :703-734 (configure remote), DesktopInput.java:726
- symptom: sorter/incinerator/unloader items cannot be set, processors cannot be opened or coded, no inventory popup for cores/containers.
- campaign: blocks
- confidence: high
GAP: Schematic copy/save/select/place is stubbed; dialog actions are no-ops
- severity: S3
- port: client/rust/mind-gdext/src/campaign.rs:428-432 (write_schematic_selection returns ""), :434-438 (place_schematic_base64 returns false); client/ui/dialogs/schematics_dialog.gd:92 emits schematic_action with no listener anywhere; no F/Z/X bindings
- upstream: DesktopInput.java:529-543 (useSchematic), :616-657 (schematic select/flip/rebuild), InputHandler.java:1528-1556 (useSchematic/showSchematicSave)
- symptom: player can browse vanilla loadouts but cannot copy a world selection to a schematic, save it, export, or paste one into the world.
- campaign: degrades
- confidence: high
GAP: Mobile/touch in-game controls are absent (desktop-only client)
- severity: S3
- port: bridge exists (client/rust/mind-gdext/src/input/mobile.rs, input/mod.rs:316-434; core mobile.rs) but zero GDScript/tscn callers for any mobile_* method; no joypad/touch HUD, no mobile placement buttons; project.godot has no input actions
- upstream: MobileInput.java (touchDown/up/tap/longPress/autoPan, placement buttons), Control.java handler swap
- symptom: on touch devices the game is unplayable — no pan/zoom/place/rotate/confirm gestures, no on-screen buttons.
- campaign: n-a on desktop; blocks if run on touch/mobile
- confidence: high
GAP: Camera never follows/recenters on the player/core and has no detach/recenter controls
- severity: S3
- port: camera.rs:192-215 only pans on world load; spectate (camera.rs:313-319) and pan_to have no GDScript callers; no Binding.detachCamera equivalent; edge pan is always on (camera.rs:110-123)
- upstream: DesktopInput.java:253-295 (detach camera, pan binding, lerp to player/core), :441-447 respawn
- symptom: after panning away there is no key/click to return to your base/core; no smooth follow; V (respawn) does nothing.
- campaign: degrades
- confidence: high
GAP: Placement/build hotkeys are all absent
- severity: S3
- port: placement_fragment.gd click-only; no keyboard handling in client/ui/** or hud_fragment.gd; BindingState consumers absent (see keybind gap)
- upstream: Binding.java:68-101 and DesktopInput.java:449-467: rotate R/scroll, clearBuilding Q, pauseBuilding E, middle-click pick, categoryPrev/Next (,/.), blockSelect01-10 (1..0), schematic menu T, minimap M, planet N, research J, blockInfo F1, togglePowerLines F5, toggleBlockStatus F6, screenshot F12, console F8, ping P
- symptom: none of these player actions exist; the placement palette can only be driven by mouse.
- campaign: degrades
- confidence: high
GAP: Item withdrawal/deposit and payload pickup/drop interactions are absent
- severity: S3
- port: client/ui/fragments/block_inventory_fragment.gd:12 take_requested has no listener; no #[func] inventory/payload API; SimCommand::Payload is explicitly unsupported (sim/mod.rs:511-519) though Inventory applies (sim/mod.rs:521-540) with no input path
- upstream: DesktopInput.java:798 (tryDropItems), :1019-1039 (pickup/drop cargo bindings), InputHandler.java:2286-2316, :1039-1064
- symptom: cannot take items from a core/container (no [/] cargo, no item transfer on right-click); core item interaction is missing.
- campaign: degrades
- confidence: high
GAP: No replacement/upgrade/rebuild-select placement mode
- severity: S3
- port: sim/mod.rs:264-270 rejects any occupied tile (no replace); no selection modes in the client
- upstream: DesktopInput.java:616-644 (rebuildSelect area), InputHandler.java:1823-1870 (flushPlans replacement), :2318-2342 (rebuildArea)
- symptom: cannot upgrade conveyors/junctions, replace a block by placing over it, or mass-rebuild destroyed structures.
- campaign: degrades
- confidence: high
COVERAGE NOTES
- RTS unit commanding: NOT playable. There is no selection, command-mode, order, stance, control-group, or possession input path in the Godot client; mind-core's rts.rs/command_emit.rs are exercised only by mind-headless scenarios, and Sim's UnitCommand/UnitStance handlers (sim/mod.rs:457-492) are reachable only via relay/MCP today.
- Schematic copy/paste: NOT playable. Both backing calls are hard stubs (campaign.rs:428-438) and the dialog's action buttons emit an unconnected signal; only the read-only list of vanilla loadouts renders.
- Pass/working: single-tile left-click place and right-click break with UI-click suppression now work (EV-0018 verified); world-load camera snap to the best core works (EV-0016 verified); wheel zoom, WASD/arrow/Shift pan, edge pan, shake and logic-cutscene math all run (camera.rs:74-155).
- Pass (editor): rotation, Ctrl+Z/Y undo/redo and tool hotkeys are wired in client/scenes/editor/map_editor_dialog.gd:161,194-239 over MindEditor.undo/redo.
- Pass (headless oracle): DesktopController, MobileController, InputState, PlanTree, A*/bridge placement and RTS selection are covered by client/rust/mind-headless/src/input_scenarios.rs; they are not untested, just unwired in the client.
- No-op/stub handlers: MindInput::get_input_state_json returns all-default state (input/mod.rs:289-314); MindInput::process counts and discards events (:84-94); MindInput::inject_key/push_event are test hooks only.
- Debug-only shortcuts, not player-facing: MindSimHost.place_block/break_block and apply_sim_command_json (sim_host.rs:271-305,355-391) are MCP/dev entry points; the latter additionally drops rotation (sim/mod.rs:261) and only accepts place/break.
- Keybind registry parity is complete (binding.rs, 89 binds; ids module) and persistence/rebind/reset work (EV-0023 fixed the rows); only consumption is missing — rebinding has no effect.
- Not re-reported (already in ledger): block-catalog unlock filtering (EV-0013), picker rebuild error (EV-0017), UI-click world leak (EV-0018), Controls row rendering (EV-0023), uiscale application (EV-0025).
- ControlMode does not exist in upstream 2cd7aeec (verified by repo-wide grep); RTS mode state is the commandMode/PlaceMode.{schematicSelect,rebuildSelect} pair above.
