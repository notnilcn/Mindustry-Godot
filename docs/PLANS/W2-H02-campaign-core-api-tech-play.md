<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W2-H02 — Campaign core API, part 2: tech tree, play flows, launch, runtime (additive)

**Wave:** W2 (helper package, RULES §4). **Priority:** first. **Needs:** W1 merged (S01, S02).
**Owns:**
- `client/rust/mind-core/src/game/{tech_tree,play}.rs`;
- the new `client/rust/mind-core/src/game/launch.rs`;
- `client/rust/mind-core/src/game/runtime/**` (split by S02);
- `client/rust/mind-core/src/game.rs` (one `pub mod launch;` line).

**Effort:** medium-large. Same rules as [H01](W2-H01-campaign-core-api-universe.md): add tested
functions copied from the facade, adopt them only inside your own files, and never touch `mind-gdext`.
You may **not** call H01's new APIs; they don't exist on your branch. If you need one, write the
code the way it is today and note it for X01.

Facade line numbers are at `291e30a8` (`mind-gdext/src/campaign.rs`; after S01, `campaign/<file>.rs`).

## `tech_tree.rs`

| New fn / type | Copied from (facade) | Notes |
|---|---|---|
| `pub struct UnlockOutcome { pub names: Vec<String>, pub contents: Vec<ContentRef>, pub any_block: bool }`, `pub fn apply_pending_unlocks(registry: &mut ContentRegistry, store: &mut dyn UnlockStore, campaign: &Campaign, unlocks: Vec<UnlockRef>) -> UnlockOutcome` | `pump_runtime` 1103–1167: map each `UnlockRef` to a `ContentRef` + name, `unlock_fields_mut().unlock`, then auto unlocks, then the `set_unlocked_content` input | `set_unlocked_content` stays with the caller (it is a `CampaignRuntime` method; return what it needs) |
| `pub fn check_campaign_auto_unlocks(registry: &mut ContentRegistry, store: &mut dyn UnlockStore, campaign: &Campaign) -> Vec<ContentRef>` | the identical blocks at 1136–1140 / 1139–1143, 1295–1299 and 2955–2965 (`CampaignObjectiveContext::new` + `check_auto_unlocks`) | First confirm the three blocks are identical |
| `pub fn unlocked_content_names(registry: &ContentRegistry) -> Vec<String>` | free `collect_unlocked` 7291–7300 | |
| `pub fn root_planets(registry: &ContentRegistry, root: TechNodeRef) -> Vec<PlanetId>` | free `root_planets` 7305–7318 | |
| `pub fn node_for_content_name(registry: &ContentRegistry, name: &str) -> Option<(ContentRef, TechNodeRef)>` | `research` 2801–2821 | |
| `pub struct ResearchInventory { pub pool: ItemModule, pub sectors: Vec<(SectorRef, Vec<(String, i32)>)>, pub played: Option<SectorRef> }`, `pub fn research_inventory(registry, campaign: &Campaign, planets: &[PlanetId], played: Option<SectorRef>, played_items: &[(String, i32)]) -> ResearchInventory` | `research_inventory` 3014–3070 | The facade reads `played_items` from the live core through the host. That read stays in the facade; you take the slice |
| `pub fn plan_research_removal(inv: &ResearchInventory, spent: &[ItemStack], registry: &ContentRegistry) -> (Vec<(PlanetId, u16, String, i32)>, Vec<(String, i32)>)` | the deduction loop 2872–2933 | The first list holds per-sector record removals, the second the core removals. The facade keeps `remove_player_core_items`. **Keep the iteration order** of the original loop |

## `play.rs`

| New fn / type | Copied from | Notes |
|---|---|---|
| `pub fn capture_replacements(registry: &ContentRegistry, session: &PlaySession) -> Vec<(String, String)>` | `pump_runtime` 1027–1044 | |
| `pub fn adopt_map_rules(session: &mut PlaySession, map: &Rules, planet: &Planet, registry: &ContentRegistry, has_preset: bool, preset_no_lighting: bool, resumed: bool)` | free `adopt_map_rules` 7250–7288 | Put it next to `apply_world_load_campaign_rules`. Match the parameter list to what the body really reads |
| `impl PlaySession { pub fn apply_save_meta(&mut self, meta: &SaveMeta) }` | free `apply_loaded_meta` 7323–7355 | |
| `pub struct SectorLaunch { resume, lost_replay, has_save, preset_name, spawn_position, … }`, `pub enum LaunchReject { … }`, `pub fn select_sector_launch(campaign: &Campaign, registry: &ContentRegistry, planet: PlanetId, sector: u16, no_live_core: bool) -> Result<SectorLaunch, LaunchReject>` | the decision core of `start_sector_with_loadout`: the locked-preset check 1453–1465 and branch selection 1527–1551 (`no_core_branch`, `clear_save`, `data_matches`, `lost_replay`, `resume`) | `LaunchReject` variants = each early `return false` with its toast key (the facade keeps the toast). Field list = whatever the later phases read |
| `pub fn begin_sector_launch(campaign: &mut Campaign, registry: &mut ContentRegistry, store: &mut dyn UnlockStore, planet: PlanetId, sector: u16)` | 1503–1520 (quiet-unlock the preset, set the last sector) and 1628–1634 (reset `being_played` on every sector) | If the two blocks are separated by side effects that must run in between (1521–1627), add **two** fns instead: `begin_sector_launch` and `clear_being_played_for_launch`. Write in your report which one you chose |
| `pub fn live_sector_record(session: &PlaySession, campaign: &Campaign, registry: &ContentRegistry, snapshot: Option<&CoreSnapshot>) -> Option<SectorInfo>` | `persist_sector_record_in` 4984–5056 (prepare from snapshot, copy wave/waves/wave_spacing/win_wave/attack) | This takes `registry` explicitly. The facade bug 02-FINDINGS #1 comes from reading `self.registry` while it is taken; X01 must keep today's behaviour anyway, see X01 |
| `pub fn reapply_planet_rules(session: &mut PlaySession, campaign: &Campaign, registry: &ContentRegistry) -> Option<CampaignApply>` | the "re-apply if the played planet" half of 5980–6050 / 6080–6130 | |

Inside `play.rs`, adopt nothing from H01 (it is not on your branch). The duplicated planet-name blocks
(`play.rs:433–436`, `512–515`) stay until X01.

## `launch.rs` (new; upstream `ui/dialogs/LaunchLoadoutDialog.java`, `game/Universe.java`, `world/blocks/campaign/LaunchPad.java`)

| New item | Copied from |
|---|---|
| `pub struct LaunchPayload { pub stacks: …, pub schematic: Option<i32>, pub origin: Option<u16>, pub launch_core: …, pub stacks_json: String }`, `impl LaunchPayload { pub fn parse(json: &str) -> LaunchPayload }` | free `parse_loadout_stacks` 7549–7560, `parse_launch_payload` 7562–7578, and the `launch_core` read at 1610–1613. Field types are exactly what the facade code produces |
| `pub fn launch_core_name(...)`, `pub fn launch_requirements(...)`, `pub fn schematic_placeable_on_planet(...)`, `pub fn item_on_planet(registry, name, planet) -> bool` | free fns 7467, 7493, 7531 and associated `item_on_planet` 817 |
| `pub fn preset_launch_loadout_filter(...)` | the loadout filter inside `preset_launch_loadout` 852–860 |

`launch_total` (7515) is **not** copied; H01 adds `launch_plan_full`, which makes it obsolete.
Move the facade's tests for these fns (`launch_cost_*`, `launch_core_*`, `launch_grid_*`, now in
`campaign/launch.rs` after S01) only in X01. Here, write new mind-core tests that pin the same cases.

## `runtime/**`

| New fn | Copied from | Notes |
|---|---|---|
| `impl UnlockRef { pub fn content_ref(self) -> ContentRef }` (`runtime/hooks.rs`, next to `event_content`) | the mapping inside the facade's unlock loop | |
| `pub fn capture_played_sector(rt: &mut CampaignRuntime) -> bool` (`runtime/game_state.rs`) | the capture arm of `game_state_tick_system`. **Extract it from there**, so the system calls it; that must be behaviour-identical (goldens) | Do **not** add `service.on_sector_capture` or anything the facade's `capture_sector` lacks. The difference is 02-FINDINGS #2 and gets its own fix |
| `impl CampaignRuntime { pub fn turn_context(&self) -> TurnContext }` | the two `TurnContext` builds, facade 1212–1220 and 2484–2492 | confirm they are identical |
| `impl CampaignRuntime { pub fn run_due_turn(&mut self, registry: &ContentRegistry, settings: &mut dyn SettingsStore) -> Option<TurnReport> }` | facade 1203–1238: clear `turn_due`, seed `JavaRandom::new(turn + 1)`, `run_turn`, `apply_played_invasions` | Compare with `run_turn` 2477–2537, which does **not** check `turn_due`. Add `run_turn_now(&mut self, …) -> TurnReport` for that path. Keep the RNG seeding identical |
| `impl CampaignRuntime { pub fn install(session: PlaySession, campaign: Campaign, service: ServiceTracker, registry: &ContentRegistry) -> Self }` | the two runtime builds 1752–1758 and 2131–2137 (they call `collect_unlocked` → `unlocked_content_names`) | |
| `pub fn apply_resume_record(rt: &mut CampaignRuntime, registry: &ContentRegistry, settings: &dyn SettingsStore, planet: PlanetId, sector: u16) -> Option<IndexMap<String, i32>>` | the resume record path 1800–1846 (`write_to_state`, inventory refill, `apply_rules`) | The facade writes the returned items into the core through the host; that write stays in the facade |

Adopt `game::test_support::{raw_registry, campaign}` in the test modules of your files
(`runtime/test_support.rs`, `play.rs:901/907`, `tech_tree.rs:643`, …). Keep `tech_tree`'s
`(ContentRegistry, MemoryUnlockStore)` variant local if it needs the store.

## Verification (RULES §6, §5)

```bash
run --cargo-test -p mind-core --lib game::
run --cargo-check --workspace --all-targets
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core --all-targets -- -D warnings
run --cargo-test -p mind-headless
run --cargo-run -q -p mind-headless -- parity goldens
run --cargo-run -q -p mind-headless -- run-all --tier T0
run --goldens --check
```

`capture_played_sector` is the only behaviour-sensitive extraction in this package. Run the campaign
scenarios in `mind-headless/src/campaign_scenarios.rs` explicitly (`run-all` covers them; confirm in
the output). In the report, list every new item with its signature and the facade sites X01 must switch.
