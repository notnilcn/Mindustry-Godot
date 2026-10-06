Content-breadth audit — Mindustry-Godot (port vs upstream 2cd7aeec)
Method: compared the 447/65/22/11/23/6 data registries in parity/golden_content.json against upstream counts (all match), then traced each family from its content definition to a registered BuildingBehavior/system and to a caller on the live tick path. parity/reports/content_audit.md passes because it audits data only; every gap below is behavioral/wiring, not metadata.
Ranked NEW gaps
GAP: Live simulation never instantiates the block runtime (BlockTable/behaviors) — every placed building is inert
- severity: S1
- port: client/rust/mind-core/src/sim/mod.rs:249-268 (Sim::apply spawns only EntitySeq+BuildingComp), client/rust/mind-core/src/ecs.rs:79-81, client/rust/mind-core/src/sim/schedule.rs:405-408 (schedule registers only update_buildings), client/rust/mind-gdext/src/sim_host.rs:920-926 (sector load swaps a raw grid into a fresh P0 Sim)
- upstream: core/src/mindustry/core/Logic.java:464 (updateEntities), entities/comp/BuildingComp.java (update/updateConsumption)
- symptom: player-placed drills, conveyors, factories, cores, turrets, power and logic blocks set a grid cell and nothing else — no mining, transport, crafting, power, item storage, or building health; Ground Zero and every launched sector are unplayable beyond terrain/placement
- campaign: blocks (all sectors, both planets)
- confidence: high
- scale: all 447 blocks
GAP: No unit, weapon, bullet, AI, wave, fire or puddle system runs in the live tick path
- port: sim/schedule.rs:346-409 (build_sim_schedule adds only update_buildings); all drivers are harness-only: combat/harness.rs:171/449/675, ai/harness.rs:156/282; ai/wave_spawner.rs has no production caller (run_wave_campaign is model-only, game/play.rs:373 ← mind-gdext/src/campaign.rs:204)
- upstream: Logic.java:464 (Groups.unit/Groups.bullet updates), ai/WaveSpawner.java
- symptom: no waves spawn, spawned units never move/acquire targets/fire, bullets never advance, fires/puddles never tick; spawners/reconstructors can create units that remain frozen
- campaign: blocks (every sector; wave defense is the core loop)
- confidence: high
- scale: 65 units, 112 bullet defs, 30 turrets, 46 sectors
GAP: Crafter output recipes are only wired for 11 of 27 factory-family blocks — 16 factories consume inputs and emit nothing
- port: world/block_kind_data.rs:640-782 (apply_vanilla_knobs name overlay is the only source of CrafterDef.output_items/output_liquids; BlockDef carries no output stacks), world/behavior/production.rs:237-251 (empty outputs → consumes, resets progress, produces nothing); the heat register (world/blocks/heat/behavior.rs:188-221) adds efficiency only, not recipes
- upstream: Blocks.java:1041 graphite-press, 1053 multi-press, 1086 silicon-crucible, 1120 plastanium-compressor, 1138 phase-weaver, 1170 cryofluid-mixer, 1191 pyratite-mixer, 1206 blast-mixer, 1335 silicon-arc-furnace, 1356 electrolyzer, 1400 atmospheric-concentrator, 1537 carbide-crucible, 1556 slag-centrifuge, 1587 surge-crucible, 1623 cyanogen-synthesizer, 1655 phase-synthesizer
- symptom: Serpulo cannot make graphite (graphite-press), plastanium, phase fabric, cryofluid, pyratite, blast compound; Erekir cannot run any HeatCrafter (carbide/surge/cyanogen/phase/atmospheric); bases stall mid-campaign
- campaign: blocks
- confidence: high
- scale: 16 of 27 production blocks
GAP: Erekir unit assemblers are constructed with empty plans and have no update_tile — 6 T4/T5 units unbuildable
- port: world/blocks/units/behavior.rs:394-397 (UnitAssembler::new(Vec::new(), 240.0) ignores def.assembler_plans), :285-296 (AssemblerBehavior only inserts state; module behavior is a no-op at :294-296); BlockDef.assembler_plans populated at content/registries/blocks/units_erekir.rs:206/240/274 but never read
- upstream: Blocks.java:6558/6573/6587/6603; world/blocks/units/UnitAssembler.java:52 (plans), :433 (updateTile)
- symptom: tank/ship/mech-assembler never assemble; vanquish, conquer, quell, disrupt, tecta, collaris and basic-assembler-module tier boosts are unreachable on Erekir
- campaign: blocks
- confidence: high
- scale: 3 assemblers + module, 6 unit types
GAP: All 28 combat turrets and 11 projector/shield/mine blocks are absent from the real behavior registry (harness-only or unimplemented)
- port: world/blocks/mod.rs:24-33 (default_registry omits defense::register), world/behavior/mod.rs:489-529 (no arm for ItemTurret…ContinuousTurret, Mend/Overdrive/Force/Regen/Shockwave/BaseShield/ShockMine/BuildTurret → NoopBehavior); turret configs registered only in combat/harness.rs:59-104, shield update fns only called from combat/harness.rs:645-666; OverdriveProjector, RegenProjector, ShockwaveTower have no Rust implementation at all (0 grep hits under world//combat/)
- upstream: Blocks.java:3276-6430 (turret block) and :1934 mend-projector, 1946 overdrive-projector, 1966 force-projector, 1999 build-tower, 2011 regen-projector, 2040 shockwave-tower, 2050/2058 shield-projectors
- symptom: turrets never create TurretState, can't be fed ammo, never target/fire; menders/force shields/shock mines/overdrive/regen do nothing; only reachable behavior is inside test harnesses
- campaign: blocks
- confidence: high
- scale: 28 turrets + build-tower + 10 defense blocks (39)
GAP: Tech-tree unlocks and BuildVisibility never gate the palette or placement
- port: ui/campaign.rs:284-337 (catalog filters only build_time > 0; comment: "Unlock filtering is the HUD's job once research state is bound"), sim/mod.rs:249-268 accepts any known non-air block; mind-gdext/src/campaign.rs:89-102 owns a separate registry + MemoryUnlockStore that MindSimHost never reads; BuildVisibility::Hidden/DebugOnly/SandboxOnly/EditorOnly ignored by the catalog and by world/block.rs:446-451
- upstream: ResearchDialog.java / Block.unlockedNow(); BuildVisibility filtering
- symptom: all blocks (including sandbox power-source/item-source/payload-source, debug and editor-only blocks) are buildable from game start; researching in the campaign UI has no effect on the live match, so progression/conquest is cosmetic
- campaign: blocks
- confidence: high
- scale: 447 blocks / both tech trees
GAP: Power network is never updated; nodes never get PowerNodeConfig
- port: world/blocks/power/module.rs:209 update_power_graph has no non-test/production caller (only mind-headless/src/network_scenarios.rs:317,344), BuildHarness::tick only runs update_buildings (world/harness.rs:428); PowerNodeConfig is declared (power/mod.rs:114) but never inserted anywhere; schedule's UpdatePowerGraph slot is empty (sim/schedule.rs:143-146)
- upstream: PowerGraph.update() in Logic.updateEntities; Block.updatePowerGraph()
- symptom: generators and batteries never charge a grid and consumers keep PowerModule.status = 0, so every power-gated machine (most factories/heat blocks) runs at efficiency 0 unless Rules.cheat; power nodes/diodes/batteries/beams have no effect
- campaign: degrades (all powered production)
- confidence: high (grep-proven, no insertion site)
- scale: 9 node/battery/beam blocks + all powered consumers
GAP: Liquid bridges are unregistered and inert
- port: world/blocks/liquid/behavior.rs:369-400 (register covers conduits/routers/junctions/containers only), liquid/bridge.rs update fns (update_liquid_bridge, update_direction_liquid_bridge) have no callers outside the module
- upstream: Blocks.java:2375 bridge-conduit, 2387 phase-conduit, 2435 reinforced-bridge-conduit; world/blocks/liquid/LiquidBridge.java
- symptom: liquids cannot cross gaps; phase-conduit teleport and reinforced bridge don't work; blocks place and do nothing
- campaign: blocks (Erekir fluid chains / Serpulo oil-water routing)
- confidence: high
- scale: 3 blocks
GAP: Production knob overlay gaps: 4 pumps produce zero liquid; eruption-drill has a zero drill time (mass-produces / wrong tier)
- port: world/block_kind_data.rs:640-782 never sets PumpDef.pump_amount for mechanical/rotary/impulse/reinforced pumps (default 0 at :120-125), yet production.rs:478-496 multiplies by it → 0 water; eruption-drill is likewise absent, leaving BurstDrillDef at zero (:94-105), so production.rs:308 clamps drill_time to 0.0001 and mines instantly with no blocked items
- upstream: Blocks.java:2299 mechanical-pump (pumpAmount 7/60), 2305 rotary, 3112 eruption-drill
- symptom: pumps are useless; eruption-drill trivializes Erekir mining
- campaign: degrades
- confidence: high
- scale: 4 pumps + 1 drill
GAP: All unit abilities are metadata-only (no runtime for AbilityKind)
- port: content/registries/units/ability.rs:20-45 (enum + names only; zero non-content/non-mods consumers). Abilities are populated for scepter, nova, pulsar, quasar, poly, oct, bryde, oxynoe, aegires, navanax (units/standard.rs:315,433,503,560,1671,1850,1855,2132,2515,2740,3040) and tecta, elude, quell-missile, disrupt, latum (units/erekir.rs:1913,2345,2728,2913,2923,2933,2991)
- upstream: entities/abilities/* (RepairFieldAbility.update, ForceFieldAbility.update, EnergyFieldAbility.update, …)
- symptom: signature unit abilities (repair fields, shield/force fields, energy fields, suppression, death spawns, neoplasm liquid regen) never run even in harness fights
- campaign: n-a (degrades combat)
- confidence: high
- scale: 15 unit types
GAP: Status effects are never applied or ticked, and weathers have no gameplay effect
- port: combat/damage/status.rs:20-43 — StatusApply has no production implementation (only the test recorder at :117), so every apply_status call is a no-op; entities/comp/unit/comp.rs:333-370 stores entries but nothing decrements duration or reads modifiers; fx/weather_fx.rs is view-only (no weather runtime)
- upstream: type/StatusEffect.java (update/apply), content/Weathers.java particle effects
- symptom: EMP/wet/burning/slow/shielded/etc. do nothing; weather is purely cosmetic
- campaign: n-a (degrades combat)
- confidence: high
- scale: 23 statuses, 6 weathers
GAP: unit-cargo-loader is inert and configured to spawn unit id 0 (dagger), not manifold
- port: world/blocks/units/behavior.rs:404-409 (unit_type: UnitTypeId::new(0)), CargoLoaderBehavior has no update_tile
- upstream: Blocks.java unit-cargo-loader → UnitTypes.manifold; world/blocks/units/UnitCargoLoader.java
- symptom: Erekir cargo distribution blocks do nothing; if ever driven, would spawn daggers
- campaign: n-a (Erekir optional logistics)
- confidence: high
- scale: 2 blocks
Coverage notes (genuinely complete)
 1. Data-level content is complete and ID/field-parity-audited: blocks 447/447, units 65/65 (Ledger units.md), items 22, liquids 11, statuses 23, weathers 6, planets 7, sectors 46, bullets 112; parity/reports/content_audit.md PASS is consistent with these counts.
 2. Item distribution is fully registered: all 26 logistics blocks (conveyors/ducts/junctions/routers/sorters/gates/bridges/mass-driver/directional unloader) in world/blocks/distribution/mod.rs:60-97 plus unloader in storage.
 3. Storage/cores are wired: container/vault/reinforced variants, unloader, and all 6 core tiers (world/blocks/storage/mod.rs:26-45).
 4. Payload family fully registered: 12 block defs plus payload source/void (world/blocks/payloads/mod.rs:47-104).
 5. Liquid conduits/routers/junctions/tanks: 13 blocks registered with movement, leak/armor flags and junction/router logic (world/blocks/liquid/behavior.rs:369-400) — only the 3 bridges are missing.
 6. Power producers/reactors have exact vanilla knob tables (combustion→neoplasm, 15 blocks) including fluid filters, outputs and explosion thresholds (world/blocks/power/behavior.rs:803-1019); the deficit is grid wiring, not data.
 7. Heat production/conditioning: 6 producers, 5 HeatCrafters and 3 conductors have state + heat math (world/blocks/heat/behavior.rs:188-245); only HeatCrafter recipes are missing.
 8. Unit factories/reconstructors read real unit_plans/reconstructor_upgrades from BlockDef and spawn the correct T1–T3 units (world/blocks/units/behavior.rs:89-111,203-215), Serpulo and Erekir.
 9. Logic blocks: 15 logic/memory/switch/message/display/canvas blocks map to logic::blocks::* behaviors (world/behavior/mod.rs:514-522).
10. Environment terrain kinds all dispatch to EnvironmentBehavior, and bullets/weapons have complete per-kind behavior modules (combat/bullet/kinds/*, weapons/*) — exercised by harnesses, not the live schedule.
Key blind spot to state plainly: the repository's behavior stack (world/blocks, ai, weapons, combat) is high-quality but is only reached by BuildHarness/CombatHarness/UnitHarness; the shipping MindSimHost path bypasses it and cannot yet present any of it to a player.
