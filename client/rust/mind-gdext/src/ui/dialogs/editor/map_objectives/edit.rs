// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Objective field interpreters and value editors of the map objectives dialog.
//! Upstream: editor/MapObjectivesDialog.java

use std::rc::Rc;

use godot::classes::Button;
use godot::classes::ColorPickerButton;
use godot::classes::ColorRect;
use godot::classes::Control;
use godot::classes::GridContainer;
use godot::classes::HBoxContainer;
use godot::classes::VBoxContainer;
use godot::classes::control::LayoutPreset;
use godot::classes::control::MouseFilter;
use godot::classes::control::SizeFlags;
use godot::global::HorizontalAlignment;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;

use super::super::util_e::*;

use super::super::map_generate::free_named;
use super::list::{
    color_from_hex, flags_has, hex_byte, objective_at, objective_default_of, objective_marker_name,
    objective_open_color_picker, objective_open_content_picker, objective_open_team_picker,
    objective_set_field, objective_small_button, objective_visible_field,
};
use super::{
    OBJ_ALIGN_NAMES, OBJ_LOGIC_BOUND_META, OBJ_LOGIC_FIELD_META, OBJ_OVERLAY, OBJ_REQUEST_PANEL,
    OBJ_SELECTED_META,
};

/// `_field_row(objective, tag, field)`: label + the type interpreter.
pub(super) fn objective_field_row(
    node: &Gd<MindDialog>,
    index: i64,
    field: &VarDictionary,
    objective: &VarDictionary,
) -> Option<Gd<Control>> {
    let field_name = dict_text(field, "name");
    if field_name == "class" || field_name == "editorPos" {
        return None;
    }
    if field_name == "parents" || flags_has(&dict_array(field, "flags"), "synthetic") {
        return None;
    }
    let kind = dict_text(field, "kind");
    let flags = dict_array(field, "flags");
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_child(&as_control(&factory::label(&format!("{field_name}:"))));
    if let Some(inner) = kind.strip_prefix("seq:") {
        box_.add_child(&objective_seq_editor(
            node,
            index,
            &field_name,
            inner,
            &flags,
        ));
        return Some(box_.upcast());
    }
    let value = objective
        .get(&GString::from(field_name.as_str()))
        .unwrap_or_else(|| objective_default_of(field));
    let value_sink = objective_field_value(node, index, &field_name);
    box_.add_child(&objective_editor(
        node,
        index,
        &field_name,
        &value_sink,
        &value,
        &kind,
        &flags,
        false,
    ));
    Some(box_.upcast())
}

/// One editable value: Java's `FieldInterpreter.get`/`set` pair. The same
/// interpreter renders the objective field itself, a `Seq` element and an
/// `ObjectiveMarker` sub-field (GAPS #46 removed the raw-text fallbacks).
#[derive(Clone)]
struct ObjectiveValue {
    get: Rc<dyn Fn() -> Variant>,
    set: Rc<dyn Fn(Variant)>,
}

impl ObjectiveValue {
    fn new(get: impl Fn() -> Variant + 'static, set: impl Fn(Variant) + 'static) -> Self {
        Self {
            get: Rc::new(get),
            set: Rc::new(set),
        }
    }

    fn get(&self) -> Variant {
        (self.get)()
    }

    fn set(&self, value: Variant) {
        (self.set)(value);
    }

    /// One dictionary key of this value (`ObjectiveMarker` sub-fields,
    /// `TextureHolder.string`, `Vec2` axes).
    fn field(&self, key: &str) -> Self {
        let getter = self.clone();
        let setter = self.clone();
        let key_get = String::from(key);
        let key_set = String::from(key);
        Self::new(
            move || {
                getter
                    .get()
                    .try_to::<VarDictionary>()
                    .ok()
                    .and_then(|dict| dict.get(&GString::from(key_get.as_str())))
                    .unwrap_or(Variant::nil())
            },
            move |value| {
                let mut dict = setter.get().try_to::<VarDictionary>().unwrap_or_default();
                dict.set(&GString::from(key_set.as_str()), &value);
                setter.set(dict.to_variant());
            },
        )
    }
}

/// The top-level objective field value (`_field_row`'s `set` closure).
fn objective_field_value(node: &Gd<MindDialog>, index: i64, field_name: &str) -> ObjectiveValue {
    let node_get = node.clone();
    let node_set = node.clone();
    let name = String::from(field_name);
    let name_get = name.clone();
    ObjectiveValue::new(
        move || {
            objective_at(&node_get, index)
                .get(&GString::from(name_get.as_str()))
                .unwrap_or(Variant::nil())
        },
        move |value| objective_set_field(&node_set, index, &name, value),
    )
}

/// One `Seq` element value (`_seq` per-element `set`).
fn objective_element_value(
    node: &Gd<MindDialog>,
    index: i64,
    field_name: &str,
    element: usize,
) -> ObjectiveValue {
    let node_get = node.clone();
    let node_set = node.clone();
    let name = String::from(field_name);
    let name_get = name.clone();
    ObjectiveValue::new(
        move || {
            objective_at(&node_get, index)
                .get(&GString::from(name_get.as_str()))
                .and_then(|value| value.try_to::<VarArray>().ok())
                .and_then(|array| array.get(element))
                .unwrap_or(Variant::nil())
        },
        move |value| objective_set_element(&node_set, index, &name, element, value),
    )
}

/// Writes one `Seq` element back into the objective dictionary.
fn objective_set_element(
    node: &Gd<MindDialog>,
    index: i64,
    field_name: &str,
    element: usize,
    value: Variant,
) {
    let mut array = objective_at(node, index)
        .get(&GString::from(field_name))
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    if element >= array.len() {
        return;
    }
    array.set(element, &value);
    objective_set_field(node, index, field_name, array.to_variant());
}

/// The generic field interpreter (`_make_editor`).
#[allow(clippy::too_many_arguments)] // one parameter per Java `_make_editor` arg
fn objective_editor(
    node: &Gd<MindDialog>,
    index: i64,
    field_name: &str,
    value_sink: &ObjectiveValue,
    value: &Variant,
    kind: &str,
    flags: &VarArray,
    nested: bool,
) -> Gd<Control> {
    match kind {
        "string" => {
            if flags_has(flags, "logiccode") {
                if nested {
                    return factory::label(&variant_text(value)).upcast();
                }
                // `@LogicCode`: Java renders only the pencil button.
                let mut pencil = factory::icon_button(
                    "pencil",
                    "",
                    "emptyi",
                    godot::classes::box_container::AlignmentMode::BEGIN,
                );
                pencil.set_custom_minimum_size(Vector2::new(34.0, 34.0));
                let target = node.clone();
                let name = String::from(field_name);
                let callable =
                    Callable::from_fn("objective_logic_code", move |_args: &[&Variant]| {
                        objective_open_logic(&target, index, &name);
                    });
                pencil.connect("pressed", &callable);
                return pencil.upcast();
            }
            if flags_has(flags, "multiline") {
                let mut area = text_area(&variant_text(value));
                area.set_custom_minimum_size(Vector2::new(0.0, 100.0));
                let sink = value_sink.clone();
                let callable = Callable::from_fn("objective_area", move |args: &[&Variant]| {
                    let text = args
                        .first()
                        .map(|value| value.to::<GString>())
                        .unwrap_or_default();
                    sink.set(text.to_variant());
                });
                area.connect("text_changed", &callable);
                return area.upcast();
            }
            let mut edit = text_field(&variant_text(value), 0);
            let sink = value_sink.clone();
            let callable = Callable::from_fn("objective_string", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>())
                    .unwrap_or_default();
                sink.set(text.to_variant());
            });
            edit.connect("text_changed", &callable);
            edit.upcast()
        }
        "bool" => {
            let checked = as_bool(Some(value.clone()));
            let sink = value_sink.clone();
            let listener = Callable::from_fn("objective_bool", move |args: &[&Variant]| {
                let pressed = args
                    .first()
                    .map(|value| value.to::<bool>())
                    .unwrap_or(false);
                sink.set(pressed.to_variant());
            });
            mind_check("", checked, listener).upcast()
        }
        "indexbool" => {
            // `@IndexBool int`: checked means `!= -1`; writes `1`/`-1`.
            let checked = as_int(Some(value.clone())) != -1;
            let sink = value_sink.clone();
            let listener = Callable::from_fn("objective_indexbool", move |args: &[&Variant]| {
                let pressed = args
                    .first()
                    .map(|value| value.to::<bool>())
                    .unwrap_or(false);
                sink.set((if pressed { 1i64 } else { -1 }).to_variant());
            });
            mind_check("", checked, listener).upcast()
        }
        "byte" | "int" | "float" => {
            let scale = if flags_has(flags, "second") {
                60.0
            } else if flags_has(flags, "tilepos") {
                8.0
            } else {
                1.0
            };
            objective_number_field(value_sink, value, kind == "float", scale)
        }
        "team" => {
            if nested {
                return factory::label(&team_name_by_id(as_int(Some(value.clone())))).upcast();
            }
            let mut button = factory::button(&team_name_by_id(as_int(Some(value.clone()))));
            let target = node.clone();
            let name = String::from(field_name);
            let callable = Callable::from_fn("objective_team", move |_args: &[&Variant]| {
                objective_open_team_picker(&target, index, &name);
            });
            button.connect("pressed", &callable);
            button.upcast()
        }
        "color" => {
            if nested {
                // A marker's colour is edited in place (`ColorPickerButton`);
                // Java's shared `ui.picker` carries no nested sink.
                let mut picker = ColorPickerButton::new_alloc();
                picker.set_custom_minimum_size(Vector2::new(50.0, 50.0));
                picker.set_pick_color(color_from_hex(&variant_text(value)));
                let sink = value_sink.clone();
                let callable =
                    Callable::from_fn("objective_marker_color", move |args: &[&Variant]| {
                        let color = args
                            .first()
                            .and_then(|value| value.try_to::<Color>().ok())
                            .unwrap_or(Color::WHITE);
                        sink.set(GString::from(hex_rgba(color).as_str()).to_variant());
                        Variant::nil()
                    });
                picker.connect("color_changed", &callable);
                return picker.upcast();
            }
            let mut button = Button::new_alloc();
            button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
            let mut swatch = ColorRect::new_alloc();
            swatch.set_color(color_from_hex(&variant_text(value)));
            swatch.set_anchors_preset(LayoutPreset::FULL_RECT);
            swatch.set_mouse_filter(MouseFilter::IGNORE);
            button.add_child(&swatch);
            let target = node.clone();
            let name = String::from(field_name);
            let current = variant_text(value);
            let callable =
                Callable::from_fn("objective_color_pick", move |_args: &[&Variant]| {
                    objective_open_color_picker(&target, &current, &name);
                });
            button.connect("pressed", &callable);
            button.upcast()
        }
        "vec2i" | "vec2f" => {
            let is_float = kind == "vec2f";
            let scale = if flags_has(flags, "tilepos") {
                8.0
            } else {
                1.0
            };
            let mut row = HBoxContainer::new_alloc();
            for axis in ["x", "y"] {
                let axis_sink = value_sink.field(axis);
                let raw = as_float(Some(axis_sink.get()));
                let display = if is_float {
                    format!("{}", raw / scale)
                } else {
                    format!("{}", (raw / scale) as i64)
                };
                let mut edit = text_field(&display, 0);
                edit.set_custom_minimum_size(Vector2::new(120.0, 0.0));
                let callable = Callable::from_fn("objective_vec2", move |args: &[&Variant]| {
                    let text = args
                        .first()
                        .map(|value| value.to::<GString>().to_string())
                        .unwrap_or_default();
                    let Some(number) = valid_float(&text) else {
                        return;
                    };
                    let stored = if is_float {
                        (number * scale).to_variant()
                    } else {
                        ((number * scale) as i64).to_variant()
                    };
                    axis_sink.set(stored);
                });
                edit.connect("text_changed", &callable);
                row.add_child(&edit);
            }
            row.upcast()
        }
        "alignment" => {
            let mut button = factory::button(&objective_align_name(as_int(Some(value.clone()))));
            let target = node.clone();
            let flags = flags.clone();
            let sink = value_sink.clone();
            let callable = Callable::from_fn("objective_alignment", move |_args: &[&Variant]| {
                objective_open_align_picker(&target, &sink, &flags);
            });
            button.connect("pressed", &callable);
            button.upcast()
        }
        "labelflag" => {
            // `@LabelFlag byte` (`:309-325`): the two `WorldLabel` flag checks.
            let mut box_ = VBoxContainer::new_alloc();
            let current = as_int(Some(value.clone()));
            for (bit, key) in [(1i64, "@marker.background"), (2, "@marker.outline")] {
                let sink = value_sink.clone();
                let listener =
                    Callable::from_fn("objective_labelflag", move |args: &[&Variant]| {
                        let pressed = args
                            .first()
                            .map(|value| value.to::<bool>())
                            .unwrap_or(false);
                        let current = as_int(Some(sink.get()));
                        let updated = if pressed {
                            current | bit
                        } else {
                            current & !bit
                        };
                        sink.set(updated.to_variant());
                    });
                box_.add_child(&mind_check(
                    &kit::t(key).to_string(),
                    (current & bit) == bit,
                    listener,
                ));
            }
            box_.upcast()
        }
        "texture" => {
            // `TextureHolder` interpreter: the string-backed name.
            let string_sink = value_sink.field("string");
            let mut edit = text_field(&variant_text(&string_sink.get()), 0);
            let callable = Callable::from_fn("objective_texture", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>())
                    .unwrap_or_default();
                string_sink.set(text.to_variant());
            });
            edit.connect("text_changed", &callable);
            edit.upcast()
        }
        "marker" => objective_marker_editor(node, value_sink),
        "verts" => objective_verts_editor(value_sink, value),
        _ if kind.starts_with("content:") => {
            if nested {
                return factory::label(&variant_text(value)).upcast();
            }
            let content_kind = kind.trim_start_matches("content:").to_string();
            let mut button = factory::button(&variant_text(value));
            let target = node.clone();
            let name = String::from(field_name);
            let callable = Callable::from_fn("objective_content", move |_args: &[&Variant]| {
                objective_open_content_picker(&target, index, &name, &content_kind);
            });
            button.connect("pressed", &callable);
            button.upcast()
        }
        // `map:` and any future descriptor kinds still render verbatim; every
        // objective/marker class field the descriptors declare has a widget.
        _ => factory::label(&variant_text(value)).upcast(),
    }
}

/// `_number_value_field(is_float, scale, get, set)` (`@Second` x60, `@TilePos` x8).
fn objective_number_field(
    value_sink: &ObjectiveValue,
    value: &Variant,
    is_float: bool,
    scale: f64,
) -> Gd<Control> {
    let raw = as_float(Some(value.clone()));
    let display = if is_float {
        format!("{}", raw / scale)
    } else {
        format!("{}", (raw / scale) as i64)
    };
    let mut edit = text_field(&display, 0);
    edit.set_custom_minimum_size(Vector2::new(120.0, 0.0));
    let sink = value_sink.clone();
    let callable = Callable::from_fn("objective_number", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| value.to::<GString>().to_string())
            .unwrap_or_default();
        if text.is_empty() || text == "-" {
            return;
        }
        let stored = if is_float {
            match valid_float(&text) {
                Some(number) => (number * scale).to_variant(),
                None => return,
            }
        } else {
            match valid_int(&text) {
                Some(number) => (number as f64 * scale).to_variant(),
                None => return,
            }
        };
        sink.set(stored);
    });
    edit.connect("text_changed", &callable);
    edit.upcast()
}

/// The `Seq` interpreter (`:335-402`): name header + add / per-element rows.
///
/// Element rows now carry the real interpreter for the element kind (string /
/// vec2 / `ObjectiveMarker`) instead of the verbatim label (GAPS #46).
fn objective_seq_editor(
    node: &Gd<MindDialog>,
    index: i64,
    field_name: &str,
    inner_kind: &str,
    inner_flags: &VarArray,
) -> Gd<Control> {
    let mut box_ = VBoxContainer::new_alloc();
    let mut header = HBoxContainer::new_alloc();
    header.add_theme_constant_override("separation", 4);
    if !field_name.is_empty() {
        let mut label = factory::label(&format!("{field_name}:"));
        label.add_theme_color_override("font_color", styles::ACCENT);
        header.add_child(&as_control(&label));
    }
    let mut spacer = Control::new_alloc();
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    spacer.set_mouse_filter(MouseFilter::IGNORE);
    header.add_child(&spacer);
    let target = node.clone();
    let name = String::from(field_name);
    let inner = String::from(inner_kind);
    let callable = Callable::from_fn("objective_seq_add", move |_args: &[&Variant]| {
        let mut array = objective_at(&target, index)
            .get(&GString::from(name.as_str()))
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default();
        array.push(&objective_seq_default(&inner));
        objective_set_field(&target, index, &name, array.to_variant());
        request_refresh(&target, OBJ_REQUEST_PANEL);
    });
    header.add_child(&objective_small_button("add", callable));
    box_.add_child(&header);
    let array = objective_at(node, index)
        .get(&GString::from(field_name))
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    for element in 0..array.len() {
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 4);
        let target = node.clone();
        let name = String::from(field_name);
        let remover = Callable::from_fn("objective_seq_remove", move |_args: &[&Variant]| {
            let mut array = objective_at(&target, index)
                .get(&GString::from(name.as_str()))
                .and_then(|value| value.try_to::<VarArray>().ok())
                .unwrap_or_default();
            if element < array.len() {
                array.remove(element);
                objective_set_field(&target, index, &name, array.to_variant());
                request_refresh(&target, OBJ_REQUEST_PANEL);
            }
        });
        row.add_child(&objective_small_button("cancel", remover));
        let element_sink = objective_element_value(node, index, field_name, element);
        let element_value = element_sink.get();
        let editor = objective_editor(
            node,
            index,
            field_name,
            &element_sink,
            &element_value,
            inner_kind,
            inner_flags,
            true,
        );
        let mut editor_control: Gd<Control> = editor;
        editor_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        row.add_child(&editor_control);
        box_.add_child(&row);
    }
    box_.upcast()
}

/// The `ObjectiveMarker` interpreter (`MapObjectivesDialog.defaultInterpreter`
/// + `getProvider(ObjectiveMarker.class)`, `:236-256`): the class button plus
///   the selected marker class's field rows.
fn objective_marker_editor(node: &Gd<MindDialog>, value_sink: &ObjectiveValue) -> Gd<Control> {
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 2);
    let value = value_sink
        .get()
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    let tag = dict_text(&value, "class");
    let mut class_button = factory::button(&objective_marker_name(&tag));
    class_button.set_theme_type_variation("flatt");
    class_button.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let target = node.clone();
    let sink = value_sink.clone();
    let callable = Callable::from_fn("objective_marker_class", move |_args: &[&Variant]| {
        objective_open_marker_picker(&target, &sink);
    });
    class_button.connect("pressed", &callable);
    box_.add_child(&class_button);
    for field in objective_marker_fields(&tag).iter_shared() {
        let Ok(field) = field.try_to::<VarDictionary>() else {
            continue;
        };
        if !objective_visible_field(&field) {
            continue;
        }
        let field_name = dict_text(&field, "name");
        if field_name == "class" || field_name == "editorPos" {
            continue;
        }
        let kind = dict_text(&field, "kind");
        let flags = dict_array(&field, "flags");
        let field_sink = value_sink.field(&field_name);
        let current = field_sink.get();
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 4);
        row.add_child(&as_control(&factory::label(&format!("{field_name}:"))));
        let editor = objective_editor(
            node,
            -1,
            &field_name,
            &field_sink,
            &current,
            &kind,
            &flags,
            true,
        );
        let mut editor_control: Gd<Control> = editor;
        editor_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        row.add_child(&editor_control);
        box_.add_child(&row);
    }
    box_.upcast()
}

/// The marker class table (`MindEditor.marker_classes_json`) filtered by tag.
fn objective_marker_fields(tag: &str) -> VarArray {
    let classes = editor_array("marker_classes_json");
    for entry in dict_rows(&classes) {
        if dict_text(&entry, "tag") == tag {
            return dict_array(&entry, "fields");
        }
    }
    VarArray::new()
}

/// `getProvider(ObjectiveMarker.class)`: the `@add` marker-class grid; picking
/// replaces the marker with Java's provider defaults (`marker_default_json`).
fn objective_open_marker_picker(node: &Gd<MindDialog>, value_sink: &ObjectiveValue) {
    let classes = editor_array("marker_classes_json");
    if classes.is_empty() {
        return;
    }
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(3);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    for row in dict_rows(&classes) {
        let tag = dict_text(&row, "tag");
        let mut button = factory::button(&objective_marker_name(&tag));
        button.set_theme_type_variation("flatt");
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_custom_minimum_size(Vector2::new(195.0, 56.0));
        let target = node.clone();
        let sink = value_sink.clone();
        let callable = Callable::from_fn("objective_pick_marker", move |_args: &[&Variant]| {
            let created = editor_json(
                "marker_default_json",
                &[GString::from(tag.as_str()).to_variant()],
            )
            .map(|text| json_object(&text))
            .unwrap_or_default();
            if created.is_empty() {
                return Variant::nil();
            }
            sink.set(created.to_variant());
            free_named(&target, OBJ_OVERLAY);
            request_refresh(&target, OBJ_REQUEST_PANEL);
            Variant::nil()
        });
        button.connect("pressed", &callable);
        grid.add_child(&button);
    }
    let title = kit::t("@add").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        OBJ_OVERLAY,
        &title,
        640.0,
        380.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.8),
    ) {
        overlay.add_content(&grid);
    }
}

/// `Vertices` interpreter (`MapObjectivesDialog.java:280-320`): one row per
/// vertex with `x`/`y` (tile-scaled), the packed colour and `u`/`v`. The colour
/// column stores `Color.toFloatBits()` bits (ABGR); alpha is clamped to 254 so
/// the JSON number never becomes NaN.
fn objective_verts_editor(value_sink: &ObjectiveValue, value: &Variant) -> Gd<Control> {
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 2);
    let array = value.try_to::<VarArray>().unwrap_or_default();
    let vertices = array.len() / OBJECTIVE_VERTEX_STRIDE;
    for vertex in 0..vertices {
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 4);
        for (component, name) in [(0usize, "x"), (1, "y")] {
            let sink = objective_vertex_value(value_sink, vertex, component);
            let display = format!("{}", as_float(Some(sink.get())) / OBJECTIVE_TILE_SIZE);
            let mut edit = text_field(&display, 0);
            edit.set_custom_minimum_size(Vector2::new(70.0, 0.0));
            edit.set_tooltip_text(&GString::from(name));
            let callable = Callable::from_fn("objective_vert_xy", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>().to_string())
                    .unwrap_or_default();
                let Some(number) = valid_float(&text) else {
                    return;
                };
                sink.set((number * OBJECTIVE_TILE_SIZE).to_variant());
            });
            edit.connect("text_changed", &callable);
            row.add_child(&edit);
        }
        let color_sink = objective_vertex_value(value_sink, vertex, 2);
        let mut picker = ColorPickerButton::new_alloc();
        picker.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        picker.set_pick_color(objective_verts_color(as_float(Some(color_sink.get()))));
        let callable = Callable::from_fn("objective_vert_color", move |args: &[&Variant]| {
            let color = args
                .first()
                .and_then(|value| value.try_to::<Color>().ok())
                .unwrap_or(Color::WHITE);
            color_sink.set(objective_verts_bits(color).to_variant());
            Variant::nil()
        });
        picker.connect("color_changed", &callable);
        row.add_child(&picker);
        for (component, name) in [(3usize, "u"), (4, "v")] {
            let sink = objective_vertex_value(value_sink, vertex, component);
            let display = format!("{}", as_float(Some(sink.get())));
            let mut edit = text_field(&display, 0);
            edit.set_custom_minimum_size(Vector2::new(70.0, 0.0));
            edit.set_tooltip_text(&GString::from(name));
            let callable = Callable::from_fn("objective_vert_uv", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>().to_string())
                    .unwrap_or_default();
                let Some(number) = valid_float(&text) else {
                    return;
                };
                sink.set(number.to_variant());
            });
            edit.connect("text_changed", &callable);
            row.add_child(&edit);
        }
        box_.add_child(&row);
    }
    box_.upcast()
}

/// `Vertices` layout: `x, y, color, u, v, ?` per vertex (`stride = 6`).
const OBJECTIVE_VERTEX_STRIDE: usize = 6;
/// `Vars.tilesize`: the vertex `x`/`y` columns are tile-scaled.
const OBJECTIVE_TILE_SIZE: f64 = 8.0;

/// One `Vertices` component value.
fn objective_vertex_value(
    value_sink: &ObjectiveValue,
    vertex: usize,
    component: usize,
) -> ObjectiveValue {
    let getter = value_sink.clone();
    let setter = value_sink.clone();
    ObjectiveValue::new(
        move || {
            getter
                .get()
                .try_to::<VarArray>()
                .ok()
                .and_then(|array| array.get(vertex * OBJECTIVE_VERTEX_STRIDE + component))
                .unwrap_or(0.0f64.to_variant())
        },
        move |value| {
            let mut array = setter.get().try_to::<VarArray>().ok().unwrap_or_default();
            let index = vertex * OBJECTIVE_VERTEX_STRIDE + component;
            if index >= array.len() {
                return;
            }
            array.set(index, &value);
            setter.set(array.to_variant());
        },
    )
}

/// `Color.toFloatBits()` ABGR bits -> `Color` (`Vertices` colour column).
fn objective_verts_color(bits: f64) -> Color {
    let bits = (bits as f32).to_bits();
    Color::from_rgba8(
        (bits & 0xff) as u8,
        ((bits >> 8) & 0xff) as u8,
        ((bits >> 16) & 0xff) as u8,
        ((bits >> 24) & 0xff) as u8,
    )
}

/// `Color` -> the `Vertices` colour column (finite JSON number; alpha 254 max).
fn objective_verts_bits(color: Color) -> f64 {
    let alpha = u32::from(hex_byte(color.a).min(254));
    let bits = (alpha << 24)
        | (u32::from(hex_byte(color.b)) << 16)
        | (u32::from(hex_byte(color.g)) << 8)
        | u32::from(hex_byte(color.r));
    f64::from(f32::from_bits(bits))
}

/// `#rrggbbaa` for a `Color` (`objective_marker_color` write-back).
fn hex_rgba(color: Color) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}",
        hex_byte(color.r),
        hex_byte(color.g),
        hex_byte(color.b),
        hex_byte(color.a)
    )
}

/// `_provider_default_of(kind)` (`getProvider(element).get`).
fn objective_provider_default(kind: &str) -> Variant {
    if kind.starts_with("content:") {
        return GString::from("copper").to_variant();
    }
    match kind {
        "string" => GString::from("").to_variant(),
        "bool" => false.to_variant(),
        "byte" | "int" => 0i64.to_variant(),
        "float" => 0.0f64.to_variant(),
        "vec2i" => {
            let mut value = VarDictionary::new();
            value.set("x", &0i64.to_variant());
            value.set("y", &0i64.to_variant());
            value.to_variant()
        }
        "vec2f" => {
            let mut value = VarDictionary::new();
            value.set("x", &0.0f64.to_variant());
            value.set("y", &0.0f64.to_variant());
            value.to_variant()
        }
        "team" => 1i64.to_variant(),
        "color" => GString::from("ffd37fff").to_variant(),
        "labelflag" => 3i64.to_variant(),
        "alignment" => 1i64.to_variant(),
        "texture" => {
            let mut value = VarDictionary::new();
            value.set("string", &GString::from("white").to_variant());
            value.to_variant()
        }
        "marker" => VarDictionary::new().to_variant(),
        _ => Variant::nil(),
    }
}

/// Java's `getProvider(element).get` for a new `Seq` element. The marker
/// provider opens the class picker upstream; the port appends the first marker
/// class's provider default so the element already renders its field rows (the
/// class button then changes the class).
fn objective_seq_default(kind: &str) -> Variant {
    if kind != "marker" {
        return objective_provider_default(kind);
    }
    let classes = editor_array("marker_classes_json");
    let Some(first) = dict_rows(&classes).first().map(|row| dict_text(row, "tag")) else {
        return VarDictionary::new().to_variant();
    };
    let parsed = editor_json(
        "marker_default_json",
        &[GString::from(first.as_str()).to_variant()],
    )
    .map(|text| json_object(&text))
    .unwrap_or_default();
    if parsed.is_empty() {
        VarDictionary::new().to_variant()
    } else {
        parsed.to_variant()
    }
}

/// `_align_name(align)`.
fn objective_align_name(align: i64) -> String {
    OBJ_ALIGN_NAMES
        .iter()
        .find(|(_, value)| *value == align)
        .map(|(name, _)| String::from(*name))
        .unwrap_or_else(|| String::from("center"))
}

/// `_open_align_picker(get, set, flags)`: a three-column alignment popup.
fn objective_open_align_picker(
    node: &Gd<MindDialog>,
    value_sink: &ObjectiveValue,
    flags: &VarArray,
) {
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(3);
    let current = as_int(Some(value_sink.get()));
    for (name, value) in OBJ_ALIGN_NAMES {
        if flags_has(flags, "align-nover") && value != 1 && value != 8 && value != 16 {
            continue;
        }
        if flags_has(flags, "align-nohor") && value != 1 && value != 2 && value != 4 {
            continue;
        }
        let mut choice = Button::new_alloc();
        choice.set_text(name);
        choice.set_custom_minimum_size(Vector2::new(100.0, 38.0));
        choice.set_theme_type_variation(if value == current {
            "flatTogglet"
        } else {
            "flatt"
        });
        let target = node.clone();
        let sink = value_sink.clone();
        let callable = Callable::from_fn("objective_align", move |_args: &[&Variant]| {
            sink.set(value.to_variant());
            free_named(&target, OBJ_OVERLAY);
            request_refresh(&target, OBJ_REQUEST_PANEL);
        });
        choice.connect("pressed", &callable);
        grid.add_child(&choice);
    }
    let title = kit::t("@editor.objectives").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        OBJ_OVERLAY,
        &title,
        320.0,
        240.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.8),
    ) {
        overlay.add_content(&grid);
    }
}

/// `ui.logic.show(code, null, true, set)` for `@LogicCode` fields.
fn objective_open_logic(node: &Gd<MindDialog>, index: i64, field_name: &str) {
    if kit::ui().is_none() {
        return;
    }
    let field = String::from(field_name);
    set_meta_text(&mut node.clone(), OBJ_LOGIC_FIELD_META, &field);
    objective_bind_logic(node);
    let code = objective_at(node, index)
        .get(&GString::from(field.as_str()))
        .map(|value| variant_text(&value))
        .unwrap_or_default();
    let mut context = VarDictionary::new();
    context.set("filter", &true.to_variant());
    context.set("privileged", &true.to_variant());
    context.set("code", &GString::from(code.as_str()).to_variant());
    ui_open_dialog("logic", &context);
}

/// Binds the `logic` dialog's `code_edited` signal (once).
fn objective_bind_logic(node: &Gd<MindDialog>) {
    if node.has_meta(OBJ_LOGIC_BOUND_META) {
        return;
    }
    let Some(mut logic) = dialog_node("logic") else {
        return;
    };
    if !logic.has_signal("code_edited") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("objective_logic_edited", move |args: &[&Variant]| {
        let code = args
            .first()
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        let field = meta_text(&target, OBJ_LOGIC_FIELD_META);
        let index = meta_int(&target, OBJ_SELECTED_META, -1);
        if index < 0 || field.is_empty() {
            return;
        }
        objective_set_field(&target, index, &field, code.to_variant());
        request_refresh(&target, OBJ_REQUEST_PANEL);
    });
    logic.connect("code_edited", &callable);
    node.clone()
        .set_meta(OBJ_LOGIC_BOUND_META, &true.to_variant());
}

#[cfg(test)]
mod tests {
    use super::*;
    use godot::builtin::ColorChannelOrder;

    #[test]
    fn verts_color_bits_decode_the_java_packing() {
        // `default_quad_vertices` stores `Color.white.toFloatBits()`, clamped to
        // alpha 254 so the number stays finite (`0xfeffffff`).
        let white = objective_verts_color(f64::from(f32::from_bits(0xfeff_ffff)));
        assert_eq!(
            white.to_u32(ColorChannelOrder::RGBA),
            Color::from_rgba8(255, 255, 255, 254).to_u32(ColorChannelOrder::RGBA)
        );
    }

    /// `editorPos` is `arc Point2.pack`: x in the high half, y in the low half
    /// (`Point2.java:40-52`). The port used to swap the halves, so a node
    /// placed at (5,2) rendered at (2,5) and wrote a Java-incompatible map.
    #[test]
    fn editor_pos_packs_x_high_and_y_low() {
        for (x, y) in [(5, 2), (0, 0), (-1, -2), (99, -100), (-32768, 32767)] {
            let packed = pack_point2(x, y);
            assert_eq!(packed, (x << 16) | (y & 0xFFFF), "Point2.pack({x}, {y})");
            assert_eq!(unpack_point2(packed), (x, y), "round trip ({x}, {y})");
        }
        // The `-999` unplaced default is symmetric, so maps written before the
        // fix keep their sentinel.
        assert_eq!(
            pack_point2(-999, -999),
            mind_core::io::json::objectives::default_editor_pos()
        );
    }

    #[test]
    fn verts_color_bits_round_trip_without_nan() {
        for color in [
            Color::from_rgba8(255, 255, 255, 255),
            Color::from_rgba8(0x7f, 0x59, 0x9a, 0xff),
            Color::from_rgba8(0, 0, 0, 0),
        ] {
            let bits = objective_verts_bits(color);
            assert!(bits.is_finite(), "verts colour bits must stay finite");
            let back = objective_verts_color(bits);
            // Alpha is clamped to 254 (`toFloatBits` 0xffffffff is NaN).
            let expected_alpha = hex_byte(color.a).min(254);
            assert_eq!(
                back.to_u32(ColorChannelOrder::RGBA),
                Color::from_rgba8(
                    hex_byte(color.r),
                    hex_byte(color.g),
                    hex_byte(color.b),
                    expected_alpha
                )
                .to_u32(ColorChannelOrder::RGBA)
            );
        }
    }
}
