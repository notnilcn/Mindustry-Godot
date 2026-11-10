// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the desktop bridge.
//! Upstream: core/src/mindustry/input/DesktopInput.java

#![allow(clippy::expect_used)]

use super::*;

/// `Binding.java:10-11` binds `move_x` to A/D and `move_y` to S/W;
/// `MapView.act` (`editor/MapView.java:180-185`) polls them through
/// `Core.input.axis`, which holds `-1`/`1` per held direction and cancels
/// when both are down (`Input.java:210-218`).
#[test]
fn move_bindings_read_the_wasd_axes() {
    let mut bridge = DesktopBridge::new();
    let bindings = BindingState::new();
    assert_eq!(bridge.binding_axis(&bindings, "move_x"), 0.0);
    assert_eq!(bridge.binding_axis(&bindings, "move_y"), 0.0);

    bridge.handle(&bindings, RawEvent::key_down("d"));
    assert_eq!(bridge.binding_axis(&bindings, "move_x"), 1.0);
    assert_eq!(bridge.binding_axis(&bindings, "move_y"), 0.0);
    bridge.handle(&bindings, RawEvent::key_down("a"));
    assert_eq!(bridge.binding_axis(&bindings, "move_x"), 0.0);
    bridge.handle(&bindings, RawEvent::key_up("a"));
    bridge.handle(&bindings, RawEvent::key_up("d"));

    bridge.handle(&bindings, RawEvent::key_down("s"));
    assert_eq!(bridge.binding_axis(&bindings, "move_y"), -1.0);
    bridge.handle(&bindings, RawEvent::key_down("w"));
    assert_eq!(bridge.binding_axis(&bindings, "move_y"), 0.0);
    bridge.handle(&bindings, RawEvent::key_up("s"));
    assert_eq!(bridge.binding_axis(&bindings, "move_y"), 1.0);

    // Unknown names, single-key binds and cleared axes read 0.
    assert_eq!(bridge.binding_axis(&bindings, "not_a_binding"), 0.0);
    assert_eq!(bridge.binding_axis(&bindings, "select"), 0.0);
}

/// A rebound axis (`KeybindDialog.rebind`) is what the editor pan reads: the
/// physical WASD keys no longer move the map.
#[test]
fn rebound_move_axis_replaces_wasd() {
    let mut bridge = DesktopBridge::new();
    let mut bindings = BindingState::new();
    bindings.set_axis(ids::MOVE_X, Some("j"), Some("l"));
    bridge.handle(&bindings, RawEvent::key_down("d"));
    assert_eq!(
        bridge.binding_axis(&bindings, "move_x"),
        0.0,
        "physical d is unbound"
    );
    bridge.handle(&bindings, RawEvent::key_down("l"));
    assert_eq!(bridge.binding_axis(&bindings, "move_x"), 1.0);
    bridge.handle(&bindings, RawEvent::key_down("j"));
    assert_eq!(bridge.binding_axis(&bindings, "move_x"), 0.0);
    bridge.handle(&bindings, RawEvent::key_up("j"));
    assert_eq!(bridge.binding_axis(&bindings, "move_x"), 1.0);
    // A wheel axis has no held key: `Core.input.axis(Binding.zoom)`'s
    // keyboard branch never produces a value (`Input.java:210-218`).
    bindings.set_axis(ids::MOVE_X, None, Some("scroll"));
    assert_eq!(bridge.binding_axis(&bindings, "move_x"), 0.0);
}

#[test]
fn skip_wave_gate_matches_the_skip_button() {
    // `HudFragment.canSkipWave` (`HudFragment.java:1234-1236`) with the
    // button's extra `has_core` requirement (`_refresh_skip_button`).
    let ready = WaveGate {
        waves: true,
        wave_sending: true,
        is_spawning: false,
        enemies: 0,
        has_core: true,
    };
    assert!(ready.can_skip_wave());
    let mut gate = ready;
    gate.waves = false;
    assert!(!gate.can_skip_wave(), "waves off");
    gate.waves = true;
    gate.wave_sending = false;
    assert!(!gate.can_skip_wave(), "wave sending off");
    gate.wave_sending = true;
    gate.is_spawning = true;
    assert!(!gate.can_skip_wave(), "spawn in flight");
    gate.is_spawning = false;
    gate.enemies = 1;
    assert!(!gate.can_skip_wave(), "enemies alive");
    gate.enemies = 0;
    gate.has_core = false;
    assert!(!gate.can_skip_wave(), "no core");
    // A negative enemy counter stays skippable (the port button gate is
    // `enemies <= 0`, PAR-0463 keeps the key on the same predicate).
    assert!(
        WaveGate {
            enemies: -1,
            ..ready
        }
        .can_skip_wave()
    );
}

#[test]
fn skip_wave_key_dispatches_and_respects_the_gate() {
    // `HudFragment.java:461-467`: the key runs `logic.skipWave()` only when
    // `canSkipWave()` holds. Before PAR-0463 the binding fell through the
    // `_ => {}` arm, so `last_action` never became `skip_wave`.
    let mut bridge = DesktopBridge::new();
    bridge.in_game = true;
    let mut bindings = BindingState::new();
    bindings.set_key(ids::SKIP_WAVE, "k");

    // Closed gate: the action is recognised but emits no skip.
    bridge.handle(&bindings, RawEvent::key_down("k"));
    assert_eq!(bridge.last_action, "skip_wave");
    assert!(!bridge.effects.contains(&Effect::SkipWave));
    bridge.handle(&bindings, RawEvent::key_up("k"));

    // Open gate: the desktop bridge queues the same skip the button runs.
    bridge.wave_gate = WaveGate {
        waves: true,
        wave_sending: true,
        is_spawning: false,
        enemies: 0,
        has_core: true,
    };
    bridge.handle(&bindings, RawEvent::key_down("k"));
    assert!(bridge.effects.contains(&Effect::SkipWave));
    bridge.handle(&bindings, RawEvent::key_up("k"));

    // Text focus wins over the gate (`!scene.hasField()`).
    bridge.take_effects();
    bridge.text_focus = true;
    bridge.handle(&bindings, RawEvent::key_down("k"));
    assert!(!bridge.effects.contains(&Effect::SkipWave));
}

#[test]
fn chat_binding_routes_to_the_fragment_toggle() {
    // `ChatFragment.java:65`/`DesktopInput.update`: `Binding.chat` (Enter)
    // opens/closes the chat input. Before this the binding fell through
    // `dispatch_action`'s `_ => {}` arm, so Enter never reached
    // `MindChatFragment.toggle` (`Effect::ToggleFragment("chat")`).
    assert_eq!(BindingState::new().name(ids::CHAT), Some("enter"));
    let mut bridge = DesktopBridge::new();
    bridge.in_game = true;
    let mut bindings = BindingState::new();
    bindings.set_key(ids::CHAT, "enter");

    bridge.handle(&bindings, RawEvent::key_down("enter"));
    assert_eq!(bridge.last_action, "chat");
    assert!(
        bridge
            .effects
            .contains(&Effect::ToggleFragment(String::from("chat")))
    );
    bridge.handle(&bindings, RawEvent::key_up("enter"));

    // `!hasOtherFocus`: a focused field/dialog consumes the edge (the
    // fragment would otherwise steal focus from a text prompt).
    bridge.take_effects();
    bridge.text_focus = true;
    bridge.handle(&bindings, RawEvent::key_down("enter"));
    assert!(
        !bridge
            .effects
            .contains(&Effect::ToggleFragment(String::from("chat")))
    );
    // Release between probes: only a fresh press edge dispatches.
    bridge.handle(&bindings, RawEvent::key_up("enter"));
    bridge.text_focus = false;
    bridge.ui_dialog = true;
    bridge.handle(&bindings, RawEvent::key_down("enter"));
    assert!(
        !bridge
            .effects
            .contains(&Effect::ToggleFragment(String::from("chat")))
    );
    bridge.handle(&bindings, RawEvent::key_up("enter"));
}

#[test]
fn player_list_binding_routes_to_the_fragment_toggle() {
    // PAR-0608: `DesktopInput.java:233-235` runs
    // `ui.listfrag.toggle()` on `keyTap(Binding.playerList)` (Tab). Before
    // the fix the `player_list` binding fell through `dispatch_action`'s
    // `_ => {}` arm, so `MindPlayerListFragment.toggle` was never called.
    assert_eq!(BindingState::new().name(ids::PLAYER_LIST), Some("tab"));
    let mut bridge = DesktopBridge::new();
    bridge.in_game = true;
    let mut bindings = BindingState::new();
    bindings.set_key(ids::PLAYER_LIST, "tab");

    bridge.handle(&bindings, RawEvent::key_down("tab"));
    assert_eq!(bridge.last_action, "player_list");
    assert!(
        bridge
            .effects
            .contains(&Effect::ToggleFragment(String::from("player_list")))
    );
    bridge.handle(&bindings, RawEvent::key_up("tab"));

    // `scene.getKeyboardFocus() == null`: a focused field/dialog consumes
    // the edge (except Java's in-list focus clause, which the fragment's
    // own search field owns).
    bridge.take_effects();
    bridge.text_focus = true;
    bridge.handle(&bindings, RawEvent::key_down("tab"));
    assert!(
        !bridge
            .effects
            .contains(&Effect::ToggleFragment(String::from("player_list")))
    );
    bridge.handle(&bindings, RawEvent::key_up("tab"));
    bridge.text_focus = false;
    bridge.ui_dialog = true;
    bridge.handle(&bindings, RawEvent::key_down("tab"));
    assert!(
        !bridge
            .effects
            .contains(&Effect::ToggleFragment(String::from("player_list")))
    );
    bridge.handle(&bindings, RawEvent::key_up("tab"));
    bridge.ui_dialog = false;

    // Outside a game (menu) the binding does not run (`state.isGame()`).
    bridge.in_game = false;
    bridge.handle(&bindings, RawEvent::key_down("tab"));
    assert!(
        !bridge
            .effects
            .contains(&Effect::ToggleFragment(String::from("player_list")))
    );
    bridge.handle(&bindings, RawEvent::key_up("tab"));
}

#[test]
fn ping_binding_emits_the_location_effect() {
    // GAPS #46/PAR-0554: `DesktopInput.java:491-497`. Before the fix the
    // `ping` binding fell through `dispatch_action`'s `_ => {}` arm, so no
    // `RemoteAction::Ping`/marker ever existed.
    assert_eq!(BindingState::new().name(ids::PING), Some("p"));
    let mut bridge = DesktopBridge::new();
    bridge.ping_at(120.0, -8.0, None);
    assert_eq!(
        bridge.take_effects(),
        vec![Effect::Ping {
            x: 120.0,
            y: -8.0,
            text: String::new()
        }]
    );
    // `InputHandler.pingLocation`: an empty label stays `null`, a long one
    // truncates to `maxPingTextLength` + "...".
    bridge.ping_at(1.0, 2.0, Some("  "));
    assert_eq!(
        bridge.take_effects(),
        vec![Effect::Ping {
            x: 1.0,
            y: 2.0,
            text: String::new()
        }]
    );
    let long = "x".repeat(MAX_PING_TEXT + 5);
    bridge.ping_at(3.0, 4.0, Some(&long));
    let effects = bridge.take_effects();
    let Effect::Ping { text, .. } = &effects[0] else {
        panic!("expected a ping effect, got {effects:?}");
    };
    assert_eq!(text.chars().filter(|ch| *ch == 'x').count(), MAX_PING_TEXT);
    assert!(text.ends_with("..."));
}

#[test]
fn plan_config_spec_json_uses_the_content_option_labels() {
    // PAR-0555: the spec is built from `Block.getPlanConfigs` options (the
    // label is the bundle key of the option's content type).
    use mind_core::content::ContentType;
    use mind_core::world::block::PlanConfigOption;
    let options = vec![
        PlanConfigOption::new(ContentType::Item, 0, "copper"),
        PlanConfigOption::new(ContentType::Liquid, 0, "water"),
    ];
    let spec = plan_config_spec_json("block.sorter.name", &options, Some("copper")).expect("spec");
    let parsed: serde_json::Value = serde_json::from_str(&spec).expect("json");
    assert_eq!(parsed["title"], "block.sorter.name");
    assert_eq!(parsed["value"], "copper");
    assert_eq!(parsed["options"][0]["name"], "copper");
    assert_eq!(parsed["options"][0]["label"], "item.copper.name");
    assert_eq!(parsed["options"][1]["label"], "liquid.water.name");
    // `if(options.isEmpty()) return;` — no popup without options.
    assert!(plan_config_spec_json("block.conveyor.name", &[], None).is_none());
    // No checked value leaves the key out.
    let spec = plan_config_spec_json("t", &options, None).expect("spec");
    assert!(
        serde_json::from_str::<serde_json::Value>(&spec)
            .expect("json")
            .get("value")
            .is_none()
    );
}

#[test]
fn plan_drag_tile_rounds_like_desktop_input() {
    // Grab at tile (10, 20), world (84, 164): a +20 px x move is +2.5 tiles
    // and `Math.round` rounds halves toward +inf.
    assert_eq!(
        plan_drag_tile((10, 20), (84.0, 164.0), (104.0, 164.0)),
        (13, 20)
    );
    assert_eq!(
        plan_drag_tile((10, 20), (84.0, 164.0), (64.0, 164.0)),
        (8, 20)
    );
    // Small moves keep the grabbed tile.
    assert_eq!(
        plan_drag_tile((10, 20), (84.0, 164.0), (87.0, 161.0)),
        (10, 20)
    );
    assert_eq!(
        plan_drag_tile((10, 20), (84.0, 164.0), (84.0, 164.0)),
        (10, 20)
    );
}

#[test]
fn plan_config_requires_an_open_plan() {
    // `PlanConfigFragment.plan_config_applied` without a `showConfig` target
    // is a no-op, and a select release without `splan` emits nothing.
    let mut bridge = DesktopBridge::new();
    assert!(!bridge.apply_plan_config("copper"));
    let bindings = BindingState::new();
    assert!(!bridge.release_splan(&bindings));
    assert!(bridge.take_effects().is_empty());
}

#[test]
fn command_mode_hold_tracks_the_shared_default_key() {
    // `DesktopInput.update` + `commandmodehold` default true: command mode
    // follows the held command key (`boost` shares `shiftLeft`).
    let mut bridge = DesktopBridge::new();
    // The whole `update` runs only in game (`Control.java:690-760`).
    bridge.in_game = true;
    let bindings = BindingState::new();
    assert_eq!(bindings.name(ids::COMMAND_MODE), Some("shiftLeft"));
    assert_eq!(bindings.name(ids::BOOST), Some("shiftLeft"));

    bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
    assert!(bridge.controller.state.command_mode);
    assert_eq!(bridge.last_action, "command_mode");
    bridge.handle(&bindings, RawEvent::key_up("shiftLeft"));
    assert!(!bridge.controller.state.command_mode);
}

#[test]
fn mouse_back_sets_pressed() {
    // T9: `pan` (mouseForward) and `mouseMove` (mouseBack) are binding keys;
    // the raw mouse press latches their key name in `pressed`, so
    // `MindInput.pan_pressed`/`mouse_move_pressed` and the player pull see
    // the held state.
    let mut bridge = DesktopBridge::new();
    bridge.in_game = true;
    let bindings = BindingState::new();
    assert_eq!(bindings.name(ids::MOUSE_MOVE), Some("mouseBack"));
    assert_eq!(bindings.name(ids::PAN), Some("mouseForward"));
    bridge.handle(
        &bindings,
        RawEvent::MouseButton {
            button: "back".to_owned(),
            down: true,
            x: 10.0,
            y: 10.0,
        },
    );
    assert!(bridge.key_down(&bindings, ids::MOUSE_MOVE));
    assert!(!bridge.key_down(&bindings, ids::PAN));
    bridge.handle(
        &bindings,
        RawEvent::MouseButton {
            button: "back".to_owned(),
            down: false,
            x: 10.0,
            y: 10.0,
        },
    );
    assert!(!bridge.key_down(&bindings, ids::MOUSE_MOVE));

    bridge.handle(
        &bindings,
        RawEvent::MouseButton {
            button: "forward".to_owned(),
            down: true,
            x: 10.0,
            y: 10.0,
        },
    );
    assert!(bridge.key_down(&bindings, ids::PAN));
}

#[test]
fn command_mode_off_on_menu() {
    // `Control.java:690-760`: `DesktopInput.update` only runs in game, so
    // the command key is inert on the menu.
    let mut bridge = DesktopBridge::new();
    let bindings = BindingState::new();
    assert!(!bridge.in_game);
    bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
    assert!(!bridge.controller.state.command_mode);
    assert!(!bridge.command_mode_allowed(&bindings));
}

#[test]
fn command_mode_tap_toggles_when_hold_disabled() {
    // `commandmodehold=false`: `input.keyTap` toggles on the press edge.
    let mut bridge = DesktopBridge::new();
    bridge.set_command_mode_hold(false);
    // The tap path runs through the gameplay binding gate (`state.isGame()`).
    bridge.in_game = true;
    let bindings = BindingState::new();

    bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
    assert!(bridge.controller.state.command_mode);
    bridge.handle(&bindings, RawEvent::key_up("shiftLeft"));
    assert!(bridge.controller.state.command_mode);

    bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
    assert!(!bridge.controller.state.command_mode);
}

#[test]
fn command_mode_ignored_while_placing() {
    // `DesktopInput.update` requires `block == null`.
    let mut bridge = DesktopBridge::new();
    bridge.in_game = true;
    let bindings = BindingState::new();
    bridge
        .controller
        .state
        .select_block(Some(BlockId::STONE_WALL));

    bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
    assert!(!bridge.controller.state.command_mode);
}

#[test]
fn command_mode_api_latch_survives_events_without_the_key() {
    // `MindInput.toggle_command_mode()` (MCP/test) latches command mode;
    // hold mode must not clear it while the command key is up.
    let mut bridge = DesktopBridge::new();
    bridge.in_game = true;
    let bindings = BindingState::new();
    bridge.toggle_command_mode();
    assert!(bridge.controller.state.command_mode);

    bridge.handle(&bindings, RawEvent::MouseMove { x: 10.0, y: 10.0 });
    bridge.update_command_mode(&bindings, None);
    assert!(bridge.controller.state.command_mode);

    // A command-key edge still takes over.
    bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
    bridge.handle(&bindings, RawEvent::key_up("shiftLeft"));
    assert!(!bridge.controller.state.command_mode);
}

#[test]
fn command_mode_released_when_placement_block_selected() {
    // `DesktopInput.update` assigns `commandMode = false` in the else
    // branch, so selecting a block while holding the key drops it.
    let mut bridge = DesktopBridge::new();
    bridge.in_game = true;
    let bindings = BindingState::new();
    bridge.handle(&bindings, RawEvent::key_down("shiftLeft"));
    assert!(bridge.controller.state.command_mode);

    bridge
        .controller
        .state
        .select_block(Some(BlockId::STONE_WALL));
    bridge.update_command_mode(&bindings, None);
    assert!(!bridge.controller.state.command_mode);
}

#[test]
fn bindings_do_not_fire_outside_game() {
    // `Control.java:690-760`: gameplay binds are gated on `state.isGame()`;
    // the menu renders no world and the HUD fragments stay closed.
    let bindings = BindingState::new();
    assert_eq!(bindings.name(ids::SCHEMATIC_MENU), Some("t"));
    let mut bridge = DesktopBridge::new();
    bridge.handle(&bindings, RawEvent::key_down("t"));
    assert!(bridge.take_effects().is_empty(), "menu T is inert");
    bridge.handle(&bindings, RawEvent::key_up("t"));

    bridge.in_game = true;
    bridge.handle(&bindings, RawEvent::key_down("t"));
    let effects = bridge.take_effects();
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::OpenDialog { name, .. } if name == "schematics"
        )),
        "in game T opens schematics"
    );
}

#[test]
fn campaign_keys_need_an_active_campaign() {
    // `DesktopInput.java:446-461`: research/planet open only in a campaign.
    let bindings = BindingState::new();
    let mut bridge = DesktopBridge::new();
    bridge.in_game = true;
    bridge.campaign_active = false;
    bridge.handle(&bindings, RawEvent::key_down("j"));
    assert!(
        !bridge
            .take_effects()
            .iter()
            .any(|effect| matches!(effect, Effect::OpenDialog { .. })),
        "J outside a campaign opens nothing"
    );
    bridge.campaign_active = true;
    bridge.handle(&bindings, RawEvent::key_down("n"));
    assert!(
        bridge.take_effects().iter().any(|effect| matches!(
            effect,
            Effect::OpenDialog { name, .. } if name == "planet"
        )),
        "N in a campaign opens the planet map"
    );
}

#[test]
fn override_line_rotation_resets_when_the_drag_ends() {
    // Java DI:762-768: the wheel snap latch clears whenever the frame is
    // not a placement line, so a second drag after a wheel step gets
    // per-step rotations again.
    let mut bridge = DesktopBridge::new();
    bridge
        .controller
        .state
        .select_block(Some(BlockId::STONE_WALL));
    bridge.controller.state.override_line_rotation = true;
    bridge.controller.state.place_mode = PlaceMode::Placing;
    bridge.commit_place(Drag {
        kind: DragKind::Place,
        start: (1, 1),
        last: (1, 2),
        moved: true,
    });
    assert!(!bridge.controller.state.override_line_rotation);

    // The per-frame mirror also clears it outside a drag.
    bridge.controller.state.override_line_rotation = true;
    bridge.sync_line_override();
    assert!(!bridge.controller.state.override_line_rotation);

    // While a placement line is active the latch survives.
    bridge.controller.state.override_line_rotation = true;
    bridge.controller.state.place_mode = PlaceMode::Placing;
    bridge.sync_line_override();
    assert!(bridge.controller.state.override_line_rotation);
}

#[test]
fn rotate_step_snaps_to_the_drag_angle_before_stepping() {
    // `pollInputPlayer` L762-768 runs before L836-849: a rotate tap during
    // a fresh drag snaps to the drag angle (upward drag = 1), then adds
    // the step and latches the override.
    let mut bridge = DesktopBridge::new();
    let bindings = BindingState::new();
    bridge
        .controller
        .state
        .select_block(Some(BlockId::STONE_WALL));
    bridge.drag = Some(Drag {
        kind: DragKind::Place,
        start: (5, 5),
        last: (5, 8),
        moved: true,
    });
    bridge.controller.state.override_line_rotation = false;
    bridge.rotate_placement_step(&bindings, 1);
    assert_eq!(bridge.controller.state.rotation, 2);
    assert!(bridge.controller.state.override_line_rotation);

    // A latched override skips the snap and only steps.
    bridge.rotate_placement_step(&bindings, 1);
    assert_eq!(bridge.controller.state.rotation, 3);
}

#[test]
fn placement_view_reports_break_drag() {
    // G03 T2: the break rectangle is exposed while a right-drag is held.
    let mut bridge = DesktopBridge::new();
    bridge.controller.state.place_mode = PlaceMode::Breaking;
    bridge.drag = Some(Drag {
        kind: DragKind::Break,
        start: (1, 1),
        last: (3, 4),
        moved: true,
    });
    let view = bridge.placement_view();
    assert_eq!(view.break_rect, Some(((1, 1), (3, 4))));
    assert_eq!(view.mode, PlaceMode::Breaking);
    assert!(view.line_plans.is_empty());
}

#[test]
fn placement_view_reports_held_selection() {
    // GAPS #47 residual (`DesktopInput.drawTop`, `DesktopInput.java:
    // 133-145`): the F/B rectangle is exposed as `(kind, start)` only while
    // the key is held with a latched `schemX/schemY`, F beats B, and
    // breaking mode / keyboard focus suppress both.
    let mut bridge = DesktopBridge::new();
    bridge.controller.state.place_mode = PlaceMode::Placing;
    bridge.schem_start = Some((2, 3));
    // No key held: nothing draws even with a stale latch.
    assert_eq!(bridge.placement_view().selection, None);

    bridge.schematic_held = true;
    assert_eq!(
        bridge.placement_view().selection,
        Some((SelectionShow::Schematic, (2, 3)))
    );
    // F beats B (`if(keyDown(schematicSelect) …) else if(keyDown(
    // rebuildSelect))`).
    bridge.rebuild_held = true;
    assert_eq!(
        bridge.placement_view().selection,
        Some((SelectionShow::Schematic, (2, 3)))
    );
    // F released -> the rebuild rect.
    bridge.schematic_held = false;
    assert_eq!(
        bridge.placement_view().selection,
        Some((SelectionShow::Rebuild, (2, 3)))
    );
    // `Core.scene.hasKeyboard()` suppresses both rects.
    bridge.text_focus = true;
    assert_eq!(bridge.placement_view().selection, None);
    bridge.text_focus = false;
    // `mode != breaking`.
    bridge.controller.state.place_mode = PlaceMode::Breaking;
    assert_eq!(bridge.placement_view().selection, None);
    bridge.controller.state.place_mode = PlaceMode::Placing;
    // The latch is required for the F rect (`schemX != -1`).
    bridge.schem_start = None;
    assert_eq!(bridge.placement_view().selection, None);
}

#[test]
fn key_edges_refresh_the_selection_held_flags() {
    // The draw pass has no binding table, so `handle` mirrors
    // `keyDown(schematicSelect/rebuildSelect)` into the view on every key
    // edge — including a key-up while a text field owns the keyboard.
    let bindings = BindingState::new();
    // Defaults (`Binding.schematicSelect` = F, `rebuildSelect` = B).
    assert_eq!(bindings.name(ids::SCHEMATIC_SELECT), Some("f"));
    assert_eq!(bindings.name(ids::REBUILD_SELECT), Some("b"));
    let mut bridge = DesktopBridge::new();
    bridge.in_game = true;
    bridge.handle(
        &bindings,
        RawEvent::KeyDown {
            code: String::from("f"),
        },
    );
    assert!(bridge.schematic_held);
    bridge.text_focus = true;
    bridge.handle(
        &bindings,
        RawEvent::KeyUp {
            code: String::from("f"),
        },
    );
    assert!(
        !bridge.schematic_held,
        "a release swallowed by a focused field must still clear the flag"
    );
    bridge.text_focus = false;
    bridge.handle(
        &bindings,
        RawEvent::KeyDown {
            code: String::from("b"),
        },
    );
    assert!(bridge.rebuild_held);
    // Focus loss drops every held key (and the selection flags).
    bridge.clear_keys();
    assert!(!bridge.rebuild_held && !bridge.schematic_held);
}

#[test]
fn placement_view_reports_select_plans() {
    // `selectPlans` paste ghosts follow the cursor until `clearBuilding`.
    let mut bridge = DesktopBridge::new();
    bridge.controller.state.select_plans = vec![ClientPlan::place(4, 5, 2, BlockId::STONE_WALL)];
    let view = bridge.placement_view();
    assert_eq!(view.select_plans.len(), 1);
    assert_eq!(
        (
            view.select_plans[0].x,
            view.select_plans[0].y,
            view.select_plans[0].rotation
        ),
        (4, 5, 2)
    );
    bridge.controller.state.select_plans.clear();
    assert!(bridge.placement_view().select_plans.is_empty());
}

#[test]
fn placement_view_reports_command_drag() {
    // GAPS #47: the live command drag rect (`InputHandler.commandRect`) is
    // exposed only while the select-rect drag is held. The stored rect is
    // Java's `(commandRectX, commandRectY, x2 - commandRectX, y2 -
    // commandRectY)`, so a left/up drag carries a negative size.
    let mut bridge = DesktopBridge::new();
    bridge.controller.state.command_mode = true;
    bridge.controller.state.command_rect = Some((16.0, 24.0, -8.0, 6.0));
    bridge.drag = Some(Drag {
        kind: DragKind::SelectRect,
        start: (2, 3),
        last: (1, 3),
        moved: true,
    });
    assert_eq!(
        bridge.placement_view().command_rect,
        Some((16.0, 24.0, -8.0, 6.0))
    );
    // After the release the state keeps the committed tile rect, but that
    // is not a held drag and must not be drawn as a live box.
    bridge.drag = None;
    assert_eq!(bridge.placement_view().command_rect, None);
    // A placement/break drag never surfaces the command rect.
    bridge.controller.state.command_rect = Some((0.0, 0.0, 8.0, 8.0));
    bridge.drag = Some(Drag {
        kind: DragKind::Break,
        start: (1, 1),
        last: (3, 4),
        moved: true,
    });
    assert_eq!(bridge.placement_view().command_rect, None);
}

fn base_registry() -> ContentRegistry {
    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    match create_base_content(&bundle, &store, true) {
        Ok(mut registry) => {
            if let Err(error) = registry.init() {
                panic!("registry init failed: {error}");
            }
            if let Err(error) = registry.post_init() {
                panic!("registry post_init failed: {error}");
            }
            registry
        }
        Err(error) => panic!("content boot failed: {error}"),
    }
}

#[test]
fn item_selection_shapes_match_the_java_call_sites() {
    // J05 T1: the `ItemSelection.buildTable` call sites in `world/**`,
    // keyed on the block kind.
    let content = base_registry();
    let shape = |name: &str| content.block_by_name(name).and_then(item_selection_shape);
    assert_eq!(shape("sorter"), Some(("item", 5, 4)));
    assert_eq!(shape("inverted-sorter"), Some(("item", 5, 4)));
    assert_eq!(shape("item-source"), Some(("item", 5, 4)));
    assert_eq!(shape("unloader"), Some(("item", 5, 4)));
    assert_eq!(shape("duct-unloader"), Some(("item", 5, 4)));
    assert_eq!(shape("duct-router"), Some(("item", 5, 4)));
    assert_eq!(shape("landing-pad"), Some(("item", 5, 4)));
    assert_eq!(shape("unit-cargo-unload-point"), Some(("item", 5, 4)));
    assert_eq!(shape("liquid-source"), Some(("liquid", 5, 4)));
    assert_eq!(shape("constructor"), Some(("block", 5, 4)));
    assert_eq!(shape("large-constructor"), Some(("block", 5, 4)));
    assert_eq!(shape("payload-router"), Some(("block", 5, 4)));
    // `PayloadSource.java:35`: `selectionRows = selectionColumns = 8`.
    assert_eq!(shape("payload-source"), Some(("block", 8, 8)));
    // Every `UnitFactory` lists plans (J05 T2 adds the command row).
    assert_eq!(shape("ground-factory"), Some(("unit", 5, 4)));
    assert_eq!(shape("air-factory"), Some(("unit", 5, 4)));
    // A `Reconstructor` panel is a plain command row, not a grid; ordinary
    // blocks never route through `ItemSelection`.
    assert_eq!(shape("additive-reconstructor"), None);
    assert_eq!(shape("duo"), None);
}

#[test]
fn content_options_carry_the_java_icon_size_and_filters() {
    let content = base_registry();
    let serpulo = content.planet_id("serpulo");
    // `unlockedNow()` outside a campaign (`UnlockableContent.java:281-283`).
    let always = |_: &str| true;
    // A non-sandbox custom game (default `Rules`, `infiniteResources`
    // false) for `Block.isHidden` (`Block.java:1553-1555`).
    let survival = mind_core::ui::campaign::BlockVisibilityContext::from_rules(
        &mind_core::io::json::rules::Rules::default(),
        false,
    );
    let item = content_option_json(
        &content,
        ContentType::Item,
        "copper",
        "item.copper.name".to_owned(),
        serpulo,
        &always,
        &survival,
    );
    assert_eq!(item["icons"][0], "item-copper-ui");
    // `UnlockableContent.selectionSize` default 24 (`:44`).
    assert_eq!(item["size"], 24.0);
    // `unlockedNow()` outside a campaign (`UnlockableContent.java:281-283`).
    assert_eq!(item["unlocked"], true);
    assert_eq!(item["on_planet"], true);
    // `Block.java:450` (`28f`) / `UnitType.java:534` (`30f`).
    let block = content_option_json(
        &content,
        ContentType::Block,
        "duo",
        "block.duo.name".to_owned(),
        serpulo,
        &always,
        &survival,
    );
    assert_eq!(block["size"], 28.0);
    let unit = content_option_json(
        &content,
        ContentType::Unit,
        "dagger",
        "unit.dagger.name".to_owned(),
        serpulo,
        &always,
        &survival,
    );
    assert_eq!(unit["size"], 30.0);
    assert_eq!(unit["icons"][0], "unit-dagger-ui");
    // Erekir content is off-planet on Serpulo (`isOnPlanet`,
    // `UnlockableContent.java:132-134`) — the Java sandbox grid shows no
    // tungsten/oxide.
    let tungsten = content_option_json(
        &content,
        ContentType::Item,
        "tungsten",
        "item.tungsten.name".to_owned(),
        serpulo,
        &always,
        &survival,
    );
    assert_eq!(tungsten["on_planet"], false);
    // A campaign with an empty unlock state hides locked content (the
    // closure is the `MindCampaign`/`Rules.researched` query).
    let locked = content_option_json(
        &content,
        ContentType::Block,
        "duo",
        "block.duo.name".to_owned(),
        serpulo,
        &|_: &str| false,
        &survival,
    );
    assert_eq!(locked["unlocked"], false);

    // L02 T3: `ItemSelection.java:68` skips `isHidden()` content.
    // `fissileMatter`/`dormantCyst` are `hidden = true`
    // (`Items.java:130,135`) and `Item.isOnPlanet` is false for them too
    // (`Item.java:61-64`); the Java sandbox source grid shows 16 items.
    let option = |content_type: ContentType,
                  name: &str,
                  visibility: &mind_core::ui::campaign::BlockVisibilityContext| {
        content_option_json(
            &content,
            content_type,
            name,
            format!("{}.{name}.name", content_type.name()),
            serpulo,
            &always,
            visibility,
        )
    };
    for name in ["fissile-matter", "dormant-cyst"] {
        let hidden = option(ContentType::Item, name, &survival);
        assert_eq!(hidden["hidden"], true, "{name}");
        assert_eq!(hidden["on_planet"], false, "{name}");
    }
    assert_eq!(item["hidden"], false);
    let shown: Vec<String> = content
        .items()
        .iter()
        .map(|item| item.name.clone())
        .filter(|name| {
            let value = option(ContentType::Item, name, &survival);
            value["unlocked"] == true && value["on_planet"] == true && value["hidden"] == false
        })
        .collect();
    assert_eq!(shown.len(), 16, "Java's Serpulo item grid: {shown:?}");
    // `Liquids.java:73` gallium; `UnitTypes.java:4318-4644` renale, latum,
    // block, manifold, assembly-drone, dummy.
    assert_eq!(
        option(ContentType::Liquid, "gallium", &survival)["hidden"],
        true
    );
    assert_eq!(
        option(ContentType::Liquid, "water", &survival)["hidden"],
        false
    );
    for name in [
        "renale",
        "latum",
        "block",
        "manifold",
        "assembly-drone",
        "dummy",
    ] {
        assert_eq!(
            option(ContentType::Unit, name, &survival)["hidden"],
            true,
            "{name}"
        );
    }
    assert_eq!(unit["hidden"], false);
    // `Block.isHidden` follows the rules: `sandboxOnly` blocks are hidden
    // outside `infiniteResources` (`BuildVisibility.java:16`).
    assert_eq!(
        option(ContentType::Block, "item-source", &survival)["hidden"],
        true
    );
    let sandbox = mind_core::ui::campaign::BlockVisibilityContext::sandbox();
    assert_eq!(
        option(ContentType::Block, "item-source", &sandbox)["hidden"],
        false
    );
    assert_eq!(block["hidden"], false);
}

#[test]
fn command_rows_follow_the_java_checked_state() {
    // J05 T2: `UnitFactory.java:311-315` / `Reconstructor.java:222-232`.
    let content = base_registry();
    let Some(unit) = content.unit_by_name("nova") else {
        panic!("nova unit");
    };
    let commands = unit.commands.clone();
    assert!(commands.len() > 1, "nova derives move + repair");
    let Some(default_id) = unit.default_command else {
        panic!("nova default command");
    };
    let default = Some(default_id.raw());
    let default_name = content
        .unit_command(default_id)
        .map(|def| def.name.clone())
        .unwrap_or_default();
    // No configured command: the unit's `defaultCommand` is checked.
    let rows = command_options_json(&content, &commands, None, default);
    assert_eq!(rows.len(), commands.len());
    let checked: Vec<&serde_json::Value> =
        rows.iter().filter(|row| row["checked"] == true).collect();
    assert_eq!(checked.len(), 1);
    assert_eq!(checked[0]["name"], serde_json::Value::from(default_name));
    // `icon` carries the `UnitCommandDef` `Icon` key for the glyph button.
    assert!(
        rows.iter()
            .all(|row| row["icon"].as_str().is_some_and(|icon| !icon.is_empty()))
    );
    // An explicit command wins over the default.
    let other = commands
        .iter()
        .find(|id| Some(id.raw()) != default)
        .copied()
        .unwrap_or(commands[0]);
    let other_name = content
        .unit_command(other)
        .map(|def| def.name.clone())
        .unwrap_or_default();
    let rows = command_options_json(&content, &commands, Some(other_name.as_str()), default);
    let checked: Vec<&serde_json::Value> =
        rows.iter().filter(|row| row["checked"] == true).collect();
    assert_eq!(checked.len(), 1);
    assert_eq!(checked[0]["name"], serde_json::Value::from(other_name));
}

#[test]
fn config_readbacks_resolve_to_content_names() {
    let content = base_registry();
    // `ItemSource`/`Sorter` configs are `{"type":"item","id":..}`.
    assert_eq!(
        config_content_name(&content, &serde_json::json!({"type": "item", "id": 0})),
        Some("copper".to_owned())
    );
    // `Reconstructor`/`UnitFactory` commands are `Content` values
    // (`behavior.rs` `ConfigValue::Content(UnitCommand, ..)`).
    let command = content
        .entries(ContentType::UnitCommand)
        .into_iter()
        .find(|entry| entry.id == 0)
        .and_then(|entry| entry.name)
        .map(str::to_owned);
    assert!(command.is_some());
    assert_eq!(
        config_content_name(
            &content,
            &serde_json::json!({"type": "content", "content": "unitCommand", "id": 0})
        ),
        command
    );
    // A `Point2` (bridge link) has no content name.
    assert_eq!(
        config_content_name(
            &content,
            &serde_json::json!({"type": "point", "x": 1, "y": 2})
        ),
        None
    );
}
