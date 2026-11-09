<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W2-H05 — Other dialog groups: adopt shared helpers, delete local copies (four sub-packages)

**Wave:** W2 (helper package, RULES §4). **Needs:** W1 (S06–S10) and W0 (F1, F2) merged.
**Effort:** medium for each sub-package. Each sub-package is a **separate agent and branch**
(`refactor/H05a`, …). They own disjoint directories:

| Sub-package | Owns (under `client/rust/mind-gdext/src/ui/dialogs/`) |
|---|---|
| **H05a** | `settings/**`, `database/**`, `menu/**`, `dev/**` |
| **H05b** | `game/**`, `net/**`, `pickers/list_select.rs`, `campaign/campaign_rules.rs`, `campaign/campaign_complete.rs` |
| **H05c** | `editor/**`, `pickers/{file_chooser,icon_select}.rs`, `schematics/schematic_info.rs` |
| **H05d** | `rules/**`, `logic/**`, `mods/**`, `pickers/color_picker.rs` |

## Common steps (every sub-package)

1. Read F1's report (`docs/PLANS/reports/F1.md`): the helper semantics catalogue, plus the list of
   existing kit helpers that dialogs ignore.
2. **Copies of the F2 common helpers.** In your files, find the local `defer_action`, `ui_call` and `rich`
   (and `Deferred`/`install_frame_tick`). `diff` each one against
   `dialogs/common/{defer,facade}.rs`. Delete the identical ones and import the common version. Keep
   any that differ, and list them.
3. **Variant/dict/JSON helpers** (`variant_i64`, `dict_bool`, `dict_string`, `parse_json_object`, …).
   Switch to `crate::gdutil::variant::*` **only** where the catalogue says your copy's semantics
   equal that helper's. In particular `dict_bool` comes in two signatures, `(d,k)` and
   `(d,k,default)`, and four bodies. Do not change behaviour to make a site fit.
4. **Merge util leaves inside your group** where bodies are identical:
   - H05b: `game/util_b.rs`, `net/util.rs`, `net/util_h.rs`, `net/util_misc.rs`. About 15 helpers are
     duplicated between the old `batch_b` and `batch_h` (`settings_int`, `stored_player_name`,
     `push_player_identity`, `close_named_dialog`, `emit_dialog_signal`, `format_millis`, `ui_show_*`).
   - H05c: `editor/util_d.rs`, `util_e.rs`, `util_f.rs`.

   The goal is one `util.rs` per group, holding the surviving helpers. You may add and remove files and
   `pub mod` lines **inside the group you own** (you are the only W2 owner).
5. **Kit adoption:**
   - `kit::clear_children`, or `queue_free_children` where the body only queue-frees. Sites: `clear_container`,
     `clear_table_nodes`, `clear_children` and the 27 inline `get_children().iter_shared()` loops.
     Check each body against the helper.
   - `factory::hbox/vbox(sep)` where the site is exactly `new_alloc` plus a separation override.
   - `add_default_close_button()` for `add_close_button(210.0)`; other widths stay.
   - `factory::scroll()` / `factory::table()` where a raw `MindScroll::new_alloc()` / `MindTable::new_alloc()` is
     followed by exactly what the factory fn does.
   - `factory::on_pressed` / `on_text_changed` for `connect("pressed", &Callable::from_fn(...))`.
     Optional and large (194 + 37 sites); do it per file and only for the plain shape (args ignored,
     returns `Variant::nil()`).
6. **Refresh trio.** H05a (`settings/util.rs`) and H05c (`editor/util_e.rs`) each have a copy of
   `request_refresh`/`request_refresh_deferred`/`take_request`. Switch to F1's `MindDialog` versions
   (same meta key `"mind_dialog_request"`).
7. **Color stripping** (`strip_colors` in `editor/util_d.rs` and `mods/util.rs`): these differ from
   each other and from `mind_core::util::strings::strip_colors` (02-FINDINGS #15). Leave them.
8. **Do not touch** meta-key strings, dialog ids or signal names.

## Sub-package specifics

- **H05a.** `settings/util.rs` holds `open_content_info` (`pub`, re-exported by the `batch_a` shim).
  Keep it `pub` and keep its path. `database/content_info.rs` and `settings/util.rs` both have a
  `flat_box`, while `editor/util_e`/`util_f` and `logic/editor` have others with **4 different
  bodies**. Leave every `flat_box`.
- **H05b.** `net/join/version_gate.rs` is pure logic with tests; leave it. Merge the `game/` and `net/`
  util files as step 4 describes.
- **H05c.** `editor/map_assets/**` is about 2200 lines across its children. Its local `connect_prompt_once`
  (from `util_f`) and `rules/util.rs`'s copy (in H05d's group) are the same helper. Merge only your own.
  Note the cross-group duplicate for C01.
- **H05d.** `logic/editor/**`. Of its 15 raw `Label::new_alloc()`, 9 are in this group; switch them to the
  kit factory label fn only where the factory sets nothing extra. `rules/custom_rules/**` is the
  biggest dialog; adopt the helpers file by file.

## Verification (each sub-package, RULES §6)

```bash
python3 tools/refactor/api_surface.py | diff /tmp/<ID>-surface-before.txt -
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib dialogs
git diff --stat main...HEAD          # paste into the report; expect a net deletion
```

Orchestrator post-merge: `dialog_sweep_check` (`DIALOGCHECK: failed=0`) and `ui_widgets_check`.
