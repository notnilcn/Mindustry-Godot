<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S04 — Split the planet dialog (`batch_c.rs`, 4042 lines) and `MindPlanetView` (`ui/planet.rs`, 2790 lines)

**Wave:** W1 (pure mechanical split, RULES §3). **Priority:** first group. **Needs:** F2 merged.
**Owns:**
- `client/rust/mind-gdext/src/ui/dialogs/batch_c.rs`;
- `client/rust/mind-gdext/src/ui/dialogs/campaign/planet/**` (F2 created only its `mod.rs` stub; you own the whole directory);
- `client/rust/mind-gdext/src/ui/planet.rs` (becomes `ui/planet/**`).

**Effort:** medium-large, mechanical.

## Part A — `batch_c.rs` → `dialogs/campaign/planet/`

`batch_c.rs` contains only the `PlanetDialog` controller (upstream `ui/dialogs/PlanetDialog.java`).
Its `dispatch` answers one id: `"planet" => Some(Box::new(PlanetDialog::default()))`. The bulk of
the file is free fns, grouped by section comments.

| Destination (`dialogs/campaign/planet/…`) | `batch_c.rs` lines | Contents |
|---|---|---|
| `mod.rs` (replace F2's stub) | header, imports, 477–717 | `PlanetState` + impl (531), `PlanetDialog` + `Default` (550) + `impl DialogController` (558), plus the real `dispatch` (moved from the old file), `mod` lines for the files below and `use` of their items |
| `util.rs` | 41–476 | row readers, settings helpers, `content_texture` 299, `icon_rect` 309, `spacer` 282, `rich` 69, `ui_call` 184, `strip_color_tags` 351, `connect_once` 387, the private `Deferred` 413 + `impl Deferred` 415 + `install_frame_tick` 429, `bundle_or` 468 |
| `reads.rs` | 718–903 | campaign/view reads (`planet_find_planet` 758, `planet_sectors` 773, `planet_localized_sector` 840, …) |
| `view_binding.rs` | 904–944 | `planet_view_call` 911–917 and the view hookup |
| `chooser.rs` | 945–1442 | planet chooser |
| `page.rs` | 1443–2030 | page build |
| `sector_panel.rs` | 2031–3128 | action row and sector info panel. About 1100 lines; if a clear section comment splits it, use `action_row.rs` + `sector_panel.rs` |
| `launch.rs` | 3129–3456 | launch flow (`planet_launch_plan` 3212, stacks 3231) |
| `stage.rs` | 3457–3555 | |
| `reveal.rs` | 3556–3668 | |
| `globe_input.rs` | 3669–3989 | globe input |
| tests | 3990–4042 | into `launch.rs` or `mod.rs`, next to what they test |

The sections are identified by their `// ---` comments; locate them at HEAD with
`grep -n "^// \(=\|-\)\{3,\}" dialogs/batch_c.rs`. The functions in a section mostly call each other
and the shared helpers.

Visibility (RULES §3.2): batch_c's private items are now private to `campaign::planet`. Children
of `planet/mod.rs` reaching each other's fns need `pub(super)`, which makes them visible in
`campaign::planet`, i.e. the original scope. Nothing outside the file used batch_c items
(`grep -rn "batch_c::" client/rust` shows only `dialogs/mod.rs`).

`batch_c.rs` becomes the shim: header, `//! Moved to dialogs/campaign/planet/. Deleted in W5.`,
and the stub `dispatch` returning `None`.

**Do not merge** batch_c's `util.rs` helpers with `campaign/util.rs` (the old batch_campaign2
copies). Some are identical and some are not (`strip_color_tags` differs, and so do the meta-key
prefixes of `Deferred`). H04 dedupes them in W2.

## Part B — `ui/planet.rs` → `ui/planet/`

`MindPlanetView` (`#[class(base=Control)]`, ~122–124) ports `ui/dialogs/PlanetDialog.java` (view)
and `graphics/g3d/PlanetRenderer.java`. It is named by `client/scenes/ui/planet_view.tscn:17`, and
its `#[func]` names are ABI. Its `#[godot_api] impl MindPlanetView` starts at 366 and holds both the
`#[func]` API and many private helpers.

`git mv ui/planet.rs ui/planet/mod.rs`, then:

| File | Lines | Contents | Block kind |
|---|---|---|---|
| `planet/mod.rs` | 1–626 | header, constants 40–101, struct, `IControl` 224–364, the `#[godot_api]` block with the `#[func]` API 365–626 (`set_planet`, `set_sectors`, `pick`, `view_tile`, …) | primary. Close the block after the last `#[func]` |
| `planet/scene.rs` | 627–1384 | scene build: background, skybox, universe/orbits, body, clouds, grid, atmosphere, light | plain `impl MindPlanetView` (these have no `#[func]`; if one does, it goes in a `#[godot_api(secondary)]` block) |
| `planet/overlays.rs` | 1385–2213 | markers, arcs, `status_color` 1673, `add_icon` 1759, hover, highlight, label | plain impl |
| `planet/mesh.rs` | 2214–2495 | `SunMesher` 2222–2278 + its impls (Godot-free, port of `graphics/g3d/SunMesh.java`), `upload_mesh`, `arc_ribbon` 2349 | free + plain impl |
| `planet/rows.rs` | 2496–2647 | `row_*` accessors; keep `row_array` 2605 and its PAR-0018 typed-array workaround comment | free |
| `planet/tests.rs` or inline modules | 2648–2790 | tests | – |

`ui/mod.rs:20` (`pub mod planet;`) needs no change.

**Must stay in the primary block in `planet/mod.rs`:** `get_planet` and `set_planet_value`. Line 151
has `#[var(get = get_planet, set = set_planet_value)]`, and godot-rust cannot reference functions from
secondary blocks there (RULES §3.5). If either one is defined after line 626, move it into the primary
block in `mod.rs`; do not leave it in a child file.

Moving `SunMesher` to `mind_core::render::g3d` would be nice, but it is **not** in scope. `mind-core/src/render/`
is off-limits (the user has in-flight edits there). List it under Follow-ups.

## Verification (RULES §6)

```bash
python3 tools/refactor/api_surface.py | diff /tmp/S04-surface-before.txt -    # empty: MindPlanetView funcs unchanged
B=$(git merge-base main HEAD); U=client/rust/mind-gdext/src/ui
python3 tools/refactor/move_check.py --base $B --old $U/dialogs/batch_c.rs --new $U/dialogs/batch_c.rs $U/dialogs/campaign/planet
python3 tools/refactor/move_check.py --base $B --old $U/planet.rs --new $U/planet
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib
```

Orchestrator post-merge: `dialog_sweep_check`. A manual MCP check of the planet dialog is
optional: open the planet dialog in a campaign profile, rotate the globe, and click a sector.
