// SPDX-License-Identifier: GPL-3.0-only

//! Fixed-step pump and capture loop driving the sim.
//! Upstream: core/src/mindustry/core/Logic.java (fixed-step pump)

use super::*;

/// Parses `--capture <path>` / `--capture=<path>` from the user args (after `--`).
pub(super) fn parse_capture_args() -> Option<CaptureRequest> {
    let args = Os::singleton().get_cmdline_user_args();
    let slice = args.as_slice();
    let mut index = 0;
    while index < slice.len() {
        let arg = slice[index].to_string();
        if arg == "--capture" {
            let path = slice.get(index + 1)?.to_string();
            return Some(CaptureRequest {
                path,
                frames_waited: 0,
            });
        }
        if let Some(path) = arg.strip_prefix("--capture=") {
            return Some(CaptureRequest {
                path: path.to_owned(),
                frames_waited: 0,
            });
        }
        index += 1;
    }
    None
}

impl MindSimHost {
    /// Applies one immediate command; `false` when `mind-core` rejects it.
    pub(super) fn apply_command(&mut self, command: Command) -> bool {
        match self.sim.apply(command) {
            Ok(()) => {
                self.world_dirty = true;
                true
            }
            Err(err) => {
                log::warn!("command rejected: {err}");
                false
            }
        }
    }

    /// Drains queued relay commands and applies them at a tick boundary
    /// (plan 21 §3.5: apply only at fixed-tick starts, never mid-frame).
    pub(super) fn drain_pending_commands(&mut self) {
        while let Some(command) = self.pending_commands.pop_front() {
            match self.sim.command(command) {
                Ok(()) => self.world_dirty = true,
                Err(error) => log::warn!("relay command rejected by sim: {error}"),
            }
        }
    }

    /// Applies queued AI base placements for their owning team at the tick
    /// boundary (`team_ai_system` -> `CampaignAiQueues`; PAR-0139).
    pub(super) fn drain_ai_placements(&mut self) {
        let Some(queues) = self
            .world()
            .get_resource::<mind_core::game::runtime::CampaignAiQueues>()
        else {
            return;
        };
        let requests: Vec<mind_core::game::runtime::AiPlaceRequest> = match queues.0.lock() {
            Ok(mut queue) => queue.drain(..).collect(),
            Err(_) => return,
        };
        for request in requests {
            let placed = self.sim.place_for_team(
                request.x,
                request.y,
                request.block,
                request.rot,
                request.team,
            );
            if placed {
                self.world_dirty = true;
                log::info!(
                    "campaign ai: placed team {} block at ({}, {})",
                    request.team,
                    request.x,
                    request.y
                );
            } else {
                log::debug!(
                    "campaign ai: rejected team {} placement at ({}, {})",
                    request.team,
                    request.x,
                    request.y
                );
            }
        }
    }

    /// Publishes the live selectable-unit snapshot to the sibling `Input`
    /// (`MindInput.set_selectable_units_json`; PAR-0140): upstream feeds the
    /// selectable list from `Units` each frame, so drag/tap selection and
    /// command mode see live units.
    pub(super) fn publish_selectable_units(&mut self) {
        let Some(mut input) = self.input.clone() else {
            return;
        };
        let json = self.sim.selectable_units_json();
        input
            .bind_mut()
            .set_selectable_units_json(GString::from(json.as_str()));
    }

    /// Runs `steps` fixed steps, applying scenario commands before each tick.
    pub(super) fn advance_steps(&mut self, steps: u32) {
        for _ in 0..steps {
            self.drain_pending_commands();
            self.drain_ai_placements();
            let applied_before = self.sim.commands_applied();
            let result = if let Some(player) = self.player.as_mut() {
                player.step(&mut self.sim)
            } else {
                self.sim.tick().map(|()| true)
            };
            match result {
                // A finished scenario keeps ticking without further commands.
                Ok(false) => {
                    if let Err(err) = self.sim.tick() {
                        log::error!("sim tick failed after scenario end: {err}");
                        break;
                    }
                }
                Ok(true) => {}
                Err(err) => {
                    log::error!("sim tick failed: {err}");
                    break;
                }
            }
            if self.sim.commands_applied() != applied_before {
                self.world_dirty = true;
            }
            // A world-processor `setblock` mutated tiles (H06 T8).
            if self.sim.logic_world_changed {
                self.sim.logic_world_changed = false;
                self.world_dirty = true;
            }
            self.tick_fx_view();
        }
        self.publish_selectable_units();
        // A rules adoption or mid-match `setrule` changed the map-area rect
        // (T5): mark the world dirty so the renderer rebuilds.
        if self.sim.grid.limit_area != self.last_limit_area {
            self.last_limit_area = self.sim.grid.limit_area;
            self.world_dirty = true;
        }
    }

    /// Advances the plan-17 `MindFx` view by one fixed tick (sim→view hook).
    pub(super) fn tick_fx_view(&mut self) {
        if let Some(fx) = self.fx.as_mut() {
            fx.bind_mut().tick_view();
        }
    }

    pub(super) fn emit_state(&mut self) {
        let tick = self.sim.tick_count() as i64;
        let checksum = GString::from(self.sim.checksum_hex().as_str());
        let _ = self
            .base_mut()
            .emit_signal("state_changed", &[tick.to_variant(), checksum.to_variant()]);
    }

    pub(super) fn emit_world_changed(&mut self) {
        if self.world_dirty {
            self.world_dirty = false;
            self.revision = self.revision.wrapping_add(1);
            let _ = self.base_mut().emit_signal("world_changed", &[]);
        }
    }

    /// Emits the `world_loaded` signal (`WorldLoadEvent`; `World.loadMap`).
    pub(super) fn emit_world_loaded(&mut self) {
        let _ = self.base_mut().emit_signal("world_loaded", &[]);
    }

    /// Captures the viewport texture to a PNG.
    ///
    /// Uses `get_viewport().get_texture().get_image()` — the canonical Godot 4
    /// path (works in windowed and embedded runs; a dummy/headless renderer has
    /// no readable image, which is reported as `false`).
    pub(super) fn capture_to(&self, path: &str) -> bool {
        let Some(viewport) = self.base().get_viewport() else {
            return false;
        };
        let Some(texture) = viewport.get_texture() else {
            return false;
        };
        let Some(image) = texture.get_image() else {
            return false;
        };
        if image.is_empty() {
            return false;
        }
        let native = ProjectSettings::singleton().globalize_path(path);
        image.save_png(&native) == godot::global::Error::OK
    }

    /// Processes a pending `-- --capture <path>` request (quit code 0 on success).
    pub(super) fn process_capture(&mut self) {
        let Some(mut request) = self.capture.take() else {
            return;
        };
        request.frames_waited += 1;
        if request.frames_waited < CAPTURE_WARMUP_FRAMES {
            self.capture = Some(request);
            return;
        }

        let ok = self.capture_to(&request.path);
        if ok {
            log::info!("capture written to `{}`", request.path);
        } else {
            log::error!(
                "capture to `{}` failed (headless/dummy renderer has no viewport image)",
                request.path
            );
        }
        if let Some(mut tree) = self.base().get_tree_or_null() {
            tree.quit_ex().exit_code(i32::from(!ok)).done();
        }
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// `Logic.reset()` world half (`Logic.java:299-311`): `Groups.clear()`,
    /// `Time.clear()`, `world.tiles` emptied and `state = new GameState()`
    /// (menu), through [`Sim::reset`]. The host's per-match leftovers (sector
    /// rules/plans/resume, pending launch/generation, the scenario player, the
    /// cutscene input lock) go with it, so `get_state_json().phase` reads
    /// `menu` and `live_rules()` is empty once a game ends (L02 T2). Called
    /// only from `MindCampaign.reset_to_menu`, the single return-to-menu path.
    #[func]
    pub fn reset_to_menu(&mut self) {
        self.sim.reset();
        self.sim.reset_logic_presentation();
        self.player = None;
        self.sector_rules = None;
        self.sector_plans.clear();
        self.sector_resume = None;
        self.pending_launch = None;
        self.pending_launch_core = None;
        self.pending_gen_extras = None;
        self.pending_map_gen_seed = None;
        self.generated_loadout_placed = false;
        self.input_locked = false;
        self.pending_commands.clear();
        self.runner = FixedStepRunner::for_rate(
            self.sim.config().fixed_hz,
            self.sim.config().max_ticks_per_frame,
        );
        self.mark_world_changed();
    }

    /// Pauses/resumes the fixed-step pump (resets the accumulator).
    #[func]
    pub fn set_paused(&mut self, paused: bool) {
        self.sim.set_paused(paused);
        self.runner = FixedStepRunner::for_rate(
            self.sim.config().fixed_hz,
            self.sim.config().max_ticks_per_frame,
        );
        self.emit_state();
    }

    /// Input-side command-mode fact for `CommandModeObjective` (G08 calls this
    /// from `MindInput` whenever the command-mode selection changes; T4).
    #[func]
    pub fn set_command_mode_satisfied(&mut self, value: bool) {
        self.sim.set_command_mode_satisfied(value);
    }

    /// Runs `ticks` sim steps synchronously and returns the new tick count.
    #[func]
    pub fn step(&mut self, ticks: i64) -> i64 {
        let count = ticks.clamp(0, MAX_STEP_TICKS) as u32;
        if count > 0 {
            self.advance_steps(count);
            self.emit_state();
            self.emit_world_changed();
        }
        self.get_tick()
    }

    /// Writes a PNG of the running viewport to `path` (native or `user://`).
    #[func]
    pub fn capture(&mut self, path: GString) -> bool {
        let requested = path.to_string();
        match self.capture_to(&requested) {
            true => {
                log::info!("capture written to `{requested}`");
                true
            }
            false => {
                log::error!("capture to `{requested}` failed (no viewport image?)");
                false
            }
        }
    }
}
