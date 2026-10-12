<!-- SPDX-License-Identifier: GPL-3.0-only -->
# G02 — Block behavior + block configuration: audit and execution plan

Date: 2026-10-13. Area: G02 (per-block behavior, block configuration panels and
tap-to-link, power/liquids/heat, crafting, logistics, defense blocks, storage/core
upgrade, block-side turret intake, placement validity and build cost rules).

This file is self-contained. Read `docs/parity-plans/00-environment.md` first (paths
`core/`, `gdext/`, `ui/`, `J/` etc., build/test/gmcp recipes, commit rules). Work the
tasks in §3 order. One task = one commit. If what you see in the code differs from what
a task says (function missing, line moved more than a search can find, behavior already
fixed), stop that task, write down what you saw, and go to the next task.

Start state: `main` contains the G00 integration merge
(`git log main --oneline | grep -m1 'G00: integrate'` prints a line).

Port line numbers below were read on the integration tree on 2026-10-13; they drift.
Always re-locate with `grep -n "<function name>" <file>`.

---

## 1. Inventory

### 1.1 How blocks are wired in the port (read once)

* Each block name has a `BlockKind` equal to its Java class (`core/content/registries/blocks/*.rs`,
  `spec("<name>", BlockKind::X)`); all 300+ non-environment blocks were checked: every
  Java class in `J/content/Blocks.java` maps to the same-named `BlockKind` (only diff:
  none).
* Behavior per block: `core/world/blocks/mod.rs::default_registry` (family `register`
  functions register by name), else `core/world/behavior/mod.rs::default_behavior`
  (by kind), else `NoopBehavior`.
* **Item/liquid intake is pure dispatch.** `transfer::dispatch_accept_item`
  (`core/world/blocks/distribution/transfer.rs:109`) calls the behavior's
  `accept_item`; the trait default (`core/world/behavior/mod.rs`, `fn accept_item`) returns
  `false`. Liquids: `core/world/blocks/liquid/movement.rs:95 accept_liquid` uses the
  `LiquidNode` component for liquid blocks, otherwise the behavior's `accept_liquid`
  (trait default `false`). Java's default is `BuildingComp.acceptItem`
  (`J/entities/comp/BuildingComp.java:869-871`: `block.consumesItem(item) &&
  items.get(item) < getMaximumAccepted(item)`) and `acceptLiquid` (`:873-875`:
  `block.hasLiquids && block.consumesLiquid(liquid)`). **Every behavior that does not
  override these rejects every input** — this is the single largest gap (T1).
* Grid access: behaviors get only `&mut World`; `Sim.grid` is not in the World. Grid-
  needing per-tick work runs as a Sim pass (template: `Sim::refresh_facing_drills`,
  `core/sim/runtime.rs:974`, called from `Sim::tick` in `core/sim/mod.rs:594`).

### 1.2 Java class -> port -> status

Status: **OK** = ported and wired (test exists); **PARTIAL**; **BROKEN** = wired but wrong;
**MISSING**; **NOT WIRED** = code exists, never called live. IO column: per-kind
`write/read` present in port (Java has one = "J").

| Java class (`J/world/blocks/...`) | Blocks | Port behavior (file) | Status | Evidence | Task | IO |
|---|---|---|---|---|---|---|
| distribution/Conveyor | conveyor, titanium-conveyor | `ConveyorBehavior` (`distribution/conveyor.rs:254`) | OK; `unitOn` (units ride belts) MISSING | no `unit_on` anywhere | T13 | J ✓ |
| distribution/ArmoredConveyor | armored-conveyor | `ConveyorBehavior::ARMORED` | OK | | | ✓ |
| distribution/StackConveyor | plastanium-conveyor, surge-conveyor | `StackConveyorBehavior` (`stack_conveyor.rs:195`) | PARTIAL: no write/read | impl list | T17 (IO), T18 (test) | J ✗ |
| distribution/Junction | junction | `JunctionBehavior` | OK | | | ✓ |
| distribution/ItemBridge, BufferedItemBridge | phase-conveyor, bridge-conveyor | `ItemBridgeBehavior` (`item_bridge.rs:299`) | PARTIAL: config works by command; tap-to-link MISSING; tap opens an item list | `config_spec` falls back to items (`gdext/input/desktop.rs:1580`) | T10 | J ✓ |
| distribution/Sorter | sorter, inverted-sorter | `SorterBehavior` | OK (item list not filtered by unlock) | `desktop.rs:1583` | T10 step 6 | ✓ |
| distribution/Router | router, distributor | `RouterBehavior` | OK | | | – |
| distribution/OverflowGate | overflow-gate, underflow-gate | `OverflowGateBehavior` | OK | | | – |
| distribution/MassDriver | mass-driver | `MassDriverBehavior` (`mass_driver.rs:305`) | PARTIAL: tap-to-link MISSING; shooting UNVERIFIED | | T10, T18 | ✓ |
| distribution/Duct, DuctRouter, DuctJunction, DuctBridge, OverflowDuct, StackRouter, DirectionalUnloader | Erekir ducts | `Duct*Behavior`, `StackRouterBehavior`, `DirectionalUnloaderBehavior` | OK (tests in `distribution/`) | | | ✓ |
| storage/StorageBlock | container, vault, reinforced-* | `StorageBehavior` | OK | | | – |
| storage/Unloader | unloader | `UnloaderBehavior` (`storage/unloader.rs:124`) | OK (item panel via fallback) | | T18 test | ✓ |
| storage/CoreBlock | core-shard..core-acropolis | `CoreBehavior` (`core_block.rs:139`) | BROKEN placement: `canPlaceOn`, `canReplace` (upgrade), `placeBegan`, `canBreak` MISSING | `world/build.rs:78 can_place_on` returns `true`; `can_replace` has no core rule | T9 | ✓ |
| production/Drill | mechanical/pneumatic/laser/blast drill | `DrillBehavior` (`behavior/production.rs:429`) | BROKEN: water boost never arrives (no `accept_liquid`); drill time ignores hardness | `delay = def.drill_time` (`production.rs` in `update_tile`) vs `Drill.java:170-172` | T1, T6 | ✓ |
| production/BurstDrill | impact-drill, eruption-drill | `DrillBehavior` (plain drill loop) | BROKEN: no burst semantics; required water/hydrogen never accepted | `BurstDrill` handled as `Drill` (`production.rs:442`) | T1, deferred D3 | ✓ |
| production/BeamDrill | plasma-bore, large-plasma-bore | `BeamDrillBehavior` | PARTIAL: hydrogen (large: required) never accepted; no IO | | T1, T17 | J ✗ |
| production/WallCrafter | cliff-crusher, large-cliff-crusher | `WallCrafterBehavior` | OK | | | – |
| production/Pump | mechanical/rotary/impulse/reinforced pump | `PumpBehavior` | OK (PAR-0078 floor read + `pump_can_place_on`) | test `pumps_read_the_floor_liquid_and_feed_conduits` | §4 | – |
| production/SolidPump, Fracker | water-extractor, oil-extractor | `SolidPumpBehavior` | OK (T7 of the previous plan) | tests `water_extractor_scales_with_floor`, `oil_extractor_needs_oil_floor` | | – |
| production/GenericCrafter, AttributeCrafter | all Serpulo crafters, cultivator, Erekir silicon-arc-furnace, electrolyzer, vent-condenser | `CrafterBehavior` (`production.rs:108`) | OK | tests `full_crafter_stops_consuming` etc. | | ✓ |
| production/Separator | separator, disassembler | `CrafterBehavior` | UNVERIFIED distribution | | T18 | ✓ |
| production/HeatCrafter | carbide-crucible, surge-crucible, cyanogen-synthesizer, phase-synthesizer, atmospheric-concentrator | `HeatCrafterBehavior` (`heat/behavior.rs:90`) | BROKEN: delegates only `update_tile` to `CrafterBehavior`; no `accept_item/liquid`, no `should_consume`, no IO → never receives inputs | impl list | T1 | J ✗ |
| heat/HeatProducer | oxidation-chamber, electric-heater, slag-heater, phase-heater, heat-reactor, heat-source | `HeatProducerBehavior` (`heat/behavior.rs:42`) | BROKEN: same delegation gap; `oxidation-chamber` has no output knob (no oxide) | `apply_vanilla_knobs` has no `"oxidation-chamber"` | T1 | J ✗ |
| heat/HeatConductor | heat-redirector, small-heat-redirector, heat-router | `HeatConductorBehavior` | OK | `heat/tests.rs` | | – |
| production/Incinerator | incinerator | `IncineratorBehavior` (`production.rs:1344`) | BROKEN: no `accept_item/liquid` → never receives anything; no `heat` ramp | Java `Incinerator.java:30-81` | T5 | – |
| production/ItemIncinerator | slag-incinerator | `CrafterBehavior` (wrong class) | BROKEN | `default_behavior` maps `ItemIncinerator` to crafter | T5 | – |
| power/ConsumeGenerator | combustion, steam, differential, rtg, chemical-combustion-chamber, pyrolysis | `ConsumeGeneratorBehavior` (`power/behavior.rs:189`) | OK intake (PAR-0075 fixed); no IO | test `combustion_generator_accepts_coal_and_generates` | §4, T17 | J ✗ |
| power/SolarGenerator | solar-panel, solar-panel-large | `SolarGeneratorBehavior` | OK; no IO (`PowerGenerator.write` productionEfficiency) | | T17 | J ✗ |
| power/ThermalGenerator | thermal-generator, turbine-condenser | `ThermalGeneratorBehavior` | UNVERIFIED | | T18 | J ✗ |
| power/NuclearReactor | thorium-reactor | `NuclearReactorBehavior` | BROKEN: thorium + cryofluid never accepted | impl list (no accept) | T1, T17 | J ✗ |
| power/ImpactReactor | impact-reactor | `ImpactReactorBehavior` | BROKEN: blast compound + cryofluid never accepted | impl list | T1, T17 | J ✗ |
| power/HeaterGenerator | neoplasia-reactor | `HeaterGeneratorBehavior` | OK intake | | T17 | J ✗ |
| power/VariableReactor | flux-reactor | `VariableReactorBehavior` | BROKEN: cyanogen never accepted | | T1, T17 | J ✗ |
| power/PowerNode | power-node, power-node-large, surge-tower | `PowerNodeBehavior` (`power/nodes.rs:43`) | BROKEN: live spawn overwrites knobs with `PowerNodeConfig::default()` (max 3, range 6); `configured` MISSING (tap/Configure does nothing); tap-link MISSING | `core/sim/runtime.rs` `SimHooks::new_building` inserts the default after `inst.spawn` | T2, T10 | (links in base) |
| power/LongPowerNode | beam-link | `PowerNodeBehavior` | same as above | | T2 | |
| power/Battery | battery, battery-large | `NoopBehavior` + graph | OK | `power/tests.rs` | | – |
| power/PowerDiode | diode | `NoopBehavior` | NOT WIRED: `nodes::update_diode` (`power/nodes.rs:558`) never called | `grep -rn update_diode` = definition only | T2 | – |
| power/BeamNode | beam-node, beam-tower | `NoopBehavior` | MISSING (Erekir power transport) | | T16 | – |
| power/PowerSource, PowerVoid, sandbox/* | sandbox | `sandbox.rs` behaviors | OK | tests `power_source_powers_consumer`, `item_source_feeds_conveyor_into_void` | T11 (bridge seam) | ✓ |
| liquid/Conduit, ArmoredConduit | conduit, pulse-conduit, plated-conduit, reinforced-conduit | `LiquidConduitBehavior` | PARTIAL: accept ignores the current-liquid rule; liquid reactions (fire/steam) MISSING | `movement.rs:95-116` vs `Conduit.java:241-245` | T15 | – |
| liquid/LiquidRouter | liquid-router, liquid-container, liquid-tank, reinforced-* | `LiquidRouterBehavior` | PARTIAL: same accept rule | `LiquidRouter.java:41-43` | T15 | – |
| liquid/LiquidJunction | liquid-junction, reinforced-liquid-junction | `LiquidJunctionBehavior` | OK | | | – |
| liquid/LiquidBridge | bridge-conduit, phase-conduit | `LiquidBridgeBehavior` (`liquid/behavior.rs:376`) | BROKEN: no `config/configured/link_valid_between/write/read` → can never be linked (no command, no tap, no auto-chain), links lost on save | impl list | T10 (step 1), T17 | J ✗ |
| liquid/DirectionLiquidBridge | reinforced-bridge-conduit | `DirectionLiquidBridgeBehavior` | OK | | | – |
| defense/Wall | copper..thorium, plastanium, phase, surge, beryllium, tungsten, carbide walls | `WallBehavior` | PARTIAL: surge lightning and phase deflection MISSING | `WallDef.lightning_chance` exists but unused; `combat/bullet/mod.rs hit_building` has no wall hook | T14 | – |
| defense/Door, AutoDoor | door, door-large, blast-door | `DoorBehavior`, `AutoDoorBehavior` | OK (T13 of prev plan) | test `door_toggles_and_throttles` | | ✓ |
| defense/MendProjector | mender, mend-projector | `MendProjectorBehavior` | BROKEN: heals every team; silicon/phase boost never applied (phase_valid hard-coded false) and never consumed | `behaviors.rs:94` passes `true,false`; `shields.rs:428` no team filter | T4 | J ✗ |
| defense/OverdriveProjector | overdrive-projector, overdrive-dome | `OverdriveProjectorBehavior` | BROKEN: boosts every team (incl. enemy); `consume()` never called; dome inputs never accepted | `projectors.rs:88-142` | T1, T4 | J ✗ |
| defense/ForceProjector | force-projector | `ForceProjectorBehavior` (`behaviors.rs:62`) | BROKEN: live path never deflects bullets (`deflect_bullets` only from `combat/harness.rs`); phase-fabric boost and coolant consumers absent from the spec; efficiency-0 report UNVERIFIED | `defense.rs:543 consumes: vec![consume_power(4.0)]` | T3 | J ✗ |
| defense/ShockMine | shock-mine | `ShockMineBehavior` | NOT WIRED: `shock_mine_unit_on` (`shields.rs:517`) only called by tests | | T13 | – |
| defense/RegenProjector, ShockwaveTower, BaseShield, ShieldWall, Radar, BuildTurret | Erekir defense | behaviors exist | PARTIAL: shockwave-tower spec has `consume_liquids(vec![])` (cyanogen dropped); BaseShield despawns bullets without `absorbable` check; ShieldWall does not absorb damage; no IO | `content/registries/blocks/defense.rs:621` | T1 (spec), deferred D4 | J ✗ |
| defense/turrets/* (block side) | all turrets | `TurretBehavior` (`turrets/behavior.rs:45`) | OK ammo/coolant intake + IO | tests in `turrets/behavior.rs`, `combat_tests.rs duo_water_coolant_speeds_fire_and_drains` | (G01 owns firing) | ✓ |
| units/UnitFactory, Reconstructor | ground/air/naval factory, additive..tetrative | `UnitFactoryBehavior`, `ReconstructorBehavior` | OK (prev plan T1-T4) | | | ✓ |
| units/RepairTurret | repair-point, repair-turret | `RepairTurretBehavior` (`units/behavior.rs:815`) | MISSING: only a target timer; never heals | `units/mod.rs:555 update_target` | T12 | J ✗ |
| units/RepairTower | repair-tower (Erekir) | `RepairTowerBehavior` | MISSING heal | | deferred D4 | – |
| units/UnitAssembler, UnitAssemblerModule | Erekir assemblers | `AssemblerBehavior` | UNVERIFIED | | deferred D4 | J ✗ |
| units/UnitCargoLoader, UnitCargoUnloadPoint | Erekir cargo | `CargoLoaderBehavior`, `CargoUnloadPointBehavior` | PARTIAL: loader cannot accept items; unload point has no item config | | deferred D4 | J ✗ |
| payloads/* | payload conveyor/router, mass drivers, constructor, deconstructor, loader, unloader, source, void | `payloads/*.rs` | OK structure; constructor UNVERIFIED; payload-mass-driver tap-link MISSING | | T10, T18 | ✓ |
| power/LightBlock | illuminator | `LightBehavior` | OK (render is G03) | | | ✓ |
| world/Build.java validPlace / validBreak | all | `core/world/build.rs:144-300` | BROKEN: see T8 list | | T7, T8 | |
| world/blocks/ConstructBlock + BuilderComp economy | all | `core/sim/builder.rs`, `core/world/construct.rs` | OK (cost multiplier, refund, core insufficiency read from the campaign `Rules`) | tests `player_place_builds_over_time_and_spends_core_items`, `player_break_deconstructs_and_refunds`, `no_items_means_no_construction` | | ✓ |

Out of area (not in the table): logic blocks (G06), launch/landing pads, accelerator (G04),
environment blocks.

### 1.3 Already done / verified — do not redo

* PAR-0075 (fuel generators accept items): fixed in code; `ConsumeGeneratorBehavior::accept_item`
  (`core/world/blocks/power/behavior.rs:225`); test `combustion_generator_accepts_coal_and_generates`
  (`core/sim/production_tests.rs:41`). Only an in-engine belt check remains (§4).
* PAR-0078 (pumps): regular pumps read `floor.liquid_drop` and `pump_can_place_on`
  (`core/world/build.rs`) ports `Pump.canPlaceOn`; solid pumps fixed by session-4 T7.
  Test `pumps_read_the_floor_liquid_and_feed_conduits`. In-engine check in §4.
* PAR-0089 (sublimate): activation latch fixed; test `sublimate_activation_window`
  (`core/world/blocks/defense/turrets/advanced.rs:2415`). Headless only (no Erekir sector).
* Previous building plan T0-T11, T13, T14, T18 are merged (shouldConsume, factories,
  reconstructor command, solid pumps, placed() auto-link, door taps).
* Builder economy (cost multiplier, refund multiplier, `shouldSkip`, quick rotate,
  unit overlap) is ported in `core/sim/builder.rs` reading `CampaignRuntime.session.rules`.

---

## 2. Tasks

Conventions for every task: tests go in `core/sim/production_tests.rs` unless stated
(helpers `runtime_sim()`, `place(&mut sim, name, x, y)`, `item()`, `liquid()`,
`set_floor()`, `stored_item()`, `stored_liquid()`, `configure()`); combat-flavoured tests go
in `core/sim/combat_tests.rs` (helpers `live_sim(size)`, `place(&mut sim, x, y, name, team)`,
`sim.spawn_live_unit(name, team, px, py, rot)`). Multiblock footprint rule: a size-`s`
block at `(x,y)` covers `x-(s-1)/2 ..= x+s/2` (3x3 at 7 covers 6..8; 2x2 at 5 covers 5..6).

### T1. Java default item/liquid intake + `canUnload` for every block (systemic)

**Symptom / acceptance.** Today a conduit of water next to a laser drill never boosts it,
a belt of silicon into a mender never boosts it, an overdrive dome never runs, a thorium
reactor / impact reactor / flux reactor can never be fuelled, Erekir heat crafters
(carbide crucible...) and heat producers never receive inputs, impact/eruption drills and
the large plasma bore (water/hydrogen required) never run, and an unloader next to a
crafter/drill/generator pulls nothing. After: each of those accepts exactly the items and
liquids its consumers declare, up to `itemCapacity`, and unloaders pull from every block
whose Java class does not set `unloadable = false`.
Acceptance: the four tests below pass; in-engine a mender fed by a belt shows
`building_debug(x,y).optional_efficiency == 1.0`.

**Java.**
* `J/entities/comp/BuildingComp.java:869-871` `acceptItem`: `return block.consumesItem(item) && items.get(item) < getMaximumAccepted(item);`
* `:865-867` `handleItem`: `items.add(item, 1);`
* `:787-789` `getMaximumAccepted`: `return block.itemCapacity;`
* `:779-785` `acceptStack`: `if(acceptItem(self(), item) && block.hasItems && (source == null || source.team() == team)) return Math.min(getMaximumAccepted(item) - items.get(item), amount); else return 0;`
* `handleStack` (same file): `noSleep(); items.add(item, amount);`
* `:873-875` `acceptLiquid`: `return block.hasLiquids && block.consumesLiquid(liquid);`
* `:877-879` `handleLiquid`: `liquids.add(liquid, amount);`
* `:727-729` `canUnload`: `return block.unloadable;` — `Block.unloadable = true` (`J/world/Block.java:108`);
  set `false` by: Unloader, Junction, OverflowDuct, Duct, Sorter, DuctJunction, Router,
  ItemBridge (+BufferedItemBridge), DuctRouter, DirectionalUnloader, Conveyor (+ArmoredConveyor),
  OverflowGate (grep `unloadable = false` under `J/world/blocks/distribution` and `storage/Unloader.java:45`).
* `Block.consumesLiquid` = `liquidFilter[id]`, filled by `ConsumeLiquid`, `ConsumeLiquids`
  and `ConsumeCoolant` (`J/world/consumers/ConsumeCoolant.java`): coolant filter =
  `liquid.coolant && (allowLiquid && !liquid.gas || allowGas && liquid.gas) && liquid.temperature <= 0.5 && liquid.flammability < 0.1`.

**Port today.**
* Trait defaults in `core/world/behavior/mod.rs` (`fn accept_item` … `fn can_unload`) all return
  `false`/`0`/no-op.
* Correct helpers already exist in `core/world/behavior/helpers.rs`:
  `consumer_accept_item`, `consumer_accept_liquid`, `block_consumes_item`,
  `block_consumes_liquid` (Liquid/Liquids only — **no Coolant**), `default_handle_liquid`;
  and in `core/world/blocks/distribution/transfer.rs`: `default_handle_item`,
  `default_accept_stack`, `default_handle_stack`, `get_maximum_accepted`.
* `HeatCrafterBehavior`/`HeatProducerBehavior` (`core/world/blocks/heat/behavior.rs:42,90`)
  delegate only `create_state`/`update_tile` to `production::CrafterBehavior`.
* `shockwave-tower` spec (`core/content/registries/blocks/defense.rs:621`) has
  `consume_liquids(vec![])`; Java `Blocks.java:2043`: `consumeLiquids(LiquidStack.with(Liquids.cyanogen, 1.5f / 60f))`.
* `oxidation-chamber` has no crafter output knob (`core/world/block_kind_data.rs`
  `apply_vanilla_knobs` has no arm). Java: `oxidationChamber` in `J/content/Blocks.java`
  (search `oxidationChamber = new HeatProducer`): read its `outputItem`, `craftTime`,
  consumers and transcribe them into a new `"oxidation-chamber"` arm (`self.set_crafter(craft_t(vec![(item("oxide"), 1)], vec![], <craftTime>, false))`).
  Expected Java values: `outputItem = new ItemStack(Items.oxide, 1)`, `craftTime = 60*2f`;
  confirm before writing.

**Changes.**
1. `core/world/behavior/helpers.rs`: extend `block_consumes_liquid` with
   `ConsumeInstanceKind::Coolant { allow_liquid, allow_gas, .. }` (check the variant's fields
   with `grep -n "Coolant" core/world/consumers.rs`; if the instance does not carry
   `allow_liquid/allow_gas`, carry them over from `ConsumeDef::Coolant` in the lowering).
   The coolant arm needs liquid defs: read them through a new resource-free path —
   add `pub fn coolant_accepts(def: &LiquidDef, allow_liquid: bool, allow_gas: bool) -> bool`
   (formula above) and look up the `LiquidDef` via the `ContentIndex`-like table: if no
   liquid table is reachable from `World`, store a `Vec<bool>` "coolant-eligible (liquid, gas)"
   per liquid in `ModuleDims`'s sibling resource `LiquidTraits` inserted by
   `install_block_runtime_from` (`core/sim/runtime.rs`) next to `ContentIndex`
   (fields `coolant: Vec<bool>`, `gas: Vec<bool>`, `temperature: Vec<f32>`,
   `flammability: Vec<f32>`, `incinerable: Vec<bool>`, `heat_capacity: Vec<f32>`;
   from `content.liquids()`; field names `coolant`, `gas`, `temperature`, `flammability`,
   `incinerable` verified in `core/content/registries/liquids.rs:68-94`; `heat_capacity`:
   `grep -n heat_capacity core/content/registries/liquids.rs`).
2. `core/world/behavior/mod.rs` trait defaults:
   * `accept_item` → `helpers::consumer_accept_item(world, e, item)`
   * `handle_item` → `transfer::default_handle_item(world, e, src, item)`
   * `get_maximum_accepted` → `transfer::get_maximum_accepted(world, e, item)`
   * `accept_stack` → `if world.get::<ItemModule>(e).is_some() && self.accept_item(world, e, source.unwrap_or(e), item) && same-team(source) { (max - count).min(amount) } else { 0 }` (write it inline; `default_accept_stack` calls the free default, not the trait).
   * `handle_stack` → `transfer::default_handle_stack`
   * `accept_liquid` → `helpers::consumer_accept_liquid(world, e, liquid)`
   * `handle_liquid` → `helpers::default_handle_liquid(world, e, liquid, amount)`
   * `can_unload` → new `helpers::default_can_unload(world, e)`: `ItemModule` present and the
     block kind is not one of `Unloader, Junction, OverflowDuct, Duct, Sorter, DuctJunction,
     Router, ItemBridge, BufferedItemBridge, DuctRouter, DirectionalUnloader, Conveyor,
     ArmoredConveyor, OverflowGate` (check the exact `BlockKind` variant names with
     `grep -n "OverflowDuct\|DuctJunction\|ArmoredConveyor" core/content/registries/blocks/mod.rs`).
   Behaviors that already override keep their override (CoreBehavior, StorageBehavior,
   PayloadLoaderBehavior override `can_unload`; logistics behaviors that rely on the old
   `false` default for `can_unload` now get it from the kind list).
3. `core/world/blocks/heat/behavior.rs`: in both `HeatProducerBehavior` and
   `HeatCrafterBehavior` add delegating methods to `production::CrafterBehavior` for
   `should_consume`, `accept_item`, `get_maximum_accepted`, `handle_item`, `accept_liquid`,
   `handle_liquid`, `accept_stack`, `write`, `read` (HeatProducer: also write/read its `heat`
   after the crafter fields — Java `HeatProducer.java` `write`: `super.write(write); write.f(heat);`).
4. `core/content/registries/blocks/defense.rs:621`: `consume_liquids(vec![liquid_stack("cyanogen", 1.5 / 60.0)])`
   (helper `liquid_stack` is imported at the top of the crafting file; import it here too).
   Regenerate `parity/golden_content.json` + manifest hash (env doc §2), commit body names
   the reason.
5. `core/world/block_kind_data.rs` `apply_vanilla_knobs`: add `"oxidation-chamber"` (values from Java, see above).

**Tests** (`core/sim/production_tests.rs`):
* `drill_accepts_water_boost`: `runtime_sim()`; `set_floor` `ore-copper` on (5,5),(6,5),(5,6),(6,6)
  (use `sim.grid.tiles.get_mut(x,y).overlay` if ores are overlays — check how
  `laser_drill_counts_centered_footprint` sets ore and copy it); `place "mechanical-drill"` at (5,5);
  `place "liquid-source"` at (4,5); `configure(4,5, Content("water"))`; tick 120.
  Assert `stored_liquid(drill, water) > 0.0`, `Building.optional_efficiency == 1.0`, and
  `DrillState.warmup > 1.0` (target speed is `lerp(1, 1.6, 1) = 1.6`, approach 0.015/tick →
  after 120 ticks warmup = 1.6; without the boost it caps at 1.0).
* `mender_accepts_silicon_and_reactor_accepts_thorium`: place `mender` and `thorium-reactor`;
  `transfer::dispatch_accept_item(world, mender, mender, silicon)` true, copper false;
  reactor: thorium true, copper false; `liquid::movement::accept_liquid(world, reactor, reactor, cryofluid)` true, water false.
* `unloader_pulls_from_crafter`: `graphite-press` at (5,5) (2x2: 5..6) with 5 graphite added via
  `transfer::dispatch_handle_stack`, `unloader` at (7,5) configured `Content("graphite")`,
  `container` at (8,5); tick 60; container graphite ≥ 1.
* `heat_crafter_accepts_inputs`: `carbide-crucible` accepts tungsten and graphite, rejects copper.

**Verify in-engine** (Ground Zero, env doc §4.1): seed `debug_add_core_items("silicon", 50)`;
`place_block(140,60,"mender")` (1x1), `place_block(139,60,"power-source")`,
`place_block(141,60,"item-source")` (read each back with `tile_debug`), then
`$I.configure_building(141,60,"content","silicon")`; step 120; expect
`$H.building_debug(140,60)["optional_efficiency"]` = 1.0 and `building_items_json(140,60)` contains silicon.

**Pitfalls.** The trait default change touches every block: run the full
`cargo test -p mind-core --lib --features msav-import` and both headless goldens; a changed
runtime checksum golden is expected only if a scenario feeds a block that used to reject —
then name the scenario in the commit. Do not change `NoopBehavior`'s identity (it inherits
defaults). Liquid blocks never reach the behavior path (they have `LiquidNode`).

Ordering with G08: G08 T2a adds `CoreBehavior`/`TurretBehavior` `accept_stack`/`handle_stack`/
`remove_stack` overrides and `transfer::dispatch_accept_stack`. G02 T1 lands **before** G08 T2a.
Do **not** add Core/Turret stack overrides here; only change the trait defaults.

**Commit.** `Gameplay G02 T1: Java default item/liquid intake and canUnload for every block`

### T2. Power nodes keep their knobs, accept Configure, and diodes run

**Symptom / acceptance.** A placed `power-node-large` links at most 3 targets within 6 tiles
(Java: 15 within 15); `surge-tower` likewise. `configure_building(node, "int", <packed>)`
does nothing. A diode never moves power. After: `PowerNodeConfig` of a placed large node is
`{max_nodes:15, laser_range:15}`; a Configure with a packed target toggles a laser link
(link/unlink + graph merge/split); a diode between a charged battery graph and an empty one
transfers power.

**Java.**
* `J/content/Blocks.java:2483-2505`: power-node maxNodes 10 laserRange 6; power-node-large 15/15; surge-tower 2/40.
* `J/world/blocks/power/PowerNode.java` config handlers (search `config(Integer.class`):
  Integer = toggle link (`links.contains(value)` → unlink both ends, reflow both graphs;
  else if `linkValid(entity, other) && valid && power.links.size < maxNodes` → add both ends
  (other end only if same team), `power.graph.addGraph(other.power.graph)`); `Point2[]` =
  unlink all old, then link every `tile + p`.
* `J/world/blocks/power/PowerDiode.java` `updateTile`: (transcribed; already ported as
  `nodes::update_diode`, `core/world/blocks/power/nodes.rs:558`) uses `front()`/`back()`.

**Port today.**
* `core/sim/runtime.rs` `impl WorldHooks for SimHooks::new_building`: after `inst.spawn(...)`
  (which runs `create_state` → `PowerNodeBehavior` inserts the real config) it does
  `if matches!(inst.def.kind, BlockKind::PowerNode) { world.entity_mut(entity).insert(PowerNodeConfig::default()); }`
  — overwriting it. `PowerNodeConfig::default()` = (3, 6.0) (`core/world/blocks/power/mod.rs:125`).
* `nodes::configure_link(graphs, world, grid, tile, packed)` and `nodes::configure_links(..., points)`
  exist (`nodes.rs:423, 502`) and implement the Java handlers; nothing calls them from
  `Sim::command` (`SimCommand::Configure`, `core/sim/mod.rs:1020`) because
  `PowerNodeBehavior` has no `configured` and behaviors cannot see the grid.
* Diode: kind `PowerDiode` resolves to `NoopBehavior`.

**Changes.**
1. `SimHooks::new_building`: insert the default only when absent:
   `if matches!(inst.def.kind, BlockKind::PowerNode | BlockKind::LongPowerNode) && world.get::<PowerNodeConfig>(entity).is_none() { ... }`.
2. `core/sim/mod.rs` `SimCommand::Configure` arm: after resolving `world_value`, before
   `world::config::configure`, if the target entity has `PowerNodeConfig`:
   * `WorldConfigValue::Number(n)` → `packed = n as i32`;
     `world.resource_scope(|world, mut graphs: Mut<PowerGrids>| nodes::configure_link(&mut graphs, world, &self.grid, entity, packed))`
     (or remove/insert the resource as `SimRuntime::merge_power_graph` does);
   * `WorldConfigValue::Point2(x, y)` → `packed = TilePos::new(tile.x()+x, tile.y()+y).pack()` and the same;
   * `WorldConfigValue::Point2Array(points)` → `configure_links(..., &packed_relative)` (the
     function takes `crate::world::pos::pack`ed relative offsets; check `nodes.rs:502-525`).
   Then `commands_applied += 1; return Ok(())`.
3. New `DiodeBehavior` in `core/world/blocks/power/behavior.rs`: `update_tile` →
   `if let (Some(f), Some(b)) = (transfer::front(world,e), transfer::back(world,e)) { nodes::update_diode(world, f, b) }`
   (`transfer::front` `transfer.rs:235`, `back` `:311`); register in `power::register` for
   `"diode"` (or in `default_behavior` for `K::PowerDiode`).

**Tests.** `core/world/blocks/power/tests.rs` or `production_tests.rs`:
* `large_node_keeps_knobs_on_live_place`: `runtime_sim`, `place "power-node-large"` → its
  `PowerNodeConfig` is (15, 15.0); same for `surge-tower` (2, 40.0).
* `node_configure_toggles_link`: `power-node` at (5,5), `battery` at (10,5) (5 tiles, in range 6),
  `configure(5,5, Int(TilePos::new(10,5).pack()))` → node `PowerModule.links` contains it and both
  share a graph; repeat → removed and graphs differ.
  Note `placed_hooks` auto-links on place; place the battery first, then the node via
  `set_block_raw`-free path... simplest: place both, clear with `Point2Array([])`
  (`configure(5,5, Bytes?)`) — if the wire has no Point2Array form, unlink by sending the same
  Int once (toggle) and assert the link is gone, then send it again and assert it is back.
* `diode_moves_power_forward`: battery A (2,5) + diode (3,5) rot 0 + battery B (4,5); fill A's
  `PowerModule.status = 1.0` (buffered status); tick 10; B status > 0 and A < 1.

**Verify in-engine.** Ground Zero: `place_block(150,60,"power-node-large")`; there is no
seam for `PowerNodeConfig`: use `$H.power_debug(150,60)` before/after
`$I.configure_building(150,60,"int", str(packed))` with a battery 10 tiles away
(`packed = y*65536 + x`? confirm `TilePos::pack` formula with `grep -n "pub fn pack" core/world/pos.rs`);
expect the battery's `graph_id` to equal the node's.

**Pitfalls.** `remove_resource::<PowerGrids>` must always be re-inserted (early returns!).
Do not route nodes through `world::config::configure`. Keep `materialize_buildings` unaffected
(it does not use `new_building`'s insert? check; if it does, the fix is the same).

**Commit.** `Gameplay G02 T2: power nodes keep knobs, accept link configs; diodes transfer power`

### T3. Force projector: power diagnosis, live bullet absorption, phase boost, coolant

**Symptom / acceptance.** Rendering session reported a placed force projector staying at
efficiency 0.0 next to power sources (UNVERIFIED cause; the most likely cause is a rejected
`place_block` — a source placed inside the 3x3 footprint — which PAR-0165 reports as success).
Independently, in the live game the shield never absorbs bullets (only the combat harness
calls `deflect_bullets`), phase fabric never boosts it, and coolant never speeds recovery.
After: powered projector reaches efficiency 1; enemy bullets entering the hexagon are
absorbed and add buildup; with phase fabric the radius grows by up to 80.

**Java.** `J/world/blocks/defense/ForceProjector.java`:
* constructor `:72-87`: `hasPower = hasLiquids = hasItems = true`; `consume(coolantConsumer = new ConsumeCoolant(coolantConsumption=0.1f)).boost().update(false)`.
* `Blocks.java:1966-1978`: radius 101.7, phaseRadiusBoost 80, shieldHealth 750, cooldownNormal 1.5, cooldownLiquid 1.2, cooldownBrokenBase 0.35, `itemConsumer = consumeItem(phaseFabric).boost()`, `consumePower(4f)`. Defaults: `phaseUseTime = 350`, `phaseShieldBoost = 400`, `sides = 6`, `shieldRotation = 0`.
* `updateTile` `:207-257` (order matters):
  ```
  phaseValid = itemConsumer != null && itemConsumer.efficiency(this) > 0   // has 1 phase fabric
  phaseHeat = lerpDelta(phaseHeat, phaseValid?1:0, 0.1)
  if(phaseValid && !broken && timer(timerUse, phaseUseTime/timeScale) && efficiency > 0) consume();   // removes 1 phase fabric
  radscl = lerpDelta(radscl, broken ? 0 : warmup, 0.05)                    // uses OLD warmup
  warmup = lerpDelta(warmup, efficiency, 0.1)
  if(buildup > 0){ scale = !broken ? cooldownNormal : cooldownBrokenBase;
     if(coolantConsumer.efficiency(this) > 0){ coolantConsumer.update(this); scale *= cooldownLiquid * (1 + (liquids.current().heatCapacity - 0.4) * 0.9); }
     buildup -= delta() * scale; }
  if(broken && buildup <= 0) broken = false;
  if(buildup >= shieldHealth + phaseShieldBoost*phaseHeat && !broken){ broken = true; buildup = shieldHealth; }
  if(hit > 0) hit -= 1/5f * delta;
  deflectBullets();   // realRadius>0 && !broken: every bullet with team != this.team, type.absorbable, !absorbed, inside the regular hexagon → absorb(), hit = 1, buildup += damage * shieldDamageMultiplier
  ```

**Port today.**
* Spec `core/content/registries/blocks/defense.rs:528-545`: `consumes: vec![consume_power(4.0)]` — no phase, no coolant.
* `ForceProjectorBehavior::update_tile` (`core/world/blocks/defense/behaviors.rs:67-74`)
  calls `update_force_projector_state(world, e, &mut state, eff, false, 0.0)`
  (`shields.rs:176`) — phase always false, coolant 0, warmup updated before radscl (wrong
  order), no deflection. `deflect_bullets(ctx: &mut CombatCtx, ...)` (`shields.rs:247`) needs
  `CombatCtx` only for `ctx.content.bullet(def)`.

**Changes.**
1. Diagnose first: test `force_projector_powered_by_adjacent_source` (below). If it fails,
   stop and report the failing value (`Building.efficiency`, `PowerModule.status`, graph
   membership); do not guess.
2. Spec: `consumes: vec![consume_optional(consume_items(vec![stack("phase-fabric", 1)])), consume_power(4.0), consume_optional(consume_coolant(0.1, true, false))]`
   — the coolant consumer must be `update(false)`: set `update: false` on the `ConsumeDef::Coolant`
   (field exists, `consume_coolant` sets `update: true`; write the struct literal). Order per
   Java: power and phase from Blocks, coolant from the constructor (constructor runs first →
   coolant index 0; keep `[coolant, phase, power]`). Regenerate the content golden.
3. `shields.rs`: change `update_force_projector_state` signature to
   `(world, e, state, eff, phase_valid: bool, coolant_heat_capacity: f32)` unchanged, but fix
   the order (radscl before warmup). Add `pub fn deflect_bullets_world(world: &mut World, content: &ContentRegistry, state: &mut ForceProjectorState, team: u8, x: f32, y: f32)` with the body of `deflect_bullets` using `world`/`content`; make `deflect_bullets` call it.
4. `ForceProjectorBehavior::update_tile`:
   * `phase_valid` = item module has ≥1 `phase-fabric` (resolve the id once in `register` and
     store it on the behavior: `phase_item: Option<ItemId>`);
   * every `350 / time_scale` ticks (use `run_timer(world, e, 1, 350.0 / ts)`; timer index 1 is
     free — check `Timers` length) with `phase_valid && !broken && eff > 0`: remove 1 phase fabric;
   * coolant: if `buildup > 0` and the liquid module's current liquid amount ≥ `0.1 * edelta`,
     remove `0.1 * edelta` of it and pass its `heat_capacity` (from `LiquidTraits`, T1 step 1);
   * then bullets: `if let Some(rt) = world.remove_non_send_resource::<UnitRuntime>() { deflect_bullets_world(world, &rt.content, ...); world.insert_non_send_resource(rt); }`
     (the exact remove/insert method names: copy from `bullet_update_system`, `core/sim/unit_runtime.rs:1547`).
5. `gdext/sim_host.rs building_debug`: add `force_radscl`, `force_buildup`, `force_broken`
   from `ForceProjectorState` (pattern: the `SandboxState` block in the same function).

**Tests.**
* `force_projector_powered_by_adjacent_source` (`production_tests.rs`): `power-source` at (5,5),
  `force-projector` at (7,5) (covers 6..8); tick 60; `Building.efficiency == 1.0`;
  `ForceProjectorState.warmup > 0.99` (lerp 0.1/tick: 1-0.9^60 ≈ 0.998) and `radscl > 0.9`.
* `force_projector_absorbs_enemy_bullet` (`combat_tests.rs`, `live_sim(48)`): projector team 1 at
  (20,20) + power-source (22,20); tick 120 (radscl ≈ 1 → real radius ≈ 101.7); spawn an enemy
  `dagger` team 2 at world (20*8+40, 20*8+4) facing it and tick until a bullet exists; after
  60 more ticks assert `ForceProjectorState.buildup > 0` and `hit > 0` or the bullet count
  dropped while the dagger is alive. Bullet damage of a dagger shot = 20 → buildup after one
  absorption ≥ 20 − 1.5·ticks.
* `force_projector_phase_boost`: add 2 phase fabric; tick 30; `phase_heat > 0.9`; tick 360;
  phase fabric count decreased by 1.

**Verify in-engine.** Ground Zero; `place_block(150,62,"force-projector")` (covers 149..151, 61..63),
`place_block(153,62,"power-source")`; read back `$H.tile_debug(153,62)` (block must be
power-source); step 120; `$H.building_debug(150,62)` → `efficiency` 1.0, `force_radscl` ≈ 1.
Spawn a dagger of team 2 near it (`apply_sim_command_json` `spawn_unit`, unit id
`content_list("unit").find("dagger")`), step 300, `force_buildup` > 0. Screenshot for G03.

**Pitfalls.** Never hold `UnitRuntime` across `return`. The bullet loop iterates
`world.iter_entities()` — collect ids first (as the existing code does). Determinism: no RNG here.

**Commit.** `Gameplay G02 T3: force projector absorbs bullets live, phase boost and coolant`

### T4. Mender / mend projector / overdrive: team filter, boost items, consume

**Symptom / acceptance.** A mender heals enemy buildings in range and an overdrive projector
speeds up enemy turrets; silicon/phase fabric boosts never apply and never get used up.
After: only same-team buildings are healed/boosted; with silicon the mender's heal adds
`phaseBoost` percent and range `phaseRangeBoost`; items are consumed every `useTime`.

**Java.**
* `J/world/blocks/defense/MendProjector.java:93-123` (transcribed):
  ```
  canHeal = !checkSuppression()            // port: true (heal suppression = Erekir unit ability; deferred)
  smoothEfficiency = lerpDelta(smoothEfficiency, efficiency, 0.08)
  heat = lerpDelta(heat, efficiency > 0 && canHeal ? 1 : 0, 0.08)
  charge += heat * delta()
  phaseHeat = lerpDelta(phaseHeat, optionalEfficiency, 0.1)
  if(optionalEfficiency > 0 && timer(timerUse, useTime / timeScale) && canHeal) consume();
  if(charge >= reload && canHeal){ realRange = range + phaseHeat*phaseRangeBoost; charge = 0;
     indexer.eachBlock(this, realRange, b -> b.damaged() && !b.isHealSuppressed(),
        other -> other.heal(other.maxHealth() * (healPercent + phaseHeat*phaseBoost) / 100 * efficiency)) }
  ```
  `MendProjector.useTime = 400` (check the field default). Blocks: mender reload 200, range 40, healPercent 4, phaseBoost 4, phaseRangeBoost 20, `consumeItem(silicon).boost()`, power 0.3; mend-projector reload 250, range 85, healPercent 11, phaseBoost 15, `consumeItem(phaseFabric).boost()`, power 1.5 (`Blocks.java:1921-1944`).
* `OverdriveProjector.java:97-125`: charge/heat as above with `charge += heat * Time.delta`;
  `if(hasBoost) phaseHeat = lerpDelta(phaseHeat, optionalEfficiency, 0.1)`; at `charge >= reload(60)`:
  `eachBlock(this, range + phaseHeat*phaseRangeBoost, other.block.canOverdrive, other.applyBoost(realBoost(), reload + 1))`,
  `realBoost = (speedBoost + phaseHeat*speedBoostPhase) * efficiency`;
  `if(efficiency > 0) useProgress += delta(); if(useProgress >= useTime){ consume(); useProgress %= useTime; }`.
  Defaults reload 60, range 80, speedBoost 1.5, speedBoostPhase 0.75, useTime 400, phaseRangeBoost 20; dome range 200, speedBoost 2.5, useTime 300, `hasBoost = false`, `consumeItems(phase 1, silicon 1)`.
* `BlockIndexer.eachBlock(team, wx, wy, range, ...)` (`J/ai/BlockIndexer.java:328-361`):
  same team only, `b.within(wx, wy, range + b.hitSize()/2)` (`hitSize = size * 8`), skips
  `block.privileged`.
* `BuildingComp.consume()`: every consumer's `trigger` → `ConsumeItems` removes its stacks (optional ones too).

**Port today.** `MendProjectorBehavior::update_tile` passes `(eff, true, false)` to
`update_mend_projector_world` (`shields.rs:428-475`), which heals every `Building` within
`range` of the centre, any team, and lerps phase toward `phase_valid` (false). Overdrive:
`update_overdrive_projector` (`projectors.rs:88-116`) uses `buildings_in_range` (`:119-142`,
any team, centre distance only) and never consumes. Neither triggers `consume()`.

**Changes.**
1. `projectors.rs::buildings_in_range(world, origin, range)`: add a team filter (origin's
   `TeamComp`) and the `range + size*8/2` test (size from `BlockTable`), skip `privileged`
   defs, and skip blocks with `can_overdrive == false` for the overdrive caller (check
   `grep -n can_overdrive core/content/registries/blocks/mod.rs`; if absent, skip this filter and note it).
2. `update_mend_projector_world`: replace the private scan with `buildings_in_range` + `damaged()`;
   take `optional_efficiency: f32` instead of `phase_valid: bool` and lerp phase toward it;
   add `use_timer` handling via a new field `MendProjectorState.use_timer: f32` (+400 default):
   `if optional > 0 { use_timer += 1/ts...}` — simplest exact form: keep a counter, when it reaches
   `use_time / time_scale` and `optional > 0`, call the new helper below and reset.
3. New `pub fn trigger_consumers(world: &mut World, e: Entity)` in `core/world/update.rs`:
   for every `ConsumeInstanceKind::Items(stacks)` of the block, `ItemModule::remove(item, amount)`
   (`ConsumeItems.trigger`). Use it from mender and overdrive (`use_progress >= use_time`).
4. Behaviors pass `optional_efficiency(world, e)` (helper already used by Overdrive).

**Tests** (`production_tests.rs`):
* `mender_heals_only_own_team`: mender (team 1) at (5,5) + power-source (5,6); copper-wall team 1 at
  (6,5) and team 2 at (5,4)? Team placement: `SimCommand::Place{team}` — check that `team` is honoured
  by `place()` helper (it passes `team: 1`; write a local variant with team 2). Set both walls'
  `Health.health = 10`. Tick 201 (reload 200). Own wall health = 10 + 80*... copper-wall health
  = 80*4 = 320 (`Blocks.java` copperWall `health = 80 * wallHealthMultiplier(4)`) → +4% = 12.8
  → 22.8 (efficiency 1). Enemy wall stays 10.
* `mender_silicon_boost_consumed`: with 2 silicon: after ≥ 400 ticks one silicon is gone; the
  first heal after phase warmup adds `(4 + 4*phaseHeat)%`.
* `overdrive_does_not_boost_enemy`: overdrive projector team 1 + source; enemy `graphite-press`
  team 2 within 3 tiles: its `Building.time_scale` stays 1.0; own press becomes 1.5 after 61 ticks.

**Verify in-engine.** Ground Zero: damage a wall (no seam: use a placed `copper-wall` and wait
for a wave dagger, or skip) — in-engine is optional; headless tests are the gate. State
"headless only" if no damage seam exists (`grep -n "fn debug_damage" gdext/sim_host.rs`).

**Pitfalls.** `heal` uses `Health.max_health`; buildings and units both have `Health` — the
scan must require `Building`. Keep `update_mend_projector` (harness) compiling.

**Commit.** `Gameplay G02 T4: menders and overdrive act on own team and use their boost items`

### T5. Incinerators accept and destroy

**Symptom / acceptance.** A powered incinerator never accepts items or liquids, so belts
into it clog. After: once `heat > 0.5` it accepts any item and any incinerable liquid and
destroys them; slag incinerator accepts items while `efficiency > 0`.

**Java.** `J/world/blocks/production/Incinerator.java:30-81`:
`updateTile: heat = approachDelta(heat, efficiency, 0.04)`;
`acceptItem: heat > 0.5 && enabled`; `handleItem`: (effect only — item vanishes);
`acceptLiquid: heat > 0.5 && liquid.incinerable && enabled`; `handleLiquid`: nothing stored.
Blocks: `incinerator` `consumePower(0.50f)` (`Blocks.java:1326`).
`ItemIncinerator.java:36-66`: `updateTile` empty; `acceptItem: efficiency > 0`; `handleItem`
destroys; consumes `consumeLiquid(slag, 0f)` (`Blocks.java:1531-1534`).

**Port today.** `IncineratorBehavior` (`core/world/behavior/production.rs:1344-1370`) only
wipes its modules each tick when efficiency > 0. `ItemIncinerator` maps to `CrafterBehavior`
(`default_behavior`, `K::ItemIncinerator`).

**Changes.** Rewrite `IncineratorBehavior`: state component `IncineratorState { heat: f32 }`
(create_state); `update_tile` → approach; `accept_item` → `heat > 0.5 && enabled`;
`handle_item` → nothing; `accept_liquid` → `heat > 0.5 && enabled && LiquidTraits.incinerable[liquid]`;
`handle_liquid` → nothing; `accept_stack` → 0 (Java default with acceptItem true would return
`itemCapacity - 0`; keep Java: `min(itemCapacity - items.get, amount)` only if `hasItems`
— incinerator `hasItems = true`? check the spec; if true, return `item_capacity.min(amount)`
and discard in `handle_stack`). New `ItemIncineratorBehavior { accept_item: efficiency > 0; handle_item: nothing }`
mapped from `K::ItemIncinerator` in `default_behavior`.

**Tests.** `incinerator_burns_belt_items`: power-source (4,5), incinerator (5,5), item-source (6,5)
`Content("copper")`; tick 60; `IncineratorState.heat` = 1.0 after 25 ticks (0.04/tick); the
item source's `SandboxState.emitted > 0` and the incinerator's item module total = 0.

**Verify in-engine.** Ground Zero: place power-source, incinerator, item-source as above
around (150,66); step 120; `building_debug(151,66)["sandbox_item"]`… read `emitted` if exposed,
else confirm with a conveyor between source and incinerator not clogging
(`building_items_json` of the conveyor stays ≤ 1).

**Commit.** `Gameplay G02 T5: incinerators accept and destroy items and liquids`

### T6. Drill time uses item hardness; burst drills burst

**Symptom / acceptance.** Every Serpulo drill mines faster than Java: a mechanical drill on
4 copper tiles yields one item every 150 ticks instead of 162.5; on titanium a pneumatic drill
is 27% too fast. Impact/eruption drills behave like normal drills (continuous trickle instead
of `dominantItems` items every `drillTime`). After: drill period = Java `getDrillTime`.

**Java.**
* `J/world/blocks/production/Drill.java:170-172`: `getDrillTime(item) = (drillTime + hardnessDrillMultiplier * item.hardness) / drillMultipliers.get(item, 1)`; `hardnessDrillMultiplier = 50` (`:28`); `updateTile` `:287-325` (already ported; only `delay` changes).
* `BurstDrill.java:35-44` sets `hardnessDrillMultiplier = 0`; `:53-55` `getDrillTime = drillTime / drillMultipliers.get(item,1)`; `updateTile` `:76-120` (transcribed):
  ```
  if(dominantItem == null) return;
  if(invertTime > 0) invertTime -= delta() / invertedTime(200);
  if(timer(timerDump, dumpTime/timeScale)) dump(items.has(dominantItem) ? dominantItem : null);
  drillTime = getDrillTime(dominantItem);
  smoothProgress = lerpDelta(smoothProgress, progress/(drillTime-20), 0.1);
  if(items.total() <= itemCapacity - dominantItems && dominantItems > 0 && efficiency > 0){
     warmup = approachDelta(warmup, progress/drillTime, 0.01);
     speed = lerp(1, liquidBoostIntensity(1.6), optionalEfficiency) * efficiency;
     timeDrilled += pow2In(progress/drillTime) * speed;  lastDrillSpeed = speed*dominantItems/drillTime;
     progress += delta() * speed;
  } else { warmup = approachDelta(warmup, 0, 0.01); lastDrillSpeed = 0; return; }
  if(dominantItems > 0 && progress >= drillTime && items.total() < itemCapacity){
     for(i < dominantItems) offload(dominantItem);  invertTime = 1;  progress %= drillTime; }
  ```
  impact-drill: drillTime 720, size 4, tier 6, itemCapacity 40, `drillMultipliers.put(beryllium, 2)` (`Blocks.java:3089-3110`); eruption-drill: read `:3112-3140` and transcribe drillTime/tier/capacity/multipliers.
* Item hardness (`J/content/Items.java`): copper 1, lead 1, coal 2, titanium 3, thorium 4, beryllium 3, tungsten 5; sand/scrap 0.

**Port today.** `DrillBehavior::update_tile` (`core/world/behavior/production.rs:436-515`)
uses `delay = def.drill_time.max(0.0001)`; `DrillDef.hardness_drill_multiplier` exists but is
never set (`set_drill`, `core/world/block_kind_data.rs:956`) nor read; no `drill_multipliers`.
`BurstDrill` runs the same loop (`production.rs:442`).

**Changes.**
1. `DrillDef`: add `drill_multipliers: Vec<(u16, f32)>`; `set_drill` sets
   `hardness_drill_multiplier = 50.0` for `BlockKindData::Drill` and `0.0` for `BurstDrill`;
   impact/eruption arms push `(beryllium, 2.0)`.
2. `DrillBehavior` gets `hardness: Arc<[i32]>` like `BeamDrillBehavior` (`production.rs:947-953`);
   register `mechanical/pneumatic/laser/blast/impact/eruption-drill` with it in
   `core/world/blocks/production.rs::register` (pattern of the plasma bores there).
   `delay = (drill_time + hdm * hardness[item]) / multiplier(item)`.
3. Burst branch: when `kind_data` is `BurstDrill`, run the transcribed burst loop (new
   `BurstState { invert_time, smooth_progress }` fields may go on `DrillState` as extra fields with
   defaults 0 — `DrillState` is in `core/entities/comp/building.rs`; adding fields changes its IO only
   if `write/read` serialises them — keep IO as Java `Drill.write` (`progress`, `warmup`).

**Tests** (`production_tests.rs`):
* `mechanical_drill_period_includes_hardness`: mechanical drill on 4 copper ore tiles (copy the ore
  setup of `laser_drill_counts_centered_footprint`), item-void adjacent to drain. Count
  offloads: run 3000 ticks; Java steady state (warmup→1 after 67 ticks) gives
  `floor((3000 - ~35)/162.5)` ≈ 18; old code gives ≈ 19-20. Assert the count is 17 or 18
  (derive exactly by simulating the warmup formula in the test if you want an equality).
* `impact_drill_bursts`: impact drill (4x4) on 16 beryllium-free ore tiles of `sand`? (tier 6
  covers all); water from a liquid source; power source. Assert the item count jumps by
  `dominantItems` (16) at once after ≥ 720 ticks and is 0 before.

**Verify in-engine.** Ground Zero: `place_block(130,48,"mechanical-drill")` is on copper ore
(env §4.1: ore at 130..131, 48..49 → 4 tiles). With a `container` at (132,48), step 1625 in
chunks; `building_items_json(132,48)` copper ≈ 10 (Java), not ≈ 10.8. Record the number.

**Commit.** `Gameplay G02 T6: drill time includes hardness; burst drills burst`

### T7. PAR-0165: `place_block` / `apply_sim_command_json` report rejections

**Symptom / acceptance.** `place_block` on an occupied/invalid tile returns `true`.
After: `place_block` returns `false` unless the tile afterwards holds the requested block
anchored at `(x,y)`; `apply_sim_command_json` `place` returns `false` when the placement is
invalid at enqueue time (instant path: `valid_place`; player path: the same plus a player unit
that can build).

**Port today.** `gdext/sim_host.rs:410 place_block` returns `apply_command(Command::Place)`,
which is `Ok` even when `Sim::apply` logs "rejected" (`core/sim/mod.rs:264-298`).
`apply_sim_command_json` (`sim_host.rs:523`) only enqueues.

**Changes.**
1. `core/sim/mod.rs`: `pub fn can_place_for_team(&self, x: i16, y: i16, block: BlockId, rot: u8, team: u8) -> bool`
   → `self.block_runtime.as_ref().is_some_and(|rt| rt.can_place(&self.ecs.0, &self.grid, x, y, block, rot, team))`.
2. `place_block`: after `apply_command` returns true, read back
   `self.sim.grid.block_at(TilePos::new(x,y)) == Some(id)` and the building anchor
   (`Building.tile == (x,y)`); return that. Same for `place_block_rot`.
3. `apply_sim_command_json` `"place"`: before enqueueing, if `!self.sim.can_place_for_team(x, y, BlockId::new(block), rotation as u8, self.sim.live_default_team())` → `log::warn!` + return false. Do not change `Sim::apply` (replay/tests unwrap it).

**Tests.** gdext has no unit tests here; add `core` test `can_place_for_team_rejects_occupied`
(`sim/tests.rs`): place a wall at (3,3), `can_place_for_team(3,3, wall)` is false, (4,4) true.

**Verify in-engine.** `$G --eval "return $H.place_block(129,55,\"copper-wall\")"` on the core → `false`;
on a free tile → `true`; again on the same tile → `false`.

**Commit.** `Fixes PAR-0165: place_block and apply_sim_command_json report rejected placements`

### T8. `Build.validPlace` / `validBreak` completion + rules sync

**Symptom / acceptance.** Today the player can: deconstruct enemy buildings, replace enemy
blocks with their own, build on deep water, build right next to enemy cores, replace a block
with the same block (losing its contents), and place blocks banned by sector rules; build
cost/refund multipliers are read correctly by the builder but `BuildRules` (used by placement
and break checks) never receives the sector's rules. After: each case is rejected exactly as
Java.

**Java.** `J/world/Build.java:183-273` `validPlaceIgnoreUnits` (transcribed order):
```
if(type == null || (!rules.editor && checkVisible && (!type.environmentBuildable() || (!type.isPlaceable() && !(rules.waves && team == rules.waveTeam && type.isVisible()))))) return false;
if(!rules.editor && checkCoreRadius && !(!checkVisible && team == derelict)){
   if(rules.polygonCoreProtection){ closest core (any active team with protectCores) to (x*8+offset, y*8+offset) by dst2; if closest.team != team → false }
   else if(teams.anyEnemyCoresWithinBuildRadius(team, x*8+offset, y*8+offset)) return false;   // other team with protectCores, core.within(x,y, rules.buildRadius(core.team) + 8)
}
tile = world.tile(x,y); if null → false
if(!type.canPlaceOn(tile, team, rotation)) return false
if(!type.ignoreBuildDarkness && world.getDarkness(x,y) >= 3) return false
if(!type.requiresWater && !contactsShallows(x, y, type) && !type.placeableLiquid) return false
if(type.isOverPlacementLimit(team)) return false
for each footprint tile check:
   check == null → false
   type.size == 2 && darkness(check) >= 3 → false
   rules.staticFog && rules.fog && !fogControl.isDiscovered(team, wx, wy) → false   (G04/G03 fog: skip, note)
   check.floor().isDeep() && !type.floating && !type.requiresWater && !type.placeableLiquid → false
   !rules.derelictRepair && check.team() == derelict && check.build != null → false
   type == check.block() && check.build != null && rotation == check.build.rotation && type.rotate && !(team != derelict && check.team() == derelict) → false
   !check.interactable(team) → false            // interactable = check.team() == team || check.team() == derelict (Teams.canInteract, Teams.java:125); tiles without a build are team derelict
   !check.floor().placeableOn && !type.ignoreBuildDarkness → false
   !( (type.canReplace(check.block()) || (check.build != null && check.build.canBeReplaced(type)) || (type == check.block && team != derelict && check.team() == derelict))
       || (check.build is ConstructBuild && build.current == type && check.centerX()==x && check.centerY()==y) ) → false
   (type.bounds must contain the replaced block's bounds)  → for same-size or larger replacements only; port: reject when the replaced building's footprint is not fully inside the new footprint
   type.requiresWater && check.floor().liquidDrop != water → false
if(rules.placeRangeCheck && checkCoreRadius && getEnemyOverlap(type, team, x, y) != null) return false   // indexer.findEnemyTile(team, x*8+size, y*8+size, placeOverlapRange(50)+4, b.team.rules().checkPlacement)
return true
```
`contactsShallows` `Build.java:293-312` (multiblock: any inside-edge or edge tile whose floor is not deep; 1x1: any d4 neighbour or the tile itself not deep). `isDeep() = drownTime > 0`.
`validBreak` `Build.java:315-318`: `tile.block() != air && block.canBreak(tile) && (tile.breakable() || rules.allowEnvironmentDeconstruct) && tile.interactable(team)`.
`Block.isPlaceable()` `Block.java:1035-1037`: `isVisible() && (!isBanned() || editor) && supportsEnv(rules.env)`;
`isVisible()` `:1031-1033`: `!isHidden() && (editor || !rules.hideBannedBlocks || !isBanned())`;
`isHidden()` `:1553`: `!buildVisibility.visible() && !rules.revealedBlocks.contains(this)`;
`BuildVisibility` (`J/world/meta/BuildVisibility.java`): hidden false, shown true, debugOnly false,
editorOnly `editor`, sandboxOnly `infiniteResources`, campaignOnly `isCampaign`, lightingOnly
`lighting || isCampaign || infiniteResources`, fogOnly `fog || editor`, worldProcessorOnly
`editor || allowEditWorldProcessors`, coreZoneOnly (core-zone block present), legacy pads (G04).
`Rules.isBanned(block)` = `blockWhitelist != bannedBlocks.contains(block)` (`Rules.java:335`).

**Port today.** `core/world/build.rs:163 valid_place_at` checks: placeable_player/in_editor,
`rules.is_banned`, limits, `supports_env_bits`, wall darkness, footprint `can_replace` with
"same block always OK", pump/beam drill/wall crafter/solid pump `canPlaceOn`. Missing:
visibility, core radius/polygon, contactsShallows, deep water, derelict repair, same-block
rotation rule, team interactability, floor `placeable_on`, bounds rule, placeRangeCheck.
`contacts_shallows` (`:426`) and `get_enemy_overlap` (`:462`) are wrong stubs.
`valid_break` (`:107`) ignores team. `BuildRules` (`core/world/limits.rs:18`) is synced from the
session only for `default_team` and `env` (`core/game/runtime.rs:281-286`).

**Changes.**
1. `core/world/limits.rs BuildRules`: add `block_whitelist: bool`, `editor: bool`,
   `hide_banned_blocks: bool`, `infinite_resources` (exists), `lighting: bool`, `fog: bool`,
   `is_campaign: bool`, `revealed_blocks: Vec<BlockId>`, `team_rules: Vec<(u8, bool /*protect_cores*/, f32 /*extra radius*/, bool /*check_placement*/)>`;
   `is_banned` → `self.block_whitelist != self.banned_blocks.contains(&block)`.
2. **One sync entry point** (G04 and G06 reference it by this name):
   `pub fn sync_build_rules(world: &mut World, rules: &crate::io::json::rules::Rules)` in
   `core/world/limits.rs`. It writes every `BuildRules` field below (block names → ids through the
   `ContentIndex` resource, `core/sim/runtime.rs` `ContentIndex.block_ids`; skip unknown names) and is
   a no-op when `BuildRules` is absent. Call it (a) on world load from `core/game/runtime.rs` (replace
   the two-field block at `:281-286`, keeping `default_team`), and (b) it is the function G06's
   `setrule` handler calls after mutating `Rules` mid-sector, and G04 calls after any rules swap.
   Fields to copy (besides `default_team`, `env`):
   `banned_blocks` (names → ids via `ContentIndex.block_ids`), `block_whitelist`,
   `deconstruct_refund_multiplier`, `build_cost_multiplier`, `enemy_core_build_radius`,
   `polygon_core_protection`, `place_range_check`, `derelict_repair` → `allow_derelict_repair`,
   `allow_environment_deconstruct`, `infinite_resources`, `editor`, `hide_banned_blocks`,
   `revealed_blocks`, `fog`, `lighting` (check `grep -n "pub lighting" core/io/json/rules.rs`),
   team rules. Field names verified in `core/io/json/rules.rs:470+` (`derelict_repair`,
   `build_cost_multiplier`, `deconstruct_refund_multiplier`, `enemy_core_build_radius`,
   `polygon_core_protection`, `place_range_check`, `hide_banned_blocks`,
   `allow_environment_deconstruct`, `block_whitelist`, `banned_blocks`, `revealed_blocks`,
   `fog`, `static_fog`, `editor`, `infinite_resources`; team rule `protect_cores`,
   `check_placement`, `extra_core_build_radius`).
3. New `pub struct PlaceCtx<'a> { pub cores: &'a [(u8, f32, f32)], pub teams_of_tiles: … }`:
   `valid_place_at` needs team per tile and core positions, which live in ECS. Add a parameter
   `world: &World` to `valid_place_at`/`valid_place` (all callers: `grep -rn "valid_place(" core gdext`)
   and read `TeamComp` of `tile.build`, and collect cores with
   `world.query::<(&CoreBuild, &TeamComp, &Pos)>()` (iterate by `EntitySeq` order if ties matter;
   only a boolean is needed so order does not matter).
4. Port the transcribed checks in order into `valid_place_at`. Rewrite `contacts_shallows` per Java
   using `edges::inside_edges(size)`/`edges::edges(size)` (`core/world/edges.rs:89,98`) and
   `def.is_deep` (`BlockDef.is_deep`, derived from `drown_time`). Rewrite `get_enemy_overlap` as
   "any building of another team with `check_placement` whose hitbox is within
   `50 + 4` world units of `(x*8+size, y*8+size)`" (Java `findEnemyTile` = closest enemy building
   within range; existence is enough).
4b. Darkness / `limitMapArea` gate. Java has no separate limit-area check in `validPlace`: the
   gate is `world.getDarkness(x,y) >= 3` (`Build.java:229`; footprint `:251` for size 2), and
   `World.getDarkness` (`J/core/World.java:484-530`) includes the border component
   `if(rules.borderDarkness){ edgeDst = limitMapArea ? min(x-limitX, y-limitY, (limitX+limitWidth-1)-x, (limitY+limitHeight-1)-y) : distance to map edge; if(edgeDst <= 2) dark = max((2-edgeDst)*(4/2), dark) }`
   — so any tile outside the limit rect (negative `edgeDst`) gets darkness ≥ 4 and is unbuildable.
   The port's `valid_place_at` uses only `get_wall_darkness`. Replace with
   `max(get_wall_darkness(..), darkness::border_darkness(grid, rules.border_darkness, grid.limit_area, x, y))`
   (`core/world/darkness.rs:134`; `grid.limit_area` is `WorldGrid.limit_area`, set by G04 T5);
   add `border_darkness: bool` to `BuildRules` (synced from `Rules.border_darkness`, default true)
   and `ignore_build_darkness` handling if `BlockDef` has the flag (else treat false). The
   sector-polygon component (`World.java:504-527`) is G04's — if G04 adds it to `darkness.rs`, call it
   here too. Test `limit_area_blocks_outside_placement`: `grid.limit_area = Some((10,10,20,20))`,
   wall at (5,15) → false, at (15,15) → true, at (10,15) → false (edgeDst 0 → darkness 4).
4c. `kind_rotates` (`core/world/block.rs:520-571`) feeds `can_replace`/quick-rotate and the same-
   block rule; it does not match Java `Block.rotate`. Java classes with `rotate = true` in their
   constructor (grep `rotate = true` under `J/world/blocks`): UnitBlock (UnitFactory, Reconstructor),
   UnitAssembler, UnitAssemblerModule, BeamDrill, WallCrafter, PayloadConveyor (+PayloadRouter),
   PayloadLoader (+PayloadUnloader), PayloadSource, BlockProducer (Constructor, PayloadDeconstructor?
   check its superclass), PayloadMassDriver, HeatProducer, HeatConductor, HeaterGenerator, Turret?,
   StackConveyor, Conveyor (+Armored), Duct, DuctRouter (+StackRouter), OverflowDuct,
   DirectionBridge (DuctBridge, DirectionLiquidBridge), DirectionalUnloader, Conduit (+Armored),
   Thruster, PowerDiode. The port lists as rotating but Java does **not** (verify each against the
   class and its superclasses, then fix): Junction, ItemBridge, BufferedItemBridge, Sorter, Router,
   OverflowGate, MassDriver, LiquidJunction, LiquidBridge, LiquidRouter, Drill, BurstDrill, Door,
   AutoDoor, PowerNode, BeamNode. Missing from the port list: UnitFactory, Reconstructor,
   UnitAssembler, UnitAssemblerModule, BeamDrill, HeatProducer, HeatConductor, HeaterGenerator,
   PayloadSource. Also check per-block `rotate = ...` overrides in `J/content/Blocks.java`
   (`grep -n "rotate = " J/content/Blocks.java`). Method: for each `BlockKind` in the enum, open the
   Java class, follow `extends` until `rotate` is assigned; record the table in the commit body.
   Test `kind_rotates_matches_java` asserting the corrected list. Pitfall: the conveyor
   autotiler and rotation-carrying placement read `kind_rotates`; run the distribution tests.
5. Visibility: `fn build_visible(def, rules) -> bool` per the BuildVisibility table; `is_placeable = build_visible && !hidden-by-ban && (!banned || editor) && supports_env_bits`.
   Apply only for player placement (`checkVisible = true`): add a `check_visible: bool` param;
   API `place_block` (`Command::Place` without player) passes `false`, so sandbox blocks can still be
   staged by tests (keep env §4.1 recipes working). `queue_player_place` and the builder's
   `can_place` call pass `true`. (Unlock state is not known to the runtime registry — leave the
   unlock gate to the palette; see §6.)
6. `valid_break(content, grid, rules, team, x, y)` → add `world: &World` and reject when the tile's
   building team is neither `team` nor derelict (0).
7. Same-block rule: remove the `tile.block == block → continue` shortcut; implement Java's
   (rotation equal + rotate → reject; non-rotating same block → `can_replace` returns false →
   reject; derelict same block → allowed).
8. `GeneratorEnv` (`core/world/blocks/power/behavior.rs:57`) is read (`power/generator.rs:39`) but
   never inserted. Depends on G07 T9, which inserts it from `weather_system` for campaigns. Here:
   (a) `install_block_runtime_from` does `init_resource::<GeneratorEnv>()` (upstream defaults);
   (b) `sync_build_rules` also writes `solar_multiplier` (`Rules.solar_multiplier`), `lighting`,
   `ambient_light_alpha` (`Rules.ambient_light` alpha), `reactor_explosions` into the resource for
   every sim (custom/non-campaign included) by calling G07 T9's helper (use its name; if G07 T9 has
   not landed, add `pub fn generator_env_from_rules(rules: &Rules, prev: GeneratorEnv) -> GeneratorEnv`
   in `power/behavior.rs` keeping `light_env/heat_env/steam_env` from `prev`, and tell G07 to reuse it).
   Test `sync_build_rules_sets_generator_env`: rules `solar_multiplier = 0.5` → resource 0.5.

**Tests** (`core/world/build.rs` tests or `production_tests.rs`, sim with runtime):
* `cannot_break_enemy_building`: wall team 2 at (5,5); `queue_player_break` path or
  `valid_break(..., team 1, 5,5)` false; derelict wall (team 0) → true.
* `cannot_replace_enemy_conveyor`: conveyor team 2 at (5,5); valid_place titanium-conveyor team 1 → false.
* `cannot_build_on_deep_water`: floor `deep-water` (check the name: `grep -n '"deep-water"' core/content/registries/blocks/environment.rs`) 3x3 around (8,8); copper-wall → false; `conduit`? (floating=false in Java base Conduit? `Conduit.java:50 floating = true`) → true.
* `enemy_core_radius_blocks_placement`: core-shard team 2 at (40,40) on a 64x64 map; wall team 1 at (40,40+45) (45 tiles = 360 px < 400+8) → false; at 52 tiles (416 px) → true.
* `banned_block_rejected`: `rules.banned_blocks = [copper-wall id]` → false; `block_whitelist = true` → true.
* `same_block_same_rotation_rejected`: conveyor rot 0 at (5,5); valid_place conveyor rot 0 → false; rot 1 → true.

**Verify in-engine.** Ground Zero has derelict scrap walls near (169,167): queue a break on
one via `apply_sim_command_json` `{"op":"break","x":169,"y":167,"player":0}` → accepted
(derelict). Spawn nothing enemy; for the core radius use sector 98 or skip ("headless only:
no enemy-core sector in the profile that is cheap to load"). `place_block` over the core → false (T7).

**Pitfalls.** Changing `valid_place`'s signature touches the team AI (`team_ai_system`
placement), schematics and the input preview; grep all callers and pass `check_visible`
appropriately (AI: Java `BaseBuilderAI` uses `Build.validPlace(..)` = checkVisible true but wave
team bypass applies). Placement previews in gdext (`placement_preview_json`) must use the same
function so red/green ghosts match. Do not apply fog discovery (not modelled; note in commit).

**Commit.** `Gameplay G02 T8: Build.validPlace/validBreak team, water, core radius, visibility and rules sync`

### T9. Core blocks: placement only by upgrade, upgrade keeps items, last core unbreakable

**Symptom / acceptance.** Today a core-foundation can be placed on any free ground (and
deconstructed down to zero cores), and cannot be placed over a shard (no upgrade). After:
foundation/nucleus are placeable only over a smaller own core (or a `core-zone` floor), the
upgrade completes instantly when construction begins and the team keeps its items minus the
cost; the last core cannot be broken.

**Java.** `J/world/blocks/storage/CoreBlock.java`:
* `:174-177 canBreak`: `editor || (rules.coreBuildAndConfig && tile.block() instanceof CoreBlock && teams.cores(tile.team()).size > 1)` → in campaign (`coreBuildAndConfig` false) cores are **never** breakable.
* `:180-183 canReplace(other)`: `super.canReplace(other) || (other instanceof CoreBlock && size >= other.size && other != this)`.
* `:186-203 canPlaceOn`: editor/coreBuildAndConfig → true; if no footprint tile has `!floor.allowCorePlacement || block instanceof CoreBlock` → true (core zone);
  `core == null || (!infiniteResources && !core.items.has(requirements, buildCostMultiplier))` → false;
  `return tile.block() instanceof CoreBlock && size > tile.block().size && (!requiresCoreZone || all floors allowCorePlacement)`.
* `:229-239 beforePlaceBegan`: `nextItems = core.items.copy() minus requirements*buildCostMultiplier` (unless infinite).
* `:206-226 placeBegan(tile, previous)`: if previous is a CoreBlock: `tile.setBlock(this, team)` immediately, then `team.core().items.set(nextItems)`.

**Port today.** `can_place_on` stub (`core/world/build.rs:78`), `can_replace` (`:89`) generic;
`can_break` stub (`:84`); builder `begin_place` (`core/sim/runtime.rs:485`) always creates a
construct block.

**Changes.**
1. `build.rs`: `can_replace`: add `|| (other.kind == CoreBlock && def.kind == CoreBlock && def.size >= other.size && other.id != def.id)`.
2. `valid_place_at` (after T8): core `canPlaceOn` per Java using `BlockDef.allow_core_placement` of
   the floor (field exists on `BlockDef`), `requires_core_zone` (check field; if absent treat false),
   team core items (pass core `ItemModule` via the `world` param from T8; `requirements` scaled by
   `rules.build_cost_multiplier`, rounded like `ItemStack.mult`: `Math.round(amount * mult)`).
3. `can_break`: `def.kind == CoreBlock` → `rules.core_build_and_config && cores_of(team) > 1` (add `core_build_and_config` to `BuildRules` in T8's sync).
4. `SimRuntime::begin_place`: if the target tile holds a `CoreBlock` of the same team and the new
   block is a core: compute `next_items` (core items minus scaled requirements, saturating at 0),
   `set_block_raw(new core)`, then set the new core's `ItemModule` to `next_items`; do not create a
   construct block; return true.

**Tests.** `core_upgrade_keeps_items` (`sim/builder.rs` tests with `sim_with_player()`): shard
with 4000 copper, 4000 lead, 4000 silicon (foundation cost: `J/content/Blocks.java:3160`
`copper 3000, lead 3000, silicon 2000`); queue foundation over the shard; tick until the plan is
gone; block is `core-foundation`; copper = 1000, lead = 1000, silicon = 2000 (note: the shard's
item capacity 4000 must allow the seed; foundation capacity 9000). `core_needs_existing_core`: foundation on
free ground → invalid. `last_core_unbreakable`: `valid_break` on the only shard → false.

**Verify in-engine.** Ground Zero: `debug_add_core_items("copper",3500)`, `("lead",3500)`, `("silicon",2500)`;
`apply_sim_command_json {"op":"place","x":129,"y":55,"block":<core-foundation id>,"player":0}`;
step 300; `tile_debug(129,55)` block is core-foundation; `core_items_json` copper/lead decreased by 3000, silicon by 2000; requires_core_zone: no such field
on `BlockDef` (only `allow_core_placement`), treat `requiresCoreZone` as false (true only for Erekir
`core-bastion`-family? check `grep -n requiresCoreZone J/content/Blocks.java` and add the field if any vanilla core sets it).

**Commit.** `Gameplay G02 T9: core placement, upgrade and last-core rules`

### T10. Tap-to-link configuration (bridges, bridge conduits, mass drivers, power nodes) + config panel parity

Previous plan T12 (unreached). `link_valid_between` for item bridges exists (prev T11).

**Symptom / acceptance.** Tapping a bridge conveyor opens an item list; tapping a second
bridge does nothing; bridge conduits cannot be linked at all; tapping a node then a consumer
does not link; mass drivers cannot be aimed. After (two real taps, env §4 `godot_input`):
* bridge A then bridge B (same row, ≤ 3 tiles apart for bridge-conveyor / 4 for bridge-conduit):
  `building_config_json(A)` = point to B; tapping B again (with A selected) unlinks.
* power node then a battery in range: link toggles (T2 plumbing); tapping the node itself twice:
  no links → auto-link up to `maxNodes` potential links; links → clear all.
* mass driver then another mass driver within range 55 tiles (`MassDriver.range = 440f` world units — confirm in `Blocks.java massDriver`): link; tap self: unlink and deselect.
* Item config lists show only unlocked, non-hidden items (`ItemSelection.buildTable`).

**Java.**
* `J/input/InputHandler.java:1972-2010 tileTapped` (transcribed):
  ```
  if build == null: hide inventory + config; return false
  if build.block.configurable && build.interactable(team):
      consumed = true
      if (!config.isShown() && build.shouldShowConfigure(player))
         || (config.isShown() && config.getSelected().onConfigureBuildTapped(build) && build.shouldShowConfigure(player)):
           config.showConfig(build)
  else if !config.hasConfigMouse():
      if config.isShown() && config.getSelected().onConfigureBuildTapped(build): consumed = true; config.hideConfig()
      if config.isShown(): consumed = true
  if !consumed && interactable: build.tapped()
  ```
  `onConfigureBuildTapped` default (`BuildingComp`): `if(block.configurable) return self() != other; return true;` (tapping the selected block again closes it — check the default body with `grep -n "public boolean onConfigureBuildTapped" -A8 J/entities/comp/BuildingComp.java`).
* `ItemBridge.java:299-316` (also LiquidBridge, which extends ItemBridge):
  reverse link (`other` is a bridge whose `link == pos()`): `configure(other.pos()); other.configure(-1); return true;`
  else if `linkValid(tile, other.tile)`: `configure(link == other.pos() ? -1 : other.pos()); return false;` else `return true`.
  `linkValid` `:152-164`: same row or column, distance ≤ range (`positionsValid`), same block (`linkSameType`) or both bridges, same team, and (checkDouble) `other.link != tile.pos()`.
* `MassDriver.java:272-288`: self → `if(link == -1) deselect(); configure(-1); return false`; `link == other.pos()` → `configure(-1); return false`; `other.block == block && other.dst(tile) <= range && same team` → `configure(other.pos()); return false`; else true.
* `PayloadMassDriver.java:448-464`: same as mass driver with `other.block instanceof PayloadMassDriver`.
* `PowerNode.java:419-441`: `linkValid(this, other)` → `configure(other.pos()); return false`; self → if no links: collect `getPotentialLinks` (not insulated, up to `maxNodes`) as relative `Point2[]` and configure; else `configure(new Point2[0])`; `deselect(); return false`; else true.

**Port today.**
* `gdext/input/desktop.rs:1106 tile_tapped`: switch/door branches, then `config_spec` →
  `Effect::OpenBlockConfig`, else inventory. No "selected config building" state; `DesktopBridge`
  is at `desktop.rs:177`. Panel: `ui/fragments/block_config_fragment.gd` (`configure(world_pos, spec_json)`,
  close button sets `visible = false` at `:155`).
* `ItemBridgeBehavior::configured` (`core/world/blocks/distribution/item_bridge.rs:341`) and
  `MassDriverBehavior::configured` (`mass_driver.rs:473`) accept `Point2` (relative) or `Number`
  (packed absolute); `PayloadMassDriverBehavior::configured` (`payload_mass_driver.rs:441`) too.
* `LiquidBridgeBehavior` (`core/world/blocks/liquid/behavior.rs:376`) stores an
  `ItemBridgeBuild` but has no `config`/`configured`/`link_valid_between`/`write`/`read`;
  `liquid/bridge.rs:97 link_valid_world(world, entity, other, range)` exists.
* Power nodes: T2 adds the Configure path.

**Changes.**
1. `LiquidBridgeBehavior`: add `config` (`ConfigValue::Point2` relative of `link`, or None when -1 — copy
   `ItemBridgeBehavior::config`), `configured` (same body as `ItemBridgeBehavior::configured`),
   `link_valid_between` → `bridge::link_valid_world(world, e, other, self.range)` and
   `!(other's link == e's pos)`, `write`/`read` = ItemBridge's format (copy `ItemBridgeBehavior::write/read`
   minus the buffer; Java `ItemBridgeBuild.write`: `link i32, warmup f32, incoming count u8 + i32 each, wasMoved/moved?` — copy exactly what the item bridge writes).
2. **Generic mechanism (one for every block; G06 T11 plugs the LogicBlock branch into it).**
   This task builds Java's `InputHandler.tileTapped` + `Building.onConfigureBuildTapped` dispatch
   through the behavior trait, not a per-kind switch. Add to `core/world/behavior/mod.rs`:
   ```rust
   /// Result of `Building.onConfigureBuildTapped(other)` (`BuildingComp`, `InputHandler.java:1972-2010`).
   pub enum ConfigTapResult {
       /// Java `return false`: tap consumed, selection stays; apply these configs in order.
       Consume(Vec<(Entity, ConfigValue)>),
       /// Java `deselect(); return false`: apply configs, then close the panel.
       ConsumeAndDeselect(Vec<(Entity, ConfigValue)>),
       /// Java `return true`: the selected block agrees to switch/close; the tap proceeds normally.
       Switch,
   }
   /// `Building.onConfigureBuildTapped(other)`; `grid` is the authoritative `Sim.grid`.
   fn on_configure_build_tapped(&self, world: &World, grid: &WorldGrid, e: Entity, other: Entity) -> ConfigTapResult {
       // BuildingComp default: `if(block.configurable) return self() != other; return true;`
       // -> tapping the selected building again closes it.
       if e == other { ConfigTapResult::ConsumeAndDeselect(Vec::new()) } else { ConfigTapResult::Switch }
   }
   ```
   (Confirm the Java default body first: `grep -n "public boolean onConfigureBuildTapped" -A8 J/entities/comp/BuildingComp.java`
   and mirror it exactly.) Overrides in this task: `ItemBridgeBehavior` and `LiquidBridgeBehavior`
   (ItemBridge.java:299-316), `MassDriverBehavior` (MassDriver.java:272-288),
   `PayloadMassDriverBehavior` (PayloadMassDriver.java:448-464), `PowerNodeBehavior`
   (PowerNode.java:419-441). **G06 T11 implements this same method on `LogicBlockBehavior`**
   (`LogicBlock.java onConfigureBuildTapped`: link/unlink a building) — no other hook is needed.
   Values: packed absolute ints as `ConfigValue::Number(pack as f64)` (bridges/drivers accept
   `Number`), `-1` unlink; power node self-tap → `ConfigValue::Point2Array` of relative offsets.
   Core entry `pub fn config_tap(world: &World, grid: &WorldGrid, selected: Entity, other: Entity) -> ConfigTapResult`
   in new `core/world/config_tap.rs` (`pub mod config_tap;` in `core/world/mod.rs`) resolves the
   selected building's behavior from `BlockTable` and calls the trait method (tests use this entry).
3. `DesktopBridge`: field `config_selected: Option<(i32, i32)>`; set it when pushing
   `Effect::OpenBlockConfig`; add `#[func] fn close_block_config(&mut self)` on `MindInput`
   (`gdext/input/mod.rs`) clearing it; call it from the fragment wherever `visible = false`
   (close button `:155` and `_submit` if it hides).
4. `tile_tapped` (port of the transcribed `InputHandler.tileTapped`, generic for every block):
   before the switch/door branches, if `config_selected = Some(sel)` and `sel`'s
   building still exists: call `config_tap` through a new `MindSimHost` `pub fn` (not
   `#[func]`; bind once, release before emitting) → for `Consume` emit
   `RemoteAction::Configure` for each pair (positions from the entities' `Building.tile`) and keep
   the panel; `ConsumeAndDeselect` → emit and close; `Switch` → clear selection and continue to
   the normal open path for the tapped tile. Interactability: skip config for buildings whose team
   is not the player's team or derelict (Java `interactable`).
5. `config_spec`: for the linkable kinds return a spec with `"link": true` and no options
   (`ui/fragments/block_config_fragment.gd` `_rebuild`: when `link` is true show only the title
   and close button). Bundle: title = `block.<name>.name`.
6. Item option filter (`desktop.rs:1583`): skip items that are hidden or not unlocked — the runtime
   registry does not know campaign unlocks; read the unlock state from `/root/Spine/MindCampaign`'s
   content snapshot if `MindSimHost::content_registry()` is the campaign one (check:
   `grep -n "fn content_registry" gdext/sim_host.rs`); if it is the vanilla registry, leave a TODO
   in §6 instead of guessing.

**Tests.** `core/world/config_tap.rs` tests with `runtime_sim()`-style setup (or a
`BuildHarness`): `bridge_tap_links_then_unlinks` (bridges at (5,5),(8,5) → `Consume([(A, Number(pack(8,5)))])`;
after applying, tapping B again → `Consume([(A,-1)])`), `bridge_reverse_tap` (B→A when A links B →
`[(B? ...)]` per Java: selected configures to other, other configures -1), `bridge_out_of_range_switches`
(distance 4 for bridge-conveyor → `Switch`), `default_tap_self_closes` (sorter tapped twice → `ConsumeAndDeselect([])`), `liquid_bridge_links` (bridge-conduit range 4),
`mass_driver_self_tap_unlinks`, `node_self_tap_autolinks`.

**Verify in-engine.** Ground Zero: `place_block(150,70,"bridge-conveyor")`, `place_block(153,70,"bridge-conveyor")`;
`$C.center_on_tile(151,70)` (check name) then `$C.tile_to_screen(150,70)`; real tap at A (press+release),
real tap at B; `building_config_json(150,70)` → a point/number targeting (153,70); tap B again →
unlinked. Repeat with `bridge-conduit` and `power-node`→`battery`. Screenshot after linking (G03
draws the laser/bridge).

**Pitfalls.** Re-entrant borrow: compute the tap result with `host.bind()`, drop the guard,
then `emit_action` (which `bind_mut`s the host). `RemoteAction::Configure` with `Number` becomes
`Float(f32)` on the wire (`to_sim_config`): packed positions up to 2^24 are exact in f32 — a
1000x1000 map packs to ≤ 2^26; prefer emitting `SimCommand::Configure { value: Int(packed) }`
directly as `configure_building(.., "int", ..)` does (`gdext/input/mod.rs:994-1012`).

**Commit.** `Gameplay G02 T10: tap-to-link for bridges, conduits, drivers and nodes`

### T11. Item-source (and other content configs) from the Input bridge (combat plan T19)

**Symptom.** Combat session: `Input.configure_building(x,y,"item","carbide")` on an
item-source returned true but nothing reached the adjacent scathe. Code reading shows the path
`"item"` → `ConfigValue::String` → `RemoteAction::Configure` → `to_sim_config` → wire
`Content("carbide")` → `SimRuntime::resolve_config` (`core/sim/runtime.rs:200`) → `Item` →
`ItemSourceBehavior::configured` works (test `item_source_feeds_conveyor_into_void` uses the same
wire value). UNVERIFIED cause: either the scathe rejects carbide (turret ammo intake) or the
action was dropped by `ActionBatcher::emit` (`gdext/input/desktop.rs:972`).

**Steps.**
1. In-engine: Ground Zero, `place_block(150,74,"item-source")`, `place_block(151,74,"container")`;
   `$I.configure_building(150,74,"item","copper")`; step 60; expect
   `building_items_json(151,74)` copper > 0 and `building_config_json(150,74)` an item config.
   If this fails: log `pending_command_count()` right after the configure and grep the run log for
   `input action rejected`; fix the batcher path (configure must not be dropped) and add the
   `"item"/"liquid"/"content"` kinds explicitly to `configure_building` (map to wire
   `Content(value)` directly like the `"string"` branch).
2. Then `place_block(152,74,"scathe")` (3x3? scathe is size 3 → covers 151..153; move the source to
   (150,74) adjacent) with `configure_building(150,74,"item","carbide")`; step 60; read
   `building_debug(152,74)`; if no ammo, test `turrets::accept_item(world, scathe, carbide)` headless
   (`combat_tests.rs`, `feed` helper) and report to G01 if it is the turret ammo table.
**Commit.** `Gameplay G02 T11: content configs from the Input bridge` (only if code changed;
otherwise record the result in the summary).

### T12. Repair point / repair turret heal units

**Symptom / acceptance.** A powered repair point never heals damaged units. After: a damaged
friendly dagger inside 60 world units gets `repairSpeed * strength * edelta` per tick once the
point has rotated within 30° of it.

**Java.** `J/world/blocks/units/RepairTurret.java:187-223` (transcribed):
```
multiplier = acceptCoolant ? 1 + liquids.current().heatCapacity * coolantMultiplier * optionalEfficiency : 1
if target != null && (target.dead || target.dst(this) - target.hitSize/2 > repairRadius || target.health >= target.maxHealth) target = null
healed = false
if target != null && efficiency > 0:
   angle = angle(x, y, target.x, target.y)            // offset is visual jitter; use 0
   if angleDist(angle, rotation) < 30: healed = true; target.heal(repairSpeed * strength * edelta() * multiplier)
   rotation = slerpDelta(rotation, angle, 0.5 * efficiency * timeScale)
strength = lerpDelta(strength, healed ? 1 : 0, 0.08 * Time.delta)
if timer(timerTarget, 20): target = Units.closest(team, x, y, repairRadius, Unit::damaged)
shouldConsume = target != null && enabled
```
Defaults: rotation 90, repairRadius 50, repairSpeed 0.3, coolantMultiplier 1. Blocks
(`Blocks.java:6420-6443`): repair-point repairSpeed 0.45, radius 60, powerUse 1;
repair-turret size 2, repairSpeed 3, radius 145, powerUse 5, coolantUse 0.16,
coolantMultiplier 1.6, acceptCoolant true. `Units.closest`: same team, `within(x,y, range + unit.hitSize/2)`, smallest dst2.

**Port today.** `RepairTurret` component (`core/world/blocks/units/mod.rs:533-565`) has only
`repair_radius`, `repair_speed`, `target_interval` and `update_target()`;
`RepairTurretBehavior::update_tile` (`units/behavior.rs:815-823`) just ticks it. Heal helper
`combat::damage::area::heal_health(world, entity, amount)` exists (`combat/damage/area.rs:257`);
unit hitbox `HitboxComp.hit_size` (`core/entities/comp/unit/comp.rs:112`); units carry `Health`,
`Pos`, `TeamComp`, `UnitCore.dead`.

**Changes.** Extend `RepairTurret` with `target: Option<Entity>`, `rotation: f32` (90),
`strength: f32`, `target_timer: f32`, `accept_coolant: bool`, `coolant_multiplier: f32`; port the
loop (angle in degrees, `slerpDelta` = `Angles.moveToward`-style slerp: use the port's
`crate::math::angles::slerp_delta` if it exists — `grep -rn "fn slerp" core/math`); `should_consume`
→ `target.is_some()`. Fix the registered values (`units/behavior.rs:920`) to Java's. Coolant: the
repair-turret spec must have an optional coolant consumer (check the spec in
`core/content/registries/blocks/units.rs`). Unit health: `Health` on the unit; `damaged()` =
`health < max_health - 0.001`.

**Tests** (`combat_tests.rs`): `repair_point_heals_damaged_dagger`: `live_sim(48)`, repair-point
team 1 at (20,20), power-source (21,20); dagger team 1 at world (20*8+4+30, 20*8+4) with health
set to 50 of 130; tick 300; health > 50 and ≤ 130. Expected: once strength≈1 and aligned,
+0.45/tick → over ~250 aligned ticks ≈ +100 (clamped to 130).

**Verify in-engine.** Ground Zero: place repair-point (135,58) + power-source (136,58); the
player alpha is a unit — damage it? No seam: "headless only" unless `unit_debug_json` shows a damaged
friendly unit after a wave.

**Commit.** `Gameplay G02 T12: repair point heals friendly units`

### T13. `unitOn` hook: shock mines trigger, conveyors carry ground units

**Symptom / acceptance.** Enemy ground units walk over shock mines with no effect, and
conveyors do not move units standing on them. After: a dagger stepping on an enemy shock mine
triggers 4 lightning tendrils (damage 25, length 10) at most every 80 ticks and the mine takes
7 damage; a dagger on a running conveyor drifts along it.

**Java.**
* `J/entities/comp/UnitComp.java:832-840`: every unit update, `if(tile != null && isGrounded() && !type.hovering && tile.build != null) tile.build.unitOn(self());` (`tile` = tile under the unit centre).
* `ShockMine.java:69-85`: `unitOn`: `if(enabled && unit.team != team && timer(timerDamage, cooldown)) { triggered(); damage(tileDamage); }`; `triggered`: `tendrils` × `Lightning.create(Bullets.damageLightningGround, team, lightningColor, damage, x, y, random(360), length)` (+ optional bullet shots, null for vanilla). Blocks shock-mine: damage 25, tileDamage 7, length 10, tendrils 4, cooldown 80 (`Blocks.java:1980-1988`).
* `Conveyor.java:227-251 unitOn` (transcribed):
  ```
  if(clogHeat > 0.5 || !enabled) return; noSleep();
  mspeed = speed * 8 * 55; centerSpeed = 0.1; centerDstScl = 3;
  tx, ty = d4(rotation); centerx = centery = 0
  if |tx| > |ty|: centery = clamp((y - unit.y)/3, -0.1, 0.1); if |y - unit.y| < 1: centery = 0
  else:           centerx = clamp((x - unit.x)/3, -0.1, 0.1); if |x - unit.x| < 1: centerx = 0
  if(len * itemSpace < 0.9) unit.impulse((tx*mspeed + centerx) * delta(), (ty*mspeed + centery) * delta())
  ```
  (`speed`: conveyor 0.03, titanium 0.08, armored 0.08; `itemSpace = 0.4`). `impulse(x,y)` = `vel += (x,y) / mass` (`VelComp`/`PhysicsComp` — use the port's impulse helper: `grep -rn "fn impulse" core/entities core/combat`).

**Port today.** No `unit_on` hook (`grep -rn unit_on core` → only `shock_mine_unit_on`,
`core/world/blocks/defense/shields.rs:517`, test-only). Unit update lives in
`core/sim/unit_runtime.rs` (`unit_update_system`, G01's file).

**Changes.**
1. Trait method `fn unit_on(&self, world: &mut World, e: Entity, unit: Entity) {}` in
   `core/world/behavior/mod.rs`.
2. `ShockMineBehavior::unit_on`: needs lightning (`crate::combat::lightning::create(ctx, ...)` takes
   `CombatCtx`). Do NOT call from the behavior; instead record the trigger: push
   `(mine, unit_team)` into a new resource `PendingMineTriggers(Vec<(Entity,u8)>)`; drain it in the
   unit pass right after the call loop (below) where a `CombatCtx` exists, calling
   `shock_mine_unit_on(&mut ctx, mine, &mut state, team, unit_team)` (take/put `ShockMineState`),
   then `apply_health` 7 to the mine on success. Fix `shock_mine_unit_on`: Java uses
   `timer(timerDamage, cooldown)` (timer returns true when ≥ cooldown elapsed since last trigger, first
   call true) — the port decrements on call; keep the port's per-tick decrement in `update_tile`
   and make `unit_on` trigger only when `timer_damage <= 0` (no decrement there).
3. `ConveyorBehavior::unit_on`: port the formula (needs `ConveyorBuild` fields `clog_heat`, `len`;
   check names in `distribution/conveyor.rs`).
4. Call site (coordinate with G01; one small hunk): in `unit_update_system` after movement, for
   each unit with `!flying` (grounded: elevation < 0.001) and not hovering (`UnitTypeDef.hovering`),
   tile = `world_to_tile(pos)`, `building = grid.entity_at(tile)` (the `UnitRuntime.grid` snapshot
   is fine), then `behavior.unit_on(world, building, unit)`; iterate units in `runtime.units`
   order (deterministic).

**Tests** (`combat_tests.rs`): `shock_mine_zaps_enemy_dagger`: mine team 1 at (20,20); dagger team 2
spawned at the mine centre, held still (set its target to itself or `Vel` zero each tick); after 2
ticks the dagger lost health and the mine health = max − 7; after 79 more ticks no second trigger,
after 81 another. `conveyor_carries_dagger`: titanium conveyor row (10..20, 20) rot 0, dagger team 1
at (12,20) with no command; tick 60; x increased.

**Verify in-engine.** Ground Zero: `place_block(140,62,"shock-mine")`; spawn a team-2 dagger on it
(`apply_sim_command_json` spawn_unit at px=140*8+4, py=62*8+4); step 10; `unit_debug_json` shows its
health reduced.

**Commit.** `Gameplay G02 T13: unitOn hook for shock mines and conveyors`

### T14. Wall bullet reactions: surge lightning, phase deflection

**Symptom / acceptance.** Surge walls never arc lightning, phase walls never reflect bullets.
After: a surge wall hit has a 5% chance to spawn lightning (damage 20, length 17) back along the
bullet; a phase wall reflects a reflectable bullet with chance `10 / bullet.damage`.

**Java.** `J/world/blocks/defense/Wall.java:164-205` `collision(bullet)` (key lines): `super.collision(bullet)` (damage); `hit = 1`;
`if(lightningChance > 0 && chance(lightningChance)) Lightning.create(team, lightningColor, lightningDamage(20), x, y, bullet.rotation() + 180, lightningLength(17))`;
`if(chanceDeflect > 0){ if(bullet.vel.len() <= 0.1 || !bullet.type.reflectable) return true; if(!chance(chanceDeflect / bullet.damage())) return true; bullet.trns(-vel.x, -vel.y); penX = |x - bullet.x|, penY = |y - bullet.y|; if(penX > penY) vel.x *= -1 else vel.y *= -1; bullet.owner = this; bullet.team = team; bullet.time += 1; return false; }`.
Blocks: phase-wall `chanceDeflect = 10` (+large), surge-wall `lightningChance = 0.05` (+large) (`Blocks.java:1759-1786`).

**Port today.** `WallDef { lightning_chance, lightning_damage, lightning_length }` exists
(`core/world/block_kind_data.rs:182-191`) but no knobs set and no caller; no deflect field;
bullet→building damage is `combat/bullet/mod.rs:514 hit_building`, called at `:957` and `:1031`.

**Changes.** Add `chance_deflect: f32` to `WallDef`; set knobs in `apply_vanilla_knobs`
(phase-wall/-large deflect 10; surge-wall/-large lightning 0.05/20/17). In `collide_bullet` at the two
`hit_building` call sites, after damage: if the target's kind data is `Wall`, run the Java branch with
`ctx.rng.chance(RngStream::Sim, p)`; lightning via `crate::combat::lightning::create(ctx, team, color, 20.0, x, y, bullet_rot + 180, 17)`;
deflect: mutate the bullet's `Vel`/`Pos`/`TeamComp`/owner and return "not removed"
(`CollisionOutcome::Pierced`-like; read how `collide_bullet` decides removal and follow it).
`BulletDef.reflectable` — check the field exists (`grep -n reflectable core/content/registries/bullets.rs`).

**Tests** (`combat_tests.rs`): `phase_wall_reflects_low_damage_bullet` (dagger bullet damage 20 →
chance 0.5; fire 40 bullets into a phase wall with a fixed seed and assert some reversed `Vel.x`);
`surge_wall_arcs` (≥ 1 lightning entity in 100 hits).

**Commit.** `Gameplay G02 T14: surge wall lightning and phase wall deflection`
(File shared with G01: `combat/bullet/mod.rs`; keep the hunk small.)

### T15. Liquid acceptance: one liquid per conduit/router; mixing reactions

**Symptom / acceptance.** A conduit already carrying water accepts oil; Java refuses unless the
current amount < 0.2. Hot + flammable liquids meeting do not burn/steam. After: Java rule.

**Java.** `Conduit.java:241-245`: `(liquids.current() == liquid || liquids.currentAmount() < 0.2f) && (source.relativeTo(tile) + 2) % 4 != rotation`;
`LiquidRouter.java:41-43`: `liquids.current() == liquid || liquids.currentAmount() < 0.2f`;
`ItemBridge.acceptLiquid` (bridge conduits): same current rule + team + not from the linked side.
`BuildingComp.moveLiquid` else-branch (search `blockReactive` in `BuildingComp.java`): when the
target refuses and `!next.block.consumesLiquid(liquid) && next.liquids.currentAmount()/cap > 0.1 && fract > 0.1`:
`other = next.liquids.current()`; if both `blockReactive`: flammable(>0.3)+hot(>0.7) → both
`damageContinuous(0.1)` (+fire effect chance); hot(>0.7)+cold(<0.55) → `liquids.remove(liquid, min(get, 0.7*delta))` (steam).

**Port today.** `movement.rs:95-116 accept_liquid` checks `node.accepts`, the conduit back side and
`node.filter`; `move_liquid_impl` (`:345-398`) has no else-branch. `LiquidModule::current()` exists.

**Changes.** In `accept_liquid` for `LiquidNode` blocks that are conduits/routers/tanks/bridges
(`!node.junction`): add `module.current() == liquid || module.current_amount() < 0.2` (check method
names in `core/world/modules/`). Add the reaction branch to `move_liquid_impl` using `LiquidTraits`
(T1) for `flammability/temperature/block_reactive` (add `block_reactive` to `LiquidTraits`;
field on `LiquidDef`: `grep -n block_reactive core/content/registries/liquids.rs`); damage via
`combat::damage::area::apply_health(world, e, 0.1)`.

**Tests.** `liquid/tests.rs`: `conduit_refuses_second_liquid` (water 5 in conduit → accept oil false;
drain to 0.1 → true); `slag_into_oil_conduit_damages` (both lose health over 60 ticks).

**Commit.** `Gameplay G02 T15: conduits keep one liquid; hot/flammable mixing reacts`

### T16. Beam node / beam tower (Erekir power transport)

**Symptom / acceptance.** Beam nodes connect nothing. After: a beam node links the first
same-team powered building (not a PowerNode) in each of the 4 directions within `range` tiles,
stopping at insulated buildings; links rebuild when tiles change.

**Java.** `J/world/blocks/power/BeamNode.java:189-293` (transcribed):
`updateTile: if(lastChange != world.tileChanges){ lastChange = world.tileChanges; updateDirections(); }`;
`updateDirections`: for i in 0..4: `prev = links[i]`; scan `j = 1+size/2 ..= range+size/2`:
`other = world.build(x + j*dx, y + j*dy)`; insulated → stop; `other.block.hasPower && connectedPower && same team && !(other.block instanceof PowerNode)` → `links[i] = other; break`.
If changed: prev unlinked (remove both `power.links` entries, reflow both graphs); new: add both
`links` entries, `power.graph.addGraph(next.power.graph)`. beam-node range 10, beam-tower size 3
range 23 (`Blocks.java:2667-2688`), both `consumesPower = outputsPower = true`,
`consumePowerBuffered(1000 / 40000)`.

**Port today.** `BlockKind::BeamNode` → `NoopBehavior`. Graph tools exist:
`PowerGrids::reflow(world, grid, e)`, `add_graph`, `PowerModule.links`.

**Changes.** Sim pass like `refresh_facing_drills`: `SimRuntime.beam_nodes: Vec<Entity>`,
`last_tile_changes: i32`; registered in `set_block_raw`/`materialize_buildings` when the kind is
`BeamNode`; `Sim::refresh_beam_nodes()` in `Sim::tick` right after `refresh_facing_drills()`,
running `update_directions` for every node when `self.grid.tile_changes` differs. Store
`BeamNodeState { links: [Option<Entity>; 4] }` on the entity. Range: from a knob table in
`power::register` (`beam-node` 10, `beam-tower` 23).

**Tests.** `beam_node_links_battery_in_line` (node (5,5), battery (12,5) → shared graph; insert a
plastanium wall at (8,5) → split).

**Commit.** `Gameplay G02 T16: beam nodes link power along four directions`

### T17. Per-kind save IO for the behaviors that lack it (unblocks PAR-0161 for G04)

**Symptom.** G04's loader (PAR-0161) will call `behavior.read` after `read_base`; these
behaviors have no `write/read`, so their Java bodies would be misread (every following
building in the save shifts). Java writes (`J/world/blocks/**` `write(Writes)`):
| Behavior (port) | Java `write` body (in order) |
|---|---|
| `ConsumeGeneratorBehavior`, `SolarGeneratorBehavior`, `ThermalGeneratorBehavior`, `HeaterGeneratorBehavior` | `PowerGenerator.write`: `f productionEfficiency`, `f generateTime` (HeaterGenerator: + `f heat`) |
| `NuclearReactorBehavior` | PowerGenerator + `f heat` (check `NuclearReactor.java write`) |
| `ImpactReactorBehavior` | PowerGenerator + `f warmup` |
| `VariableReactorBehavior` | PowerGenerator + `f heat, f instability, f warmup` (check order) |
| `ForceProjectorBehavior` | `bool broken, f buildup, f radscl, f warmup, f phaseHeat` |
| `MendProjectorBehavior` | `f heat, f phaseHeat` |
| `OverdriveProjectorBehavior` | `f heat, f phaseHeat` |
| `StackConveyorBehavior` | `i link, f cooldown` (check) |
| `LiquidBridgeBehavior` | ItemBridge body (T10 step 1) |
| `HeatCrafterBehavior` / `HeatProducerBehavior` | GenericCrafter body (+ `f heat` for producer) (T1 step 3) |
| `BeamDrillBehavior` | `f time, f warmup` (check `BeamDrill.java write`) |
| `RadarBehavior` | `f progress` |
| `ShieldWallBehavior` | `f shield` |
| `BaseShieldBehavior` | `f smoothRadius, bool broken` (check) |
| `TargetDummyDefenseBehavior` | see `TargetDummy.java write` |
| `RepairTurretBehavior` | `f rotation` (check `RepairTurret.java write`) |
| `AssemblerBehavior`, `CargoLoaderBehavior`, `CargoUnloadPointBehavior` | see the Java classes (Erekir; lower priority) |
Before writing each, open the Java `write`/`read`/`version` and copy the exact field list and
revision handling; add `version()` when Java overrides it.

**Changes.** Implement `write/read` for every row (one commit per family is fine: power,
defense, logistics). Each gets a round-trip test in the family's test module using
`BuildingCodec` (pattern: `placed_turret_ammo_and_reload_roundtrip`,
`core/world/blocks/defense/turrets/behavior.rs`).

**Commit.** `Gameplay G02 T17: per-kind save IO for generators, projectors, stack conveyors, bridges`

### T18. Verification tests for the previous plan's §5 UNVERIFIED rows

One test each in `production_tests.rs`; fix only if it fails against Java (then stop and write a
task note in the summary rather than a large fix):
* `separator_weighted_results` — separator with slag + power for 1000 crafts; every output is in
  {copper, lead, graphite, titanium} with Java weights 5/3/2/2 (`Blocks.java separator results`);
  assert each frequency within ±5 percentage points of weight/12.
* `thermal_generator_reads_heat_floor` — on 4 `hotrock` tiles (heat 0.5 each? read
  `Blocks.java hotrock attributes`), production = 1.8 * sum / 60 per tick? (`ThermalGenerator`
  `productionEfficiency = sumAttribute(heat)`; power per tick = powerProduction * productionEfficiency).
* `thorium_reactor_heats_without_coolant` — 30 thorium, no cryo: heat rises by
  `fullness * heating(0.005)` per tick; explodes at heat ≥ 0.999? (read `NuclearReactor.updateTile`).
* `plastanium_conveyor_moves_stacks` — 10 items loaded at the start reach the end as one stack.
* `mass_driver_shoots_to_linked` — two linked drivers 20 tiles apart with power; items arrive.
* `constructor_builds_selected_block` — constructor configured `copper-wall` with copper + power
  outputs a wall payload.
* `differential_needs_cryofluid` — no cryo → production 0.
**Commit.** `Gameplay G02 T18: verification tests for separator, thermal, reactor, stack conveyor, mass driver, constructor`

### Deferred (not tasks in this plan; why)

* D1 Fog-of-war placement check (`rules.staticFog && rules.fog && !fogControl.isDiscovered`):
  fog discovery is not modelled in the sim (G03/G04 own fog). Add once a discovered-tiles
  resource exists.
* D2 Campaign unlock gate in the sim placement path: the block runtime boots its own vanilla
  `ContentRegistry` (`load_vanilla_content`, `core/sim/runtime.rs:926`) with no research state;
  the palette gates unlocks (`core/ui/campaign.rs:429-470`). Syncing research into the runtime is
  a G05/G08 decision.
* D3 Eruption drill / impact drill visual & sound (`shake`, `drillSound`) — G03/G07.
* D4 Erekir unit/defense residuals: repair tower heal, unit assembler verification, cargo
  loader item intake (`acceptItem: items.total() < itemCapacity`) + unload point item config,
  `ShieldWall.damage` absorption, `BaseShield` unit push + `absorbable` check, `RegenProjector`
  verification, build tower IO. Lower player impact than Serpulo; plan them after T1-T18.
* D5 Heal suppression (`checkSuppression`, `isHealSuppressed`) — needs the Erekir suppression
  ability (G01).
* D6 Config panel unlock filtering if the gdext content snapshot is the vanilla registry (T10 step 6).

---

## 3. Execution order, dependencies, shared files

1. T7 (tiny, makes every later in-engine check honest).
2. T1 (systemic intake) → T5 (incinerator uses `LiquidTraits` from T1) → T6.
3. T2 (node knobs + Configure) → T10 (generic tap mechanism; must land before G06 T11).
   T1 must land before G08 T2a (stack hooks).
4. T3 (force projector; uses `LiquidTraits`) → T4.
5. T8 (validPlace + `sync_build_rules` + `kind_rotates` + darkness/limit-area + GeneratorEnv) → T9 (core rules use T8's `world` param and rules fields).
6. T12, T13, T14 (combat-adjacent; touch G01 files, coordinate).
7. T11 (diagnostic), T15, T16, T17, T18.

After every task: `cargo test -p mind-core --lib` (full suite with `--features msav-import` before
committing), `mind-headless --lib` (only G00's known failures), clippy clean, fmt, build,
`tools/session-play.sh N`, the task's in-engine recipe, `grep -c panic` on the newest log = 0.

Shared files (pull `main` first; small hunks):
* `core/world/behavior/mod.rs` (T1 trait defaults, T13 hook) — every block family.
* `core/sim/mod.rs` (T2 Configure arm, T7 `can_place_for_team`, T16 tick pass).
* `core/sim/runtime.rs` (T2 `new_building`, T9 `begin_place`, T16 list).
* `core/world/build.rs` + callers (T8/T9): team AI (`core/game/*` `team_ai_system`), schematics,
  gdext placement preview — G08 (input preview) and G01/G04 (AI) read it.
* `core/sim/unit_runtime.rs` (T13 call site) and `combat/bullet/mod.rs` (T14) — G01 owns.
* `gdext/input/desktop.rs`, `gdext/input/mod.rs`, `ui/fragments/block_config_fragment.gd` (T10) — G08 owns input.
* `gdext/sim_host.rs` (T3 building_debug fields, T7) — everyone; after editing run
  `grep -nE '///.*\S\s{2,}#\[' client/rust/mind-gdext/src/sim_host.rs` (must print nothing).
* `parity/golden_content.json` + manifest (T1 shockwave spec, T3 force projector spec).

---

## 4. Ledger changes to report (do not edit chains.json)

| Item | Change |
|---|---|
| PAR-0075 | Code fixed (test `combustion_generator_accepts_coal_and_generates`). Executor: in-engine belt check — Ground Zero `place_block(150,80,"combustion-generator")`, `item-source` at (148,80) configured coal, `conveyor` at (149,80) rot 0 (`place_block_rot`); step 120; `power_debug(150,80)` shows production > 0 → status `fixed` (or `verified` per ledger rules). |
| PAR-0076 | Already fixed by session-4 T1-T4; nothing left in G02 → recommend `fixed`. |
| PAR-0078 | Code fixed (pumps + solid pumps; tests listed in §1.3). In-engine: `place_block` a mechanical pump on Ground Zero dry ground → `place_block` returns false after T7; on shallow water (find one with `tile_debug`) → water output. → `fixed`. |
| PAR-0089 | Code fixed (test `sublimate_activation_window`); headless only (no Erekir sector) → `fixed (headless)`. |
| PAR-0098 | Sim side done except validPlace completeness (T8) and core upgrade (T9). Remaining outside G02: plan/beam drawing (G03), rebuild select (G08), construct-block load (PAR-0161, G04). Keep `fixing` until those land. |
| PAR-0165 | T7 → `fixed`. |
| PAR-0161 | G04 owns the loader; T17 supplies the missing per-kind `write/read` (list in T17). |
| New: "Blocks without a consumer override reject all items/liquids (drill water boost, mender silicon, reactors, heat crafters, unloaders from crafters)" | file → fixed by T1 |
| New: "Power-node-large/surge-tower use the default 3-link/6-tile node config; node Configure ignored; diode inert" | T2 |
| New: "Force projector never absorbs bullets in the live game; no phase/coolant" | T3 |
| New: "Menders/overdrive affect enemy buildings and never use boost items" | T4 |
| New: "Incinerator accepts nothing" | T5 |
| New: "Drill period ignores item hardness; burst drills do not burst" | T6 |
| New: "Placement allows enemy-core radius, deep water, enemy tiles; break allows enemy buildings; sector banned blocks ignored" | T8 |
| New: "Cores placeable anywhere, no core upgrade, last core breakable" | T9 |
| New: "Bridges/nodes/drivers cannot be linked by tapping; bridge conduits cannot be linked at all" | T10 |
| New: "Repair point never heals" | T12 |
| New: "Shock mines and conveyor unit transport inert" | T13 |
| New: "Surge/phase walls have no special reaction" | T14 |

---

## 5. Done criteria

* Every task T1-T18 is committed or reported blocked with the exact failing value.
* New tests listed per task exist and pass; full `mind-core` suite (with `msav-import`) passes;
  `mind-headless` shows only G00's known failures; clippy and fmt clean; GDScript parse gate clean.
* In-engine recipes for T1, T2, T3, T5, T6, T7, T9, T10, T11, T13 recorded under
  `runs/<YYYYMMDD>-s<N>-G02-<task>/` (JSON of the eval outputs + screenshots where stated); no
  `panic` in the run log.
* Summary `docs/parity-plans/G02-blocks_summary.md` with the per-task table, ledger table (§4),
  and any goldens regenerated (named with the Java reason).

---

## 6. Cross-area findings

1. **G03 (rendering)**: force projector dome needs `ForceProjectorState.radscl/hit/broken`
   (exposed by T3 in `building_debug`); bridge/node laser drawing after T10; the illuminator light
   colour (prev T10) still undrawn.
2. **G01 (units)**: `UnitComp.update` `unitOnAny`/`unitOn` call site and floor damage (`apply damage`
   after `unitOn`, `UnitComp.java:842+`) are absent; T13 adds only the `unitOn` call. Heal
   suppression (D5). `BaseShield` unit push (`Units.nearbyEnemies` in `BaseShield.java`).
3. **G04 (saves)**: T17's table is the list of behaviors whose per-kind body must exist before
   PAR-0161's loader calls `read`; `TurretBehavior` uses one `save::write` for every turret kind —
   verify `ContinuousTurret`, `PointDefenseTurret`, `TractorBeamTurret`, `PayloadAmmoTurret` and
   `BuildTurret` bodies (`J/world/blocks/defense/turrets/*.java write`) against
   `core/world/blocks/defense/turrets` `save::write`.
4. **G08 (input)**: placement preview must call the same `valid_place_at(.., check_visible=true)`
   as T8 so ghost colours match; config tap interactability for enemy buildings (T10 step 4).
5. **G05/G08**: research state is not in the block runtime registry (D2) — any sim-side unlock
   check is wrong today; the palette is the only gate.
6. **G04/G06**: rules reach `BuildRules` only through `world::limits::sync_build_rules(world, &rules)`
   (T8). G04 calls it on load (T8 wires that call) and after any rules swap; G06's `setrule` handler
   must call it after mutating `Rules`. G07 T9 owns `GeneratorEnv` weather values; T8 step 8 fills
   the rules half for non-campaign sims.
7. **G06**: logic-block linking plugs into `BuildingBehavior::on_configure_build_tapped` (T10); no
   second tap mechanism.
