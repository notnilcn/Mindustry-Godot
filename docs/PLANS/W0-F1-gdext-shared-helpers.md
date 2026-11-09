<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W0-F1 — Shared gdext helper modules (additive)

**Wave:** W0, after F0, in parallel with F2 and F3.
**Owns:**
- `client/rust/mind-gdext/src/gdutil/**` (new)
- `client/rust/mind-gdext/src/lib.rs` (only the one `mod gdutil;` line)
- `client/rust/mind-gdext/Cargo.toml` (only to add `serde.workspace = true` to `[dependencies]`; `serde_json` does not re-export `serde`, and `to_gstring` needs `serde::Serialize`)
- `client/rust/mind-gdext/src/ui/kit/**` (additions only)
- `client/rust/mind-gdext/src/ui/dialog.rs` (additions only)

**Effort:** medium.

## Goal

Create the shared helpers that the W2 packages will adopt. The same few lines are hand-copied in
dozens of places: JSON → `GString`, lenient `Variant` coercion, `i16` tile coercion, clearing
children, button wiring. **This package only adds code and changes no existing call site**,
except where it says so. W2 packages do the adoption.

The hard part is not writing the helpers. It is writing down **which existing copy has which
semantics**, so the W2 agents can match each copy to the right helper. Step 2 produces that
catalogue.

## Steps

### 1. Worktree and baseline
RULES §1 and §6 (`../mg-F1`, branch `refactor/F1`). Record `api_surface.py` before you start.

### 2. Catalogue the duplicated helpers (read-only first)

Run these greps from `client/rust/mind-gdext/src`. For **every** hit, copy the body into a scratch
file and group identical bodies:
```bash
grep -rn "fn variant_i64\|fn variant_f64\|fn variant_bool\|fn variant_string\|fn variant_text\|fn text_of\|fn int_of" .
grep -rn "fn dict_bool\|fn dict_i64\|fn dict_f64\|fn dict_string\|fn dict_str\|fn row_str\|fn row_bool\|fn row_i64\|fn parse_json_object\|fn parse_object\|fn parse_array" .
grep -rn "fn clear_children\|fn clear_container\|fn clear_table_nodes" .
grep -rn "fn flat_box\|fn spacer\|fn content_texture\|fn icon_rect" .
```
At `291e30a8` the dialogs analysis found:
- `dict_bool` 8 copies with 4 distinct bodies and two signatures: `(d,k)` in batch_a/e, `(d,k,default)` in b/f/g/h;
- `variant_i64` 7 copies with 4 bodies;
- `dict_string` 6, `variant_bool` 5, `dict_i64` 5, `parse_json_object` 3;
- more copies in `ui/fragments/*`, `ui/hud.rs`, `ui/editor_views.rs` and the kit widgets.

The lenient body in `ui/dialogs/batch_b.rs` (`variant_i64`, around line 272) tries in this order:
`i64`, then `f64` (truncated with `as i64`), then `bool` (0 or 1), then a trimmed `GString` parse
defaulting to 0, then 0.

Write the catalogue into your report as a table with these columns: `helper name | semantics
(ordered fallbacks) | copies (file:line)`. Each **distinct** body becomes one row.

### 3. Create `src/gdutil/` (new module, Godot-type utilities with no UI)

Add `mod gdutil;` to `lib.rs`, next to the other `mod` lines in alphabetical order. Files:

- **`gdutil/mod.rs`**: header plus `//! Small Godot-type utilities shared across mind-gdext
  (no UI, no game rules).` and `pub mod json; pub mod variant; pub mod coords;`.
- **`gdutil/json.rs`**:
  ```rust
  /// `"{}"` as a GString: the empty-object fallback used by every `*_json` #[func].
  pub fn empty_object() -> GString { GString::from("{}") }
  /// `"[]"` as a GString.
  pub fn empty_array() -> GString { GString::from("[]") }
  /// `GString::from(value.to_string().as_str())` (compact serde_json output).
  pub fn value_gstring(value: &serde_json::Value) -> GString { GString::from(value.to_string().as_str()) }
  /// `serde_json::to_string(value)` or `fallback` on error, as a GString.
  pub fn to_gstring<T: serde::Serialize + ?Sized>(value: &T, fallback: &str) -> GString {
      GString::from(serde_json::to_string(value).unwrap_or_else(|_| fallback.to_owned()).as_str())
  }
  ```
  These are byte-identical to the inline patterns counted in the analyses: 22× `GString::from("{}")`
  in `campaign.rs`, 23× `GString::from(x.to_string().as_str())` and 8×
  `serde_json::to_string(..).unwrap_or_else(|_| "[]")` in `sim_host.rs`, 50 `GString::from` in
  `net/mod.rs`. Add a `#[cfg(test)]` test for `to_gstring` fallback only if the test can run
  without the Godot engine. GString allocation needs the engine, so it most likely cannot. In
  that case add no test and say so in the report.
- **`gdutil/variant.rs`**: one `pub fn` per **distinct semantics row** of your catalogue.
  Each name must state its behaviour. Suggested names:
  - `to_i64_lenient(&Variant) -> i64` (the batch_b body above)
  - `to_i64_numeric(&Variant) -> i64` (if a copy has no bool/string fallback)
  - `to_f64_lenient`, `to_bool_lenient`, `to_text(&Variant) -> String`
  - `dict_i64_or(&VarDictionary, &str, i64) -> i64`, `dict_bool_or(.., bool)`, `dict_text_or(.., &str)`
  - `parse_object(&str) -> VarDictionary`, `parse_array(&str) -> VarArray`

  Copy each body verbatim from the copy you took it from, and put that copy's `file:line` in the
  doc comment ("Body from `ui/dialogs/batch_b.rs:272` at 291e30a8"). **Do not** invent a merged
  "best" version.
- **`gdutil/coords.rs`**:
  ```rust
  /// `(i16::try_from(x), i16::try_from(y))` as one Option; `None` if either is out of range.
  pub fn tile_i16(x: impl TryInto<i16>, y: impl TryInto<i16>) -> Option<(i16, i16)> {
      Some((x.try_into().ok()?, y.try_into().ok()?))
  }
  ```
  This one is pure Rust; add a unit test for 0, -1, 32767, 32768 and -32769.

### 4. Kit additions (`ui/kit/**`)

First grep what exists: `grep -n "pub fn" ui/kit/*.rs ui/kit/widgets/*.rs`. **Do not add a helper
that already exists under another name.** Note in the report every existing helper that the
dialogs ignore (the analysis found `factory::scroll()` with 0 uses against 15 `MindScroll::new_alloc()`,
and `factory::table()` 17 uses against 14 raw `MindTable::new_alloc()`).

Add what is missing:
- `kit::clear_children`. Copy `batch_f.rs:158` `clear_children` **verbatim**, with its signature; a
  hand-written generic `Gd<T>` version will not compile without the right `upcast`/`Inherits` bounds.
  Check the existing copies first: `clear_container` b:2237, `clear_table_nodes` b:2516/h:436,
  `clear_children` f:158, `fragments/player_list.rs:927`. Some only `queue_free` without
  `remove_child`. In that case add a second helper `queue_free_children` with that exact body.
- `factory::hbox(separation: i32) -> Gd<HBoxContainer>` and `factory::vbox(separation: i32)`.
  Each is `new_alloc` plus `add_theme_constant_override("separation", sep)`. Copy the exact call
  from an existing site.
- `factory::on_pressed(button: &mut Gd<impl Inherits<BaseButton>>, name: &'static str, f: impl FnMut() + 'static)`.
  It wraps `button.connect("pressed", &Callable::from_fn(name, move |_| { f(); Variant::nil() }))`. Find
  how the existing 194 `connect("pressed", &Callable::from_fn(` sites build the closure (args
  ignored, return `Variant::nil()`) and match it exactly. Add `on_text_changed(field: &mut Gd<LineEdit>, name, f: impl FnMut(String) + 'static)` the same way, for the 37 `connect("text_changed"` sites.
- `factory::content_icon(name: &str, px: f32) -> Option<Gd<TextureRect>>` **only if** the identical
  pair `content_texture` + `icon_rect` (`batch_campaign2.rs:410/420` = `batch_c.rs:299/309`, byte
  identical) can be lifted as is. Keep their prefix order (item, liquid, block, unit, status). Write in the doc that
  `ui/planet.rs:1799` and `ui/state_inspector.rs:389` use different orders and must **not** adopt it.

### 5. `ui/dialog.rs` additions (`impl MindDialog`, plain non-`#[func]` methods)

- `pub fn add_default_close_button(&mut self)` calls the existing `add_close_button(210.0)`. Java
  `BaseDialog.addCloseButton()` uses 210; 36 call sites pass 210.0 today.
- `pub fn campaign(&self) -> Option<Gd<crate::campaign::MindCampaign>>` returns the
  `CAMPAIGN_PATH` node `try_cast`. Precedent: `ui/landing.rs` around lines 348/404 and
  `ui/root.rs:339`. T02 uses it; nothing else does yet.
- The "refresh request" trio, copied byte-identically from `ui/dialogs/batch_a.rs` 66–95 (same as
  `batch_e.rs` 75–100): `REQUEST_META`, `request_refresh`, `request_refresh_deferred`,
  `take_request`. Make them `pub(crate)` associated fns or methods with the **same meta key string**
  `"mind_dialog_request"`. The batch files keep their own copies until W2.

### 6. Verify (RULES §6)

- `api_surface.py` diff must be empty: you added no `#[func]`.
- Scoped clippy for mind-gdext. mind-gdext is a `cdylib` and `mod gdutil`/`mod ui` are private, so
  unused helpers **will** trigger `dead_code` even if they are `pub`. Put
  `#![allow(dead_code)] // adopted in W2 (docs/PLANS); C01 removes this` at the top of each new module file
  (`gdutil/json.rs`, `gdutil/variant.rs`, `gdutil/coords.rs`). For additions inside existing kit or
  `dialog.rs` files, put `#[allow(dead_code)]` on each new item. Do not leave warnings.
- `run --cargo-test -p mind-gdext --lib`.

### 7. Report

Your report must contain:
- the **catalogue table** from step 2, which the W2 packages read;
- the list of existing kit helpers that dialogs ignore;
- the CI entry with gates `fmt,clippy,cargo-check,cargo-test`.

## Done when

`gdutil/` and the kit/dialog additions compile without warnings, no existing call site has changed
(`move_check.py` is not needed), and the catalogue is in the report.
