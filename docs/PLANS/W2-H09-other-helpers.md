<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W2-H09 — Helpers for the non-campaign split areas (five sub-packages)

**Wave:** W2 (helper package, RULES §4). Each sub-package is a separate agent and branch. Each
needs its W1 split merged, and F1 for the gdext ones.

| Sub-package | Needs | Owns |
|---|---|---|
| **H09a** headless | S15 | `client/rust/mind-headless/src/{exec,server/commands}/**` |
| **H09b** mods | S16 | `client/rust/mind-core/src/mods/{json,patch}/**` |
| **H09c** content/world/io | S17 | the blocks registry files S17 created, plus `mod.rs`; the turrets files S17 created, plus `mod.rs`; `client/rust/mind-core/src/io/sim_io/**` |
| **H09d** input | S18 | `client/rust/mind-gdext/src/input/**` |
| **H09e** net/root/editor | S19 | `client/rust/mind-gdext/src/{net,editor}/**`, `ui/root/**`, `ui/editor_views/**` |

RULES §4 applies to all of them. **Diff every copy before you replace it**, and keep statement order
where there are side effects. Counts below are at `291e30a8`.

## H09a — headless

1. `fn exit_code(pass: bool) -> i32 { if pass { EXIT_PASS } else { EXIT_FAIL } }` for the 18×
   `Ok(if pass {EXIT_PASS} else {EXIT_FAIL})`. The 11× `ok` variant uses the same fn.
2. `fn time_runs(runs: usize, mut f: impl FnMut()) -> Vec<u64>` for the bench loops
   (`let start = Instant::now()` 15×, `samples.push(start.elapsed().as_nanos() as u64)` 9×). Convert only
   loops whose body is exactly `start; f; push`. If a loop does setup inside the timed region,
   **leave it**, because that changes what is timed.
3. An editor fixture: `fn editor_world(...) -> (…)` for the 14× `WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render)`
   with `NoopRenderHooks` 8× and `boot_content()` 30×. Only where the setup is identical.
4. `server/commands/`: `fn report_err(state: &mut ServerState, r: Result<(), impl Display>)` for the 11×
   `Err(error) => state.warn(&error.to_string())`. Add an `infof!`-style macro or `fn infof` only if it
   reads better than `state.info(&format!(…))` (13×).
5. **Do not** route the hand-rolled `println!(to_string_pretty(..))` sites through `print_report`. Some print
   compact output and quiet mode would change (it is user-visible CLI behaviour). List them.

Verify with `run --cargo-test -p mind-headless`, `run --cargo-run -q -p mind-headless -- run-all --tier T0`,
and the `--help` diff from S15.

## H09b — mods

1. **Field setters.** There are 71 loops of the shape
   `for (field, target) in [ … ] { if let Some(value) = field_f32(object, field) { *target = value } }`.
   Add to `json/fields.rs`:
   ```rust
   pub(super) fn set_f32s(object: &Map<String, Value>, fields: &mut [(&str, &mut f32)]);
   pub(super) fn set_i32s(..); pub(super) fn set_bools(..);
   pub(super) fn set_opt_f32s(object, fields: &mut [(&str, &mut Option<f32>)]);   // `*target = Some(value)` (23×)
   ```
   Each one iterates **in array order**, the same as the loops do.
2. `macro_rules! parse_err { ($($t:tt)*) => { return Err(ContentParseError::new(format!($($t)*))) } }`
   for the 19× `return Err(ContentParseError::new(format!(`.
3. **`patch/scalar.rs`.** `set_i32` (2346), `set_f32` (2375) and `set_bool` (2404) are near-identical:
   mode check, parse, `mark_used`, record original, push resetter, set. Make one generic
   `fn set_scalar<T: Copy + 'static>(&mut self, …, parse: fn(&Value) -> Option<T>, get: …, set: …)`,
   and keep the three names as thin wrappers (39 `=> self.set_` call sites stay unchanged). Diff the
   three bodies first. If they differ in more than the type, stop and keep them.
4. `fn push_reset(&mut self, f: impl Fn(&mut ContentRegistry) + 'static)` for the 17×
   `.push(Box::new(move |r: &mut ContentRegistry| {`.

Verify with `run --cargo-test -p mind-core --lib mods::`, `parity goldens` and `--goldens --check`.

## H09c — content/world/io (determinism-sensitive for turrets and sim_io)

1. **Blocks registry.** Add `fn need<T>(v: Option<T>, name: &str) -> Result<T, ContentError>` for the 26×
   `.ok_or_else(|| ContentError::UnknownName(x.to_owned()))?`. Leave the 99 `spec.X.unwrap_or(…)` in
   `from_spec`; they are the 1:1 field-default table.
2. **Turrets.** Add `fn turret(world: &World, e: Entity) -> Option<&TurretState>` and a `_mut` variant for
   the 36 component accesses. Add `fn ammo_meta(def) -> (mult, range, min_range)` for the
   `range_change`/`min_range_change` lookup repeated in `config_for_inner`'s `item_ammo`/`liquid_ammo`
   closures.
3. **`sim_io` tests only:**
   - `fn io_sim() -> Sim`, for the 20× `set_io_handler(Box::new(SimIoHandler::boot()...))` plus
     `install_block_runtime`/`install_unit_runtime` plus `Sim::new(1, 64, 64, …)`. Several variants
     differ; give each a name.
   - `fn save_then_load(sim: &mut Sim, path: &Path)` for the `request_save` + `tick` (19×) and
     `request_load` + `tick` (16×) pairs.
   - a `TempSave` guard (`Drop` removes the file) for the 39× `let _ = std::fs::remove_file(&path)`.

   The test count must not change.

Verify with `run --cargo-test -p mind-core --lib`, `parity goldens`, `run-all --tier T0` and `--goldens --check`.

## H09d — input

1. `fn with_host<R>(&self, f: impl FnOnce(&MindSimHost) -> R) -> Option<R>` and `with_host_mut` on
   `DesktopBridge`, for the 47× `self.host.clone()` + `bind()`/`bind_mut()`. **Godot re-entrancy:** switch a
   site only if nothing inside the bind scope calls back into Godot in a way that re-binds the host
   (signals, `call("…")`, UI calls). Keep the separate-statement shape where the original drops a
   guard early.
2. `crate::gdutil::coords::tile_i16` for the 20× `(i16::try_from(x), i16::try_from(y))`.
3. **Do not hoist** `self.fire_hotkey(name)` out of the `dispatch_action` arms (23 of 30 sit at arm
   ends). Hoisting would move it after code that runs between the arm and the match's end. Note it
   as a possible later change, with a test.
4. `MindInput`: a `fn rebind_and_save(&mut self)` for the 6× `bindings::save(&self.bindings)` sites, but
   only where the preceding statements are identical. Reuse `with_host` for the 11 `let guard = host.bind()` sites.

Verify with clippy, `run --cargo-test -p mind-gdext --lib input` and the `api_surface` diff.
Orchestrator: an MCP input playtest.

## H09e — net / root / editor_views / editor

1. **`net/`:** add `fn with_connector<R>(&mut self, f: impl FnOnce(&mut Connector) -> R) -> Option<R>` for the
   15× `let Some(connector) = self.connector.as_mut() else {`, where the `else` body is the plain early return.
   Switch to `gdutil::json::value_gstring` for the 50 `GString::from(…to_string().as_str())`
   (1005–1033 et al.).
2. **`ui/editor_views/`:** `fn resolve_editor`/`fn editor(&self)` are copy-pasted in 4 classes, and
   `call_editor` twice (30 call sites). Add a shared
   `struct EditorLink { path: NodePath, cached: Option<Gd<Node>> }` with `get(&mut self, base: &Gd<Node>)`
   and `call(...)`. Each class keeps a field, or calls a free fn `cached_editor(cached, &base, &path)`
   if adding a field would change the class's exported properties. **Do not** add `#[export]`/`#[var]`
   fields; that changes the Godot property list.
3. **`editor/`:**
   - `fn content(&self) -> Option<&ContentRegistry>` for the 31× `let Some(content) = self.content.as_ref() else {`;
   - `fn with_world<R>(&mut self, f: impl FnOnce(&mut WorldEditorGrid, &ContentRegistry) -> R) -> Option<R>`,
     which destructures the fields to keep the borrows split, for the 14× `WorldEditorGrid::new(...)`;
   - `fn set_load_error(&mut self, e: impl Display)` for the 12 + 11 `last_error_key`/`last_error_message`
     pairs, next to the existing `set_map_error` (241).
4. **`ui/root/`:** use the existing `json_string`/`json_bool` (2340/2349) or `gdutil::variant` for the
   8 + 8 `.and_then(|value| value.try_to::<GString>().ok())` / `.try_to::<bool>()` chains.

Verify with clippy, `run --cargo-test -p mind-gdext --lib`, and the `api_surface` diff (property lists
too: the script lists funcs only, so manually confirm that no `#[var]`/`#[export]` was added).
Orchestrator: `dialog_sweep_check`, plus an MCP smoke test of the editor and of host/join.
