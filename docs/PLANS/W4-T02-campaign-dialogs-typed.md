<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W4-T02 — Campaign dialogs consume typed views (no more JSON round trip)

**Wave:** W4, after T01 merged.
**Owns** (paths under `client/rust/mind-gdext/src/ui/`):
- `dialogs/campaign/**` (all files, including `campaign_rules.rs`/`campaign_complete.rs` now);
- `dialogs/maps/**`, `dialogs/schematics/**`, `dialogs/pickers/palette.rs`;
- `planet/**`;
- `kit/widgets/{research_view,research_items_display}.rs`;
- `dialog.rs`;
- `landing.rs`.

**Effort:** large.

## Goal

Each campaign dialog gets a typed `Gd<MindCampaign>` (F1 added `MindDialog::campaign()`) and calls
T01's Rust methods. It reads **struct fields** instead of `row_*("key")` lookups on a reparsed
`VarDictionary`.

## Steps (one dialog per commit)

1. **Cache per show.** On show or rebuild, fetch `campaign.bind().campaign_views("")` (or the planet
   you need) **once**, and keep the `CampaignViews` in the controller state. Each repeated
   `campaign_section(...)` call in loops is replaced by reading the cached struct. This includes
   `schematic_row` through `schematic_file_of`/`schematic_index_by_file` (old bc2 2686/2724), which
   re-read schematics from disk each time (02-FINDINGS #9).
2. **Bind discipline.** Take `campaign.bind()` / `bind_mut()` in a **separate statement**, copy out
   what you need, and drop the guard before you call into the UI or any Godot method that could
   re-enter `MindCampaign`. Today's dynamic `call` has the same re-entrancy hazard, but `bind()` panics
   instead of erroring.
3. **Missing node.** Wherever `campaign()` returns `None`, keep the current behaviour: the empty-default
   view. `dialog_sweep_check` builds dialogs without a campaign.
4. **Per dialog:**
   - **Research:** `research_tree(root)` → `MindResearchView.set_tree_view(&ResearchTreeView)`. This is a new
     **non-`#[func]`** method on `kit/widgets/research_view.rs`; the `#[func] set_tree(VarDictionary)` stays
     for MCP. `research_items_display.rs` (~138) likewise. `ResearchRootSelectDialog` (`campaign/research_root_select.rs`)
     may receive `Vec<ResearchRootRow>` through controller state instead of `context["roots"]`; keep the
     `VarArray` path working if MCP opens it with a context.
   - **Launch loadout and planet launch:** `launch_plan(...)` → `LaunchPlanView`; `launch_resource_stacks()`.
     This removes the duplicated fetchers H04 consolidated.
   - **Sector stats, sector select, planet dialog:** `campaign_views` → `views.sectors.iter().find(..)`
     instead of `sector_stats_find_row`, `planet_find_planet`, `planet_sectors` and the raw `.get(`
     in `sector_select.rs`. Fix the requested-planet filter (02-FINDINGS #5) **only** as its own
     commit, with a note in the report. It is a behaviour fix, so preferably leave it to a separate task;
     never hide it in this refactor.
   - **Planet view (`ui/planet/`):** add a typed `set_sector_views(&[SectorView])` next to the
     `#[func] set_sectors(VarArray)` (ABI, keep it). Switch `dialogs/campaign/planet/view_binding.rs` from
     dynamic `planet_view_call` (old batch_c 911–917) to a typed `Gd<MindPlanetView>`.
   - **`landing.rs`** (~348/404): it already holds a typed `Gd<MindCampaign>` but still parses
     `campaign_views_json`. Switch it to `campaign_views`.
5. **Delete** the `row_*` helpers, `int_of`/`text_of` and `campaign_section`/`campaign_json` uses that
   became dead. Keep `MindDialog::campaign_json` if non-campaign dialogs still use it (grep).
6. **`get_sector_state` consumers** (old bc2 5718, batch_c 1453/3041) → `sector_state()`.

## Verification

```bash
python3 tools/refactor/api_surface.py | diff /tmp/T02-surface-before.txt -     # only allowed: none (set_tree_view/set_sector_views are not #[func])
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib
grep -c 'row_str(\|row_bool(\|row_i64_or(\|row_arr(\|campaign_section(' -r client/rust/mind-gdext/src/ui/dialogs/campaign client/rust/mind-gdext/src/ui/dialogs/maps client/rust/mind-gdext/src/ui/dialogs/schematics   # report before/after
```

Orchestrator post-merge: `dialog_sweep_check`. Then an MCP click-through of every campaign dialog
on a throwaway campaign profile: research (pan, select, research a node), planet (rotate, select a
sector, launch preview), launch loadout (change the loadout, check the cost updates), sector stats,
schematics (open, tag, import an `.msch`), custom game and map play. Each must look and behave the
same as before. Take screenshots before and after, per `parity/AGENTS.md` conventions, into
`runs/<date>-T02/`.
