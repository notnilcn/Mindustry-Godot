// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: arc.scene.ui.Tooltip (plan 14 §3.3).

//! `MindTooltip` — the tooltip helper (`Tooltip`).
//!
//! Godot's native `tooltip_text` renders the black8+margins look through the
//! theme; `allow_mobile` and the top-left container variant are recorded as
//! metadata for the mobile branches (plan 14 §3.3/M7).
//!
//! The GDScript statics live on as module functions here (`attach`,
//! `refresh`, `set_container_position`) and as `MindTooltip` static `#[func]`s,
//! so both Rust call sites and the MCP/GDScript surface keep the same shape.

use godot::classes::{Control, IRefCounted, RefCounted};
use godot::prelude::*;

/// Meta key recording `Tooltip.allowMobile` for the mobile tooltip branch.
const ALLOW_MOBILE_META: &str = "mind_tooltip_allow_mobile";
/// Meta key recording `setContainerPosition`'s top-left variant.
const TOP_LEFT_META: &str = "mind_tooltip_top_left";

/// Attaches a tooltip to `control` (`Tooltip(background, textProvider)`).
pub fn attach(control: &mut Gd<Control>, provider: &Variant, allow_mobile: bool) {
    let text = resolve(provider);
    control.set_tooltip_text(&text);
    control.set_meta(ALLOW_MOBILE_META, &allow_mobile.to_variant());
}

/// Re-resolves the tooltip text from its provider.
pub fn refresh(control: &mut Gd<Control>, provider: &Variant) {
    let text = resolve(provider);
    control.set_tooltip_text(&text);
}

/// `setContainerPosition` top-left variant for `addDescTooltip`.
pub fn set_container_position(control: &mut Gd<Control>, top_left: bool) {
    control.set_meta(TOP_LEFT_META, &top_left.to_variant());
}

/// `Tooltip`'s text provider: a `Callable` is invoked, `null` is empty and any
/// other value is `str()`-converted.
pub fn resolve(provider: &Variant) -> GString {
    if let Ok(callable) = provider.try_to::<Callable>() {
        if callable.is_valid() {
            return callable.call(&[]).stringify();
        }
        return GString::new();
    }
    if provider.is_nil() {
        return GString::new();
    }
    provider.stringify()
}

/// `MindTooltip` — the static tooltip helper class (`Tooltip`).
#[derive(GodotClass)]
#[class(rename = RustMindTooltip, base=RefCounted)]
pub struct MindTooltip {
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for MindTooltip {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl MindTooltip {
    /// Attaches a tooltip to `control` (`Tooltip(background, textProvider)`).
    #[func]
    pub fn attach(control: Gd<Control>, provider: Variant, allow_mobile: bool) {
        let mut control = control;
        attach(&mut control, &provider, allow_mobile);
    }

    /// Re-resolves the tooltip text from its provider.
    #[func]
    pub fn refresh(control: Gd<Control>, provider: Variant) {
        let mut control = control;
        refresh(&mut control, &provider);
    }

    /// `setContainerPosition` top-left variant for `addDescTooltip`.
    #[func]
    pub fn set_container_position(control: Gd<Control>, top_left: bool) {
        let mut control = control;
        set_container_position(&mut control, top_left);
    }
}
