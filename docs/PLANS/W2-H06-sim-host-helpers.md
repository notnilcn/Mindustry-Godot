<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W2-H06 — `MindSimHost`: accessor helpers and decomposition of the giant view builders

**Wave:** W2 (helper package, RULES §4). **Needs:** W1 S05 and W0 F1 merged.
**Owns:** `client/rust/mind-gdext/src/sim_host/**`.
**Effort:** large. Do it in the commit order below and run `--cargo-check -p mind-gdext` after each commit.

Line numbers are at `291e30a8` (pre-split `sim_host.rs`). Use `docs/PLANS/data/S05-sim-host-methods.tsv`
to find each method's new file.

## Part 1: accessors (mechanical; prefer the compiler's verdict)

| # | Helper (in `sim_host/mod.rs`, plain `impl MindSimHost`) | Replaces | Count | Caveat |
|---|---|---|---|---|
| 1 | `pub(super) fn world(&self) -> &bevy_ecs::world::World { &self.sim.ecs.0 }` and `pub(super) fn world_mut(&mut self) -> &mut World` | `self.sim.ecs.0`: 121 one-line sites plus about 150 rustfmt-split `self\n.sim\n.ecs\n.0` chains | ~270 | A method call borrows **all** of `self`, while a field path borrows only `self.sim`. Where the field-path site also borrows another field of `self` at the same time, the compiler rejects the method; **keep the field path there**. Do this file by file |
| 2 | `pub(super) fn build_at(&self, x: i32, y: i32) -> Option<Entity>` | `.tiles.in_bounds(x, y).then(\|\| self.sim.grid.tile(x, y).build).flatten()` (8×: 2008, 2223, 2256, 2280, …) | 8 | the existing `tile_at` (8105) differs; leave it |
| 3 | `crate::gdutil::coords::tile_i16(x, y)` (F1) | `let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else { … }` | 22 | Keep each site's `else` body as it is: `let Some((x, y)) = tile_i16(x, y) else { … };` |
| 4 | reuse the existing `content_registry()` (6715) | `let Some(content) = self.content_snapshot.as_ref() else {` | 35 | only where `content_registry()` returns exactly `self.content_snapshot.as_ref()`; check its body |
| 5 | `pub(super) fn campaign_runtime(&self) -> Option<&CampaignRuntime>` and `_mut` | `self.sim.ecs.0.get_resource::<CampaignRuntime>()` (20) / `get_resource_mut` (4) | 24 | |
| 6 | `crate::gdutil::json::{value_gstring, to_gstring, empty_array, empty_object}` | 23× `GString::from(x.to_string().as_str())`; 8× `GString::from(serde_json::to_string(&rows).unwrap_or_else(\|_\| String::from("[]")).as_str())`; 8× `"[]"`, 5× `"{}"` | ~44 | `"null"` (5×) stays inline |
| 7 | `fn dset(d: &mut VarDictionary, k: &str, v: impl ToGodot)` (or `d.set(k, v)` directly, if 0.5's `AsArg` accepts `&str`; try that first) | `out.set(&GString::from("key"), v)` | 66 | |
| 8 | `fn grown_view(&self, rect: (f32, f32, f32, f32), grow: f32) -> CameraView` | `CameraView { x: rect.0 + rect.2 / 2.0, … }` (4×: 5154, 5400, 5490, 5593) | 4 | the expressions must be identical, including the float operation order |
| 9 | `fn mark_world_changed(&mut self)`: `self.world_dirty = true; self.emit_state(); self.emit_world_changed();` | 9 sites | 9 | same order |
| 10 | reuse `building_center` (453) | `x as f32 * TILESIZE_F + def.offset` (12×) | 12 | only where the expression is identical |

## Part 2: shared JSON command parsing

`apply_sim_command_json` (fn 1327–1492) and `submit_player_command_json` (1505–1609) both parse
`place`/`break` from JSON: 35 `.and_then(|v| v.as_u64()/as_i64/…)` chains.

- Add `struct JsonArgs<'a>(&'a serde_json::Value)` with `fn i16(&self, k) -> Option<i16>`,
  `u16_or(k, default)`, `u8_or`, `f64`, `str`, `i32_list`. Each accessor's body must reproduce the
  **exact** chain it replaces (`as_u64` then `try_from`, vs `as_i64` then `as`, and so on). If two
  sites use different chains for the same key, make two accessors.
- Do **not** merge the two functions' `place` branches into one `parse_sim_command`. One of them
  runs a `can_place_for_team` pre-check, and they push to different queues. Only the field reads are
  shared.

## Part 3: decompose the giant pure builders (one commit each)

1. **`building_bar_rows`** (free, `bar_rows.rs`, 580 lines). Each `match def.kind` arm becomes
   `fn push_<family>_rows(rows: &mut Vec<BarRow>, def: &BlockDef, extras: &BarExtras /*, …*/)`. The arms
   are: core 11788, drill 11803, wall crafter 11817, heat 11831/11844, unit factory 11864, assembler
   11888, cargo 11902, turrets 11907–11968, power 11969–12044, nuclear 12045, variable reactor 12056,
   generators 12079–12111, overdrive 12112, force 12130, pumps 12141, payload 12152, launch 12173,
   landing 12197. The prelude (11645–11786) becomes `push_common_rows`. The match keeps its arms
   and calls the fns. The tests `bar_rows_for_the_remaining_families` (338 lines) etc. must pass
   unchanged; they are pure Rust.
2. **`building_bars_json`** (415 lines, `bars.rs`). Extract
   `fn bar_extras(&self, world, content, entity, def, instance) -> BarExtras`, with one private helper per
   family (power + diode closure, generator, unit bars, factory, assembler, cargo, launch,
   variable reactor, overdrive, shield, pump, dummy, `output_liquids`). Keep the order in which
   the fields are filled.
3. **`unit_draws_full`** (292 lines, `render_units.rs`). Extract per-part builders from the `filter_map`
   closure, each taking `(world, content, entity, def)`: mech 6252, treads 6262, crawl 6273, payload
   6290–6329, legs 6330, weapons 6358–6390, item/mine 6398–6416, statuses 6427–6446, floor
   6447–6461.
4. **`light_sources`** (164 lines, `render_light.rs`): `block_lights`, `hazard_lights`, `unit_lights`,
   `bullet_lights`. Each one appends to the same `Vec` **in the same order**, because light order can
   matter for rendering.
5. **`hint_state_value`** (226 lines, `state.rs`): player-unit inputs, team unit count, building pass,
   campaign sector state, assembly.

**Do not decompose** `load_sector_world` (registry take/restore) or `finish_world_load`. Note them for a
later pass.

## Verification (RULES §6)

```bash
python3 tools/refactor/api_surface.py | diff /tmp/H06-surface-before.txt -
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib sim_host
wc -l client/rust/mind-gdext/src/sim_host/*.rs
```

Orchestrator post-merge: a headless boot, `ui_widgets_check`, and an MCP playtest. In the playtest,
hover a turret, a power node and a unit factory and check their bars. Spawn units and check that
they draw. Check that lights render at night. Run one place/break command through the MCP JSON
command path.
