// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map-editor shell: tool rail, team/brush pickers, block palette, rotation/undo/redo.
//! Upstream: core/src/mindustry/editor/MapEditorDialog.java

use super::*;

// ---------------------------------------------------------------------------
// MapEditorDialog (`map_editor_dialog.gd`, `MapEditorDialog.java`)
// ---------------------------------------------------------------------------

/// `build()`: tool button node name -> `EditorTool` name.
const TOOL_BUTTONS: [(&str, &str); 7] = [
    ("ZoomButton", "zoom"),
    ("PickButton", "pick"),
    ("LineButton", "line"),
    ("PencilButton", "pencil"),
    ("EraserButton", "eraser"),
    ("FillButton", "fill"),
    ("SprayButton", "spray"),
];
/// `float size = mobile ? 50f : 58f`.
const TOOL_SIZE: f32 = 58.0;
/// `rebuildBlockSelection`: `blockSelection.add(button).size(50f)`.
const BLOCK_BUTTON_SIZE: f32 = 50.0;
/// `button.resizeImage(8 * 4f)`.
const BLOCK_ICON_SIZE: f32 = 32.0;
/// `search.image(Icon.zoom)`: `Fonts.icon` glyphs are 30 px.
const SEARCH_ICON_SIZE: i32 = 30;
/// `Tex.underline` padding (the 9-patch's 12 px pads) around the block name.
const UNDERLINE_PAD: f32 = 12.0;
/// Arc's line box is 2 px taller than Godot's, so the name row splits 14/12.
const UNDERLINE_PAD_TOP: f32 = 14.0;
const UNDERLINE_PAD_BOTTOM: f32 = 12.0;
/// `scroll-knob-vertical-thin` width (`Styles.smallPane`).
const SMALL_PANE_BAR: f32 = 12.0;
/// `_last_block` value that never matches a block name (forces a refresh).
const NO_BLOCK: &str = "<none>";

/// `MindMapEditorDialog` — the full-screen editor shell over `Ui/EditorDialog`.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindMapEditorDialog {
    base: Base<Control>,

    /// Path to the MindEditor facade.
    #[export]
    pub editor_path: NodePath,

    editor: Option<Gd<Node>>,
    brush_sizes: Vec<i64>,
    /// Team id -> its `clearNoneTogglei` button (`teamgroup`).
    team_buttons: Vec<(i64, Gd<Button>)>,
    /// Block name -> its palette button (`blockSelection`).
    block_buttons: Vec<(String, Gd<Button>)>,
    /// Tool name -> the `M<n>` mode label stacked on its button.
    mode_labels: Vec<(String, Gd<Label>)>,
    /// Tool name -> last seen alternate mode (`EditorTool.mode`).
    tool_modes: Vec<(String, i64)>,
    undo_glyph: Option<Gd<Label>>,
    redo_glyph: Option<Gd<Label>>,
    rotate_glyph: Option<Gd<Label>>,
    last_block: String,
    /// A `MindUi` confirm for `@editor.unsaved` is awaiting an answer.
    pending_exit: bool,

    tool_grid: Option<Gd<GridContainer>>,
    search: Option<Gd<LineEdit>>,
    selected: Option<Gd<Label>>,
    config_panel: Option<Gd<VBoxContainer>>,
    config_label: Option<Gd<Label>>,
    block_grid: Option<Gd<GridContainer>>,
    team_grid: Option<Gd<GridContainer>>,
    brush: Option<Gd<HSlider>>,
}

#[godot_api]
impl IControl for MindMapEditorDialog {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            editor_path: NodePath::from("/root/Spine/MindEditor"),
            editor: None,
            brush_sizes: Vec::new(),
            team_buttons: Vec::new(),
            block_buttons: Vec::new(),
            mode_labels: Vec::new(),
            tool_modes: Vec::new(),
            undo_glyph: None,
            redo_glyph: None,
            rotate_glyph: None,
            last_block: NO_BLOCK.to_string(),
            pending_exit: false,
            tool_grid: None,
            search: None,
            selected: None,
            config_panel: None,
            config_label: None,
            block_grid: None,
            team_grid: None,
            brush: None,
        }
    }

    fn ready(&mut self) {
        self.editor = self.resolve_editor();
        self.tool_grid = self.node_as::<GridContainer>("LeftTools/ToolGrid");
        self.search = self.node_as::<LineEdit>("RightPalette/SearchPad/SearchRow/SearchField");
        self.selected = self.node_as::<Label>("RightPalette/SelectedPanel/SelectedLabel");
        self.config_panel = self.node_as::<VBoxContainer>("RightPalette/ConfigPanel");
        self.config_label = self.node_as::<Label>("RightPalette/ConfigPanel/ConfigLabel");
        self.block_grid = self.node_as::<GridContainer>("RightPalette/BlockScroll/BlockGrid");
        self.team_grid = self.node_as::<GridContainer>("LeftTools/TeamGrid");
        self.brush = self.node_as::<HSlider>("LeftTools/BrushStack/BrushSlider");
        self.apply_theme();
        self.apply_labels();
        self.bind_toolbar();
        self.bind_toggles();
        self.build_teams();
        self.build_brush();
        if self.editor.is_some() {
            self.rebuild_palette();
        }
        self.refresh();
        self.connect_confirm();
    }

    fn process(&mut self, _delta: f64) {
        if self.base().is_visible() {
            self.refresh();
        }
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if !self.base().is_visible() || self.editor().is_none() {
            return;
        }
        // `update(() -> { if(hasKeyboard()) doInput(); })`: dialogs/prompts on
        // top own the keys (UiRoot's Escape closes them).
        if self.has_modal() {
            return;
        }
        let Ok(key) = event.try_cast::<InputEventKey>() else {
            return;
        };
        if !key.is_pressed() || key.is_echo() {
            return;
        }
        let code = key.get_keycode();
        if key.is_ctrl_pressed() {
            match code {
                Key::Z => {
                    if key.is_shift_pressed() {
                        self.call_editor("redo", &[]);
                    } else {
                        self.call_editor("undo", &[]);
                    }
                }
                Key::Y => self.call_editor("redo", &[]),
                Key::S => self.call_editor("save", &[]),
                Key::G => self.toggle_grid(),
                // `_call("set_tool_mode", _number_index(key))`.
                Key::KEY_1 => self.call_editor("set_tool_mode", &[0i64.to_variant()]),
                Key::KEY_2 => self.call_editor("set_tool_mode", &[1i64.to_variant()]),
                Key::KEY_3 => self.call_editor("set_tool_mode", &[2i64.to_variant()]),
                Key::KEY_4 => self.call_editor("set_tool_mode", &[3i64.to_variant()]),
                Key::KEY_5 => self.call_editor("set_tool_mode", &[4i64.to_variant()]),
                _ => {}
            }
            self.consume_input();
            return;
        }
        match code {
            // `MapEditorDialog.doInput`: Escape opens the menu; it does not exit.
            Key::ESCAPE => self.open_menu(),
            Key::V => self.apply_tool("zoom"),
            Key::I => self.apply_tool("pick"),
            Key::L => self.apply_tool("line"),
            Key::B => self.apply_tool("pencil"),
            Key::E => {
                self.apply_tool("eraser");
                self.rotate(-1);
            }
            Key::G => self.apply_tool("fill"),
            Key::R => {
                self.apply_tool("spray");
                self.rotate(1);
            }
            _ => return,
        }
        // `MapEditorDialog.doInput` consumes its keys; UiRoot's Escape handler
        // would otherwise open the pause menu behind the editor shell.
        self.consume_input();
    }
}

#[godot_api]
impl MindMapEditorDialog {
    /// Shows the editor dialog (MCP / menu / `EditorMapsDialog` entry point).
    ///
    /// `UiRoot.set_editor_open` hides the standalone menu and the in-game HUD,
    /// closes the config/inventory panels and lets editor dialogs take the
    /// pointer above the shell. The call is deferred because `UiRoot` invokes
    /// `show_dialog` from its own bound methods (`check_playtest`).
    #[func]
    pub fn show_dialog(&mut self) {
        self.base_mut().set_visible(true);
        self.refresh();
        self.set_editor_open_deferred(true);
    }

    /// Hides the editor dialog; the caller shows the next surface.
    #[func]
    pub fn hide_dialog(&mut self) {
        self.base_mut().set_visible(false);
        self.drop_temporary_input();
        self.set_editor_open_deferred(false);
    }

    /// True while a `MindUi` dialog or prompt is up over the shell.
    #[func]
    pub fn has_modal(&self) -> bool {
        let Some(mut ui_root) = self.ui_root() else {
            return false;
        };
        if !ui_root.has_method("has_modal") {
            return false;
        }
        ui_root
            .call("has_modal", &[])
            .try_to::<bool>()
            .unwrap_or(false)
    }

    /// `MapEditorDialog.tryExit`: answers through the shared `MindUi` confirm.
    #[func]
    fn _confirm_exit(&mut self) {
        let Some(mut ui) = kit::ui() else {
            self.exit_now();
            return;
        };
        if !ui.has_method("show_confirm") {
            self.exit_now();
            return;
        }
        self.pending_exit = true;
        let text = kit::markup_text(&kit::t("@editor.unsaved").to_string());
        let _ = ui.call("show_confirm", &[text.to_variant()]);
    }

    /// The `MindUi.confirm_result` answer for a pending `_confirm_exit`.
    #[func]
    fn on_confirm_result(&mut self, confirmed: bool) {
        if !self.pending_exit {
            return;
        }
        self.pending_exit = false;
        if confirmed {
            self.exit_now();
        }
    }

    /// `build()`: the first tool-grid button is `Icon.menu` -> `menu.show()`.
    #[func]
    fn open_menu(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("open_dialog") {
            return;
        }
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from("editor_menu").to_variant(),
                GString::new().to_variant(),
            ],
        );
    }

    /// One toolbar tool button (`button.pressed.connect(func(): _select_tool(tool))`).
    #[func]
    fn select_tool(&mut self, tool: GString) {
        self.apply_tool(&tool.to_string());
    }

    /// One team swatch (`button.pressed.connect(func(): _select_team(team))`).
    #[func]
    fn select_team(&mut self, team_name: GString) {
        self.call_editor("set_draw_team", &[team_name.to_variant()]);
        self.refresh();
    }

    /// One palette block button (`button.pressed.connect(func(): ...)`).
    #[func]
    fn on_block_selected(&mut self, block_key: GString) {
        self.call_editor("set_draw_block", &[block_key.to_variant()]);
        self.refresh();
    }

    /// `_search.text_changed.connect(func(_text): _rebuild_palette())`.
    #[func]
    fn on_search_changed(&mut self, _text: GString) {
        self.rebuild_palette();
    }

    /// `_brush.value_changed` -> `set_brush_size(sizes[index] / 10f)`.
    #[func]
    fn on_brush_changed(&mut self, value: f64) {
        if self.brush_sizes.is_empty() {
            return;
        }
        let last = self.brush_sizes.len() as i64 - 1;
        let index = (value as i64).clamp(0, last) as usize;
        let radius = self.brush_sizes.get(index).copied().unwrap_or(0) as f64 / 10.0;
        self.call_editor("set_brush_size", &[radius.to_variant()]);
    }

    /// `editor.rotation = (editor.rotation + 1) % 4`.
    #[func]
    fn rotate(&mut self, delta: i64) {
        let Some(editor) = self.editor() else {
            return;
        };
        let current = editor_call(&editor, "rotation", &[])
            .map(|value| variant_i64(&value))
            .unwrap_or(0);
        self.call_editor("set_rotation", &[(current + delta).to_variant()]);
    }

    /// `_toggle_grid()`: `set_grid(!editor.grid)`.
    #[func]
    fn toggle_grid(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let grid = editor_call(&editor, "grid", &[])
            .and_then(|value| value.try_to::<bool>().ok())
            .unwrap_or(false);
        self.call_editor("set_grid", &[(!grid).to_variant()]);
    }

    /// `mid.button("@editor.center", Icon.move, Styles.flatt, view::center)`.
    #[func]
    fn center_view(&mut self) {
        self.call_editor("map_view_center", &[]);
    }

    /// `undo` toolbar button.
    #[func]
    fn on_undo(&mut self) {
        self.call_editor("undo", &[]);
    }

    /// `redo` toolbar button.
    #[func]
    fn on_redo(&mut self) {
        self.call_editor("redo", &[]);
    }

    /// `set_show_buildings(pressed)`.
    #[func]
    fn on_show_blocks(&mut self, pressed: bool) {
        self.call_editor("set_show_buildings", &[pressed.to_variant()]);
    }

    /// `set_show_terrain(pressed)`.
    #[func]
    fn on_show_terrain(&mut self, pressed: bool) {
        self.call_editor("set_show_terrain", &[pressed.to_variant()]);
    }

    /// `set_show_floor(pressed)`.
    #[func]
    fn on_show_floor(&mut self, pressed: bool) {
        self.call_editor("set_show_floor", &[pressed.to_variant()]);
    }
}

impl MindMapEditorDialog {
    /// One `@onready`-style node lookup with the GDScript cast semantics.
    fn node_as<T>(&self, path: &str) -> Option<Gd<T>>
    where
        T: Inherits<Node>,
    {
        self.base().get_node_or_null(path)?.try_cast::<T>().ok()
    }

    /// The live `MindEditor` facade from the exported path.
    fn resolve_editor(&self) -> Option<Gd<Node>> {
        let path = self.editor_path.clone();
        self.base().get_node_or_null(&path)
    }

    /// The stored facade, re-resolved when the exported path changed.
    fn editor(&self) -> Option<Gd<Node>> {
        if let Some(editor) = &self.editor
            && editor.is_instance_valid()
        {
            return Some(editor.clone());
        }
        self.resolve_editor()
    }

    /// `/root/Spine/Ui/UiRoot` (`_ui_root()`).
    fn ui_root(&self) -> Option<Gd<Node>> {
        kit::node("/root/Spine/Ui/UiRoot")
    }

    /// `_call(method, ...)` with the facade's graceful degradation.
    fn call_editor(&self, method: &str, args: &[Variant]) {
        let Some(editor) = self.editor() else {
            return;
        };
        let _ = editor_call(&editor, method, args);
    }

    /// `get_viewport().set_input_as_handled(); accept_event()`.
    fn consume_input(&mut self) {
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.set_input_as_handled();
        }
        self.base_mut().accept_event();
    }

    /// `UiRoot.set_editor_open(...)`, queued out of the caller's borrow.
    fn set_editor_open_deferred(&self, open: bool) {
        let Some(mut ui_root) = self.ui_root() else {
            return;
        };
        defer_action("editor_open", move || {
            if !ui_root.is_instance_valid() || !ui_root.has_method("set_editor_open") {
                return;
            }
            let _ = ui_root.call("set_editor_open", &[open.to_variant()]);
        });
    }

    /// The shell is a sibling of `UiRoot`, so it takes the shared skin
    /// explicitly (`UiRoot._enter_tree` builds the theme before any `_ready`).
    fn apply_theme(&mut self) {
        let Some(ui_root) = self.ui_root() else {
            return;
        };
        let Ok(ui_root) = ui_root.try_cast::<Control>() else {
            return;
        };
        let Some(theme) = ui_root.get_theme() else {
            return;
        };
        self.base_mut().set_theme(&theme);
    }

    /// Bundle text and icon-font glyphs for the statically declared controls.
    fn apply_labels(&mut self) {
        self.set_glyph("LeftTools/ToolGrid/MenuButton", "menu", "menu");
        self.set_glyph("LeftTools/ToolGrid/GridButton", "grid", "grid");
        self.undo_glyph = self.set_glyph("LeftTools/ToolGrid/UndoButton", "undo", "undo");
        self.redo_glyph = self.set_glyph("LeftTools/ToolGrid/RedoButton", "redo", "redo");
        self.rotate_glyph = self.set_glyph("LeftTools/ToolGrid/RotateButton", "right", "rotate");
        for (button_name, tool) in TOOL_BUTTONS {
            let Some(button) = self.tool_button(button_name) else {
                continue;
            };
            self.set_glyph_on(&button, tool, tool);
            let label = self.add_mode_label(&button);
            self.mode_labels.push((tool.to_string(), label));
        }
        // `search.image(Icon.zoom).padRight(8)`: the icon at its natural size.
        if let Some(mut icon) = self.node_as::<Label>("RightPalette/SearchPad/SearchRow/SearchIcon")
        {
            icon.set_text(&icons::glyph("zoom"));
            let mut control: Gd<Control> = icon.clone().upcast();
            icons::apply_icon_font(&mut control);
            icon.add_theme_font_size_override("font_size", SEARCH_ICON_SIZE);
        }
        if let Some(mut search) = self.search.clone() {
            search.set_placeholder(&kit::t("@players.search"));
        }
        self.apply_block_panel_styles();
        if let Some(mut label) = self.node_as::<Label>("LeftTools/BrushStack/BrushLabel") {
            label.set_text(&kit::t("@editor.brush"));
        }
        for (path, key) in [
            (
                "LeftTools/ChecksPad/ShowChecks/ShowBlocks",
                "@editor.showblocks",
            ),
            (
                "LeftTools/ChecksPad/ShowChecks/ShowTerrain",
                "@editor.showterrain",
            ),
            (
                "LeftTools/ChecksPad/ShowChecks/ShowFloor",
                "@editor.showfloor",
            ),
        ] {
            if let Some(mut check) = self.node_as::<Button>(path) {
                check.set_text(&kit::t(key));
            }
        }
        let center = kit::t("@editor.center").to_string();
        self.set_icon_text("LeftTools/CenterButton", "move", &center);
    }

    /// `addBlockSelection` (MapEditorDialog.java:797-832) drawables the theme
    /// has no type for: the `underline` name table and the thin pane bars.
    fn apply_block_panel_styles(&mut self) {
        if let Some(underline) = kit::icon_texture("underline") {
            let mut box_ = StyleBoxTexture::new_gd();
            box_.set_texture(&underline);
            box_.set_texture_margin_all(UNDERLINE_PAD);
            box_.set_content_margin_all(UNDERLINE_PAD);
            box_.set_content_margin(Side::TOP, UNDERLINE_PAD_TOP);
            box_.set_content_margin(Side::BOTTOM, UNDERLINE_PAD_BOTTOM);
            if let Some(mut panel) = self.node_as::<PanelContainer>("RightPalette/SelectedPanel") {
                panel.add_theme_stylebox_override("panel", &box_);
            }
        }
        let Some(scroll) = self.node_as::<ScrollContainer>("RightPalette/BlockScroll") else {
            return;
        };
        let Some(mut bar) = scroll.get_v_scroll_bar() else {
            return;
        };
        let mut track = StyleBoxEmpty::new_gd();
        track.set_content_margin(Side::LEFT, SMALL_PANE_BAR * 0.5);
        track.set_content_margin(Side::RIGHT, SMALL_PANE_BAR * 0.5);
        bar.add_theme_stylebox_override("scroll", &track);
        bar.add_theme_stylebox_override("scroll_focus", &track);
        if let Some(thin) = kit::icon_texture("scroll-knob-vertical-thin") {
            let mut knob = StyleBoxTexture::new_gd();
            knob.set_texture(&thin);
            for slot in ["grabber", "grabber_highlight", "grabber_pressed"] {
                bar.add_theme_stylebox_override(slot, &knob);
            }
        }
    }

    /// `mid.button("@editor.center", Icon.move, Styles.flatt, view::center)`:
    /// an image text button — the icon at the left margin, the label centred.
    fn set_icon_text(&self, path: &str, icon_name: &str, text: &str) {
        let Some(mut button) = self.node_as::<Button>(path) else {
            return;
        };
        button.set_text(text);
        if icons::glyph(icon_name).is_empty() {
            return;
        }
        button.set_text("");
        // code-instantiated: icon-font glyphs need the `MindAssets` icon font.
        let mut row = HBoxContainer::new_alloc();
        row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        row.set_offset(Side::LEFT, 9.0);
        row.set_offset(Side::RIGHT, -9.0);
        row.set_mouse_filter(MouseFilter::IGNORE);
        // code-instantiated: the icon-font glyph cell of the icon text button.
        let mut glyph = factory::glyph_label(icon_name, 28, Color::WHITE);
        glyph.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&glyph);
        // code-instantiated: the centred label cell of the icon text button.
        let mut label = Label::new_alloc();
        label.set_text(text);
        label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        label.set_vertical_alignment(VerticalAlignment::CENTER);
        label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        label.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&label);
        button.add_child(&row);
    }

    /// `_set_glyph`: the icon-font glyph filling its button, or the text
    /// fallback when the glyph is unknown.
    fn set_glyph(&self, path: &str, icon_name: &str, fallback: &str) -> Option<Gd<Label>> {
        let button = self.node_as::<Button>(path)?;
        self.set_glyph_on(&button, icon_name, fallback)
    }

    /// [`Self::set_glyph`] on an already resolved button.
    fn set_glyph_on(
        &self,
        button: &Gd<Button>,
        icon_name: &str,
        fallback: &str,
    ) -> Option<Gd<Label>> {
        let mut button = button.clone();
        button.set_text("");
        if icons::glyph(icon_name).is_empty() {
            button.set_text(fallback);
            return None;
        }
        // code-instantiated: icon-font glyphs need the `MindAssets` icon font.
        let mut glyph = factory::glyph_label(icon_name, 28, Color::WHITE);
        glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        glyph.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&glyph);
        Some(glyph)
    }

    /// `tools.stack(button, mode)`: the `M<n>` alt-mode label in `Pal.remove`.
    fn add_mode_label(&self, button: &Gd<Button>) -> Gd<Label> {
        // code-instantiated: the alternate-mode badge of one tool button.
        let mut label = Label::new_alloc();
        label.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        label.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        label.set_vertical_alignment(VerticalAlignment::BOTTOM);
        label.set_mouse_filter(MouseFilter::IGNORE);
        label.add_theme_color_override("font_color", styles::REMOVE);
        label.add_theme_font_size_override("font_size", 14);
        let mut button = button.clone();
        button.add_child(&label);
        label
    }

    /// `MapEditorDialog.tryExit` answers through `MindUi.confirm_result`.
    fn connect_confirm(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_signal("confirm_result") {
            return;
        }
        let target = self.to_gd();
        let callable = Callable::from_fn("map_editor_confirm", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .and_then(|value| value.try_to::<bool>().ok())
                .unwrap_or(false);
            let target = target.clone();
            defer_action("map_editor_confirm_deferred", move || {
                if !target.is_instance_valid() {
                    return;
                }
                let mut node = target.clone();
                let _ = node.call("on_confirm_result", &[confirmed.to_variant()]);
            });
        });
        ui.connect("confirm_result", &callable);
    }

    /// The toolbar wiring of `_bind_toolbar()`.
    fn bind_toolbar(&mut self) {
        if let Some(mut menu) = self.node_as::<Button>("LeftTools/ToolGrid/MenuButton") {
            menu.connect("pressed", &self.base().callable("open_menu"));
        }
        for (button_name, tool) in TOOL_BUTTONS {
            let Some(mut button) = self.tool_button(button_name) else {
                continue;
            };
            let mut args = VarArray::new();
            args.push(&GString::from(tool).to_variant());
            button.connect("pressed", &self.base().callable("select_tool").bindv(&args));
        }
        if let Some(mut grid) = self.node_as::<Button>("LeftTools/ToolGrid/GridButton") {
            grid.connect("pressed", &self.base().callable("toggle_grid"));
        }
        if let Some(mut undo) = self.node_as::<Button>("LeftTools/ToolGrid/UndoButton") {
            undo.connect("pressed", &self.base().callable("on_undo"));
        }
        if let Some(mut redo) = self.node_as::<Button>("LeftTools/ToolGrid/RedoButton") {
            redo.connect("pressed", &self.base().callable("on_redo"));
        }
        if let Some(mut rotate) = self.node_as::<Button>("LeftTools/ToolGrid/RotateButton") {
            let mut args = VarArray::new();
            args.push(&1i64.to_variant());
            rotate.connect("pressed", &self.base().callable("rotate").bindv(&args));
        }
        if let Some(mut center) = self.node_as::<Button>("LeftTools/CenterButton") {
            center.connect("pressed", &self.base().callable("center_view"));
        }
        if let Some(mut search) = self.search.clone() {
            search.connect("text_changed", &self.base().callable("on_search_changed"));
        }
    }

    /// The show toggles of `_bind_toggles()`.
    fn bind_toggles(&mut self) {
        for (path, handler) in [
            (
                "LeftTools/ChecksPad/ShowChecks/ShowBlocks",
                "on_show_blocks",
            ),
            (
                "LeftTools/ChecksPad/ShowChecks/ShowTerrain",
                "on_show_terrain",
            ),
            ("LeftTools/ChecksPad/ShowChecks/ShowFloor", "on_show_floor"),
        ] {
            if let Some(mut check) = self.node_as::<Button>(path) {
                check.connect("toggled", &self.base().callable(handler));
            }
        }
    }

    /// `for(Team team : Team.baseTeams)`: one tinted toggle per base team.
    fn build_teams(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let Some(mut grid) = self.team_grid.clone() else {
            return;
        };
        let rows = editor_call(&editor, "palette_teams", &[])
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default();
        for row in rows.iter_shared() {
            let value = row.try_to::<VarDictionary>().unwrap_or_default();
            let id = dict_i64(&value, "id");
            let team_name = dict_string(&value, "name");
            // code-instantiated: team buttons are data-driven from the
            // base-team registry.
            let mut button = Button::new_alloc();
            button.set_theme_type_variation("clearNoneTogglei");
            button.set_toggle_mode(true);
            button.set_custom_minimum_size(Vector2::new(TOOL_SIZE, TOOL_SIZE));
            // code-instantiated: the team colour swatch (`Tex.whiteui` tint).
            let mut swatch = ColorRect::new_alloc();
            swatch.set_anchors_preset(LayoutPreset::FULL_RECT);
            swatch.set_offset(Side::LEFT, 4.0);
            swatch.set_offset(Side::TOP, 4.0);
            swatch.set_offset(Side::RIGHT, -4.0);
            swatch.set_offset(Side::BOTTOM, -4.0);
            swatch.set_mouse_filter(MouseFilter::IGNORE);
            let packed = dict_i64(&value, "color");
            swatch.set_color(Color::from_rgba8(
                ((packed >> 24) & 0xFF) as u8,
                ((packed >> 16) & 0xFF) as u8,
                ((packed >> 8) & 0xFF) as u8,
                255,
            ));
            button.add_child(&swatch);
            let mut args = VarArray::new();
            args.push(&GString::from(team_name.as_str()).to_variant());
            button.connect("pressed", &self.base().callable("select_team").bindv(&args));
            grid.add_child(&button);
            self.team_buttons.push((id, button));
        }
    }

    /// `_build_brush()`: the brush radius slider over `editor.brush_sizes()`.
    fn build_brush(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        self.brush_sizes.clear();
        let sizes = editor_call(&editor, "brush_sizes", &[])
            .and_then(|value| value.try_to::<PackedInt64Array>().ok())
            .unwrap_or_default();
        self.brush_sizes.extend(sizes.as_slice().iter().copied());
        if self.brush_sizes.is_empty() {
            return;
        }
        let Some(mut brush) = self.brush.clone() else {
            return;
        };
        brush.set_max((self.brush_sizes.len() - 1) as f64);
        brush.set_value(0.0);
        brush.connect("value_changed", &self.base().callable("on_brush_changed"));
    }

    /// `rebuildBlockSelection(searchText)`: the Rust palette order, filtered by
    /// the localized block name, six 50 px toggles per row.
    fn rebuild_palette(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let Some(mut grid) = self.block_grid.clone() else {
            return;
        };
        for child in grid.get_children().iter_shared() {
            let mut child = child.clone();
            grid.remove_child(&child);
            child.queue_free();
        }
        self.block_buttons.clear();
        self.last_block = NO_BLOCK.to_string();
        let mut search = String::new();
        if let Some(field) = &self.search {
            search = field
                .get_text()
                .to_string()
                .trim()
                .split(' ')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
        }
        let rows = editor_call(&editor, "palette_blocks", &[GString::new().to_variant()])
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default();
        for row in rows.iter_shared() {
            let value = row.try_to::<VarDictionary>().unwrap_or_default();
            let block_key = dict_string(&value, "name");
            let localized = self.block_name(&block_key);
            if !search.is_empty() && !localized.to_lowercase().contains(&search) {
                continue;
            }
            let Some(texture) = kit::icon_texture(&format!("block-{block_key}-ui")) else {
                continue;
            };
            // code-instantiated: one palette button per editor block
            // (content-driven count).
            let mut button = Button::new_alloc();
            button.set_theme_type_variation("clearNoneTogglei");
            button.set_toggle_mode(true);
            button.set_custom_minimum_size(Vector2::new(BLOCK_BUTTON_SIZE, BLOCK_BUTTON_SIZE));
            button.set_tooltip_text(&localized);
            // code-instantiated: the block's `uiIcon` inside its palette cell.
            let mut icon = TextureRect::new_alloc();
            icon.set_texture(&texture);
            icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
            icon.set_mouse_filter(MouseFilter::IGNORE);
            icon.set_anchors_preset(LayoutPreset::CENTER);
            let half = BLOCK_ICON_SIZE / 2.0;
            icon.set_offset(Side::LEFT, -half);
            icon.set_offset(Side::TOP, -half);
            icon.set_offset(Side::RIGHT, half);
            icon.set_offset(Side::BOTTOM, half);
            button.add_child(&icon);
            let mut args = VarArray::new();
            args.push(&GString::from(block_key.as_str()).to_variant());
            button.connect(
                "pressed",
                &self.base().callable("on_block_selected").bindv(&args),
            );
            grid.add_child(&button);
            self.block_buttons.push((block_key, button));
        }
    }

    /// `block.localizedName`: `Core.bundle.get("block." + name + ".name", name)`.
    fn block_name(&self, block_key: &str) -> String {
        if block_key.is_empty() {
            return String::new();
        }
        let key = format!("block.{block_key}.name");
        let resolved = kit::t(&key).to_string();
        if resolved == key || resolved.is_empty() {
            block_key.to_string()
        } else {
            resolved
        }
    }

    /// `_select_tool(tool)`: `set_tool(tool)` + refresh.
    fn apply_tool(&mut self, tool: &str) {
        self.call_editor("set_tool", &[GString::from(tool).to_variant()]);
        self.refresh();
    }

    /// The per-frame `update(...)` hooks of `build()`.
    fn refresh(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let status = editor_call(&editor, "status", &[])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let tool = dict_string(&status, "tool");
        let tool_mode = dict_i64(&status, "tool_mode");
        self.set_tool_mode(&tool, tool_mode);
        for (button_name, tool_name) in TOOL_BUTTONS {
            let Some(mut button) = self.tool_button(button_name) else {
                continue;
            };
            button.set_pressed_no_signal(tool_name == tool.as_str());
            let mode = self.tool_mode_for(tool_name);
            if let Some(mut label) = self.mode_label(tool_name) {
                label.set_text(&if mode == -1 {
                    String::new()
                } else {
                    format!("M{} ", mode + 1)
                });
            }
        }
        if let Some(mut grid_button) = self.node_as::<Button>("LeftTools/ToolGrid/GridButton") {
            grid_button.set_pressed_no_signal(dict_bool(&status, "grid"));
        }
        let undo_glyph = self.undo_glyph.clone();
        self.refresh_undo(
            "LeftTools/ToolGrid/UndoButton",
            &undo_glyph,
            dict_bool(&status, "can_undo"),
        );
        let redo_glyph = self.redo_glyph.clone();
        self.refresh_undo(
            "LeftTools/ToolGrid/RedoButton",
            &redo_glyph,
            dict_bool(&status, "can_redo"),
        );
        if let Some(mut glyph) = self.rotate_glyph.clone() {
            // Arc turns counter-clockwise (y-up); Godot's y-down turns
            // clockwise.
            let size = glyph.get_size();
            glyph.set_pivot_offset(size / 2.0);
            glyph.set_rotation_degrees(-90.0 * dict_i64(&status, "rotation") as f32);
        }
        let team = dict_i64(&status, "draw_team");
        for (id, button) in &mut self.team_buttons {
            button.set_pressed_no_signal(*id == team);
        }
        let block = dict_string(&status, "draw_block");
        if block != self.last_block {
            self.last_block = block.clone();
            for (name, button) in &mut self.block_buttons {
                button.set_pressed_no_signal(*name == block);
            }
            if let Some(mut selected) = self.selected.clone() {
                // `extra.labelWrap(() -> editor.drawBlock.localizedName)`.
                selected.set_text(&self.block_name(&block));
            }
        }
        self.refresh_config(&block);
    }

    /// `undo.setDisabled(() -> !editor.canUndo())` + the gray image tint.
    fn refresh_undo(&self, path: &str, glyph: &Option<Gd<Label>>, enabled: bool) {
        if let Some(mut button) = self.node_as::<Button>(path) {
            button.set_disabled(!enabled);
        }
        if let Some(glyph) = glyph
            && glyph.is_instance_valid()
        {
            let mut glyph = glyph.clone();
            glyph.set_modulate(if enabled {
                Color::WHITE
            } else {
                styles::ARC_GRAY
            });
        }
    }

    /// `_refresh_config(block)`: the `block_info` configurable row.
    fn refresh_config(&mut self, block: &str) {
        let Some(editor) = self.editor() else {
            return;
        };
        let info = editor_call(&editor, "block_info", &[GString::from(block).to_variant()])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let configurable = dict_bool(&info, "configurable");
        if let Some(mut panel) = self.config_panel.clone() {
            panel.set_visible(configurable);
        }
        if configurable && let Some(mut label) = self.config_label.clone() {
            label.set_text(&format!(
                "config: {block} (size {})",
                dict_i64(&info, "size")
            ));
        }
    }

    /// Releases any in-progress stroke / temporary tool in the view.
    fn drop_temporary_input(&self) {
        let Some(mut view) = self.base().get_node_or_null("MapView") else {
            return;
        };
        if !view.has_method("cancel_input") {
            return;
        }
        let _ = view.call("cancel_input", &[]);
    }

    /// The accept path: `logic.reset(); hide()` (`MindEditor.try_exit`).
    fn exit_now(&mut self) {
        if let Some(editor) = self.editor() {
            let _ = editor_call(&editor, "try_exit", &[]);
        }
        self.exit_to_menu();
    }

    /// Closing the editor returns to the main menu (its `tryExit` confirm runs
    /// `logic.reset(); hide()` and `MenuFragment`'s visibility group brings the
    /// menu back). Every open dialog on top (the editor menu) closes too.
    fn exit_to_menu(&mut self) {
        self.hide_dialog();
        if let Some(mut ui) = kit::ui()
            && ui.has_method("close_dialog")
        {
            let _ = ui.call("close_dialog", &[GString::from("editor_menu").to_variant()]);
        }
        if let Some(mut ui_root) = self.ui_root()
            && ui_root.has_method("return_to_menu")
        {
            let _ = ui_root.call("return_to_menu", &[]);
        }
    }

    /// One tool button from the static tool grid.
    fn tool_button(&self, name: &str) -> Option<Gd<Button>> {
        let grid = self.tool_grid.as_ref()?;
        grid.get_node_or_null(name)?.try_cast::<Button>().ok()
    }

    /// The `M<n>` label of one tool (valid handles only).
    fn mode_label(&self, tool: &str) -> Option<Gd<Label>> {
        self.mode_labels
            .iter()
            .find(|(name, label)| name == tool && label.is_instance_valid())
            .map(|(_, label)| label.clone())
    }

    /// The last seen alternate mode of one tool (`-1` when unknown).
    fn tool_mode_for(&self, tool: &str) -> i64 {
        self.tool_modes
            .iter()
            .find(|(name, _)| name == tool)
            .map(|(_, mode)| *mode)
            .unwrap_or(-1)
    }

    /// `_tool_modes[tool] = status.tool_mode`.
    fn set_tool_mode(&mut self, tool: &str, mode: i64) {
        if let Some(entry) = self.tool_modes.iter_mut().find(|(name, _)| name == tool) {
            entry.1 = mode;
            return;
        }
        self.tool_modes.push((tool.to_string(), mode));
    }
}
