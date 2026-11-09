<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W2-H04 — Campaign dialogs: adopt shared helpers, dedupe the two campaign helper sets

**Wave:** W2 (helper package, RULES §4). **Priority:** first. **Needs:** W1 (S03, S04) and W0 (F1, F2) merged.
**Owns** (paths under `client/rust/mind-gdext/src/ui/`):
- `dialogs/campaign/**`, except `campaign_rules.rs` and `campaign_complete.rs` (those belong to H05b);
- `dialogs/maps/**`;
- `dialogs/schematics/{schematics_dialog,files,subdialogs,import_export}.rs`;
- `dialogs/pickers/palette.rs`;
- `planet/**`.

**Effort:** medium.

## Background

After S03/S04, the campaign dialogs have two private helper sets:
- `dialogs/campaign/util.rs`, from the old `batch_campaign2`;
- `dialogs/campaign/planet/util.rs`, from the old `batch_c`.

The analysis compared them at `291e30a8`:

| Helper | campaign/util.rs (bc2 line) | planet/util.rs (batch_c line) | Same? |
|---|---|---|---|
| `content_texture` | 410 | 299 | **byte-identical** |
| `icon_rect` | 420 | 309 | **byte-identical** |
| `spacer` | 393 | 282 | **byte-identical** |
| `connect_once` | 477 | 387 | near-identical (diff it) |
| `setting_*` | 271–306 | 205–245 | near-identical (diff it) |
| `bundle_or` | 557 | 468 | near-identical (diff it) |
| `strip_color_tags` | 441 | 351 | **different**: batch_c also strips `[]` |
| `Deferred` + `install_frame_tick` | 503 / 519 | 413 / 429 | differ only by meta prefix (`"_campaign2_frame_"` / `"_batch_c_frame_"`) and callable name. F2's `dialogs/common/defer.rs` takes both as parameters |
| `row_*` family | 98–156 | 74–182 | **not identical**: bc2 `row_str` stringifies any value, batch_c accepts only `GString`; `row_i64_or` differs. Only `row_rows` is byte-identical |
| `ui_call`, `rich`, `dialog_node`, `defer_action` | yes | yes | `ui_call`/`rich`/`defer_action` identical to F2's `common/facade.rs`/`common/defer.rs` |

Logic is also duplicated between dialogs (bc2 = old `batch_campaign2`):

| Duplicate | Sites | Same? |
|---|---|---|
| fetch the launch resource stacks | bc2 5157 vs batch_c 3231 | **byte-equivalent**; bc2 4386 differs |
| "is this sector being played" (`get_sector_state` + compare planet/sector) | bc2 5707–5725 vs batch_c 3029–3048 | identical in logic (diff the text) |
| fetch the launch plan | bc2 4787 `launch_plan` vs batch_c 3212 `planet_launch_plan` | diff |
| localized sector name | batch_c 840, bc2 5619, bc2 4872 (preset only) | likely different; diff |
| sector row lookup | bc2 5636, batch_c 758/773, `sector.rs` 99–125 | different shapes; leave for T02 (typed views) |

## Steps

1. **Diff first.** For every row above, `diff` the two bodies at HEAD and put the result in your
   report.
2. **Common helpers.** Replace local `ui_call`, `rich` and `defer_action` with
   `crate::ui::dialogs::common::{facade, defer}`. Replace both `Deferred`/`install_frame_tick` copies
   with `common::defer`, passing each caller's **original** prefix and callable name.
   **Keep the names `campaign/util.rs::{Deferred, install_frame_tick}`.** They are `pub(crate)`,
   re-exported by the `batch_campaign2.rs` shim, and used by `editor/editor_maps.rs` (H05c, not yours).
   So `campaign/util.rs` keeps `pub(crate) use crate::ui::dialogs::common::defer::Deferred;` and a
   thin wrapper `pub(crate) fn install_frame_tick(dialog: &mut MindDialog, key: &str, deferred: Deferred, tick: impl FnMut() + 'static)`
   that calls the common fn with `"_campaign2_frame_"` and `"campaign2_frame"`. The same rule (RULES §4)
   covers any other helper here that is used outside your files; grep before you delete.
3. **Kit helpers.** Replace `content_texture` + `icon_rect` with F1's `kit::…::content_icon` (only if F1
   added it; else move one copy to `dialogs/campaign/util.rs` and delete the other). Replace
   `spacer` with one copy. Do **not** let `ui/planet/` adopt `content_icon`: it uses a different prefix
   order (item, block, unit, liquid at 1799).
4. **Near-identical helpers.** For `connect_once`, `setting_*` and `bundle_or`: merge only if the
   diff is whitespace or naming. Otherwise keep both and rename one after its behaviour (for example
   `row_str_any` vs `row_str_gstring`).
5. **Logic duplicates.** Move the byte-equivalent stacks fetch and the "sector being played" check
   into `dialogs/campaign/util.rs` as `pub(in crate::ui::dialogs)` fns. Call them from both dialogs.
6. **Variant helpers.** Switch to `crate::gdutil::variant::*` wherever F1's catalogue maps a local copy
   to a helper with identical semantics. Use F1's catalogue table in `docs/PLANS/reports/F1.md`.
7. **Close buttons.** `add_close_button(210.0)` → `add_default_close_button()` (F1).
8. **`ui/planet/rows.rs`.** Leave it. Its `row_*` family differs, and T02 replaces it with typed views.

## Not in scope

- The JSON boundary (`campaign_section`, `campaign_json`, `row_*` reads of `MindCampaign` JSON). That
  is T01/T02 in W4.
- Any change to meta-key strings or dialog ids.

## Verification (RULES §6)

```bash
python3 tools/refactor/api_surface.py | diff /tmp/H04-surface-before.txt -
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib dialogs
run --cargo-test -p mind-gdext --lib planet
```

Orchestrator post-merge: `dialog_sweep_check`, then an MCP click-through of the planet dialog, research
dialog, launch loadout and sector stats (open, interact, close).
