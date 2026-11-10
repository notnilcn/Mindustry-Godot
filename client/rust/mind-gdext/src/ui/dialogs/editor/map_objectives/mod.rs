// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map objectives dialog controller: objective list, panel and overlays.
//! Upstream: editor/MapObjectivesDialog.java

mod edit;
mod list;

use godot::classes::HBoxContainer;
use godot::classes::VBoxContainer;
use godot::classes::control::SizeFlags;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindScroll;
use crate::ui::kit::widgets::factory;

use super::util_e::*;

use self::edit::objective_field_row;
use self::list::{
    instantiate_objectives_canvas, objective_apply_picked, objective_bind_canvas,
    objective_bind_color, objective_bind_confirm, objective_bind_picker, objective_move,
    objective_normalize_positions, objective_open_class_picker, objective_remove_selected,
    objective_show_edit_menu, objective_small_button, objective_type_name, objective_visible_field,
};
use super::map_generate::find_named;

// ---------------------------------------------------------------------------
// MapObjectivesDialog (`map_objectives`, `MapObjectivesDialog.java`)
// ---------------------------------------------------------------------------

/// Panel width for the selected objective's fields (`PANEL_WIDTH`).
const OBJ_PANEL_WIDTH: f32 = 380.0;
/// Objective list (`MapObjectivesDialog.objectives`).
const OBJ_OBJECTIVES_META: &str = "mind_obj_objectives";
/// Selected objective index (`-1` for none).
const OBJ_SELECTED_META: &str = "mind_obj_selected";
/// The objective being edited in the class picker (`-1` = append).
const OBJ_PENDING_CLASS_META: &str = "mind_obj_pending_class";
/// The objective created by `@add` waiting for a canvas click (`canvas.query`).
const OBJ_PENDING_QUERY_META: &str = "mind_obj_pending_query";
/// Set while the `@clear` confirm is pending (`@settings.clear.confirm`).
const OBJ_PENDING_CLEAR_META: &str = "mind_obj_pending_clear";
/// The pending `list_select` target for content/team fields.
const OBJ_PICK_INDEX_META: &str = "mind_obj_pick_index";
const OBJ_PICK_FIELD_META: &str = "mind_obj_pick_field";
const OBJ_PICK_KIND_META: &str = "mind_obj_pick_kind";
const OBJ_PICKED_META: &str = "mind_obj_picked";
/// The color picker's answer target (`Color` interpreter).
const OBJ_COLOR_FIELD_META: &str = "mind_obj_color_field";
/// The `@LogicCode` answer target.
const OBJ_LOGIC_FIELD_META: &str = "mind_obj_logic_field";
const OBJ_LOGIC_BOUND_META: &str = "mind_obj_logic_bound";
/// Signal-binding guards.
const OBJ_PICKER_BOUND_META: &str = "mind_obj_picker_bound";
const OBJ_COLOR_BOUND_META: &str = "mind_obj_color_bound";
const OBJ_CONFIRM_BOUND_META: &str = "mind_obj_confirm_bound";
/// Node names for the code-built parts.
const OBJ_OVERLAY: &str = "ObjectivesOverlay";
const OBJ_CANVAS_NODE: &str = "ObjectivesCanvas";
const OBJ_PANEL_NODE: &str = "ObjectivesPanel";

const OBJ_REQUEST_PANEL: &str = "panel";
const OBJ_REQUEST_BUILD: &str = "build";
const OBJ_REQUEST_PICKED: &str = "picked";

/// `LStatement.aligns` / `nameToAlign` (`Arc Align` bit values).
const OBJ_ALIGN_NAMES: [(&str, i64); 9] = [
    ("topLeft", 10),
    ("top", 2),
    ("topRight", 18),
    ("left", 8),
    ("center", 1),
    ("right", 16),
    ("bottomLeft", 12),
    ("bottom", 4),
    ("bottomRight", 20),
];

/// The objectives editor: the canvas plus the selected objective's fields.
#[derive(Default)]
pub struct MapObjectivesDialog {
    /// `[{tag, fields: [{name, kind, flags}]}]`.
    classes: VarArray,
}

impl DialogController for MapObjectivesDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.objectives");
        dialog.should_pause = false;
        dialog.clear_buttons();
        let node = dialog.to_gd();
        // `buttons.button("@back", Icon.left, ...)`, `@add`, `@edit.menu`.
        dialog.add_close_button(153.0);
        let add = kit::t("@add").to_string();
        let add_target = node.clone();
        let callable = Callable::from_fn("objectives_add", move |_args: &[&Variant]| {
            set_meta_int(&mut add_target.clone(), OBJ_PENDING_CLASS_META, -1);
            objective_open_class_picker(&add_target);
        });
        dialog.add_button(&add, Some(callable), "add", 157.0);
        let edit = kit::t("@edit.menu").to_string();
        let edit_target = node.clone();
        let callable = Callable::from_fn("objectives_edit", move |_args: &[&Variant]| {
            objective_show_edit_menu(&edit_target);
        });
        dialog.add_button(&edit, Some(callable), "edit", 158.0);
        objective_bind_confirm(&node);
        objective_bind_picker(&node);
        objective_bind_color(&node);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let request = take_request(dialog);
        match request.as_str() {
            OBJ_REQUEST_PANEL => self.rebuild_panel(dialog),
            OBJ_REQUEST_PICKED => {
                objective_apply_picked(&dialog.to_gd());
                self.rebuild_panel(dialog);
            }
            OBJ_REQUEST_BUILD => self.build(dialog),
            // `shown()`: read the descriptors + objectives, then build.
            _ => self.setup(dialog),
        }
    }
}

impl MapObjectivesDialog {
    fn setup(&mut self, dialog: &mut MindDialog) {
        let node = dialog.to_gd();
        self.classes = editor_array("objective_classes_json");
        let marker_classes = editor_array("marker_classes_json");
        let objectives = editor_array("objectives_json");
        let mut meta = node.clone();
        set_meta_array(&mut meta, OBJ_OBJECTIVES_META, &objectives);
        set_meta_int(&mut meta, OBJ_SELECTED_META, -1);
        set_meta_int(&mut meta, OBJ_PENDING_CLASS_META, -1);
        set_meta_dict(&mut meta, OBJ_PENDING_QUERY_META, &VarDictionary::new());
        set_meta_bool(&mut meta, OBJ_PENDING_CLEAR_META, false);
        objective_normalize_positions(&node);
        let _ = marker_classes;
        self.build(dialog);
    }

    fn build(&mut self, dialog: &mut MindDialog) {
        if dialog.content().is_none() {
            return;
        }
        dialog.clear_content();
        let mut root = dialog.content_table();
        let node = dialog.to_gd();
        let mut row = HBoxContainer::new_alloc();
        row.set_h_size_flags(SizeFlags::EXPAND_FILL);
        row.set_v_size_flags(SizeFlags::EXPAND_FILL);
        row.add_theme_constant_override("separation", 10);
        root.bind_mut().add(&row).grow_x().grow_y().pad(4.0);
        // `canvas = new MapObjectivesCanvas()` (the Java dialog is canvas-first).
        if let Some(canvas) = instantiate_objectives_canvas() {
            row.add_child(&canvas);
            objective_bind_canvas(&node, &canvas);
        }
        // code-instantiated: the field panel is driven by the class descriptors.
        let mut scroll = MindScroll::new_alloc();
        scroll.bind_mut().set_scrolling_disabled(true, false);
        let mut scroll_control = as_control(&scroll);
        scroll_control.set_custom_minimum_size(Vector2::new(OBJ_PANEL_WIDTH, 0.0));
        scroll_control.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut panel = scroll.bind_mut().content();
        panel.set_name(OBJ_PANEL_NODE);
        row.add_child(&scroll_control);
        let _ = &mut panel;
        self.rebuild_panel(dialog);
    }

    /// The selected objective's editor (type, position actions, field rows).
    fn rebuild_panel(&mut self, dialog: &mut MindDialog) {
        let node = dialog.to_gd();
        let Some(mut panel) = find_named(&node, OBJ_PANEL_NODE)
            .and_then(|child| child.try_cast::<VBoxContainer>().ok())
        else {
            return;
        };
        for child in panel.get_children().iter_shared() {
            let mut child = child.clone();
            panel.remove_child(&child);
            child.queue_free();
        }
        let objectives = meta_array(&node, OBJ_OBJECTIVES_META);
        let selected = meta_int(&node, OBJ_SELECTED_META, -1);
        // With none selected there is no side panel at all.
        panel.set_visible(selected >= 0 && selected < objectives.len() as i64);
        if !panel.is_visible() {
            return;
        }
        let Ok(objective) = objectives.at(selected as usize).try_to::<VarDictionary>() else {
            return;
        };
        let tag = dict_text(&objective, "class");
        let mut header = HBoxContainer::new_alloc();
        header.add_theme_constant_override("separation", 4);
        let mut type_button = factory::button(&objective_type_name(&tag));
        type_button.set_theme_type_variation("flatt");
        type_button.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let type_target = node.clone();
        let callable = Callable::from_fn("objective_type", move |_args: &[&Variant]| {
            let selected = meta_int(&type_target, OBJ_SELECTED_META, -1);
            set_meta_int(&mut type_target.clone(), OBJ_PENDING_CLASS_META, selected);
            objective_open_class_picker(&type_target);
        });
        type_button.connect("pressed", &callable);
        header.add_child(&type_button);
        let up_target = node.clone();
        let callable = Callable::from_fn("objective_up", move |_args: &[&Variant]| {
            objective_move(&up_target, -1);
        });
        header.add_child(&objective_small_button("up", callable));
        let down_target = node.clone();
        let callable = Callable::from_fn("objective_down", move |_args: &[&Variant]| {
            objective_move(&down_target, 1);
        });
        header.add_child(&objective_small_button("down", callable));
        let remove_target = node.clone();
        let callable = Callable::from_fn("objective_remove", move |_args: &[&Variant]| {
            objective_remove_selected(&remove_target);
        });
        header.add_child(&objective_small_button("cancel", callable));
        panel.add_child(&header);
        for field in dict_array(&self.class_entry(&tag), "fields").iter_shared() {
            let Ok(field) = field.try_to::<VarDictionary>() else {
                continue;
            };
            if !objective_visible_field(&field) {
                continue;
            }
            let field_name = dict_text(&field, "name");
            if field_name == "class" || field_name == "editorPos" || field_name == "parents" {
                continue;
            }
            let Some(editor) = objective_field_row(&node, selected, &field, &objective) else {
                continue;
            };
            panel.add_child(&editor);
        }
    }

    fn class_entry(&self, tag: &str) -> VarDictionary {
        for entry in self.classes.iter_shared() {
            let Ok(entry) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            if dict_text(&entry, "tag") == tag {
                return entry;
            }
        }
        VarDictionary::new()
    }
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_objectives" => Some(Box::new(MapObjectivesDialog::default())),
        _ => None,
    }
}
