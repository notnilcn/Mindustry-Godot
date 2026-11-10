// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers of the old `misc.rs` (string coercion and the deferred action). Filled by S07 (docs/PLANS).
//! Upstream: none (port-only; shared helpers of the old misc)

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::RichTextLabel;
use godot::prelude::*;

use crate::ui::kit::{self, MindLabel};

/// Godot-style string conversion for a context value.
pub(in crate::ui::dialogs) fn variant_text(value: &Variant) -> String {
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string();
    }
    if let Ok(boolean) = value.try_to::<bool>() {
        return boolean.to_string();
    }
    if let Ok(integer) = value.try_to::<i64>() {
        return integer.to_string();
    }
    if let Ok(float) = value.try_to::<f64>() {
        return float.to_string();
    }
    String::new()
}

/// `Planet.localizedName` (`planet.<name>.name`) for the completion banner.
pub(in crate::ui::dialogs) fn planet_localized(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    let key = format!("planet.{name}.name");
    let localized = kit::t(&format!("@{key}")).to_string();
    if localized.is_empty() || localized == key {
        name.to_string()
    } else {
        localized
    }
}

/// Reads a JSON-derived number as an integer (JSON numbers are `FLOAT`).
pub(in crate::ui::dialogs) fn variant_i64(value: &Variant) -> i64 {
    if let Ok(integer) = value.try_to::<i64>() {
        return integer;
    }
    value
        .try_to::<f64>()
        .map(|number| number as i64)
        .unwrap_or(0)
}

pub(in crate::ui::dialogs) fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

pub(in crate::ui::dialogs) fn string_array(value: Option<Variant>) -> Vec<String> {
    value
        .map(|value| value.to::<VarArray>())
        .map(|array| {
            array
                .iter_shared()
                .map(|entry| entry.to::<GString>().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// A queued one-shot action (`Callable.call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// A `Callable` that queues `action` for the idle phase (`call_deferred`).
pub(in crate::ui::dialogs) fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
    let slot: DeferredAction = Rc::new(RefCell::new(Some(Box::new(action))));
    let callable = Callable::from_fn(name, move |_args: &[&Variant]| {
        let queued = slot.borrow_mut().take();
        if let Some(action) = queued {
            action();
        }
        Variant::nil()
    });
    callable.call_deferred(&[]);
}
