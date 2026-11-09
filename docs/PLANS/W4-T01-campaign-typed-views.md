<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W4-T01 — Typed campaign view-models (producer side)

**Wave:** W4, first. **Needs:** X01 merged.
**Owns:**
- `client/rust/mind-core/src/ui/campaign.rs` (it may become `ui/campaign/**`);
- `client/rust/mind-gdext/src/campaign.rs` and `campaign/**`.

**Effort:** medium-large.

## The problem

The campaign dialogs and `MindCampaign` are Rust in the same crate. Yet every campaign view crosses a
**stringly-typed round trip**:

```
typed struct / json!  →  serde_json String  →  GString  →  Godot Json::parse_string  →  VarDictionary
                     →  row_str("key") / row_i64_or("key") / int_of(..)  (floats fixed back to ints)
```

The round trip goes through `MindDialog::campaign_json` (`ui/dialog.rs` ~397–406) and
`campaign_section`/`campaign_views` (~418–429).

This has three costs:
- **No type checking.** A misspelled key returns a default silently.
- **Wasted work.** `campaign_views_json` re-reads every `.msch` from disk and scans the save slots on
  each call, and the dialogs call it repeatedly (02-FINDINGS #9).
- **Hundreds of `row_*` lookups.** At `291e30a8`, `batch_campaign2.rs` alone had 54 `row_str(`,
  26 `row_bool(`, 20 `row_i64_or(` and 26 `row_arr(`.

## The fix

Add typed Rust-only methods that return view structs, and keep every `*_json` `#[func]` as a thin
`serialize(typed)` wrapper (MCP ABI, append-only). T02 then switches the dialogs to the typed methods.

### Structs

Reuse the structs that **already exist** in `mind-core/src/ui/campaign.rs`: `CampaignViews`,
`SectorView`, `SectorStatsView`, `PlanetView`, `SchematicView`, `LoadoutView`, `MapEntryView`,
`CampaignRulesView`, `CampaignCompleteView` and `ResearchView` (the flat list, not the tree).
`PlanetView` there is a different concept from `ui/planet/` in gdext; do not rename it.

Create these, in new files `mind-core/src/ui/campaign/{research_tree,launch_plan,sector_state}.rs`.
Convert `ui/campaign.rs` to `ui/campaign/mod.rs`, keep everything else in `mod.rs`, and re-export the new types.

- **`ResearchTreeView`**: the JSON that `research_tree_json` emits (roots ~3212, items ~3190, reqs ~3306,
  nodes ~3324, view ~3354 at `291e30a8`):
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct ResearchTreeView { pub root: String, pub root_title: String, pub root_type: String, pub root_icon: String,
      pub roots: Vec<ResearchRootRow>, pub items: Vec<ResearchItemRow>, pub bounds: [f32; 4] /* check the shape */,
      pub nodes: Vec<ResearchNodeRow> }
  pub struct ResearchRootRow { pub name: String, pub title: String, #[serde(rename = "type")] pub kind: String, pub icon: String, pub selected: bool }
  pub struct ResearchItemRow { pub item: String, pub amount: i32, pub text: String, pub name: String }
  pub struct ResearchNodeRow { pub index: usize, pub parent: i64 /* -1 = none */, pub children: Vec<usize>, pub content: String,
      #[serde(rename = "type")] pub kind: String, pub icon: String, pub name: String, pub x: f32, pub y: f32,
      pub visible: bool, pub locked: bool, pub selectable: bool, pub can_spend: bool, pub description: String,
      pub inline_description: String, pub progress: f32, pub reqs: Vec<ResearchReqRow>, pub objectives: Vec<String> }
  pub struct ResearchReqRow { pub item: String, pub name: String, pub have: i32, pub need: i32, pub has: bool }
  ```
  **The field names, order and types must reproduce the current JSON exactly.** Read the `json!`
  calls and match them; the sketch above comes from the analysis and may be off.
- **`LaunchPlanView`**: what `launch_dialog_json` emits (~6529–6548):
  `from, from_preset, capacity, allow_launch_loadout, allow_launch_schematics, preset_description, core,
  stored/requirements/resources/total/preset_loadout: IndexMap<String, i32>, max, valid,
  launch_cost: Vec<ItemAmount { item, amount }>, loadouts: Vec<LaunchLoadoutRow { index, name, core, is_default, file }>, selected, selected_file`.
  Item maps are `IndexMap`, whose order only survives serialization because mind-core enables
  `serde_json/preserve_order`. Keep it. Zero entries are omitted by `nonzero`; keep that.
- **`SectorStateView`**: what `get_sector_state` builds today as a hand-made `VarDictionary`.

### Producing them

The builder code needs facade-side text (bundle lookups, `tree_title`, `localized_content_name`,
`objective_display`). Move the **pure** part of each builder into mind-core, and pass the text in as a
trait or closures:
```rust
pub trait CampaignText { fn bundle(&self, key: &str) -> Option<String>; fn content_name(&self, kind: &str, name: &str) -> String; /* … */ }
pub fn research_tree_view(registry: &ContentRegistry, campaign: &Campaign, text: &dyn CampaignText, root: &str, …) -> Option<ResearchTreeView>
pub fn launch_plan_view(…) -> Option<LaunchPlanView>
```
X01 already split `research_tree_json` and `launch_dialog_json` into pieces; reuse those pieces. If a
piece needs the host (live core items), the facade reads that first and passes the data in.

In the facade, add a plain `impl MindCampaign` block (no `#[func]`) in `campaign/views.rs`:
```rust
pub fn campaign_views(&self, planet: &str) -> Option<CampaignViews>;        // same data as campaign_views_json, incl. the playtime + maps patch-ups
pub fn research_tree(&self, root: &str) -> Option<ResearchTreeView>;
pub fn launch_plan(&mut self, /* same args as launch_dialog_json */) -> Option<LaunchPlanView>;   // &mut: it persists, like today
pub fn launch_resource_stacks(&mut self) -> Vec<(String, i32)>;
pub fn sector_state(&self) -> SectorStateView;
```
Each `*_json` `#[func]` becomes `gdutil::json::to_gstring(&self.<typed>(..), "{}")`. `get_sector_state`
keeps returning a `VarDictionary`, built from `SectorStateView` with the same keys and Variant types.

### Proving equivalence (do this first)

1. **Before changing anything**, add a mind-gdext unit test module `campaign/views_snapshot_tests.rs`.
   It calls the **current** pure JSON-building pieces (from X01's decomposition) with a fixture
   registry/campaign: `mind_core::ui::campaign::vanilla_fixture` (`ui/campaign.rs` ~1954), or
   `game::test_support` through a `pub` test helper. It records the `serde_json::Value` outputs for
   the research tree of Serpulo, a launch plan to Frozen Forest, and the sector state.
   `serde_json::Value` needs no Godot engine.
2. Implement the typed path.
3. In the same test, assert `serde_json::to_value(typed) == old_value` for each case.
4. When the facade's `#[func]`s switch to the typed path, delete the old pieces. The test then compares
   against checked-in expected JSON (`campaign/testdata/*.json`), which you write from step 1's
   output.

`CampaignViews` serialization is sha256-pinned (`mind-core/tests/goldens/ui/campaign.json`,
`parity/golden_manifest.json` ~536). **Do not** change its fields or order. Adding `Deserialize` is safe.

## Verification

```bash
python3 tools/refactor/api_surface.py | diff /tmp/T01-surface-before.txt -
run --cargo-test -p mind-core --lib ui::
run --cargo-test -p mind-gdext --lib campaign
run --goldens --check
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml --workspace --all-targets -- -D warnings
```

Orchestrator: the X01 campaign smoke test again, comparing the `*_json` outputs before and after
(they must be identical).
