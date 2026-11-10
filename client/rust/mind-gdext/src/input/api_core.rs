// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! First Godot-callable group: text-input result, plan config, frame probes, injection, focus locks.
//! Upstream: core/src/mindustry/input/InputHandler.java

use super::*;

#[godot_api(secondary)]
impl MindInput {
    /// Ctrl+`ping` label answer (`DesktopInput.java:492-493`); `""` is the
    /// prompt's cancel (`UI.showTextInput`). Other prompts also emit this
    /// signal, so it only acts while a ping prompt is in flight.
    #[func]
    fn _on_text_input_result(&mut self, text: GString) {
        let Some((x, y)) = self.ping_pending.take() else {
            return;
        };
        let text = text.to_string();
        if text.is_empty() {
            return;
        }
        self.bridge.ping_at(x, y, Some(text.trim()));
    }

    /// `PlanConfigFragment.plan_config_applied`: the chosen plan-point config
    /// (`selected.config = content`, `DesktopInput.java:808`).
    #[func]
    fn _on_plan_config_applied(&mut self, config: VarDictionary) {
        let value = config
            .get("value")
            .and_then(|value| value.try_to::<GString>().ok())
            .unwrap_or_default();
        self.bridge.apply_plan_config(value.to_string().as_str());
    }

    /// Frames pumped.
    #[func]
    pub fn frame(&self) -> i64 {
        self.frame as i64
    }

    /// Events drained on the last `_process`.
    #[func]
    pub fn last_event_count(&self) -> i64 {
        self.last_event_count as i64
    }

    /// Number of binds that differ from upstream defaults.
    #[func]
    pub fn rebind_count(&self) -> i64 {
        let defaults = BindingState::new();
        (0..KeyBindTable::len())
            .filter(|index| {
                let id = *index as u16;
                self.bindings.value(id) != defaults.value(id)
            })
            .count() as i64
    }

    /// Injects a key edge (GDScript/MCP testing).
    #[func]
    pub fn inject_key(&mut self, code: GString, down: bool) {
        let code = code.to_string();
        self.pending.push(if down {
            RawEvent::key_down(code)
        } else {
            RawEvent::key_up(code)
        });
    }

    /// Parses and queues one `RawEvent` JSON object (`{"t":...}`).
    #[func]
    pub fn push_event(&mut self, json: GString) -> bool {
        match serde_json::from_str::<RawEvent>(&json.to_string()) {
            Ok(event) => {
                self.pending.push(event);
                true
            }
            Err(error) => {
                if !self.warned {
                    log::warn!("MindInput: bad RawEvent JSON ({error})");
                    self.warned = true;
                }
                false
            }
        }
    }

    /// Sets the UI focus flags from a `FocusState` JSON object.
    #[func]
    pub fn set_ui_focus(&mut self, json: GString) -> bool {
        match serde_json::from_str::<FocusState>(&json.to_string()) {
            Ok(focus) => {
                self.focus = focus;
                true
            }
            Err(error) => {
                log::warn!("MindInput: bad FocusState JSON ({error})");
                false
            }
        }
    }

    /// `InputHandler.inputLocks`' `renderer.isCutscene()` entry: the camera
    /// rig raises it while the landing/launch cutscene runs (PAR-0131) and
    /// drops it when the animation ends. Pushes the change to the sim host.
    #[func]
    pub fn set_cutscene_lock(&mut self, locked: bool) {
        if self.locks.cutscene == locked {
            return;
        }
        self.locks.cutscene = locked;
        let any = self.locks.any();
        if self.bridge.locked != any {
            self.bridge.locked = any;
            if let Some(mut host) = self.bridge.host.clone() {
                host.bind_mut().set_input_locked(any);
            }
        }
    }

    /// `InputHandler.inputLocks`' `logicCutscene` entry (`InputHandler.java:86`):
    /// the camera rig raises it while a world-processor cutscene is active
    /// (H06 T3). Same bridge push as `set_cutscene_lock`.
    #[func]
    pub fn set_logic_cutscene_lock(&mut self, locked: bool) {
        if self.locks.logic_cutscene == locked {
            return;
        }
        self.locks.logic_cutscene = locked;
        let any = self.locks.any();
        if self.bridge.locked != any {
            self.bridge.locked = any;
            if let Some(mut host) = self.bridge.host.clone() {
                host.bind_mut().set_input_locked(any);
            }
        }
    }

    /// Replaces the lock set from a JSON list of lock names.
    #[func]
    pub fn set_locks(&mut self, json: GString) -> bool {
        match serde_json::from_str::<Vec<mind_core::input::LockId>>(&json.to_string()) {
            Ok(locks) => {
                let mut state = InputLocks::new();
                for lock in locks {
                    state.add_lock(lock);
                }
                self.locks = state;
                true
            }
            Err(error) => {
                log::warn!("MindInput: bad lock JSON ({error})");
                false
            }
        }
    }
}
