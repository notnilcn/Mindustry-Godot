<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W3-X01 — Make `MindCampaign` a thin facade: adopt the core APIs, decompose the giant functions

**Wave:** W3. **Needs:** H01, H02 and H03 merged. It may start while the other W2 packages are still running.
**Owns:**
- `client/rust/mind-gdext/src/campaign.rs` and `campaign/**`;
- `client/rust/mind-core/src/game/**` (only to delete copies that became dead or to adopt H01↔H02 helpers in each other's files).

**Effort:** large. This is the central campaign refactor. Work in many small commits, and verify
after each group.

## Inputs

- `docs/PLANS/reports/H01.md` and `H02.md`: the new core APIs, with the facade sites each replaces.
- `docs/PLANS/reports/H03.md`: the facade helpers that now exist, and the take/put sites H03 skipped.
- `docs/PLANS/02-FINDINGS.md` #1–#4: behaviour quirks you must **preserve**.

## Part 1 — Adopt the core APIs (one commit per API family)

For each H01/H02 API, switch every facade site the report lists. Before you switch a site,
re-read the site and the core fn side by side, and confirm they do the same thing **including
early exits, log lines and toast keys**. Logs and toasts stay in the facade. Then delete the
facade's copy: the private fn, or the inline block.

| Family | Core API | Facade sites (at `291e30a8`, before S01) |
|---|---|---|
| lookups | `Campaign::planet_name`, `sector_by_name(_mut)`, `sector_key` | ~10 planet-name sites, 12 `planet_id_by_name` sites |
| sector edits | `Campaign::edit_sector`, `save_sector_info` | 1175, 1822, 2687, 2945, 5552, 6631, 6661, 6697, 6733; 1165–1180 |
| records | `Sector::record_json`, `clear_base`, `abandon`; `play::live_sector_record` | `sector_record_value`, `clear_sector_record`, 5545–5553, `persist_sector_record_in` |
| boot / clear | `Campaign::boot`, `clear_saves`, `clear_being_played` | `bootstrap_with` 636–661, `clear_campaign_saves_for`, 1632/4944/5729 |
| rules | `Campaign::set_campaign_rules` + `play::reapply_planet_rules` | `set_campaign_rule`, `set_campaign_rules_json` |
| unlocks | `tech_tree::apply_pending_unlocks`, `check_campaign_auto_unlocks`, `unlocked_content_names` | `pump_runtime` 1103–1167, the three auto-unlock blocks, `collect_unlocked` |
| research | `tech_tree::{root_planets, node_for_content_name, research_inventory, plan_research_removal}` | `research`, `research_inventory`, `root_planets` |
| play flow | `play::{capture_replacements, adopt_map_rules, select_sector_launch, begin_sector_launch}`, `PlaySession::apply_save_meta` | `pump_runtime` 1027–1044, `adopt_map_rules`, `start_sector_with_loadout`, `apply_loaded_meta` |
| runtime | `CampaignRuntime::{install, turn_context, run_due_turn, run_turn_now}`, `runtime::apply_resume_record` | 1752–1758 / 2131–2137, 1203–1238, 2477–2537, 1800–1846 |
| launch | `game::launch::{LaunchPayload, launch_core_name, launch_requirements, schematic_placeable_on_planet, item_on_planet, preset_launch_loadout_filter}`, `Campaign::launch_plan_full`, `Universe::launch_resource_stacks`, `sector::{dense_from_names, stacks_to_pairs, fill_module}` | `launch.rs` free fns, `launch_dialog_json`, `get_launch_resource_stacks`, 6170–6177/6389–6401/6431–6438, 5405–5416, 1826–1834 |

Also inside `mind-core/src/game`:
- adopt H01's `planet_name`/`sector_key` in `play.rs` (433–436, 512–515);
- adopt H01's `fill_module`/`stacks_to_pairs` in `play.rs:397–409/401` and in `runtime/session.rs`;
- delete any core fn that is now unused (grep).

Move the facade tests that only exercised moved logic (`launch_cost_*`, `launch_core_*`,
`launch_grid_*`, `preset_map_rules_*`, `loaded_slot_becomes_current_*`) to mind-core next to the core
fn, **only if** H01/H02 did not already write equivalent tests. Otherwise delete the facade copy.
Name the replacement test in the commit message.

### Behaviour you must preserve (do not fix here)

- **02-FINDINGS #1.** On capture and game-over, `persist_sector_record_in` runs while `self.registry` is
  taken and silently skips the record write. When `pump_runtime` calls the new
  `play::live_sector_record(…, registry, …)` from inside that window, you **must** keep today's
  skip: guard the call with the same `self.registry.is_none()`-equivalent condition, and add the
  comment `// preserves 02-FINDINGS #1; fix separately`. Fixing it changes saved data and needs its own test.
- **02-FINDINGS #2.** The facade's `capture_sector` **keeps its own body**. Do not route it through
  `runtime::capture_played_sector`, which would add `service.on_sector_capture`.
- **02-FINDINGS #3/#4.** `sector_toast_name` and the `launch_dialog_json` display-name copy keep their
  bodies. Only `ui/campaign.rs:917`-identical sites may use `Sector::display_name`.
- **GameOver / SectorLose `being_played = false`** (1070–1076). The analysis says it is redundant with
  `play::apply_sector_loss` (`play.rs:861`). Delete it **only if** you can show that both write the
  **same** `Campaign` instance (runtime copy vs facade mirror). Otherwise keep it, with a comment.

## Part 2 — Decompose the giant functions

These were moved whole by S01. Split each into named private steps, in a plain `impl MindCampaign`
in the same file. **Each piece takes explicit `&mut` parameters** for the locals it touches
(`registry`, `campaign`, `runtime`, `host`). These functions run with `self.registry`/`self.campaign`
taken out, so a `&mut self` helper that reads `self.registry` would see `None`. That is exactly bug #1's
trap. Keep the statement order. After each function, run the gdext tests and the campaign smoke test.

| Function (file) | Pieces |
|---|---|
| `pump_runtime` (`pump.rs`, ~350 lines) | `drain_event_outbox`; `pump_saves_policy(host, &mut runtime) -> (SavesUpdate, SavesUpdateResult, u64, bool)`; `handle_play_events(&mut self, host, &mut runtime, reg: Option<&mut ContentRegistry>, &events, tick) -> bool`; `resolve_pending_unlocks(&mut runtime, &mut ContentRegistry) -> (Vec<ContentRef>, bool)` (via `apply_pending_unlocks`); `flush_pending_info_saves(&mut runtime) -> bool`; `run_due_turn(&mut self, &mut runtime) -> bool` (via `CampaignRuntime::run_due_turn`); `mirror_session_from(&mut self, &runtime, full: bool)` |
| `start_sector_with_loadout` (`sector_launch.rs`, ~570 lines) | It builds a `SectorLaunch` (from `play::select_sector_launch`), then `validate_sector_launch`, `discard_corrupt_resume`, `begin_sector_launch` (core), `replay_lost_sector`, `LaunchPayload::parse` (core), `adopt_loaded_world_state` (meta + rules + weather + objectives), `carry_team_plans`, `install_sector_runtime` (via `CampaignRuntime::install` + `apply_resume_record`), `finish_launch` (**shared with `start_loaded_world`**: spawn, landing cutscene, `last_pump_tick` reset; the bodies at 1878–1926 vs 2154–2190 are identical), `bind_sector_slot` |
| `research` (`research.rs`, ~190) | find ref, inventory, spend, ResearchEvent, deduction (core `plan_research_removal`), record updates (`edit_sector`), auto unlocks, finish |
| `research_tree_json` (`research.rs`, ~290) | `tree_order`, `tree_visibility`, `tree_bounds`, `research_items_json`, `research_roots_json`, `research_node_json`, `node_progress` (free). T01 then turns these into a typed `ResearchTreeView`; name the pieces so that T01 can reuse them |
| `launch_dialog_json` (`launch.rs`, ~300) | Replace the 8-field tuple snapshot (6272–6323) with a `LaunchSectorSnapshot` struct. Pieces: `select_loadout`, `launch_resources_on_planet`, `persist_planned_resources`, `loadout_rows`, `nonzero` (free). It holds `self.registry.as_ref()` across `self.campaign.as_mut()` and `self.host()` (see its comment at 6424–6428), so the pieces must take field references, not `&mut self` |
| `objective_live_stats_json` (`objectives.rs`) | `unit_type_count(&self, unit, team)` (4170–4187), `destroy_blocks_progress(&self, obj, block_id)` (4194–4228). Do **not** hoist the per-objective `unit_debug_json` fetch here; that is 02-FINDINGS #8, a perf change for later |
| `objective_markers_json` (`objectives.rs`) | free `marker_row(marker, &dyn Fn(&str) -> String) -> Option<Value>` per `match` arm |
| `planet_view` (`planet_view.rs`) | `grid_arrays`, `load_preset_remap`, `orbit_rows` (H03 added `vec3_array`/`rgba8`) |
| `get_sector_state` (`sectors.rs`) | `sector_state_rows` plus the dict shorthand |

**Target:** no function in `campaign/**` is longer than about 120 lines, and `campaign/**` totals at
least 30% less than after S01. Report `wc -l` before and after.

## Verification

```bash
python3 tools/refactor/api_surface.py | diff /tmp/X01-surface-before.txt -          # empty: every #[func] kept
run --cargo-check --workspace --all-targets
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml --workspace --all-targets -- -D warnings
run --cargo-test -p mind-core --lib game::
run --cargo-test -p mind-gdext --lib campaign
run --cargo-test -p mind-headless
run --cargo-run -q -p mind-headless -- run-all --tier T0
run --cargo-run -q -p mind-headless -- parity goldens
run --goldens --check
```

**Campaign smoke test** (the orchestrator runs it post-merge; you run it too if you can build the
client): use `bash tools/godot.sh` with the MCP addon, or `tools/session-play.sh`. Use a throwaway
profile, never the user's Java profile or default data dir.

1. Start a new Serpulo campaign and land on Ground Zero. `get_sector_state` shows the sector as played.
2. Wait for one wave, then `run_wave`. Save the slot, quit to the menu (`reset_to_menu`), and load the slot.
   The wave count and core items survive.
3. Open research and research one cheap node. The item pool shrinks; `get_tech_state` shows it unlocked.
4. Open the planet dialog. `launch_dialog_json` for Frozen Forest lists the resources and the cost, and
   `valid` matches the item pool.
5. `capture_sector`. The sector is captured; toast and record are as before.
6. Compare `campaign_views_json`, `get_tech_state` and `launch_dialog_json` output with the same steps on
   the pre-X01 build. Expect them to be identical, apart from playtime fields.

Paste the step results into the report.
