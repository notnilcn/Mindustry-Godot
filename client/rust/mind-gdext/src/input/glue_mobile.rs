// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Mobile touch routing into the gesture/mobile bridges.
//! Upstream: core/src/mindustry/input/MobileInput.java

use super::*;

impl MindInput {
    /// Folds a Godot touch event into the mobile gesture bridge, applies the
    /// resulting camera pan/zoom and runs the placement decision tree
    /// (`MobileInput.touchDown/tap/longPress/touchUp`). Returns whether the
    /// event was a touch event.
    pub(super) fn route_touch(&mut self, raw: &RawEvent) -> bool {
        let time = self.mobile_time;
        let events = match raw {
            RawEvent::TouchDown { pointer, x, y } => {
                if self.ui_captures_at(*x, *y) {
                    self.ui_pointers.insert(*pointer);
                    return true;
                }
                self.mobile.note_pointer(*x, *y);
                self.mobile.touch_down(time, *x, *y, *pointer)
            }
            RawEvent::TouchMove { pointer, x, y } => {
                if self.ui_pointers.contains(pointer) {
                    return true;
                }
                self.mobile.note_pointer(*x, *y);
                self.mobile.touch_drag(time, *x, *y, *pointer)
            }
            RawEvent::TouchUp { pointer, x, y } => {
                if self.ui_pointers.remove(pointer) {
                    return true;
                }
                self.mobile.note_pointer(*x, *y);
                self.mobile.touch_up(time, *x, *y, *pointer)
            }
            _ => return false,
        };
        self.apply_gestures(&events);
        // The core tree (`handle_gesture_with`) decides taps/line starts; the
        // selected block's `LineBlock` + `LineRules` come from content.
        for event in &events {
            self.fold_mobile_gesture(*event);
        }
        match raw {
            // `MobileInput.update` lineMode branch runs every frame while the
            // finger is held; a drag event is the closest frame boundary here.
            RawEvent::TouchMove { .. } => self.update_mobile_line(),
            // `MobileInput.touchUp` (`MobileInput.java:557-580`): commit the
            // placement line or deconstruct the breaking rect.
            RawEvent::TouchUp { .. } => self.finish_mobile_touch(),
            _ => {}
        }
        true
    }

    /// Folds one gesture event into the mobile controller with the live world,
    /// content and caps (`MobileInput`'s callbacks run in world pixels: Java
    /// converts screen points through `Core.input.mouseWorld`).
    pub(super) fn fold_mobile_gesture(&mut self, event: gesture::GestureEvent) {
        let Some(camera) = self.camera.clone() else {
            return;
        };
        let world_event = {
            let camera = camera.bind();
            mobile::event_to_world(&event, |x, y| {
                let world = camera.screen_to_world(f64::from(x), f64::from(y));
                (world.x, world.y)
            })
        };
        let caps = self.mobile_caps();
        let Some(host) = self.sim_host.clone() else {
            return;
        };
        let guard = host.bind();
        let world = desktop::HostWorld::new(&guard);
        let _ = self
            .mobile
            .handle_gesture(world_event, &world, guard.content_registry(), &caps);
    }

    /// `MobileInput.update` lineMode branch: rebuild the active line at the
    /// tracked pointer's world position (auto-panned lines follow the finger).
    pub(super) fn update_mobile_line(&mut self) {
        if !self.mobile.line_mode() {
            return;
        }
        let Some((x, y)) = self.mobile.pointer() else {
            return;
        };
        let Some(camera) = self.camera.clone() else {
            return;
        };
        let world_pos = camera.bind().screen_to_world(f64::from(x), f64::from(y));
        let Some(host) = self.sim_host.clone() else {
            return;
        };
        let guard = host.bind();
        let world = desktop::HostWorld::new(&guard);
        self.mobile
            .update_line(&world, guard.content_registry(), world_pos.x, world_pos.y);
    }

    /// `MobileInput.touchUp` line branch: queue the placed plans and enqueue
    /// break commands for a deconstruction line, then leave line mode.
    fn finish_mobile_touch(&mut self) {
        if !self.mobile.line_mode() {
            return;
        }
        let Some(mut host) = self.sim_host.clone() else {
            return;
        };
        // Java `touchUp` reads the release point (`tileX(screenX)`, `tileY(screenY)`),
        // not the last drag event: fold it into the tracked line end first.
        let release = self.mobile.pointer().and_then(|(x, y)| {
            let camera = self.camera.clone()?;
            let world = camera.bind().screen_to_world(f64::from(x), f64::from(y));
            Some((world.x, world.y))
        });
        let result = {
            let guard = host.bind();
            let world = desktop::HostWorld::new(&guard);
            if let Some((world_x, world_y)) = release {
                self.mobile
                    .update_line(&world, guard.content_registry(), world_x, world_y);
            }
            self.mobile.finish_touch(&world)
        };
        self.enqueue_mobile_result(&mut host, result);
        // The line is over; a new touch records a fresh pointer.
        self.mobile.clear_pointer();
    }

    /// Enqueues the sim commands for a committed mobile line/selection
    /// (`flushSelectPlans` -> `player.unit().addBuild`, `tryBreakBlock`).
    pub(super) fn enqueue_mobile_result(
        &self,
        host: &mut Gd<MindSimHost>,
        result: mind_core::input::MobileTouchResult,
    ) {
        let team = host.bind().player_team();
        let mut commands = Vec::new();
        for plan in result.placed {
            commands.push(SimCommand::Place {
                x: plan.x as i16,
                y: plan.y as i16,
                block: plan.block.raw(),
                rotation: plan.rotation as i8,
                team,
                // Player input queues on the player unit (`flushPlans`).
                player: Some(0),
            });
        }
        for (x, y) in result.breaks {
            commands.push(SimCommand::Break {
                x,
                y,
                player: Some(0),
            });
        }
        if commands.is_empty() {
            return;
        }
        let mut guard = host.bind_mut();
        for command in commands {
            guard.submit_player_command(command);
        }
    }

    /// Capabilities pushed into the mobile decision tree (`settings`/focus).
    fn mobile_caps(&self) -> mobile::MobileCaps {
        let focus = FocusState {
            has_dialog: self.bridge.ui_dialog,
            has_field: self.bridge.text_focus,
            is_menu: !self.bridge.in_game,
            ..Default::default()
        };
        mobile::MobileCaps {
            focus,
            menu: !self.bridge.in_game,
            editor: false,
            cutscene: false,
            keyboard: self.mobile.keyboard_mode(),
            block_replace: self.bridge.block_replace,
            swap_diagonal: crate::settings_hub::with_settings(|settings| {
                settings.get_bool("swapdiagonal", false)
            }),
        }
    }

    /// Applies gesture events that map directly to the camera rig (mobile pan
    /// and pinch zoom); taps/long-presses stay with the plan-14 mobile HUD.
    fn apply_gestures(&mut self, events: &[gesture::GestureEvent]) {
        let Some(camera) = self.bridge.camera.clone() else {
            return;
        };
        for event in events {
            match event {
                gesture::GestureEvent::Pan {
                    delta_x, delta_y, ..
                } => {
                    // `MobileInput.pan`: while a queued plan is selected the
                    // drag shifts the plans instead of moving the camera (the
                    // core controller does that shift in `handle_gesture_with`).
                    if self.mobile.controller.selecting {
                        continue;
                    }
                    camera
                        .clone()
                        .bind_mut()
                        .mobile_pan(*delta_x as f64, *delta_y as f64);
                }
                gesture::GestureEvent::Zoom {
                    initial_distance,
                    distance,
                } if *initial_distance > 0.0 => {
                    let amount = (*distance / *initial_distance - 1.0) * 4.0;
                    camera.clone().bind_mut().zoom_by(amount as f64);
                }
                _ => {}
            }
        }
    }

    /// Calls `hide_panel()` on a HUD fragment (or clears `visible` when the
    /// fragment has no such method).
    pub(super) fn hide_panel_at(&self, name: &str) {
        let Some(ui_root) = self.ui_root.clone() else {
            return;
        };
        let fragment = ui_root.bind().hud_fragment(name);
        let Some(mut node) = fragment else {
            return;
        };
        if node.has_method("hide_panel") {
            let _ = node.call("hide_panel", &[]);
        } else {
            node.set("visible", &false.to_variant());
        }
    }
}
