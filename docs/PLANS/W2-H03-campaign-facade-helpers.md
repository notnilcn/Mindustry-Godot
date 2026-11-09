<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W2-H03 — `MindCampaign` facade: Godot-side helpers and boilerplate removal

**Wave:** W2 (helper package, RULES §4). **Priority:** first. **Needs:** W1 S01 and W0 F1 merged.
**Owns:** `client/rust/mind-gdext/src/campaign.rs` and `client/rust/mind-gdext/src/campaign/**`.
**Effort:** medium.

## Scope

This package removes **Godot-side** repetition only: host and runtime access, `GString`/JSON, save
paths, toasts and settings flushes. It must **not** move campaign rules into mind-core, and it must
not adopt H01/H02's new APIs (they are not on your branch). X01 does both in W3.

So leave these facade copies alone even though they look duplicated:
- the auto-unlock block;
- `TurnContext` building;
- the "edit sector, then `save_info`" sites;
- the launch maths;
- `adopt_map_rules`, `apply_loaded_meta`, `collect_unlocked`.

Facade line numbers are at `291e30a8`. Find code by function name in `campaign/<file>.rs`.

## Helpers to add (in `campaign/runtime.rs` unless noted) and the sites to switch

| # | Helper | Replaces | Count | Caveats |
|---|---|---|---|---|
| 1 | `pub(super) fn with_host_runtime<R>(host: Option<Gd<MindSimHost>>, f: impl FnOnce(&mut CampaignRuntime) -> R) -> Option<R>`. A **free fn**, so the closure can still borrow `self` | `take_campaign_runtime()` … `put_campaign_runtime(..)` pairs | 23 sites: 899, 921, 944, 1797, 2143, 2380, 2483, 2521, 2548, 2622, 2713, 2766, 3539, 4068, 4118, 4146, 4768, 4971, 5097, 5468, 6041, 6122, 6441 | Switch a site **only** if (a) the take/put brackets a block that does not use the host binding in between, (b) "no runtime" makes the same early exit as `None`, and (c) put happens on every path. `pump_runtime` (944) and `objective_live_stats_json` (4146) call host methods while the runtime is out, which the comments call the "runtime-out window"; **leave them**. Keep a list of the sites you switched and those you skipped, with the reason |
| 2 | `pub(super) fn read_live_campaign<R>(&self, f: impl Fn(&Campaign) -> R) -> Option<R>`: the runtime copy if installed, else the facade copy | `universe_json` 2540–2565, `sector_record_json` 2617–2629, `runtime_or_facade_record` 5091–5113 | 3 | the bodies must choose runtime vs facade in exactly the same way |
| 3 | `fn push_session_rules_to_runtime(&self)` | 6040–6046 = 6121–6127 (identical) | 2 | |
| 4 | `pub(super) fn planet_def(&self, name: &str) -> Option<&PlanetDef>` and `pub(super) fn planet_id_by_name(&self, name: &str) -> Option<PlanetId>` (on the registry) | `registry.planet_by_name(&…)` at 10 sites (2214, 2234, 2590, 5537, 5645, 5686, 6760, 6792, 6945, 7002) | 10 | the campaign-side `planet_id_by_name` (12 sites) is replaced in X01 by H01's `sector_by_name`; leave it |
| 5 | `pub(super) fn sector_arg(sector: i64) -> Option<u16>` with the body **`if sector < 0 { None } else { Some(sector as u16) }`** | `if sector < 0 { return …; }` followed by `sector as u16` | 7 / 9 sites | Do **not** use `u16::try_from`: `as u16` wraps values above 65535, and that must stay identical. Only switch sites that do exactly the check and then the cast |
| 6 | `gdutil::json::{empty_object, value_gstring, to_gstring}` (F1) | `GString::from("{}")` ×22; `GString::from(x.to_string().as_str())` at 2631, 3364, 3618, 3732, 4252, 4386, 6178, 6552, 7135; `serde_json::to_string(..).unwrap_or_else(\|_\| "{}")` at 2728, 3836, 3925, 4056, 6068 | ~36 | Match the fallback string per site (`"{}"` vs `"[]"`) |
| 7 | `fn queue_slot_save(&mut self, host: &mut Gd<MindSimHost>, file: &Path, finish_tick: Option<u64>, flush: bool, label: &str)` (`campaign/saves.rs`) | `write_live_slot_preview` + `request_save(GString::from(file.to_string_lossy()))` + `saving_finish_tick = Some(tick + 3)` + log, at 1013–1018, 1053–1058, 1079–1084; the no-tick plus `flush_io` variant at 4838–4844, 4874–4877 | 5 | The statement order inside the helper must be the same as at each site. If the sites order those statements differently, make two helpers or skip |
| 8 | `fn store_service(&mut self)` (`campaign/service.rs`) | 711–714, 1008–1011, 4834–4837 | 3 | no sync, only the store |
| 9 | `fn flush_settings(&self, ctx: &str)` | `settings_hub::save(&self.paths)` + warn at 635, 880, 5065, 5083, 5908, 5926, 5950 | 7 | keep each site's log text by passing it as `ctx` |
| 10 | `fn slot_file(&self, stem: &str) -> PathBuf` and `fn slot_user_path(stem: &str) -> String` (`campaign/saves.rs`) | `paths.saves().join("{slot}.{SAVE_EXTENSION}")` at 2099, 4704, 4780; `"user://saves/{slot}.msav"` at 1962, 4714, 4754 | 6 | |
| 11 | make `list_save_slots` (5765) use `save_paths()` (5824) instead of its own `globalize_path("user://")` | 599, 5765, 5826 | 1 | only if `save_paths()` produces the identical `Paths`; `save_preview_texture` (4734) uses `user_data_root()`, so leave it |
| 12 | `fn delete_save_and_backup(file: &Path)` | 1491–1492, 5510–5517, 5731–5734 | 3 | same order (save, then backup), same error handling |
| 13 | `fn autoload(&self, name: &str) -> Option<Gd<Node>>` and `fn assets_string(&self, method: &str, args: &[Variant]) -> Option<String>` (`campaign/text.rs`) | `get_node_or_null("/root/...")` for MindUi (791, 2304, 3896), MindHud (2447, 3001), MindAssets (2313, 2452, 3408) | 8 | These are autoloads (allowed). Add no new lookups |
| 14 | `fn fail_map(&self, msg: &str) -> bool` (`campaign/map_play.rs`) | `start_map_mode`: `warn + show_error("@map.invalid") + return false` ×5 (1999–2033) | 5 | the three `show_error_key + set_menu_visible(true) + return false` (2060–2073) get a second helper `fail_map_key` |
| 15 | `fn bundle_lookup(&self, key: &str) -> Option<&str>` (`campaign/text.rs`): the "value == key means missing" rule | 3368, 3379, 3395, 4392, 4047 | 5 | |
| 16 | `fn push_unlock_toasts(&self, reg: &ContentRegistry, list: &[ContentRef])` and `fn emit_block_unlocked_if_any(&mut self, list: &[ContentRef])` | 1183–1194, 1303–1314, 2971–2984 | 3 | Keep them as **two calls**. Today's code relies on the `self.registry` borrow ending before `base_mut()` |
| 17 | `fn vec3_array(points: &[[f32; 3]]) -> PackedVector3Array` and `fn rgba8(..)` (`campaign/planet_view.rs`) | 7 `push(Vector3::new(` loops in `planet_view`/`planet_mesh`; 6962, 7061 | 9 | |
| 18 | optional: `dict.set("k", v)` instead of `set(&GString::from("k"), &v.to_variant())` | 39 sites | | newer code already uses this shorthand (6765, 6846). Do it only if it compiles without changing types |

## Not in this package (leave as is; X01 or a fix task)

- The registry `take()` windows (`self.registry.take()` 13×, restores 31×). A generic
  `with_registry_taken` helper is tempting, but the windows are where 02-FINDINGS #1 lives. X01
  decomposes those functions with explicit parameters instead.
- Anything that changes which `Campaign` copy (runtime or facade) is read or written.

## Verification (RULES §6)

```bash
python3 tools/refactor/api_surface.py | diff /tmp/H03-surface-before.txt -     # empty
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib campaign
wc -l client/rust/mind-gdext/src/campaign/*.rs        # report before/after totals
```

Orchestrator post-merge: a campaign smoke test through MCP (or `tools/session-play.sh`, if it
covers it). Start a new Serpulo campaign on Ground Zero, save and quit to the menu, load the save, open
the research dialog, and research one node. Then call `get_tech_state` and `campaign_views_json`
and compare them with the same calls on the pre-merge build.
