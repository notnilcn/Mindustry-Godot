<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S01 — Split `MindCampaign` (`mind-gdext/src/campaign.rs`, 8325 lines) into `campaign/`

**Wave:** W1 (pure mechanical split, RULES §3). **Priority:** first.
**Owns:** `client/rust/mind-gdext/src/campaign.rs` and the new `client/rust/mind-gdext/src/campaign/**`.
**Effort:** large but mechanical: about 7,000 lines move.

## What the file is

`MindCampaign` is a Godot `Node` (declared in `client/scenes/game.tscn`). It is the GDScript- and
MCP-callable facade over `mind_core::game` (campaign state, play flows, research, saves, launch,
planet view data). It has:
- one struct, `MindCampaign` (272–349);
- an `INode` impl (351–413);
- **one** `#[godot_api] impl` block (573–7248) with **104 `#[func]`s and 2 `#[signal]`s**
  (`campaign_complete`, `block_unlocked`, at 1319–1326);
- about 30 free fns (7250–7645);
- tests (7646–8325).

## Target layout

Keep **`campaign.rs` as the module root**, with a sibling directory `campaign/`, the same style as
`fx.rs` + `fx/`. `lib.rs` then stays untouched (`mod campaign;` at line 13,
`pub use campaign::MindCampaign;` at line 37).

`campaign.rs` keeps:
- the header, all imports (1–85), and `mod` declarations for the children;
- `struct MindCampaign` (272–349), unchanged;
- `impl INode for MindCampaign` (351–413);
- the **primary** `#[godot_api] impl MindCampaign` block with only the two `#[signal]` fns
  (1319–1326) and `bootstrap`/`bootstrap_with` (578–684);
- `fn collect_unlocked` (7291–7300), used by 4 groups.

`events` and `fired_events` are created and the listeners installed in `init`. Keep it there;
moving it to `bootstrap` would double-record every row.

Every child file:
- has the two header lines copied from `campaign.rs`, plus `//! <purpose>` and `//! Upstream: <java>`;
- starts with `use super::*;`;
- holds a `#[godot_api(secondary)] impl MindCampaign { … }` for its `#[func]`s, and a plain
  `impl MindCampaign { … }` for its private helpers;
- has its free fns at the end, before its tests.

Visibility follows RULES §3.2. Every private method or free fn called from another file becomes
`pub(super)`. The crate-only `pub fn`s (`start_loaded_world`, `schematic_at`) stay `pub`.

### Destination table

Ranges are at `291e30a8`. **F** = `#[func]`, **p** = private method, **free** = free fn or type.

| File | Items (start line) | Upstream (`core/src/mindustry/…`) |
|---|---|---|
| `campaign/events.rs` | free: `FiredEvent`/`FiredEvents` 87–162 (the field `counts` is read by `fired_event_counts_json`, so keep both in this file), `event_content_detail` 166, `event_content_of` 180, `install_event_listeners` 197–257, `dispatch_event_rows` 260. F: `fire_player_event` 3581, `take_fired_events` 3596, `fired_event_counts_json` 3611, `fired_event_total` 3622, `fired_event_count` 3628, `fired_event_pending` 3634, `clear_fired_events` 3640 | `game/EventType.java`; `core/Control.java` (`Events.on`) |
| `campaign/service.rs` | free: `load_service` 450, `save_service` 458, `service_achievement_rows_json` 465, `service_stat_rows_json` 484 (**same names as two `#[func]`s**: call the free fns as `self::service_…` or keep both in this file so that resolution is unchanged). p: `sync_service` 697, `persist_service` 709. F: `service_achievement_rows_json` 720, `service_stat_rows_json` 727, `service_completion_json` 735, `service_unlocked_json` 748 | `service/GameService.java`, `game/Achievement.java`, `SStat.java` |
| `campaign/runtime.rs` | p: `host` 687, `ui_has_dialog` 790, `persist` 870, `with_campaign_mut` 894, `mirror_runtime_campaign` 917, `push_campaign_to_runtime` 3532 | none (port-only facade seam: host ↔ `CampaignRuntime`) |
| `campaign/pump.rs` | p: `pump_runtime` 936–1283, `apply_capture_bookkeeping` 1287–1315, `toast_campaign_events` 2429–2472. F: `capture_sector` 2377, `run_turn` 2476, `run_wave` 2709 | `core/Control.java` (`update`), `core/Logic.java` (`sectorCapture`, `runWave`), `game/Universe.java` (`runTurn`), `ui/fragments/HudFragment.java` (toasts) |
| `campaign/sector_launch.rs` | p: `sector_save_file` 1336. F: `prime_sector_load` 1360, `start_sector` 1402, `start_sector_with_loadout` 1408–1975. free: `adopt_map_rules` 7250–7288, `parse_loadout_stacks` 7549, `parse_launch_payload` 7562 | `core/Control.java` (`playSector`, `playNewSector`), `core/World.java` (`loadSector`), `game/SectorInfo.java` |
| `campaign/map_play.rs` | F: `start_map_mode` 1988–2114, `playtesting_map` 2197, `start_map` 2345, `start_joined_world` 2353. pub (Rust-only): `start_loaded_world` 2120–2192. p: `resolve_map` 2277. free: `save_date_stamp` 7384 | `core/Control.java` (`playMap`), `core/World.java` (`loadMap`) |
| `campaign/text.rs` | p: `show_error` 2291, `show_error_key` 2298, `show_error_text` 2303, `set_menu_visible` 2336, `push_unlock_toast` 2994, `localized_content_name` 3379, `localized_content_description` 3390, `localized_content_field` 3395, `content_emoji` 3407, `emoji_content_name` 3428, `sector_localized_name` 3440, `planet_localized_name` 3476, `invalidate_ui_block_catalog` 3895. F: `sector_name_localized` 3451. free: `resolve_preset_description` 422, `content_icon_name` 7411, `sector_toast_name` 7435 | `ctype/UnlockableContent.java` (`localizedName`), `core/UI.java` (`showErrorMessage`), `ui/fragments/HudFragment.java` (`showUnlock`) |
| `campaign/research.rs` | type alias `SectorInventory` 85. F: `research` 2795–2986, `research_tree_json` 3072–3365, `content_unlocked` 3549, `take_unlocked_content` 3563, `get_tech_state` 3736. p: `research_inventory` 3014, `tree_title` 3368, `research_parent_unlocked` 3422, `objective_display` 3486. free: `root_planets` 7305 | `ui/dialogs/ResearchDialog.java`, `game/Objectives.java` |
| `campaign/database.rs` | F: `content_info_json` 3652, `block_catalog_json` 3790, `database_json` 3909. p: `block_visibility_context` 3845. free: `render_stat_cells` 7422 | `ui/dialogs/ContentInfoDialog.java`, `ui/dialogs/DatabaseDialog.java`, `ui/fragments/PlacementFragment.java` |
| `campaign/rules.rs` | F: `apply_planet_rules` 2205, `planet_default_env` 2228, `default_env` 2243, `rules_planets` 2258, `get_rules_json` 2724, `set_rules_json` 2741, `set_campaign_rule` 5979, `get_campaign_rules_json` 6055, `set_campaign_rules_json` 6079 | `ui/dialogs/CustomRulesDialog.java`, `ui/dialogs/CampaignRulesDialog.java`, `type/Planet.java` (`applyRules`) |
| `campaign/planet_view.rs` | free: `OUTLINE_RAD` 81, `PlanetBodyRow` 494, `planet_body_rows` 517. F: `universe_json` 2539, `planet_light` 2584, `planet_bodies` 6753, `planet_view` 6785–6931, `planet_clouds` 6939, `planet_mesh` 6988–7097. p: `universe_secondsf` 2569 | `graphics/g3d/PlanetRenderer.java`, `graphics/g3d/HexMesh.java`, `HexSkyMesh.java`, `game/Universe.java` |
| `campaign/sectors.rs` | F: `sector_record_json` 2610, `set_launch_destination` 2660, `campaign_views_json` 3938–4059, `get_sector_state` 4410–4534, `take_serpulo_remaps_notice` 5075, `abandon_sector` 5465, `abandon_sector_at` 5527, `take_new_presets` 6644, `set_sector_name` 6681, `set_sector_icon` 6709. p: `sector_record_value` 2636 (associated fn), `clear_played_sector_record` 5485. free: `clear_sector_record` 7632 | `ui/dialogs/PlanetDialog.java`, `type/Sector.java`, `world/blocks/campaign/LaunchPad.java` |
| `campaign/launch.rs` | assoc fns: `item_on_planet` 817, `preset_launch_loadout` 826 (also called from `campaign_views_json`, so make it `pub(super)`). F: `get_launch_resources` 6134, `get_launch_resource_stacks` 6155, `set_launch_resources` 6186, `update_launch_resources_json` 6222, `launch_dialog_json` 6249–6553, `get_last_loadout` 6556, `update_loadout` 6567, `clear_loadout_info` 6585, `remove_sector_items` 6599. free: `variant_amount` 7451, `launch_core_name` 7467, `launch_requirements` 7493, `launch_total` 7515, `schematic_placeable_on_planet` 7531 | `ui/dialogs/LaunchLoadoutDialog.java`, `ui/dialogs/PlanetDialog.java` (`playSelected`), `game/Universe.java` |
| `campaign/accelerator.rs` | F: `accelerator_launch_to` 5123, `accelerator_launch_pending` 5175. p: `accelerator_fx` 5180, `accelerator_sound` 5187, `accelerator_lightning` 5205, `accelerator_dust` 5239, `accelerator_white_fade` 5293, `pump_accelerator_launch` 5318–5457. free: `PendingAcceleratorLaunch` 7582 (**`pub(super)`: it is a field type of the struct in `campaign.rs`**), `accelerator_sequence` 7613, `ACCELERATOR_DUST_RINGS` 7619, `dust_ring_at` 7623 | `world/blocks/campaign/Accelerator.java` |
| `campaign/saves.rs` | p: `write_live_slot_preview` 757, `render_slot_preview_from_file` 769, `save_paths` 5824, `save_slot_by_name` 5832. F: `save_slot` 4686, `save_preview_texture` 4730, `load_slot` 4743, `get_total_playtime` 4790, `current_save_is_autosave` 4798, `list_save_slots` 5758, `import_save` 5843, `delete_save_slot` 5889, `rename_save_slot` 5915, `current_save_autosave` 5935, `set_slot_autosave` 5943, `export_save_slot` 5958, `current_save_playtime_ms` 7142. free: `apply_loaded_meta` 7323, `sanitize_slot_name` 7361, `adopt_loaded_slot` 7372 | `game/Saves.java`, `ui/dialogs/LoadDialog.java`, `ui/dialogs/SaveDialog.java` |
| `campaign/lifecycle.rs` | p: `save_outgoing_slot` 4820, `persist_sector_record` 4969, `persist_sector_record_in` 4984–5068, `runtime_or_facade_record` 5091, `clear_campaign_saves_for` 5697. F: `save_outgoing_sector` 4853, `run_exit_save` 4861, `check_playtest` 4897, `reset_to_menu` 4932, `selectable_planets` 5564, `clear_saves` 5588, `clear_research` 5613, `clear_planet_research` 5638, `clear_campaign_saves` 5674, `clear_planet_campaign_saves` 5680, `content_error` 7100, `reload_from_disk` 7107, `clear_in_memory` 7116 | `ui/dialogs/PausedDialog.java` (`runExitSave`, `checkPlaytest`), `core/Logic.java` (`reset`), `ui/dialogs/SettingsMenuDialog.java` (clear data) |
| `campaign/objectives.rs` | F: `complete_objective` 4062, `get_objective_state` 4085, `objective_runtime_debug` 4109, `objective_live_stats_json` 4138–4253, `objective_markers_json` 4266–4387, `fog_query` 4539. p: `localized_marker_text` 4392 | `game/MapObjectives.java`, `core/Renderer.java` (~352–360 markers) |
| `campaign/schematics.rs` | p: `load_schematics` 804. pub (Rust-only, used by `input/mod.rs:1762`): `schematic_at` 4556. F: `list_schematics` 4565, `write_schematic_selection` 4591, `place_schematic_base64` 4646 | `game/Schematics.java` |
| `campaign/hud.rs` | F: `game_stats_json` 7130, `campaign_active` 7159, `get_hud_state` 7167 | `ui/fragments/HudFragment.java`, `ui/dialogs/GameOverDialog.java` |
| `campaign/tests/…` | see "Tests" below | – |

If at HEAD you find a method that is in neither the table nor the file, place it by topic and record it (RULES §8).

## Order of work (each step is one commit)

1. **Pilot `accelerator.rs` first**, the most self-contained group. Create `campaign/accelerator.rs`
   and add `mod accelerator;` to `campaign.rs`. Move the items, wrap the two `#[func]`s in
   `#[godot_api(secondary)] impl MindCampaign { … }` and the private ones in `impl MindCampaign { … }`.
   Make `PendingAcceleratorLaunch` `pub(super)` and import it in `campaign.rs`
   (`use accelerator::PendingAcceleratorLaunch;`). Then run `run --cargo-check -p mind-gdext`
   and the scoped clippy. **This is the first `#[godot_api(secondary)]` in the crate.** If it does
   not compile, stop and report the error verbatim; do not work around it.
2. Then, one commit each: `events.rs`, `service.rs`, `text.rs`, `runtime.rs`, `hud.rs`,
   `schematics.rs`, `objectives.rs`, `database.rs`, `rules.rs`, `planet_view.rs`, `saves.rs`,
   `lifecycle.rs`, `launch.rs`, `sectors.rs`, `research.rs`, `map_play.rs`, `sector_launch.rs`, `pump.rs`.
   Run `--cargo-check -p mind-gdext` after each.
3. Tests (next section).
4. Final pass: remove imports from `campaign.rs` that are now unused there. Each child's
   `use super::*;` still sees them only if they stay in the parent; prefer moving an import into
   the one child that uses it. Clippy decides.

## Tests (7646–8325)

Create `campaign/test_util.rs` (`#[cfg(test)]`) with `require` (7654) and `require_ok` (7662).
Then put each test in a `#[cfg(test)] mod tests { use super::*; use super::super::test_util::*; … }`
at the bottom of the file that owns the code under test. mind-gdext has no crate-level test
allow, so keep the existing `#![allow(clippy::unwrap_used, clippy::expect_used)]` (or whatever
the current module has) on each new test module.

| Test (start line) | Goes to |
|---|---|
| `preset_description_falls_back_to_the_live_bundle` 7672, `preset_description_prefers_the_registry_value` 7695 | `text.rs` |
| `preset_map_rules_carry_objectives_and_wave_timer` 7720 | `sector_launch.rs` |
| `loaded_slot_becomes_current_and_autosaves` 7766 | `saves.rs` |
| `launch_cost_prices_the_selected_schematic_not_the_default_core_list` 7812, `launch_core_follows_destination_allow_launch_schematics` 7869, `launch_grid_skips_blocks_not_buildable_on_the_planet` 7911 | `launch.rs` |
| `service_injection_roundtrips_through_the_unlock_store` 7966, `service_rows_json_cover_every_stat_and_achievement` 8022 | `service.rs` |
| `planet_bodies_derive_the_solar_system_in_the_viewed_frame` 8073 | `planet_view.rs` |
| `campaign_bus_dispatches_the_play_flow_outbox` 8150 (also uses `play_map`), `campaign_bus_routes_game_and_sim_rows_with_payloads` 8182, `research_payload_covers_item_liquid_and_unit_only` 8223, `fired_events_record_is_bounded_and_counted` 8246 | `events.rs` |
| `accelerator_cutscene_timeline_matches_the_java_schedule` 8272, `accelerator_burst_builds_twenty_lightning_polylines` 8301 | `accelerator.rs` |

The test paths change from `campaign::tests::x` to `campaign::<file>::tests::x`. Nothing outside
the crate cites these paths; grep `TWIN-VERIFICATION.json` for `campaign::tests` to confirm, and
list any hit for C01.

## Hazards (read before moving)

- **Private across files.** Every helper called from more than one new file needs `pub(super)`.
  Known cross-file helpers: `host`, `persist`, `with_campaign_mut`, `mirror_runtime_campaign`,
  `push_campaign_to_runtime`, `push_unlock_toast`, `show_error*`, `set_menu_visible`,
  `localized_*`, `sector_localized_name`, `planet_localized_name`, `content_emoji`,
  `save_paths`, `load_schematics`, `persist_sector_record*`, `sync_service`, `persist_service`,
  `invalidate_ui_block_catalog`, `write_live_slot_preview`, `research_inventory`,
  `objective_display`, `sector_save_file`, `save_outgoing_slot`, `item_on_planet`,
  `preset_launch_loadout`, `sector_record_value`, `collect_unlocked`, `adopt_map_rules`,
  `apply_loaded_meta`, `adopt_loaded_slot`. `Self::item_on_planet(...)`-style calls keep working
  once the fn is `pub(super)`.
- **Name collisions.** These free fns share names with `#[func]` methods:
  `service_achievement_rows_json` and `service_stat_rows_json`. A method call `self.x()` and a free
  call `x()` resolve differently. Keep the free fns in `service.rs` next to the methods, so every
  call keeps resolving to the same item.
- **No logic edits.** `pump_runtime`, `start_sector_with_loadout` and `launch_dialog_json` rely on
  disjoint field borrows, `take()` windows and statement order. Move them whole; W3 (X01)
  decomposes them.
- **Do not move** `install_event_listeners` out of `init`'s call path, and do not reorder `init`.
- **Allowed doc fixes** (02-FINDINGS #14): the stale `start_sector` doc (1398–1401, it mentions a
  `loadout_json` argument that doesn't exist), the two conflicting paragraphs at 3526–3531 (keep
  the one that matches the code) and the duplicated doc on `parse_launch_payload` (7557–7561).
  Nothing else.

## Verification (RULES §6), plus these specifics

```bash
python3 tools/refactor/api_surface.py | diff /tmp/S01-surface-before.txt -          # must be empty (104 funcs, 2 signals)
python3 tools/refactor/move_check.py --base $(git merge-base main HEAD) \
    --old client/rust/mind-gdext/src/campaign.rs \
    --new client/rust/mind-gdext/src/campaign.rs client/rust/mind-gdext/src/campaign
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib campaign
wc -l client/rust/mind-gdext/src/campaign.rs client/rust/mind-gdext/src/campaign/*.rs   # paste into the report
```

The expected file sizes are 200–1300 lines; `sector_launch.rs` and `pump.rs` are the largest, because
their giant fns move whole. `campaign.rs` should end up under 600 lines.

Orchestrator post-merge: a headless `game.tscn` boot (`run --godot --headless --path client --quit`)
with no `ERROR:` lines, then `dialog_sweep_check`.
