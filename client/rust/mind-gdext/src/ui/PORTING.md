# Porting GDScript UI to Rust (agent guide)

This tree replaces every non-addon `.gd` file under `client/` with native Rust
GDExtension classes. This guide is the contract for the remaining ports: the
Rust UI kit API, the gdext patterns that bite, and how a ported dialog or
fragment is wired into the scenes.

Read [`client/AGENTS.md`](../../../../../../AGENTS.md) and
[`client/rust/mind-gdext/AGENTS.md`](../../../AGENTS.md) first. The invariants
that still apply: SPDX header + upstream citation on every file, UTF-8/LF,
Rust owns behaviour, no game rules in the UI, bundle keys for user-visible
strings, and append-only ABIs.

## Layout

| Path | Responsibility |
|---|---|
| `ui/kit/` | The widget toolkit: `table.rs` (`MindTable`/`MindStack`/`Cell`), `cell.rs` (`CellData`, `Align`), `text.rs` (`MindLabel`/`MindRichLabel`/`MindIconEffect`), `scroll.rs` (`MindScroll`), `styles.rs` (constants + style lookup), `theme.rs` (`MindThemeBuilder`), `icons.rs` (`MindIcons`), `margins.rs`, `widgets/` (interactive widgets + `factory`). |
| `ui/dialog.rs` | `MindDialog` — the one native root class for every dialog scene, plus the `DialogController` trait. |
| `ui/dialogs/` | One controller module per dialog, dispatched by registry key. |
| `ui/fragments/` | One native class per fragment scene. |
| `ui/root.rs` | `MindUiRoot` (layer groups, manifest binding, prompts). |
| `ui/checks.rs` | Headless oracles (`MindShadersCheck`, `MindUiLayoutCheck`, `MindUiWidgetsCheck`) replacing `client/tools/*.gd`; run through `client/scenes/tools/*.tscn`. |
| `ui/planet.rs`, `ui/inspector.rs`, `ui/landing.rs`, `ui/menu.rs` | The remaining root modules (`planet_view`, `state_inspector`, `landing_core`, `menu_background`/`menu_flyers`). |

## gdext 0.5 / Godot 4.7 patterns

- Node-derived classes (`base=Control`, `base=Button`, `base=Label`,
  `base=RichTextLabel`, `base=TextureRect`, `base=Panel`, `base=Container`,
  `base=Node2D`, `base=ScrollContainer`): instantiate with `T::new_alloc()`
  (`godot::obj::NewAlloc`). Only `RefCounted`-derived classes use `new_gd()`
  (`godot::obj::NewGd`).
- `#[class(base=Control, init)]` + `base: Base<Control>` gives a generated
  `init` from field defaults; do **not** write `fn init`. For custom init logic
  omit both `init` and `no_init` and implement
  `fn init(base: Base<Control>) -> Self` in `IControl`. `#[class(no_init)]`
  means "not constructible" and forbids `init`.
- `Gd<UserClass>` has no `Deref`: call methods through `.bind()` / `.bind_mut()`
  (`widget.bind_mut().setup(...)`). `Gd<EngineClass>` (Button, Label, …) derefs
  normally.
- `BaseMut<T>` derefs to `T`; function arguments need `&mut *self.base_mut()`.
- `Gd::try_cast::<U>()` consumes the handle: test with
  `gd.clone().try_cast::<U>().is_ok()`.
- Optional **object** parameters (`set_texture`, `Theme::set_stylebox`,
  `set_icon`, `set_default_font`, `add_theme_stylebox_override`, …) take
  `&Gd<T>` (gdext `AsArg` docs), not `Some(gd)`. Optional **scalars**
  (`Option<f32>`, `Option<i32>`) still take `Some(value)`.
- Arrays iterate with `.iter_shared()`; build with `Array::<Variant>::new()`.
  `Node::call(name, &[Variant])` takes a slice; `callv` takes
  `&Array<Variant>`.
- Signal connect: `let callable = self.base().callable("method_name");`
  (`method_name` must be `#[func]`) then
  `row.signals().child_entered_tree().connect(&callable);`.
- Size flags: `set_h_size_flags(SizeFlags::EXPAND_FILL)`,
  `set_v_size_flags(...)` with `godot::classes::control::SizeFlags`.
- Enums live in the class sidecar module: `godot::classes::control::LayoutPreset`,
  `MouseFilter`, `godot::classes::box_container::AlignmentMode`,
  `godot::classes::text_server::AutowrapMode`,
  `godot::classes::scroll_container::ScrollMode`.
- `ScrollContainer` scroll position is `set_v_scroll(i32)`; `Range` bars use
  `get_max()`, `get_page()`, `get_value()`.
- `get_child(i)` returns `Option<Gd<Node>>` in 4.7; `get_children()` returns an
  `Array<Gd<Node>>`.
- Workspace lints deny `unsafe_code`, clippy `all`, `unwrap_used`,
  `expect_used`. No `unwrap()`/`expect()` in library code.
- `self.to_gd()` yields `Gd<Self>` (trait `WithBaseField`).

## Kit API

```rust
use crate::ui::kit::{self, styles, MindLabel, MindScroll, MindStack, MindTable};

let mut table = MindTable::new_alloc();
let id = table.add(&label).pad(3.0).grow_x();   // Cell guard invalidates layout
table.cell(id).map(|c| c.width(120.0));          // re-borrow later
table.row();

let mut scroll = MindScroll::new_alloc();
let content = scroll.content();                  // creates the content table

kit::icon_texture("button")                      // Option<Gd<Texture2D>>
kit::glyph("ok")                                 // GString glyph
kit::t("@back")                                  // bundle lookup
kit::tm("@key")                                  // bundle markup -> BBCode
kit::ui() / kit::hud() / kit::campaign() / kit::sim_host() / kit::editor()
```

`styles::ACCENT`, `styles::GRAY_PANEL`, `styles::FLAT_OVER`,
`styles::ARC_GRAY`, `styles::drawable_color(name)` mirror `MindStyles`.
`kit::theme::build()` builds the runtime theme.

`widgets::factory` is the replacement for the old `MindWidgets` static
helpers: `table()`, `stack()`, `scroll()`, `label(text)`, `styled_label`,
`button(text)`, `image_button`, `icon_button(icon, text, style, align)`,
`glyph_label`, `image`, `field`, `slider`, `space`, `bar`, `warning_bar`,
`grid_image`, `border_image`, `check`, `mobile_button`, `req_image`,
`items_display`, `core_items_display`, `fade_in`, `fade_out`, `remove_after`,
`content_icon`, `inset_to_content`.

## Dialog controllers

```rust
use super::super::dialog::{DialogController, MindDialog};

pub struct AboutDialog;

impl DialogController for AboutDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@about");
        dialog.add_close_button(210.0);
    }
    fn shown(&mut self, dialog: &mut MindDialog) { /* rebuild dynamic rows */ }
}
```

`MindDialog` provides: `title_node`, `content`, `body`, `buttons_row`, `panel`,
`dialog_id`, `t`, `tm`, `context_value`, `context_string`, `context_int`,
`add_button`, `add_close_button`, `clear_buttons`, `clear_content`,
`content_table`, `set_title_key`, `set_title_literal`, `enable_full_dialog`,
`apply_cont_top`, `apply_button_overlay`, `apply_fill_parent`, `close`,
`open_dialog`, `show_toast`, `campaign_json`, `campaign_call`,
`campaign_section`, `campaign_node`, `campaign_views`, `format_time_ms`,
`pane_box`, `apply_pane_button`, `build_button_content`, `fill_panel`.
The per-dialog `#[signal]` union (emit with
`dialog.to_gd().emit_signal("name", &[...])`) lives at the bottom of
`dialog.rs`; never declare a dialog-local signal in a controller.

The registry dispatch lives in `ui/dialogs/mod.rs`:
`controller_for(id) -> Option<Box<dyn DialogController>>`.

## Scene wiring

`scenes/ui/dialogs/mind_dialog_base.tscn` roots on `type="MindDialog"`; the 46
derived dialog scenes keep `instance=ExtResource(...)` and **drop** their
`script = ...` line. The two non-inherited dialogs change
`type="Control"` → `type="MindDialog"` and drop the script.

Fragment scenes set `type="<RustClass>"` (the widget/fragment class) and drop
the `script = ...` line. A node that used a script on an engine type
(`type="Label"` + `mind_rich_label.gd`) becomes `type="MindRichLabel"`.
`type="ScrollContainer"` becomes `type="MindScroll"`.

## Verification

```bash
cargo check --manifest-path client/rust/Cargo.toml -p mind-gdext --target-dir client/bin/rust
cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --target-dir client/bin/rust --all-targets -- -D warnings
bash tools/build.sh
bash tools/godot.sh --headless --editor --quit --path client
```

The headless import must report no script errors. `grep -rn "res://.*\.gd" client --include='*.tscn'`
must be empty when the port is complete.

The three headless oracles are native scenes (no `--script`):

```bash
bash tools/godot.sh --headless --path client res://scenes/tools/shaders_check.tscn
bash tools/godot.sh --headless --path client res://scenes/tools/ui_layout_check.tscn
bash tools/godot.sh --headless --path client res://scenes/tools/ui_widgets_check.tscn
```

Each prints its summary (`SHADERCHECK`/`LAYOUTCHECK`/`UICHECK: failed=0`) and
exits non-zero on a failure; `tools/ci.sh` gates on both.
