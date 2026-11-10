// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot-callable keybind endpoints: rebind/reset/unbind and the keybind registry dump.
//! Upstream: core/src/mindustry/input/KeyBinds.java

use super::*;

#[godot_api(secondary)]
impl MindInput {
    /// Rebinds a single key by registry name and persists it.
    #[func]
    pub fn rebind(&mut self, name: GString, code: GString) -> bool {
        self.rebind_and_save(|state| bindings::rebind(state, &name.to_string(), &code.to_string()))
    }

    /// Rebinds `name` from a Godot key display string (`OS.get_keycode_string`),
    /// normalizing it to the registry name (GDScript capture path).
    #[func]
    pub fn rebind_key(&mut self, name: GString, godot_text: GString) -> bool {
        let code = events::normalize_key_name(&godot_text.to_string());
        self.rebind_and_save(|state| bindings::rebind(state, &name.to_string(), &code))
    }

    /// Rebinds an axis binding to a negative/positive key pair (PAR-0102,
    /// `KeybindDialog.rebind`): both halves are Godot key display strings
    /// (empty clears a direction) and a wheel capture passes `scroll`. Persists
    /// on success.
    #[func]
    pub fn rebind_axis(&mut self, name: GString, negative: GString, positive: GString) -> bool {
        let negative = events::normalize_key_name(&negative.to_string());
        let positive = events::normalize_key_name(&positive.to_string());
        self.rebind_and_save(|state| {
            bindings::rebind_axis(state, &name.to_string(), &negative, &positive)
        })
    }

    /// Resets one binding to its upstream default and persists it.
    #[func]
    pub fn reset_keybind(&mut self, name: GString) -> bool {
        let Some(id) = self.bind_id(&name.to_string()) else {
            return false;
        };
        *self.bindings.value_mut(id) = mind_core::input::binding::default_value_for(id);
        bindings::save(&self.bindings);
        true
    }

    /// Resets every binding to its upstream default and persists it.
    #[func]
    pub fn reset_keybinds(&mut self) {
        self.bindings.reset();
        bindings::save(&self.bindings);
    }

    /// Unbinds one binding and persists it (`Binding.unset`).
    #[func]
    pub fn unbind_keybind(&mut self, name: GString) -> bool {
        let Some(id) = self.bind_id(&name.to_string()) else {
            return false;
        };
        self.bindings.clear(id);
        bindings::save(&self.bindings);
        true
    }

    /// The full binding registry + current values as JSON. `display` and
    /// `negativeDisplay` are the Arc `KeyCode.getName()` strings the keybind
    /// dialog renders; `default` drives the per-row Reset disabled state.
    #[func]
    pub fn keybinds_json(&self) -> GString {
        let mut list = Vec::new();
        for (index, bind) in KeyBindTable::all().iter().enumerate() {
            let id = index as u16;
            let (negative, positive) = match self.bindings.value(id) {
                BindingValue::Axis { negative, positive } => {
                    (negative.as_deref(), positive.as_deref())
                }
                other => (None, other.display_name()),
            };
            list.push(serde_json::json!({
                "name": bind.name,
                "category": bind.category.map(|c| c.name()),
                "axis": bind.kind == mind_core::input::KeyKind::Axis,
                "bundle": bind.bundle_key(),
                "value": self.bindings.name(id),
                "display": positive.map(key_display_name),
                "negativeDisplay": negative.map(key_display_name),
                "default": self.bindings.is_default(id),
            }));
        }
        GString::from(&serde_json::Value::Array(list).to_string())
    }
}

impl MindInput {
    /// Applies one binding-table change and persists the table when it reports
    /// success (`KeyBinds.rebind`/`rebindAxis` + `bindings::save`).
    fn rebind_and_save(&mut self, apply: impl FnOnce(&mut BindingState) -> bool) -> bool {
        let changed = apply(&mut self.bindings);
        if changed {
            bindings::save(&self.bindings);
        }
        changed
    }

    /// Numeric id for a registry binding name.
    fn bind_id(&self, name: &str) -> Option<u16> {
        KeyBindTable::all()
            .iter()
            .position(|bind| bind.name == name)
            .map(|index| index as u16)
    }
}
