<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Findings from the refactor analysis — NOT in scope for refactor packages

These issues turned up while mapping the code at `291e30a8`. Most are behaviour or parity bugs.
**No refactor package fixes them**, because a refactor must be behaviour-identical. Each needs its own
change with a failing test first, run as a normal parity task. The exception is #12–#14:
they are doc hygiene, and C01 may fix them.

| # | Where (at `291e30a8`) | Finding | Suggested fix |
|---|---|---|---|
| 1 | `mind-gdext/src/campaign.rs` `pump_runtime` 1025–1181, calls at 1060 and 1089 | `self.registry` is `take()`n for the whole window, but `persist_sector_record_in` (4984–4995) reads `self.registry` and returns early. The sector record write on **capture** and **game over** is therefore silently skipped. | Pass the registry explicitly into the record writer. After X01 this is `play::live_sector_record(…, registry, …)`. |
| 2 | `campaign.rs` `capture_sector` 2378–2421 | It re-implements the capture arm of `game_state_tick_system` (`mind-core/src/game/runtime.rs` ~972–1048) but skips `service.on_sector_capture`, so a capture through this path does not count for achievements. | Use the shared `runtime::capture_played_sector` (planned in H02); verify against Java `Logic.checkGameState`/`sectorCapture`. |
| 3 | `campaign.rs` `sector_toast_name` 7435 | Its fallback is `<planet>-<id>`; Java uses the planet name or the bare id. There are three implementations in total (see #4). | `Sector::display_name(&Planet)` (H01) and one call site. |
| 4 | `mind-core/src/ui/campaign.rs:917` `sector_display_name`, `campaign.rs` 6298–6312, `sector_toast_name` | Three diverging sector display-name implementations. | Same as #3. |
| 5 | `mind-gdext/src/ui/dialog.rs:423` | `campaign_section`/`campaign_views` always ask for the active planet, while `ui/dialogs/sector.rs:99` filters by `context.planet`, so the sector-select dialog is empty for any other planet. | Pass the requested planet through. |
| 6 | `ui/dialogs/batch_e.rs` 5508–5518 | `MapLocalesDialog` is an empty stub (`impl DialogController for MapLocalesDialog {}` plus a stray `// __APPEND__`). | Port `editor/MapLocalesDialog.java`. |
| 7 | `mind-core/src/sim/unit_runtime.rs` 8243–9192 | `msav_tests` and `markers_region_tests` are `cfg(all(test, feature = "msav-import"))`, and CI never enables the feature, so they never run. | Add a CI stage with `--features msav-import`, or drop the gate. |
| 8 | `campaign.rs` `objective_live_stats_json` 4139–4253 | `unit_debug_json` is fetched and JSON-parsed once per objective (perf only). | Hoist it out of the loop (safe once covered by a test). |
| 9 | `campaign.rs` `campaign_views_json` 3939 via `load_schematics` 804 | Every call re-reads every `.msch` from disk and scans save slots. Dialogs call it repeatedly per rebuild, including in loops through `schematic_row` (`batch_campaign2.rs` 2686/2724). | T01/T02 cache one typed `CampaignViews` per show. |
| 10 | `ui/dialogs/batch_a.rs:276` vs `batch_campaign2.rs:308` | Two different `open_content_info` implementations. | Pick the Java-faithful one (`ContentInfoDialog.show`). |
| 11 | `client/scenes/editor/*.tscn`, e.g. `map_info_dialog.tscn:6-7` | The scenes set `editor_path`/`routes` properties that `MindDialog` does not export (GDScript leftovers). | Remove them in a scene-hygiene pass (scene ABI owner decides). |
| 12 | `ui/dialogs/simple.rs`, `misc.rs`, `sector.rs` headers | No upstream citation, which breaks the root `AGENTS.md` invariant. | W1 adds `//! Upstream:` lines to the new files; C01 checks. |
| 13 | `mind-core/src/ui/campaign.rs:12-17`, `batch_campaign2.rs:15`, `batch_b.rs` header | Stale docs: they mention "GDScript dialogs" and cite dialogs that moved to `batch_h`. | C01. |
| 14 | `campaign.rs` 1398–1401, 3526–3531, 7557–7561 | Stale or duplicated doc comments (`start_sector` mentions a `loadout_json` argument it does not take; two conflicting paragraphs; a duplicated doc on `parse_launch_payload`). | S01 may fix them (they are listed in S01). |
| 15 | `ui/dialogs/batch_c.rs:351` vs `batch_campaign2.rs:441`, `batch_d.rs:384`, `batch_g.rs:259` | Four `strip_color(s|_tags)` helpers with different semantics (c also strips `[]`, d keeps non-color tags). `mind_core::util::strings::strip_colors` exists. | Decide the Java-faithful one per call site (Java `Strings.stripColors`). |
| 16 | `ui/kit/widgets/factory.rs` vs `ui/PORTING.md` | `PORTING.md` documents `factory::check`, `bar`, `warning_bar`, `items_display`, which do not exist. | C01 fixes the doc (or F1 adds the helpers). |
