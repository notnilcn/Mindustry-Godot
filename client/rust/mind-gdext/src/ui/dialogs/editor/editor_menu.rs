// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map editor menu dialog controller.
//! Upstream: editor/MapEditorDialog.java

use godot::classes::box_container::AlignmentMode;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::widgets::factory;

use super::util::*;

// ---------------------------------------------------------------------------
// EditorMenuDialog
// ---------------------------------------------------------------------------

/// One `MapEditorDialog` menu row (`ENTRIES`); `wide` entries span both
/// columns and carry the `menu.cont` cell padding.
struct MenuEntry {
    key: &'static str,
    icon: &'static str,
    action: &'static str,
    wide: bool,
    pad_bottom: f32,
}

/// The editor menu grid rows in order (the Steam workshop button is skipped).
const MENU_ENTRIES: [MenuEntry; 10] = [
    MenuEntry {
        key: "@editor.savemap",
        icon: "save",
        action: "save",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.mapinfo",
        icon: "pencil",
        action: "map_info",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.generate",
        icon: "terrain",
        action: "map_generate",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.resize",
        icon: "resize",
        action: "map_resize",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.import",
        icon: "download",
        action: "editor_import",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.export",
        icon: "upload",
        action: "editor_export",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.ingame",
        icon: "right",
        action: "ingame",
        wide: false,
        pad_bottom: 0.0,
    },
    MenuEntry {
        key: "@editor.playtest",
        icon: "play",
        action: "playtest",
        wide: false,
        pad_bottom: 0.0,
    },
    // `MapEditorDialog.java:181-184` (`menu.cont` cells; measured in
    // java/32-edmenu.png at y=370 and y=434).
    MenuEntry {
        key: "@editor.sectorgenerate",
        icon: "terrain",
        action: "sector_generate",
        wide: true,
        pad_bottom: 4.0,
    },
    MenuEntry {
        key: "@quit",
        icon: "exit",
        action: "quit",
        wide: true,
        pad_bottom: 0.0,
    },
];

/// `MapEditorDialog` button sizes: `swidth = 180f`, buttons `(swidth, 60)`,
/// wide rows `swidth * 2 + 10`.
const MENU_BUTTON_SIZE: Vector2 = Vector2::new(180.0, 60.0);
const MENU_WIDE_SIZE: Vector2 = Vector2::new(370.0, 60.0);

/// `@editor_menu` (`client/ui/dialogs/editor_menu_dialog.gd`,
/// `MapEditorDialog.java:65-270`): the editor menu grid. Every action forwards
/// to `/root/Spine/MindEditor` or opens another editor dialog; the shell owns
/// Escape and the unsaved-changes confirm.
pub struct EditorMenuDialog;

impl DialogController for EditorMenuDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@menu");
        dialog.should_pause = false;
        dialog.add_default_close_button();
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.build(dialog);
    }
}

impl EditorMenuDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        // A reused dialog node: drop the previous open's body before rebuilding
        // (`shown()` rebuilds on every open).
        dialog.clear_content();
        let mut root = dialog.content_table();
        let target = dialog.to_gd();
        // code-instantiated: the menu grid is data-driven from `MENU_ENTRIES`.
        let mut grid = factory::table();
        let mut column = 0usize;
        for entry in MENU_ENTRIES {
            // code-instantiated: one button per `MapEditorDialog` menu entry.
            let mut button = factory::icon_button(
                entry.icon,
                &kit::t(entry.key).to_string(),
                "defaultt",
                AlignmentMode::BEGIN,
            );
            let action = entry.action;
            let callable = {
                let target = target.clone();
                Callable::from_fn("editor_menu_run", move |_args: &[&Variant]| {
                    editor_menu_run(&target, action);
                })
            };
            button.connect("pressed", &callable);
            let size = if entry.wide {
                MENU_WIDE_SIZE
            } else {
                MENU_BUTTON_SIZE
            };
            if entry.wide {
                // `t.defaults().size(...)`: wide rows span both columns and
                // carry the `menu.cont` padBottom only.
                let mut guard = grid.bind_mut();
                guard
                    .add(&button)
                    .width(size.x)
                    .height(size.y)
                    .colspan(2)
                    .pad_bottom(entry.pad_bottom);
                drop(guard);
                grid.bind_mut().row();
                column = 0;
            } else {
                // `t.defaults().size(swidth, 60f).padBottom(5).padRight(5)
                // .padLeft(5)` (`MapEditorDialog.java:71`): a 65 px row pitch
                // and 190 px columns.
                let mut guard = grid.bind_mut();
                guard
                    .add(&button)
                    .width(size.x)
                    .height(size.y)
                    .pad_left(5.0)
                    .pad_right(5.0)
                    .pad_bottom(5.0);
                drop(guard);
                column += 1;
                if column.is_multiple_of(2) {
                    grid.bind_mut().row();
                }
            }
        }
        if !column.is_multiple_of(2) {
            grid.bind_mut().row();
        }
        // `menu.cont.table(t -> ...)` under `cont.defaults().pad(3)`.
        root.bind_mut().add(&grid).pad(3.0);
    }
}

/// `_run`: dispatches one menu row.
fn editor_menu_run(target: &Gd<MindDialog>, action: &str) {
    match action {
        "save" => editor_menu_save(target),
        "map_info" | "map_generate" | "map_resize" | "editor_import" | "editor_export"
        | "sector_generate" => editor_menu_open_dialog(target, action),
        "ingame" => editor_menu_edit_in_game(target),
        "playtest" => editor_menu_playtest(target),
        "quit" => editor_menu_quit(target),
        _ => {}
    }
}

/// `MapEditorDialog.save` (`:410-450`): name -> info dialog + `@editor.save.noname`;
/// builtin map -> `@editor.save.overwrite`; success -> `@editor.saved`. The menu
/// hides on every path.
fn editor_menu_save(target: &Gd<MindDialog>) {
    let Some(mut editor) = kit::editor() else {
        return;
    };
    let result = editor
        .call("save", &[])
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    if dict_bool(&result, "ok", false) {
        notify_info(&kit::t("@editor.saved").to_string());
        close_node(target);
        return;
    }
    match dict_string(&result, "error").as_str() {
        "noname" => {
            editor_menu_open_dialog(target, "map_info");
            notify_info(&kit::t("@editor.save.noname").to_string());
        }
        "overwrite" => notify_info(&kit::t("@editor.save.overwrite").to_string()),
        _ => notify_info(&kit::t("@editor.errorsave").to_string()),
    }
    close_node(target);
}

/// Every non-action entry opens a registered editor dialog (`menu.hide()` +
/// `dialog.show()` upstream).
fn editor_menu_open_dialog(target: &Gd<MindDialog>, dialog_name: &str) {
    // From the menu the generate dialog applies to the editor (`applyToEditor`).
    if dialog_name == "map_generate"
        && let Some(mut generate) = dialog_node("map_generate")
    {
        generate.set("applied", &true.to_variant());
    }
    open_named_dialog(dialog_name, "");
    close_node(target);
}

/// `MapEditorDialog.editInGame` (`:326-372`): save first, then play the hidden
/// `editor` gamemode.
fn editor_menu_edit_in_game(target: &Gd<MindDialog>) {
    let Some(mut editor) = kit::editor() else {
        return;
    };
    let saved = editor
        .call("save", &[])
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    if !dict_bool(&saved, "ok", false) {
        notify_error_key("@editor.errorsave");
        return;
    }
    let map_name = dict_string(&editor_tags(), "name");
    if map_name.is_empty() {
        notify_error_key("@editor.errorsave");
        return;
    }
    let Some(mut campaign) = kit::campaign() else {
        notify_error_key("@editor.errorsave");
        return;
    };
    let started = campaign.has_method("start_map_mode")
        && campaign
            .call(
                "start_map_mode",
                &[
                    GString::from(map_name.as_str()).to_variant(),
                    GString::from("editor").to_variant(),
                    true.to_variant(),
                    GString::new().to_variant(),
                ],
            )
            .try_to::<bool>()
            .unwrap_or(false);
    if !started {
        notify_error_key("@editor.errorsave");
        return;
    }
    close_node(target);
    hide_editor_shell();
    show_game();
}

/// `MapEditorDialog.playtest` (`:374-393`): save, then play immediately (shift)
/// or open the map-play dialog in playtesting mode.
fn editor_menu_playtest(target: &Gd<MindDialog>) {
    let Some(mut editor) = kit::editor() else {
        return;
    };
    let result = editor
        .call("playtest", &[])
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    if !dict_bool(&result, "ok", false) {
        let error_key = editor_save_error_key(&dict_string(&result, "error"));
        notify_error(&kit::t(error_key).to_string());
        return;
    }
    close_node(target);
    if dict_bool(&result, "dialog", false) {
        let mut context = VarDictionary::new();
        context.set(
            "map",
            &GString::from(dict_string(&editor_tags(), "name").as_str()).to_variant(),
        );
        context.set("playtest", &true.to_variant());
        open_named_dialog("map_play", &json_stringify(&context));
        return;
    }
    // Play started directly (the Java shift path): hide the editor shell.
    hide_editor_shell();
    show_game();
}

/// The menu's `@quit` -> `tryExit()` (`MapEditorDialog.java:262-265`): the shell
/// owns the unsaved-changes confirm.
fn editor_menu_quit(target: &Gd<MindDialog>) {
    close_node(target);
    if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
        && shell.has_method("_confirm_exit")
    {
        let _ = shell.call("_confirm_exit", &[]);
    }
}

/// `Save`/`playtest` error -> bundle key (`MapEditorDialog.save`, `:410-450`).
fn editor_save_error_key(error: &str) -> &'static str {
    match error {
        "noname" => "@editor.save.noname",
        "overwrite" => "@editor.save.overwrite",
        _ => "@editor.errorsave",
    }
}

/// `MindEditor.tags()` (`{name, author, description}`, `{}` when absent).
pub(super) fn editor_tags() -> VarDictionary {
    let Some(mut editor) = kit::editor() else {
        return VarDictionary::new();
    };
    if !editor.has_method("tags") {
        return VarDictionary::new();
    }
    editor
        .call("tags", &[])
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// Hides the editor shell (`/root/Spine/Ui/EditorDialog`).
fn hide_editor_shell() {
    if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
        && shell.has_method("hide_dialog")
    {
        let _ = shell.call("hide_dialog", &[]);
    }
}

/// `ui_root.set_menu_visible(false)`: the game view without the menu.
fn show_game() {
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("set_menu_visible")
    {
        let _ = root.call("set_menu_visible", &[false.to_variant()]);
    }
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "editor_menu" => Some(Box::new(EditorMenuDialog)),
        _ => None,
    }
}
