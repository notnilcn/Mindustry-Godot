<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W2-H01 — Campaign core API, part 1: `Campaign` / `Sector` / `Saves` helpers (additive)

**Wave:** W2 (helper package, RULES §4). **Priority:** first. **Needs:** W1 merged (S01, S02).
**Owns:**
- `client/rust/mind-core/src/game/{universe,sector,saves,campaign_rules,planet}.rs`;
- `client/rust/mind-core/src/game/test_support.rs` (created in F3; you may add helpers).

It does **not** own `play.rs`, `tech_tree.rs` or `runtime/` (H02), or `mind-gdext` (H03).
**Effort:** medium.

## Goal

The Godot facade `mind-gdext/src/campaign/*` (split by S01) contains campaign **business logic**
that belongs in `mind-core`: sector lookups, record edits, save-path rules, launch maths. This
package **adds** that logic to `mind-core` as tested functions. It also replaces the duplicated
copies **inside the files you own**. It does **not** touch the facade; X01 (W3) switches the
facade over and deletes its copies.

Each new function's body must be **copied** from the facade (or core) code it replaces, with
`self.<field>` turned into parameters. Do not "improve" the logic. Each doc comment names:
- its Java origin;
- `Extracted from mind-gdext campaign.rs:<lines at 291e30a8> (adopted by X01).`

Facade line numbers below are at `291e30a8`. After S01, that code lives in `campaign/<file>.rs`;
find it by function name.

## APIs to add

### On `Campaign` (`universe.rs`)

| New fn | Replaces (facade, `291e30a8`) | Notes |
|---|---|---|
| `pub fn planet_name(&self, id: PlanetId) -> Option<&str>` | `campaign.planet(id).map(\|p\| p.name.clone()).unwrap_or_default()` at about 10 facade sites (837, 1171, 2438, 2941, 3443, 3479, 4453, 4992, 5493, 7216); core `saves.rs:598–601`, `universe.rs:853`, `play.rs:433–436`/`512–515` | `Option<&str>`; callers keep their own `unwrap_or_default()` |
| `pub fn sector_key(&self, planet: PlanetId, id: u16) -> Option<SectorKey>` | the `session.rules.sector = Some(SectorKey::format(..))` pairs in `play.rs` (H02 adopts) | copy `SectorKey::format`'s argument order exactly |
| `pub fn sector_by_name(&self, planet: &str, id: u16) -> Option<(PlanetId, &Sector)>` and `sector_by_name_mut` | `planet_id_by_name` + `sector`/`sector_mut` at 12 facade sites (1369, 1441, 2641, 2692, 3951, 3960, 5986, 6267, 6624, 6652, 6690, 6724) | `planet_id_by_name` is a linear scan (`universe.rs:433`); keep using it |
| `pub fn save_sector_info(&self, settings: &mut dyn SettingsStore, planet: PlanetId, id: u16) -> Result<bool, IoError>` | the pending-info-save flush (facade 1165–1180) and the `save_info(settings, &planet_name)` sites (core `universe.rs:763/857`) | It resolves the planet name itself. Use the exact error and return types that `Sector::save_info` uses today |
| `pub fn edit_sector<R>(&mut self, settings: &mut dyn SettingsStore, planet: &str, id: u16, f: impl FnOnce(&mut Sector) -> R) -> Option<R>` | resolve, then mutate, then `record.save_info(settings, &planet_name)`. That happens at 9 facade sites (1175, 1822, 2687, 2945, 5552, 6631, 6661, 6697, 6733) | If `save_info` returns an error, do what the facade does at those sites; read them first. If they differ, add the variant that most of them use and list the rest for X01 |
| `pub fn clear_being_played(&mut self)` | sets `being_played = false` on every sector (facade 1632, 4944, 5729; core tests `universe.rs:1266`/`1345`) | |
| `pub fn boot(registry: &ContentRegistry, settings: &dyn SettingsStore, saves: &Saves) -> (Campaign, usize)` | facade `bootstrap_with` 636–661: `from_registry`, `load_all`, `apply_sector_saves`, `update_base_coverage_from_neighbors` | `usize` is whatever count the facade logs. Match the sequence exactly |
| `pub fn clear_saves(&mut self, settings: &mut dyn SettingsStore, paths: &Paths, only: Option<PlanetId>) -> Vec<PathBuf>` | facade `clear_campaign_saves_for` 5697–5756 | Return the files the facade deletes, **in the same order**. The FS deletion stays in the facade |
| `pub fn set_campaign_rules(&mut self, planet: PlanetId, rules: CampaignRules)` | the "write planet rules" half of `set_campaign_rule` 5980–6050 and `set_campaign_rules_json` 6080–6130 | The "re-apply to the played session" half is H02's `play::reapply_planet_rules` |
| `pub fn launch_plan_full(...) -> LaunchPlan { resources, total, valid }` | `universe::launch_plan` (218–262), plus the facade `launch_total` 7515, which recomputes `total` that `launch_plan` builds internally (`universe.rs:251–254`) | Keep `launch_plan` itself unchanged (facade callers). Make `launch_plan` call `launch_plan_full(..).resources` **only if** that is byte-equivalent; otherwise leave both bodies and add a test that their outputs match |
| `impl Universe { pub fn launch_resource_stacks(&self, registry: &ContentRegistry) -> IndexMap<String, i32> }` | the "dense launch sequence → name map" conversion, 3× in the facade (6170–6177, 6389–6401, 6431–6438) | `mind_core::ui::campaign::launch_resource_sequence` (`ui/campaign.rs:1596`) is the inverse; reference it in the doc |

### On `Sector` (`sector.rs`)

| New fn | Replaces | Notes |
|---|---|---|
| `pub fn record_json(&self) -> serde_json::Value` | facade `sector_record_value` 2636–2659 | |
| `pub fn clear_base(&mut self, settings, planet_name: &str)` | facade free `clear_sector_record` 7632–7645 | |
| `pub fn abandon(&mut self, settings, planet_name: &str)` | facade 5545–5553 (off-play abandon: clear items, `has_core`, production) | |
| `pub fn display_name(&self, planet: &Planet) -> String` | **only** `mind-core/src/ui/campaign.rs:917 sector_display_name` (private). Copy its body | The facade has two other variants that **differ** (02-FINDINGS #3/#4). Do not make them call this; X01 leaves them alone too. Mark the doc: "Java `Sector.name()`; the facade's `sector_toast_name` diverges, see 02-FINDINGS #3" |

### Item-stack conversions (`sector.rs`, free fns)

| New fn | Replaces |
|---|---|
| `pub fn stacks_to_pairs(stacks: &[JsonItemStack]) -> Vec<(String, i32)>` | the `filter_map` at `play.rs:401` (H02 adopts) and facade 6617 |
| `pub fn fill_module(module: &mut ItemModule, registry: &ContentRegistry, pairs: &[(String, i32)], cap: i32)` | `play.rs:397–409`, `runtime/session.rs` (was `runtime.rs:503–509`), facade 1826–1834 |
| `pub fn dense_from_names(registry: &ContentRegistry, pairs: &[(String, i32)]) -> Vec<i32>` | facade 5405–5416, 6431–6438 |

Diff the copies first. If they disagree on clamping or on unknown names (skip vs 0), add only the
variant the majority uses, name it after its behaviour, and list the others.

### On `Saves` (`saves.rs`)

| New fn | Replaces |
|---|---|
| `pub fn sector_save_file(&self, paths: &Paths, planet: &str, id: u16, stem: &str) -> PathBuf` | facade `sector_save_file` 1336–1350; put it next to `get_sector_file` (`saves.rs` ~362) |

### Inside your own files

Replace the copies inside `universe.rs`, `saves.rs` and `sector.rs` with the new helpers
(`saves.rs:598–601`, `universe.rs:853`, `universe.rs:763/857`), and only where the replaced code is
identical.

### Test helpers (`game/test_support.rs`)

Adopt `raw_registry()`/`campaign()` in the test modules of **your** files (`saves.rs:700`,
`planet.rs:648`, plus `universe.rs`/`sector.rs`/`campaign_rules.rs` if they have local copies).
Add `own_sector(...)` if F3 did not, using the canonical body.

## Tests

Every new fn gets at least one unit test in its file's test module, using `game::test_support`.
Where the logic came from the facade, write the test **from the facade's observable behaviour**.
For example, `edit_sector` on an unknown planet returns `None` and writes nothing; `clear_saves(only=Some(p))`
returns only `p`'s files, in the facade's order.

## Verification (RULES §6, §5)

```bash
run --cargo-test -p mind-core --lib game::
run --cargo-check --workspace --all-targets
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core --all-targets -- -D warnings
run --cargo-test -p mind-headless
run --cargo-run -q -p mind-headless -- parity goldens
run --goldens --check
```

The test inventory grows only by your new tests. In the report, list every new fn with its signature
and the facade sites X01 must switch. That list is X01's input.
