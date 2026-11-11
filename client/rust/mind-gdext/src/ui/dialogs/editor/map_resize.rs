// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map resize dialog controller.
//! Upstream: editor/MapResizeDialog.java

use godot::classes::Control;
use godot::classes::VBoxContainer;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::widgets::factory;

use super::util::*;
use crate::gdutil::variant::to_i64_no_text;

use super::map_info::close_dialog;

// ---------------------------------------------------------------------------
// MapResizeDialog (`map_resize`, `MapResizeDialog.java`)
// ---------------------------------------------------------------------------

/// `MapResizeDialog.{minSize, maxSize}`.
const RESIZE_MIN_SIZE: i64 = 50;
const RESIZE_MAX_SIZE: i64 = 800;

/// Meta keys holding the working width/height/shift values.
const RESIZE_WIDTH_META: &str = "mind_resize_width";
const RESIZE_HEIGHT_META: &str = "mind_resize_height";
const RESIZE_SHIFT_X_META: &str = "mind_resize_shift_x";
const RESIZE_SHIFT_Y_META: &str = "mind_resize_shift_y";

/// `@editor.resizemap` — the four fields with Java's filters and the
/// `@cancel`/`@ok` buttons calling `MindEditor.resize_map(w, h, sx, sy)`.
pub struct MapResizeDialog;

impl DialogController for MapResizeDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.resizemap");
        dialog.should_pause = false;
        dialog.clear_buttons();
        let cancel = kit::t("@cancel").to_string();
        let close = dialog.to_gd().callable("close_pressed");
        dialog.add_button(&cancel, Some(close), "", 200.0);
        let ok = kit::t("@ok").to_string();
        let target = dialog.to_gd();
        let apply = Callable::from_fn("map_resize_ok", move |_args: &[&Variant]| {
            resize_apply(&target);
        });
        dialog.add_button(&ok, Some(apply), "", 200.0);
        let mut node = dialog.to_gd();
        set_meta_int(&mut node, RESIZE_SHIFT_X_META, 0);
        set_meta_int(&mut node, RESIZE_SHIFT_Y_META, 0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let _ = MindDialog::take_request(dialog);
        let mut node = dialog.to_gd();
        let status = editor_call("status", &[])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let width = match status.get("width") {
            Some(value) => to_i64_no_text(Some(value)),
            None => RESIZE_MIN_SIZE,
        };
        let height = match status.get("height") {
            Some(value) => to_i64_no_text(Some(value)),
            None => RESIZE_MIN_SIZE,
        };
        set_meta_int(&mut node, RESIZE_WIDTH_META, width);
        set_meta_int(&mut node, RESIZE_HEIGHT_META, height);
        build_resize_body(dialog);
    }
}

/// `shown(() -> { cont.clear(); width = editor.width(); ... })`.
fn build_resize_body(dialog: &mut MindDialog) {
    dialog.clear_content();
    let mut root = dialog.content_table();
    // `cont.table(table -> ...)` with no grow: the label/field column is
    // content-sized and centred.
    let mut column = VBoxContainer::new_alloc();
    root.bind_mut().add(&column).pad(4.0);
    let node = dialog.to_gd();
    let width = meta_int(&node, RESIZE_WIDTH_META, RESIZE_MIN_SIZE);
    let height = meta_int(&node, RESIZE_HEIGHT_META, RESIZE_MIN_SIZE);
    let shift_x = meta_int(&node, RESIZE_SHIFT_X_META, 0);
    let shift_y = meta_int(&node, RESIZE_SHIFT_Y_META, 0);
    column.add_child(&resize_size_row("width", width, true));
    column.add_child(&resize_size_row("height", height, false));
    column.add_child(&resize_shift_row("editor.shiftx", shift_x, true));
    column.add_child(&resize_shift_row("editor.shifty", shift_y, false));
}

/// One size row (`table.field(...).valid(canParsePositiveInt && in range)`).
fn resize_size_row(label_key: &str, value: i64, horizontal: bool) -> Gd<Control> {
    // code-instantiated: one label+field row per resize value.
    let mut row = factory::hbox(8);
    let mut label = factory::label(&kit::t(&format!("@{label_key}")).to_string());
    label.set_custom_minimum_size(Vector2::new(90.0, 0.0));
    row.add_child(&as_control(&label));
    let mut field = text_field(&value.to_string(), 3);
    field.set_custom_minimum_size(Vector2::new(180.0, 60.0));
    let target = resize_dialog_handle();
    let callable = Callable::from_fn("map_resize_size", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| value.to::<GString>().to_string())
            .unwrap_or_default();
        let Some(number) = valid_int(&text) else {
            return;
        };
        if !(RESIZE_MIN_SIZE..=RESIZE_MAX_SIZE).contains(&number) {
            return;
        }
        if let Some(target) = target.clone() {
            let mut node = target;
            set_meta_int(
                &mut node,
                if horizontal {
                    RESIZE_WIDTH_META
                } else {
                    RESIZE_HEIGHT_META
                },
                number,
            );
        }
    });
    field.connect("text_changed", &callable);
    row.add_child(&field);
    row.upcast()
}

/// One shift row (`Strings::canParseInt`, max length 4).
fn resize_shift_row(label_key: &str, value: i64, horizontal: bool) -> Gd<Control> {
    // code-instantiated: one label+field row per scroll shift.
    let mut row = factory::hbox(8);
    let mut label = factory::label(&kit::t(&format!("@{label_key}")).to_string());
    label.set_custom_minimum_size(Vector2::new(90.0, 0.0));
    row.add_child(&as_control(&label));
    let mut field = text_field(&value.to_string(), 4);
    field.set_custom_minimum_size(Vector2::new(180.0, 60.0));
    let target = resize_dialog_handle();
    let callable = Callable::from_fn("map_resize_shift", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| value.to::<GString>().to_string())
            .unwrap_or_default();
        // Leading `-` stays typeable; the value commits only on a parseable string.
        if text.is_empty() || text == "-" {
            return;
        }
        let Some(number) = valid_int(&text) else {
            return;
        };
        if let Some(target) = target.clone() {
            let mut node = target;
            set_meta_int(
                &mut node,
                if horizontal {
                    RESIZE_SHIFT_X_META
                } else {
                    RESIZE_SHIFT_Y_META
                },
                number,
            );
        }
    });
    field.connect("text_changed", &callable);
    row.add_child(&field);
    row.upcast()
}

/// The dialog node the row callbacks write their values into.
///
/// The rows are built from [`build_resize_body`], which only sees
/// `&mut MindDialog`; the callbacks re-find the dialog by its `MindUi` name so
/// the metadata writes land on the same node.
fn resize_dialog_handle() -> Option<Gd<MindDialog>> {
    dialog_node("map_resize").and_then(|node| node.try_cast::<MindDialog>().ok())
}

/// `cons.get(width, height, shiftX, shiftY)` + `hide()`.
fn resize_apply(dialog: &Gd<MindDialog>) {
    let node = dialog.clone();
    let width = meta_int(&node, RESIZE_WIDTH_META, RESIZE_MIN_SIZE) as i32;
    let height = meta_int(&node, RESIZE_HEIGHT_META, RESIZE_MIN_SIZE) as i32;
    let shift_x = meta_int(&node, RESIZE_SHIFT_X_META, 0) as i32;
    let shift_y = meta_int(&node, RESIZE_SHIFT_Y_META, 0) as i32;
    let _ = editor_call(
        "resize_map",
        &[
            width.to_variant(),
            height.to_variant(),
            shift_x.to_variant(),
            shift_y.to_variant(),
        ],
    );
    close_dialog(dialog);
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_resize" => Some(Box::new(MapResizeDialog)),
        _ => None,
    }
}
