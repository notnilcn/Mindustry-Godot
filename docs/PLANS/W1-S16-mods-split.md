<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S16 — Split `mods/json/mod.rs` (6096) and `mods/patch/mod.rs` (3836)

**Wave:** W1 (pure mechanical split, RULES §3).
**Owns:** `client/rust/mind-core/src/mods/json/**` (existing: `mod.rs`, `classmap.rs`, `classmap_gen.rs`)
and `client/rust/mind-core/src/mods/patch/**`.
**Effort:** medium-large.

Read `client/rust/mind-core/src/mods/AGENTS.md` first. Mods content parsing feeds content IDs and
goldens, so RULES §5 applies in spirit: the parse results must be identical. The tests and goldens
prove it.

## `json/mod.rs`: `ContentJsonParser` (port of `mod/ContentParser.java`)

`ContentJsonParser`'s methods are spread over many `impl` sections. Split them into sibling files
inside `json/`. Each file holds an `impl ContentJsonParser { … }` (or the free fns):

| File | Lines at `291e30a8` | Contents |
|---|---|---|
| `json/mod.rs` | 1–58, 59–735 | header, imports, parser core, research, parse entry, unlock fields, name resolution; `mod`/`pub use` lines |
| `json/content.rs` | 736–796, 1211–1761 | `parse_item`; plans, liquid, status, weather, team, sector, planet |
| `json/block.rs` | 797–1210 | `parse_block` |
| `json/unit.rs` | 1762–2161, 2162–2654 | `parse_unit`; weapons, abilities, shoot pattern. About 900 lines; split to `unit.rs` + `weapons.rs` at 2162 if you prefer |
| `json/bullet.rs` | 2655–3075 | bullet + `register_bullet_spec` |
| `json/draw.rs` | 3076–3514 | drawers and parts |
| `json/effect.rs` | 3515–3891 | effects and particles |
| `json/consume.rs` | 3892–4118 | sound, consumes, stacks |
| `json/resolve.rs` | 4119–4217, 4218–4602 | resolve methods, `warn`; the free `pub fn resolve_*`/`parse_*` |
| `json/fields.rs` | 4603–4625, 4626–4840 | `field_f32/i32/bool`; the `*_FIELDS` consts |
| `json/tests.rs` | 4841–6096 | tests |

**Keep every `pub fn` reachable at `mods::json::`**, via `pub use resolve::*;` or explicit lists. Importers:
- `mods/provider.rs:14` (`ContentJsonParser`, `content_type_for_folder`);
- `mods/patch/mod.rs:1861,1956,2172,2459` (`ContentJsonParser::new`, `resolve_entity_def`, `parse_stack`);
- `mods/assets/mod.rs:1079` (`ContentJsonParser::restricted`);
- `content/registries/blocks/mod.rs:4693–4694` (`classmap`, `resolve_block_kind`);
- `mind-tools/src/main.rs:263` (`json::classmap::ClassTagMap`).

Private methods used across files become `pub(super)`; that is the original scope `mods::json`.

## `patch/mod.rs`: `DataPatcher` (port of `mod/DataPatcher.java`)

| File | Lines | Contents |
|---|---|---|
| `patch/mod.rs` | 1–65, 66–342 | consts, `FieldMode`, struct; apply/unapply/key/path/field |
| `patch/block.rs` | 343–1336 | `apply_block`, `apply_block_scalar`, block `edit_*` (about 990 lines) |
| `patch/content.rs` | 1337–1643, 2197–2345 | item, liquid, status; weather, planet, sector, team |
| `patch/unit.rs` | 1644–2196 | unit + unit edits (weapons, abilities, bullet, flags, entity) |
| `patch/scalar.rs` | 2346–2509 | `set_i32`/`set_f32`/`set_bool` + helpers |
| `patch/load.rs` | 2510–2730 | `load_mod_patches`, `collect_mod_patches`, `resolve_block_flag` |
| `patch/tests.rs` | 2731–3836 | tests |

Importers: `content/boot.rs`, `mods/assets/mod.rs`, `mods/provider.rs` (`patches`), mind-headless
`exec.rs:1772,2166` (`DataPatcher`). Keep their paths valid.

Keep any `#[allow(clippy::too_many_arguments)]` attached to its fn.

## Verification (RULES §6)

```bash
B=$(git merge-base main HEAD); M=client/rust/mind-core/src/mods
python3 tools/refactor/move_check.py --base $B --old $M/json/mod.rs --new $M/json
python3 tools/refactor/move_check.py --base $B --old $M/patch/mod.rs --new $M/patch
run --cargo-check --workspace --all-targets
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core --all-targets -- -D warnings
run --cargo-test -p mind-core --lib mods::
run --cargo-test -p mind-headless
run --cargo-run -q -p mind-headless -- parity goldens
run --goldens --check
```

For `move_check.py --new $M/json`: `classmap*.rs` are unchanged and show identically on both sides,
so they are not counted as moved. That is fine.
