// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The developer widget gallery controller.
//! Upstream: none (port-only; developer widget gallery)

use godot::classes::Control;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit::MindTable;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::bar::MindBar;
use crate::ui::kit::widgets::check::MindCheck;
use crate::ui::kit::widgets::core_items_display::CoreItemsDisplay;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::widgets::grid_image::MindGridImage;
use crate::ui::kit::widgets::items_display::ItemsDisplay;
use crate::ui::kit::widgets::warning_bar::MindWarningBar;

// ---------------------------------------------------------------------------
// WidgetGalleryDialog
// ---------------------------------------------------------------------------

/// Dev-only gallery: one of each M1 widget for in-engine/MCP inspection
/// (`client/ui/dialogs/widget_gallery.gd`).
pub struct WidgetGalleryDialog;

impl DialogController for WidgetGalleryDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `title_text = "Widget Gallery"` (a literal, not a bundle key).
        dialog.set_title_literal("Widget Gallery");
        self.build(dialog);
    }
}

impl WidgetGalleryDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        let Some(mut content) = dialog.content() else {
            return;
        };
        let mut grid = MindTable::new_alloc();
        grid.bind_mut().grow_horizontal();
        let control: Gd<Control> = grid.clone().upcast();
        content.add_child(&control);

        // code-instantiated: the gallery is a dev harness that adds one of each
        // widget; its contents are intentionally data/code-driven (plan 14 §5 M1).
        let mut bar = MindBar::new_alloc();
        bar.bind_mut()
            .setup(GString::from("Bar"), styles::ACCENT, 0.5);
        grid.bind_mut().add(&bar).width(200.0).height(24.0).pad(2.0);
        let warning = MindWarningBar::new_alloc();
        grid.bind_mut()
            .add(&warning)
            .width(200.0)
            .height(24.0)
            .pad(2.0);
        grid.bind_mut().row();

        let grid_image = MindGridImage::with_size(8, 4);
        grid.bind_mut()
            .add(&grid_image)
            .width(200.0)
            .height(64.0)
            .pad(2.0);
        let mut check = MindCheck::new_alloc();
        let noop = Callable::from_fn("gallery_check", |_args: &[&Variant]| {});
        check.bind_mut().setup(GString::from("Check"), true, noop);
        grid.bind_mut().add(&check).pad(2.0);
        grid.bind_mut().row();

        let items = ItemsDisplay::new_alloc();
        grid.bind_mut().add(&items).pad(2.0);
        let core_items = CoreItemsDisplay::new_alloc();
        grid.bind_mut().add(&core_items).pad(2.0);
        grid.bind_mut().row();

        let label = factory::label("Label");
        grid.bind_mut().add(&label).pad(2.0);
        let button = factory::button("Button");
        grid.bind_mut().add(&button).pad(2.0);

        dialog.add_close_button(210.0);
    }
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "widget_gallery" => Some(Box::new(WidgetGalleryDialog)),
        _ => None,
    }
}
