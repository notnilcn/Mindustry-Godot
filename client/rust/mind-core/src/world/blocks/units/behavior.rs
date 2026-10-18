// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit-block `BuildingBehavior`s (plan 11 §3.11, M7 wiring half).
//!
//! Ported from `world/blocks/units/*.java`'s `updateTile`/`acceptItem`/
//! `acceptPayload` overrides. Plan 07's [`BuildingBehavior::update_tile`] only
//! receives `&mut World`, so the resolved content (factory plans, upgrade pairs,
//! produced [`UnitTypeDef`]s) is captured into the behavior at registry-build
//! time in [`register`]; the kernels from [`super`] are then driven per tick by
//! `world::update::update_buildings`. Unit spawning uses plan 11's
//! [`spawn_unit_def`], so produced units enter the same lifecycle as any other
//! spawn.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::registries::units::UnitTypeDef;
use crate::content::{BlockKind, ContentRegistry, ItemId, UnitCommandId, UnitTypeId};
use crate::entities::comp::unit::lifecycle::spawn_unit_def;
use crate::entities::comp::{Pos, TeamComp};
use crate::world::behavior::{BuildingBehavior, BuildingReader, BuildingWriter, PayloadRef};
use crate::world::blocks::distribution::transfer;
use crate::world::config::{ConfigKind, ConfigValue};
use crate::world::modules::ItemModule;

use super::{
    AssemblerPayload, Reconstructor, ReconstructorBuild, RepairTower, RepairTurret, UnitAssembler,
    UnitCargoLoader, UnitCargoUnloadPoint, UnitFactory, UnitFactoryBuild, UnitPlan,
};

/// Monotonic entity-sequence allocator for units produced by blocks.
///
/// Mirrors plan 11's per-harness `seq`; the host/reset inserts or resets it.
#[derive(Debug, Default, bevy_ecs::prelude::Resource)]
pub struct UnitBlockSeq(pub u64);

impl UnitBlockSeq {
    /// First entity sequence a block-produced unit gets.
    pub const BASE: u64 = 1_500_000;

    /// Raises the allocator past the restored unit id `past`
    /// (`EntityGroup.checkNextId`, I01 T10): after a load, block outputs must
    /// not reuse an id a saved unit already owns. Inserts the resource when
    /// absent, so the next block spawn keeps the [`Self::BASE`] floor.
    pub fn bump_past(world: &mut World, past: Option<u64>) {
        let Some(past) = past else {
            return;
        };
        let mut seq = world
            .remove_resource::<UnitBlockSeq>()
            .unwrap_or(UnitBlockSeq(Self::BASE));
        seq.0 = seq.0.max(past.saturating_add(1)).max(Self::BASE);
        world.insert_resource(seq);
    }
}

pub(crate) fn next_unit_seq(world: &mut World) -> u64 {
    if !world.contains_resource::<UnitBlockSeq>() {
        world.insert_resource(UnitBlockSeq(UnitBlockSeq::BASE));
    }
    let mut seq = world.resource_mut::<UnitBlockSeq>();
    let value = seq.0;
    seq.0 = seq.0.wrapping_add(1);
    value
}

/// Spawns `unit` at the center of building `e` (`UnitBlock` output path).
///
/// The spawned entity is adopted into the live [`UnitRuntime`] list (the same
/// bookkeeping `UnitRuntime::spawn`/`SpawnedByBullet` do), so factory output
/// updates, fights and is selectable instead of sitting inert.
fn spawn_output_unit(world: &mut World, e: Entity, def: &UnitTypeDef) -> Option<Entity> {
    let pos = world.get::<Pos>(e).copied()?;
    let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
    let seq = next_unit_seq(world);
    let entity = spawn_unit_def(world, seq, def, team, pos.x, pos.y, 90.0);
    adopt_output_unit(world, entity, &def.name);
    Some(entity)
}

/// Adopts a block-produced unit into the live [`UnitRuntime`] list (the same
/// bookkeeping `UnitRuntime::spawn`/`SpawnedByBullet` do), so its controller
/// updates, fights and is selectable instead of sitting inert.
pub(crate) fn adopt_output_unit(world: &mut World, entity: Entity, name: &str) {
    if let Some(mut runtime) = world.get_non_send_mut::<crate::sim::unit_runtime::UnitRuntime>() {
        runtime.units.push(entity);
        runtime.units_created += 1;
    }
    // `Logic`'s `UnitCreateEvent` (`Logic.java:228-235`, factory/reconstructor/
    // assembler output): H09 T10.
    crate::game::stats::notify_unit_created(world, entity, Some(name));
}

/// `Reconstructor.java:346-352`: spawns the upgraded unit and gives it the
/// configured command (or the type's `defaultCommand`) when it is commandable.
fn spawn_output_unit_with_command(
    world: &mut World,
    e: Entity,
    def: &UnitTypeDef,
    command: Option<UnitCommandId>,
) -> Option<Entity> {
    let entity = spawn_output_unit(world, e, def)?;
    if let Some(mut slot) = world.get_mut::<crate::ai::ControllerSlot>(entity)
        && slot.kind == crate::ai::AiKind::Command
        && let Some(command) = command.or(def.default_command)
    {
        slot.command = Some(command);
    }
    Some(entity)
}

/// Resolves every produced unit's [`UnitTypeDef`] from content.
fn resolve_units(
    content: &ContentRegistry,
    ids: impl IntoIterator<Item = UnitTypeId>,
) -> BTreeMap<u16, Arc<UnitTypeDef>> {
    let mut map = BTreeMap::new();
    for id in ids {
        if let Some(def) = content.unit(id) {
            map.insert(id.raw(), Arc::new(def.clone()));
        }
    }
    map
}

// ---------------------------------------------------------------------------
// UnitFactory
// ---------------------------------------------------------------------------

/// `UnitFactoryBuild` behavior (`UnitFactory.updateTile`/`acceptItem`).
pub struct UnitFactoryBehavior {
    /// Resolved factory plans + capacities.
    pub factory: UnitFactory,
    /// Produced unit defs keyed by raw [`UnitTypeId`].
    pub units: BTreeMap<u16, Arc<UnitTypeDef>>,
    /// `UnitFactoryBuild.created()` default plan: the first unlocked plan
    /// (`plans.indexOf(u -> u.unit.unlockedNow())`).
    pub default_plan: i32,
}

impl UnitFactoryBehavior {
    /// Builds a behavior from a block's resolved metadata.
    pub fn from_def(def: &crate::content::BlockDef, content: &ContentRegistry) -> Self {
        let plans: Vec<UnitPlan> = def
            .unit_plans
            .iter()
            .map(|plan| {
                UnitPlan::new(
                    plan.unit,
                    plan.requirements
                        .iter()
                        .map(|stack| (stack.item, stack.amount))
                        .collect(),
                    plan.time,
                )
            })
            .collect();
        let factory = UnitFactory::new(plans, content.items().len());
        let units = resolve_units(content, def.unit_plans.iter().map(|plan| plan.unit));
        // `created()` auto-selects the first plan whose unit is
        // `unlockedNow()` (`unlocked() || !state.isCampaign()`). The live block
        // runtime boots its own vanilla registry (`load_vanilla_content`), which
        // never sees the campaign's research, so `unlocked()` here is false for
        // researched units; fall back to the first plan instead of leaving a
        // freshly placed factory idle (audit of PAR-0092).
        let default_plan = def
            .unit_plans
            .iter()
            .position(|plan| {
                content
                    .unit(plan.unit)
                    .map(|unit| unit.unlock.unlocked())
                    .unwrap_or(false)
            })
            .or((!def.unit_plans.is_empty()).then_some(0))
            .map(|index| index as i32)
            .unwrap_or(-1);
        Self {
            factory,
            units,
            default_plan,
        }
    }

    /// `UnitFactoryBuild.config()`: the selected plan index (`None` = unset).
    fn plan_index(&self, world: &World, e: Entity) -> Option<usize> {
        let state = world.get::<UnitFactoryBuild>(e)?;
        usize::try_from(state.current_plan)
            .ok()
            .filter(|index| *index < self.factory.plans.len())
    }

    /// `Block.configurations` body for `Integer`/`UnitType`
    /// (`UnitFactory.java:58-80`): clamp the index, reset progress and drop a
    /// command the new plan's unit cannot use.
    fn set_plan(&self, world: &mut World, e: Entity, index: i32) {
        let clamped = if index < 0 || index as usize >= self.factory.plans.len() {
            -1
        } else {
            index
        };
        let Some(mut state) = world.get_mut::<UnitFactoryBuild>(e) else {
            return;
        };
        if state.current_plan != clamped {
            state.current_plan = clamped;
            state.progress = 0.0;
        }
        if let Some(command) = state.command {
            let supported = usize::try_from(state.current_plan)
                .ok()
                .and_then(|index| self.factory.plans.get(index))
                .and_then(|plan| self.units.get(&plan.unit.raw()))
                .is_some_and(|unit| unit.commands.contains(&command));
            if !supported {
                state.command = None;
            }
        }
    }
}

impl BuildingBehavior for UnitFactoryBehavior {
    fn logic_sense(
        &self,
        world: &World,
        e: Entity,
        a: crate::logic::LAccess,
    ) -> Option<crate::logic::access::Sensed> {
        use crate::logic::LAccess;
        use crate::logic::access::Sensed;
        Some(match a {
            // `UnitFactory.sense` (`UnitFactory.java:278-282`).
            LAccess::Progress => {
                let state = world.get::<UnitFactoryBuild>(e)?;
                let plan = state.current_plan;
                let time = self
                    .factory
                    .plans
                    .get(usize::try_from(plan).unwrap_or(usize::MAX))
                    .map(|plan| plan.time)
                    .unwrap_or(0.0);
                Sensed::Num(if time > 0.0 {
                    f64::from((state.progress / time).clamp(0.0, 1.0))
                } else {
                    0.0
                })
            }
            // `UnitFactory.senseObject(config)` (`UnitFactory.java:272-275`):
            // `currentPlan == -1 ? null : plans.get(currentPlan).unit`.
            LAccess::Config => Sensed::Obj(
                self.plan_index(world, e)
                    .and_then(|index| self.factory.plans.get(index))
                    .map(|plan| {
                        crate::logic::value::LogicObject::Content(crate::content::ContentRef::new(
                            crate::content::ContentType::Unit,
                            plan.unit.raw(),
                        ))
                    }),
            ),
            _ => return None,
        })
    }
    fn create_state(&self, world: &mut World, e: Entity) {
        let mut state = UnitFactoryBuild::new();
        // `UnitFactoryBuild.created()`: auto-set to the first plan.
        state.current_plan = self.default_plan;
        world.entity_mut(e).insert(state);
    }

    /// `UnitFactory.shouldConsume` (`UnitFactory.java:453-456`): `currentPlan !=
    /// -1 && enabled && payload == null`. The port has no payload slot, so the
    /// payload term is skipped (T1).
    fn should_consume(&self, world: &World, e: Entity) -> bool {
        world
            .get::<UnitFactoryBuild>(e)
            .map(|state| state.current_plan >= 0)
            .unwrap_or(false)
    }

    fn config_kinds(&self) -> &'static [ConfigKind] {
        // `UnitFactory.config(Integer.class/UnitType.class/UnitCommand.class)`
        // (`UnitFactory.java:58-82`).
        &[ConfigKind::Unit, ConfigKind::Content]
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        // `UnitFactoryBuild.config()` (`UnitFactory.java:375-378`):
        // `currentPlan == -1 ? null : plans.get(currentPlan).unit`.
        self.plan_index(world, e)
            .and_then(|index| self.factory.plans.get(index))
            .map(|plan| ConfigValue::Unit(plan.unit))
            .unwrap_or(ConfigValue::None)
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        match value {
            ConfigValue::Number(number) => self.set_plan(world, e, number as i64 as i32),
            ConfigValue::Unit(unit) => {
                match self.factory.plans.iter().position(|plan| plan.unit == unit) {
                    Some(index) => self.set_plan(world, e, index as i32),
                    None => self.set_plan(world, e, -1),
                }
            }
            // `UnitFactory.config(UnitCommand.class)` (`UnitFactory.java:81`).
            ConfigValue::Content(crate::content::ContentType::UnitCommand, id) => {
                if let Some(mut state) = world.get_mut::<UnitFactoryBuild>(e) {
                    state.command = Some(crate::content::UnitCommandId::new(id));
                }
            }
            // `UnitFactory.configClear` (`UnitFactory.java:82`): only the
            // command is cleared; the plan stays selected.
            ConfigValue::None => {
                if let Some(mut state) = world.get_mut::<UnitFactoryBuild>(e) {
                    state.command = None;
                }
            }
            _ => {}
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        // `UnitFactoryBuild.version() = 3`.
        3
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        // `UnitFactoryBuild.write`: PayloadBlock body first
        // (`UnitFactory.java:486-506` extends `PayloadBlock.java:276-292`).
        crate::world::blocks::payloads::payload_block::write_base(world, e, w);
        // progress (f32), currentPlan (short), commandPos (nullable vec),
        // command (TypeIO command). The port has no commandPos state, so it is
        // written unset.
        if let Some(state) = world.get::<UnitFactoryBuild>(e) {
            w.f(state.progress);
            w.s(state.current_plan as i16);
        }
        crate::io::typeio::codecs::write_vec_nullable(w, None);
        crate::io::typeio::codecs::write_command(
            w,
            world
                .get::<UnitFactoryBuild>(e)
                .and_then(|state| state.command),
        );
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8) {
        if !crate::world::blocks::payloads::payload_block::read_base(world, e, r) {
            return;
        }
        let progress = r.f().unwrap_or(0.0);
        let current_plan = r.s().unwrap_or(-1) as i32;
        if revision >= 2 {
            let _ = crate::io::typeio::codecs::read_vec_nullable(r);
        }
        let command = if revision >= 3 {
            crate::io::typeio::codecs::read_command(r).ok().flatten()
        } else {
            None
        };
        if let Some(mut state) = world.get_mut::<UnitFactoryBuild>(e) {
            state.progress = progress;
            state.current_plan = current_plan;
            state.command = command;
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<UnitFactoryBuild>() else {
            return;
        };
        // `time += edelta() * speedScl * Vars.state.rules.unitBuildSpeed(team)`
        // and `progress += edelta() * unitBuildSpeed(team)`: `edelta()` carries
        // `Building.efficiency`, so the power gate works through it.
        let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
        let delta_progress = crate::world::update::edelta(world, e) * unit_build_speed(world, team);
        let plan_index = usize::try_from(state.current_plan).ok();
        let produced = world
            .get::<ItemModule>(e)
            .and_then(|items| state.update(&self.factory, delta_progress, items));
        if let Some(unit) = produced {
            let def = self.units.get(&unit.raw());
            // `UnitPayload.dump` refuses when `!Units.canCreate(team, type)`
            // (UnitPayload.java:104-112; G01 T12): hold the finished payload —
            // no item consumption and no spawn until the cap frees up.
            let allowed = def.is_some_and(|def| {
                crate::entities::comp::unit::queries::can_create_unit(world, team, def)
            });
            if !allowed {
                if let Some(mut st) = world.get_mut::<UnitFactoryBuild>(e) {
                    st.progress = 1.0;
                }
                world.entity_mut(e).insert(state);
                return;
            }
            // `consume()` with the `ConsumeItemDynamic` multiplier
            // (`UnitFactory.java:108`: `c.multiplier = rules.unitCost(team)`).
            if let Some(index) = plan_index
                && let Some(plan) = self.factory.plans.get(index)
            {
                let cost = unit_cost(world, team);
                if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                    for (item, amount) in &plan.requirements {
                        items.remove(*item, (*amount as f32 * cost).round() as i32);
                    }
                }
            }
            if let Some(def) = def {
                // `UnitFactory.java:437`: `unit.command().command(command ==
                // null && unit.type.defaultCommand != null ? defaultCommand :
                // command)` — an unset factory command keeps the type default.
                spawn_output_unit_with_command(world, e, def, state.command);
                // `UnitFactory.java:440`: `createSound.at(this, 1 + range(0.06), 1)`.
                if let Some((x, y)) = crate::audio::sim::building_centre_of(world, e) {
                    crate::audio::sim::emit_world_sound(
                        world,
                        crate::audio::SoundId::UNIT_CREATE,
                        x,
                        y,
                        1.0,
                        1.0,
                        0.06,
                    );
                }
            }
        }
        world.entity_mut(e).insert(state);
    }

    fn accept_item(&self, world: &World, e: Entity, _src: Entity, item: ItemId) -> bool {
        if !self.factory.accepts(item) {
            return false;
        }
        transfer::item_count(world, e, item) < self.factory.capacity(item)
    }

    fn get_maximum_accepted(&self, _world: &World, _e: Entity, item: ItemId) -> i32 {
        self.factory.capacity(item)
    }

    fn accept_stack(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        amount: i32,
        _source: Option<Entity>,
    ) -> i32 {
        if !self.accept_item(world, e, Entity::PLACEHOLDER, item) {
            return 0;
        }
        let held = transfer::item_count(world, e, item);
        let free = (self.factory.capacity(item) - held).max(0);
        free.min(amount)
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            let capacity = self.factory.capacity(item);
            items.add(item, amount, capacity);
        }
    }
}

// ---------------------------------------------------------------------------
// Reconstructor
// ---------------------------------------------------------------------------

/// `Trigger.cannotUpgrade` (`HintsFragment.java:84`, `Reconstructor.java:
/// 252-257`): a Reconstructor refused a locked upgrade target for a non-AI
/// team. Interior mutability because the `accept_payload` hook only gets
/// `&World`; drained by `Sim::tick` into `hint_events.events` (the
/// `FireCreated` pattern).
#[derive(Debug, Default, bevy_ecs::resource::Resource)]
pub struct CannotUpgradeFired(pub std::sync::atomic::AtomicBool);

/// `UnlockableContent.unlockedNowHost()` (`UnlockableContent.java:269-277`):
/// outside a campaign everything is unlocked; in one, the live unlocked set
/// (`CampaignRuntime.unlocked_content`) must contain the unit.
fn unit_unlocked_now_host(world: &World, name: &str) -> bool {
    let Some(runtime) = world.get_resource::<crate::game::runtime::CampaignRuntime>() else {
        return true;
    };
    !runtime.session.is_campaign() || runtime.unlocked_content.contains(name)
}

/// `Team.isAI()` (`Team.java:112-114`) for the building's team.
fn team_is_ai(world: &World, team: u8) -> bool {
    let is_campaign = world
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .is_some_and(|runtime| runtime.session.is_campaign());
    let rules = crate::game::runtime::rules_of(world)
        .cloned()
        .unwrap_or_default();
    rules.team_is_ai(team, is_campaign)
}

/// `ReconstructorBuild` behavior (`Reconstructor.updateTile`/`acceptPayload`).
pub struct ReconstructorBehavior {
    /// Resolved upgrade pairs + construct time.
    pub reconstructor: Reconstructor,
    /// Upgrade result defs keyed by raw [`UnitTypeId`].
    pub units: BTreeMap<u16, Arc<UnitTypeDef>>,
}

impl ReconstructorBehavior {
    /// Builds a behavior from a block's resolved metadata.
    pub fn from_def(def: &crate::content::BlockDef, content: &ContentRegistry) -> Self {
        let mut reconstructor =
            Reconstructor::new(def.reconstructor_upgrades.clone(), def.construct_time);
        // `Reconstructor.initCapacities`: every `ConsumeItems` stack's `2x`
        // amount becomes the item's capacity.
        reconstructor.capacities = vec![0; content.items().len()];
        for consumer in &def.consumes {
            if let crate::content::Consume::Items(stacks) = &consumer.consume {
                for stack in stacks {
                    if let Some(slot) = reconstructor.capacities.get_mut(stack.item.index()) {
                        *slot = (*slot).max(stack.amount * 2);
                    }
                }
            }
        }
        let units = resolve_units(
            content,
            def.reconstructor_upgrades.iter().map(|(_, to)| *to),
        );
        Self {
            reconstructor,
            units,
        }
    }

    /// `getMaximumAccepted(item) = round(capacities[item] * rules.unitCost(team))`.
    fn maximum_accepted(&self, world: &World, e: Entity, item: ItemId) -> i32 {
        let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
        (self.reconstructor.capacity(item) as f32 * unit_cost(world, team)).round() as i32
    }
}

/// `Rules.unitBuildSpeed(team)` (1.0 outside a campaign).
fn unit_build_speed(world: &World, team: u8) -> f32 {
    world
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .map(|runtime| runtime.session.rules.unit_build_speed(team))
        .unwrap_or(1.0)
}

/// `Rules.unitCost(team)` (1.0 outside a campaign).
fn unit_cost(world: &World, team: u8) -> f32 {
    world
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .map(|runtime| runtime.session.rules.unit_cost(team))
        .unwrap_or(1.0)
}

/// `ConsumeItems.trigger` with the reconstructor's `unitCost` multiplier: removes
/// `round(stack.amount * unitCost(team))` of each configured item.
fn consume_items(world: &mut World, e: Entity, reconstructor: &Reconstructor) {
    let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
    let cost = unit_cost(world, team);
    let Some(mut items) = world.get_mut::<ItemModule>(e) else {
        return;
    };
    for index in 0..reconstructor.capacities.len() {
        let capacity = reconstructor.capacities[index];
        if capacity <= 0 {
            continue;
        }
        // `capacities = 2 * amount` -> `amount = capacity / 2`.
        let amount = ((capacity / 2) as f32 * cost).round() as i32;
        items.remove(ItemId::new(index as u16), amount);
    }
}

impl BuildingBehavior for ReconstructorBehavior {
    fn logic_sense(
        &self,
        world: &World,
        e: Entity,
        a: crate::logic::LAccess,
    ) -> Option<crate::logic::access::Sensed> {
        use crate::logic::LAccess;
        use crate::logic::access::Sensed;
        Some(match a {
            // `Reconstructor.sense(progress)` (`Reconstructor.java:370-373`):
            // the build fraction.
            LAccess::Progress => {
                // The reconstructor's state is `ReconstructorBuild`
                // (`create_state`), not the factory's `UnitFactoryBuild`.
                let state = world.get::<ReconstructorBuild>(e)?;
                let time = self.reconstructor.construct_time.max(1.0);
                Sensed::Num(f64::from((state.progress / time).clamp(0.0, 1.0)))
            }
            // `Reconstructor.senseObject(config)` = `unit()`
            // (`Reconstructor.java:320-323`, `:386-391`): the upgrade of the
            // held payload's type, or null. Java also nulls a target that is
            // not `unlockedNowHost()` for a non-AI team; the sim has no host
            // unlock state, so every upgrade counts as unlocked (the same
            // simplification as `ucontrol build`'s `unlockedNow`).
            LAccess::Config => Sensed::Obj(
                world
                    .get::<ReconstructorBuild>(e)
                    .and_then(|state| state.payload)
                    .and_then(|unit| self.reconstructor.can_upgrade(unit))
                    .map(|unit| {
                        crate::logic::value::LogicObject::Content(crate::content::ContentRef::new(
                            crate::content::ContentType::Unit,
                            unit.raw(),
                        ))
                    }),
            ),
            _ => return None,
        })
    }

    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(ReconstructorBuild::default());
    }

    /// `Reconstructor.shouldConsume` (`Reconstructor.java:376-379`):
    /// `constructing && enabled` (T1).
    fn should_consume(&self, world: &World, e: Entity) -> bool {
        world
            .get::<ReconstructorBuild>(e)
            .map(|state| state.constructing(&self.reconstructor))
            .unwrap_or(false)
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<ReconstructorBuild>() else {
            return;
        };
        // `progress += edelta() * unitBuildSpeed(team)` (0 while `efficiency` 0).
        let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
        let edelta = crate::world::update::edelta(world, e);
        let delta_progress = edelta * unit_build_speed(world, team);
        if let Some(target) = state.update(&self.reconstructor, delta_progress, edelta) {
            // `consume()` then `Events.fire(new UnitCreateEvent(...))`, with the
            // configured command applied to the upgraded unit
            // (`Reconstructor.java:346-352`).
            consume_items(world, e, &self.reconstructor);
            if let Some(def) = self.units.get(&target.raw()) {
                spawn_output_unit_with_command(
                    world,
                    e,
                    def,
                    state.command.map(UnitCommandId::new),
                );
                // `Reconstructor.java:354`: `createSound.at(this, 1 + range(0.06), 1)`.
                if let Some((x, y)) = crate::audio::sim::building_centre_of(world, e) {
                    crate::audio::sim::emit_world_sound(
                        world,
                        crate::audio::SoundId::UNIT_CREATE,
                        x,
                        y,
                        1.0,
                        1.0,
                        0.06,
                    );
                }
            }
        }
        world.entity_mut(e).insert(state);
    }

    fn accept_item(&self, world: &World, e: Entity, _src: Entity, item: ItemId) -> bool {
        // Java default: `block.consumesItem(item) && items.get(item) < getMaximumAccepted(item)`.
        if !self.reconstructor.accepts(item) {
            return false;
        }
        let held = world
            .get::<ItemModule>(e)
            .map(|items| items.get(item))
            .unwrap_or(0);
        held < self.maximum_accepted(world, e, item)
    }

    fn config_kinds(&self) -> &'static [ConfigKind] {
        // `Reconstructor.config(UnitCommand.class, ...)`: the panel lists the
        // unit commands (`Reconstructor.java:215-240`; T4).
        &[ConfigKind::Content]
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        world
            .get::<ReconstructorBuild>(e)
            .and_then(|state| state.command)
            .map(|command| ConfigValue::Content(crate::content::ContentType::UnitCommand, command))
            .unwrap_or(ConfigValue::None)
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        // `Reconstructor.config(UnitCommand.class, (tile, command) -> tile.command = command)`.
        match value {
            ConfigValue::Content(crate::content::ContentType::UnitCommand, id) => {
                if let Some(mut state) = world.get_mut::<ReconstructorBuild>(e) {
                    state.command = Some(id);
                }
            }
            ConfigValue::None => {
                if let Some(mut state) = world.get_mut::<ReconstructorBuild>(e) {
                    state.command = None;
                }
            }
            _ => {}
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        // `ReconstructorBuild.version() = 3`.
        3
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        // `ReconstructorBuild.write`: PayloadBlock body first
        // (`Reconstructor.java:413-436` extends `PayloadBlock.java:276-292`).
        crate::world::blocks::payloads::payload_block::write_base(world, e, w);
        // progress (f32), commandPos (nullable vec), command (TypeIO command).
        // The port has no commandPos.
        let (progress, command) = world
            .get::<ReconstructorBuild>(e)
            .map(|state| (state.progress, state.command))
            .unwrap_or((0.0, None));
        w.f(progress);
        crate::io::typeio::codecs::write_vec_nullable(w, None);
        crate::io::typeio::codecs::write_command(
            w,
            command.map(crate::content::UnitCommandId::new),
        );
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8) {
        if !crate::world::blocks::payloads::payload_block::read_base(world, e, r) {
            return;
        }
        if revision < 1 {
            return;
        }
        let progress = r.f().unwrap_or(0.0);
        if revision >= 2 {
            let _ = crate::io::typeio::codecs::read_vec_nullable(r);
        }
        let command = if revision >= 3 {
            crate::io::typeio::codecs::read_command(r)
                .ok()
                .flatten()
                .map(UnitCommandId::raw)
        } else {
            None
        };
        if let Some(mut state) = world.get_mut::<ReconstructorBuild>(e) {
            state.progress = progress;
            state.command = command;
        }
    }

    fn get_maximum_accepted(&self, world: &World, e: Entity, item: ItemId) -> i32 {
        self.maximum_accepted(world, e, item)
    }

    fn handle_item(&self, world: &mut World, e: Entity, _src: Entity, item: ItemId) {
        crate::world::blocks::distribution::transfer::default_handle_item(world, e, e, item);
    }

    fn accept_stack(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        amount: i32,
        _source: Option<Entity>,
    ) -> i32 {
        if !self.accept_item(world, e, Entity::PLACEHOLDER, item) {
            return 0;
        }
        let held = world
            .get::<ItemModule>(e)
            .map(|items| items.get(item))
            .unwrap_or(0);
        let free = (self.maximum_accepted(world, e, item) - held).max(0);
        free.min(amount)
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        let capacity = self.maximum_accepted(world, e, item);
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, amount, capacity.max(0));
        }
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        _source: Entity,
        payload: PayloadRef,
    ) -> bool {
        if payload.is_block {
            return false;
        }
        let unit = UnitTypeId::new(payload.content);
        let Some(target) = self.reconstructor.can_upgrade(unit) else {
            return false;
        };
        // `Reconstructor.acceptPayload` (`Reconstructor.java:247-262`): a
        // locked upgrade target is refused for a non-AI team and flashes the
        // research overlay (`Trigger.cannotUpgrade`); AI teams upgrade
        // regardless (`team.isAI() || upgrade.unlockedNowHost()`).
        let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
        let unlocked = self
            .units
            .get(&target.raw())
            .is_some_and(|def| unit_unlocked_now_host(world, &def.name));
        if !unlocked && !team_is_ai(world, team) {
            if let Some(fired) = world.get_resource::<CannotUpgradeFired>() {
                // `Events.fire(Trigger.cannotUpgrade)`.
                fired.0.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            return false;
        }
        let Some(state) = world.get::<ReconstructorBuild>(e) else {
            return false;
        };
        state.payload.is_none()
    }

    fn handle_payload(&self, world: &mut World, e: Entity, _source: Entity, payload: PayloadRef) {
        if payload.is_block {
            return;
        }
        if let Some(mut state) = world.get_mut::<ReconstructorBuild>(e) {
            let _ = state.accept_payload(&self.reconstructor, UnitTypeId::new(payload.content));
        }
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        let state = world.get::<ReconstructorBuild>(e)?;
        let unit = state.payload?;
        Some(PayloadRef {
            entity: None,
            content: unit.raw(),
            is_block: false,
        })
    }
}

// ---------------------------------------------------------------------------
// UnitAssembler / module
// ---------------------------------------------------------------------------

/// `UnitAssemblerBuild` behavior (tier/module rules + payload assembly).
pub struct AssemblerBehavior {
    /// Resolved assembler configuration.
    pub assembler: UnitAssembler,
    /// Produced unit defs keyed by raw [`UnitTypeId`].
    pub units: BTreeMap<u16, Arc<UnitTypeDef>>,
    /// Drone definition (`UnitAssembler.droneType` = `assembly-drone`).
    pub drone_def: Option<Arc<UnitTypeDef>>,
    /// Block size in tiles (spawn offset math).
    pub size: i32,
}

impl AssemblerBehavior {
    /// Builds a behavior from a block's resolved metadata
    /// (`UnitAssembler.plans`).
    pub fn from_def(def: &crate::content::BlockDef, content: &ContentRegistry) -> Self {
        let plans: Vec<super::AssemblerUnitPlan> = def
            .assembler_plans
            .iter()
            .map(|plan| super::AssemblerUnitPlan {
                unit: plan.unit,
                payloads: plan
                    .payloads
                    .iter()
                    .map(|payload| super::AssemblerPayload {
                        is_block: payload.item.type_ == crate::content::ContentType::Block,
                        content: payload.item.id,
                        amount: payload.amount,
                    })
                    .collect(),
                time: plan.time,
                drones: 4,
            })
            .collect();
        let units = resolve_units(content, def.assembler_plans.iter().map(|plan| plan.unit));
        // `UnitAssembler.droneType = UnitTypes.assemblyDrone`.
        let drone_def = content
            .unit_id("assembly-drone")
            .and_then(|id| content.unit(id))
            .map(|def| Arc::new(def.clone()));
        let mut assembler = UnitAssembler::new(plans, 240.0);
        // `areaSize = 13` on the three vanilla assemblers
        // (`Blocks.java:6566,6581,6596`); the class default 11
        // (`UnitAssembler.java:45`) stays for mod assemblers. It places the
        // spawn point (`getUnitSpawn`) and the drone ring (L06 T1).
        if matches!(
            def.name.as_str(),
            "tank-assembler" | "ship-assembler" | "mech-assembler"
        ) {
            assembler.area_size = 13;
        }
        Self {
            assembler,
            units,
            drone_def,
            size: def.size,
        }
    }

    /// `UnitAssemblerBuild.getUnitSpawn()`: the unit spawn point is
    /// `TILE_SIZE * (areaSize + size)/2` along the building rotation.
    fn unit_spawn(&self, world: &World, e: Entity) -> Option<(f32, f32)> {
        assembler_spawn_at(world, e, self.assembler.area_size, self.size)
    }
}

/// `UnitAssemblerBuild.getUnitSpawn()` (`UnitAssembler.java:336-340`):
/// `len = tilesize * (areaSize + size) / 2` along `Geometry.d4(rotation)`.
pub fn assembler_spawn_at(
    world: &World,
    e: Entity,
    area_size: i32,
    size: i32,
) -> Option<(f32, f32)> {
    let pos = world.get::<Pos>(e)?;
    let rotation = world
        .get::<crate::entities::comp::Building>(e)
        .map(|building| building.rotation)
        .unwrap_or(0);
    let len = crate::world::block::TILE_SIZE * (area_size + size) as f32 / 2.0;
    let (dx, dy) = crate::world::blocks::autotiler::d4(rotation);
    Some((pos.x + dx as f32 * len, pos.y + dy as f32 * len))
}

/// `UnitAssemblerBuild.checkSolid(v, same)` (`UnitAssembler.java:679-685`):
/// a ground `output` overlaps a solid tile (`EntityCollisions.overlapsTile`
/// with `EntityCollisions::solid`, `EntityCollisions.java:98-120`), or an
/// added unit's `hitboxTile` (`HitboxComp.java:69-76`) overlaps the
/// `hitSize * 1.4` square at `v` (`Units.anyEntities`, `Units.java:199-209`),
/// not counting `spawnedByCore` units, units of the output type when `same`,
/// and only units in the output's layer (leg-step pair, both flying, or a
/// grounded unit for a ground output).
///
/// Deviation: Java's tile radius reads an unrelated temp rect
/// (`EntityCollisions.r1`); the port scans `round(hitSize / 8) + 1`, which
/// always covers the rect (the overlap test itself is exact).
pub fn assembler_check_solid(
    world: &World,
    grid: &crate::world::WorldGrid,
    content: &ContentRegistry,
    output: &UnitTypeDef,
    spawn: (f32, f32),
    same: bool,
) -> bool {
    let ts = crate::world::block::TILE_SIZE;
    let overlaps = |ax: f32, ay: f32, aw: f32, ah: f32, bx: f32, by: f32, bw: f32, bh: f32| {
        ax < bx + bw && ax + aw > bx && ay < by + bh && ay + ah > by
    };
    if !output.flying {
        let half = output.hit_size / 2.0;
        let (rx, ry) = (spawn.0 - half, spawn.1 - half);
        let radius = ((output.hit_size / ts).round() as i32).max(1) + 1;
        let tx = crate::world::WorldGrid::to_tile(spawn.0);
        let ty = crate::world::WorldGrid::to_tile(spawn.1);
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                let (wx, wy) = (tx + dx, ty + dy);
                // `EntityCollisions.solid`: `tile == null || tile.solid()`.
                let solid = grid
                    .tiles
                    .getn(wx, wy)
                    .is_none_or(|tile| tile.solid_full(content));
                if solid
                    && overlaps(
                        wx as f32 * ts - ts / 2.0,
                        wy as f32 * ts - ts / 2.0,
                        ts,
                        ts,
                        rx,
                        ry,
                        output.hit_size,
                        output.hit_size,
                    )
                {
                    return true;
                }
            }
        }
    }
    let hsize = output.hit_size * 1.4;
    let (qx, qy) = (spawn.0 - hsize / 2.0, spawn.1 - hsize / 2.0);
    let Some(runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>() else {
        return false;
    };
    runtime.units.iter().any(|&unit| {
        use crate::entities::comp::unit::comp::{HitboxComp, UnitCore, UnitTypeComp};
        let Some(core) = world.get::<UnitCore>(unit) else {
            return false;
        };
        if core.dead || core.spawned_by_core {
            return false;
        }
        let Some(def) = world
            .get::<UnitTypeComp>(unit)
            .and_then(|comp| content.unit(comp.type_id))
        else {
            return false;
        };
        if same && def.id == output.id {
            return false;
        }
        let flying = crate::combat::bullet::unit_is_flying(world, unit);
        let grounded = core.elevation < 0.001;
        let layer = (def.allow_leg_step && output.allow_leg_step)
            || (output.flying && flying)
            || (!output.flying && grounded);
        if !layer {
            return false;
        }
        let Some(pos) = world.get::<Pos>(unit) else {
            return false;
        };
        let hit = world
            .get::<HitboxComp>(unit)
            .map_or(def.hit_size, |hitbox| hitbox.hit_size);
        let size = (hit * 0.66).min(7.8);
        overlaps(
            pos.x - size / 2.0,
            pos.y - size / 2.0,
            size,
            size,
            qx,
            qy,
            hsize,
            hsize,
        )
    })
}

/// `UnitAssemblerBuild.spawned()` product half (`UnitAssembler.java:556-574`):
/// the unit takes `commandPos` when commandable, then goes to the payload
/// block under the spawn point (`unit.buildOn()`, same team, `acceptPayload`
/// with the target itself as the source) or is added to the world
/// (`unit.add()`; `Units.notifyUnitSpawn`). `UnitCreateEvent` fires either
/// way. Returns the product and whether it was handed off.
#[allow(clippy::too_many_arguments)]
pub fn deliver_assembled_unit(
    world: &mut World,
    def: &UnitTypeDef,
    team: u8,
    x: f32,
    y: f32,
    rotation: f32,
    command_pos: Option<(f32, f32)>,
) -> (Entity, bool) {
    let seq = next_unit_seq(world);
    let product = spawn_unit_def(world, seq, def, team, x, y, rotation);
    // `if(unit.isCommandable() && commandPos != null)
    // unit.command().commandPosition(commandPos)`: `targetPos = pos`, no
    // attack target (`CommandAI.commandPosition`).
    if let Some((cx, cy)) = command_pos
        && let Some(mut slot) = world.get_mut::<crate::ai::ControllerSlot>(product)
        && slot.kind == crate::ai::AiKind::Command
    {
        slot.target = Some(crate::world::TilePos::new(
            crate::world::WorldGrid::to_tile(cx) as i16,
            crate::world::WorldGrid::to_tile(cy) as i16,
        ));
        world
            .entity_mut(product)
            .insert(crate::ai::types::command::AttackTargetComp(None));
    }
    // `var targetBuild = unit.buildOn()` = `world.buildWorld(x, y)`
    // (`World.toTile` rounding).
    let target = world
        .get_resource::<crate::world::TileBuilds>()
        .and_then(|builds| {
            builds.get(
                crate::world::WorldGrid::to_tile(x),
                crate::world::WorldGrid::to_tile(y),
            )
        });
    let payload = crate::world::blocks::payloads::payload_ref(
        crate::world::blocks::payloads::PayloadKind::Unit,
        product,
        def.id.raw(),
    );
    let handed =
        target.is_some_and(|target| {
            world
                .get::<TeamComp>(target)
                .is_some_and(|t| t.team == team)
                && {
                    world.entity_mut(product).insert(
                        crate::world::blocks::payloads::UnitPayloadSize(def.hit_size),
                    );
                    crate::world::blocks::payloads::payload_conveyor::dispatch_accept_payload(
                        world, target, target, payload,
                    )
                }
        });
    if let (true, Some(target)) = (handed, target) {
        crate::world::blocks::payloads::payload_conveyor::dispatch_handle_payload(
            world, target, target, payload,
        );
        // `UnitCreateEvent` (`UnitAssembler.java:574`) without `unit.add()`.
        crate::game::stats::notify_unit_created(world, product, Some(&def.name));
    } else {
        world
            .entity_mut(product)
            .remove::<crate::world::blocks::payloads::UnitPayloadSize>();
        adopt_output_unit(world, product, &def.name);
    }
    (product, handed)
}

/// Sets one drone's `AssemblerAI` target (`UnitAssembler.java:509-513`:
/// `ai.targetPos.trns(i * 90f + 45f, len).add(spawn); ai.targetAngle =
/// i * 90f + 45f + 180f`).
fn arrange_drone(world: &mut World, unit: Entity, target_x: f32, target_y: f32, target_angle: f32) {
    use crate::ai::types::assembler::AssemblerAi;
    if let Some(mut ai) = world.get_mut::<AssemblerAi>(unit) {
        ai.unit = Some(unit);
        ai.target_x = target_x;
        ai.target_y = target_y;
        ai.target_angle = target_angle;
    } else if world.get_entity(unit).is_ok() {
        world.entity_mut(unit).insert(AssemblerAi {
            unit: Some(unit),
            target_x,
            target_y,
            target_angle,
        });
    }
}

impl BuildingBehavior for AssemblerBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.assembler.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut assembler) = world.entity_mut(e).take::<UnitAssembler>() else {
            return;
        };
        // `UnitAssemblerBuild.updateTile` (`UnitAssembler.java:443-532`).
        // 1. Resolve the drone ids read from the save body (`:444-456`).
        if !assembler.read_unit_ids.is_empty() {
            let ids = std::mem::take(&mut assembler.read_unit_ids);
            assembler.units.clear();
            for id in ids {
                if let Some(unit) = crate::sim::unit_runtime::unit_by_id(world, id) {
                    assembler.units.push(unit);
                }
            }
        }
        // 2. A tier change resets progress (`:458-466`).
        let current_tier = assembler.module_tier.max(0);
        if assembler.last_tier != current_tier {
            if assembler.last_tier >= 0 {
                assembler.progress = 0.0;
            }
            assembler.last_tier = if assembler.last_tier == -2 {
                -1
            } else {
                current_tier
            };
        }
        // 3. Drop dead or re-controlled drones (`:468-472`).
        assembler.units.retain(|unit| {
            world
                .get::<crate::entities::comp::unit::comp::UnitCore>(*unit)
                .is_some_and(|core| !core.dead)
                && world
                    .get::<crate::ai::controller::ControllerSlot>(*unit)
                    .is_some_and(|slot| slot.kind == crate::ai::AiKind::Assembler)
        });
        let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
        let enabled = world
            .get::<crate::entities::comp::Building>(e)
            .is_some_and(|building| building.enabled);
        // 4. Drone construction (`:480-500`): `powerStatus`, then
        //    `droneProgress += delta() * unitBuildSpeed * powerStatus /
        //    droneConstructTime`; one drone per completed unit.
        let power_status = if !enabled {
            0.0
        } else {
            world
                .get::<crate::world::modules::PowerModule>(e)
                .map(|power| power.status)
                .unwrap_or(1.0)
        };
        let delta = crate::world::update::delta(world, e);
        let speed = unit_build_speed(world, team);
        if (assembler.units.len() as i32) < assembler.drones_created && enabled {
            assembler.drone_progress +=
                delta * speed * power_status / assembler.drone_construct_time.max(0.0001);
            if assembler.drone_progress >= 1.0 {
                assembler.drone_progress = 0.0;
                if let Some(def) = self.drone_def.clone()
                    && let Some(pos) = world.get::<Pos>(e).copied()
                {
                    let seq = next_unit_seq(world);
                    let drone = spawn_unit_def(world, seq, &def, team, pos.x, pos.y, 90.0);
                    // `UnitCargoLoaderBuild`-style tether adoption
                    // (`UnitAssembler.java:489-494`: `bt.building(this)`).
                    world.entity_mut(drone).insert(
                        crate::entities::comp::unit::comp::BuildingTetherComp { building: Some(e) },
                    );
                    adopt_output_unit(world, drone, &def.name);
                    assembler.units.push(drone);
                }
            }
        }
        if (assembler.units.len() as i32) >= assembler.drones_created {
            assembler.drone_progress = 0.0;
        }
        // 5. Arrange the drones around the spawn point (`:509-513`).
        let spawn = self.unit_spawn(world, e);
        if let Some((sx, sy)) = spawn {
            let radius = (assembler.area_size as f32 / 2.0)
                * std::f32::consts::SQRT_2
                * crate::world::block::TILE_SIZE;
            for (index, unit) in assembler.units.iter().enumerate() {
                let angle = index as f32 * 90.0 + 45.0;
                let radians = angle.to_radians();
                arrange_drone(
                    world,
                    *unit,
                    sx + radians.cos() * radius,
                    sy + radians.sin() * radius,
                    angle + 180.0,
                );
            }
        }
        // 6. Efficiency = in-position drones / dronesCreated (`:516-517`).
        let positioned = assembler
            .units
            .iter()
            .filter(|unit| {
                world
                    .get::<crate::ai::types::assembler::AssemblerAi>(**unit)
                    .is_some_and(|ai| {
                        crate::ai::types::assembler::in_position_world(world, **unit, ai)
                    })
            })
            .count() as f32;
        let eff = positioned / assembler.drones_created.max(1) as f32;
        // 7. Product progress (`:520-532`); the port keeps the requirement
        //    gate inside `UnitAssembler::update`.
        let efficiency = world
            .get::<crate::entities::comp::Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        if efficiency > 0.0
            && let Some(unit) = assembler.update(delta * speed, eff)
            && let Some(def) = self.units.get(&unit.raw())
            && let Some((x, y)) = spawn
        {
            // `spawned()`: the product spawns at `getUnitSpawn()` with the
            // building's rotation (`UnitAssembler.java:558-561`).
            let rotation = world
                .get::<crate::entities::comp::Building>(e)
                .map(|building| building.rotation as f32 * 90.0)
                .unwrap_or(0.0);
            // `spawned()` (`UnitAssembler.java:556-574`): commandPos, payload
            // hand-off or `unit.add()` (L06 T1).
            let _ = deliver_assembled_unit(world, def, team, x, y, rotation, assembler.command_pos);
            // `UnitAssembler.java:568`: `createSound.at(spawn.x, spawn.y, 1 + range(0.06), 1)`.
            crate::audio::sim::emit_world_sound(
                world,
                crate::audio::SoundId::UNIT_CREATE_BIG,
                x,
                y,
                1.0,
                1.0,
                0.06,
            );
        }
        world.entity_mut(e).insert(assembler);
    }

    /// `UnitAssemblerBuild.version` returns 1 (the save body gained
    /// `commandPos`; `UnitAssembler.java:751-753`).
    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    /// `UnitAssemblerBuild.write` (`UnitAssembler.java:756-769`): `progress`,
    /// the drone id list, the `blocks` `PayloadSeq`, then `commandPos`.
    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        let Some(assembler) = world.get::<UnitAssembler>(e) else {
            return;
        };
        w.f(assembler.progress);
        let count = assembler.units.len().min(i8::MAX as usize) as i8;
        w.b(count);
        for unit in assembler.units.iter().take(count as usize) {
            let id = world
                .get::<crate::ecs::EntitySeq>(*unit)
                .map(|seq| seq.0 as i32)
                .unwrap_or(-1);
            w.i(id);
        }
        assembler.stored.write(w);
        crate::io::typeio::codecs::write_vec_nullable(w, assembler.command_pos);
    }

    /// `UnitAssemblerBuild.read` (`UnitAssembler.java:772-789`): the drone ids
    /// land in `readUnits` and are resolved by `updateTile`.
    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8) {
        let progress = r.f().unwrap_or(0.0);
        let count = r.b().unwrap_or(0);
        let mut read_unit_ids = Vec::new();
        for _ in 0..count.max(0) {
            read_unit_ids.push(r.i().unwrap_or(-1));
        }
        let mut stored = crate::content::PayloadSeq::default();
        let _ = stored.read(r);
        let command_pos = if revision >= 1 {
            crate::io::typeio::codecs::read_vec_nullable(r).unwrap_or(None)
        } else {
            None
        };
        if let Some(mut assembler) = world.get_mut::<UnitAssembler>(e) {
            assembler.progress = progress;
            assembler.read_unit_ids = read_unit_ids;
            assembler.stored = stored;
            assembler.command_pos = command_pos;
        }
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        _source: Entity,
        payload: PayloadRef,
    ) -> bool {
        let Some(assembler) = world.get::<UnitAssembler>(e) else {
            return false;
        };
        assembler.accepts_payload(AssemblerPayload {
            is_block: payload.is_block,
            content: payload.content,
            amount: 1,
        })
    }

    fn handle_payload(&self, world: &mut World, e: Entity, _source: Entity, payload: PayloadRef) {
        let Some(mut assembler) = world.get_mut::<UnitAssembler>(e) else {
            return;
        };
        assembler.store_payload(AssemblerPayload {
            is_block: payload.is_block,
            content: payload.content,
            amount: 1,
        });
    }

    fn get_payload(&self, _world: &World, _e: Entity) -> Option<PayloadRef> {
        None
    }
}

/// `UnitAssemblerModuleBuild` behavior (registers its tier with the assembler).
pub struct AssemblerModuleBehavior;

impl AssemblerModuleBehavior {
    /// `basic-assembler-module` tier (`UnitAssemblerModule.tier` default 1).
    pub const TIER: i32 = 1;
}

impl BuildingBehavior for AssemblerModuleBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<super::UnitAssemblerModule>(e).is_none() {
            world.entity_mut(e).insert(super::UnitAssemblerModule {
                tier: Self::TIER,
                assembler: None,
            });
        }
    }

    /// `UnitAssemblerModuleBuild.findLink`/`updateModules`: an adjacent
    /// assembler takes the module's tier (the module is always built against
    /// the assembler's build area).
    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        let tier = world
            .get::<super::UnitAssemblerModule>(e)
            .map(|module| module.tier)
            .unwrap_or(Self::TIER);
        let neighbors: Vec<Entity> = world
            .get::<crate::entities::comp::Building>(e)
            .map(|building| building.proximity.iter().copied().collect())
            .unwrap_or_default();
        for other in neighbors {
            if world.get::<UnitAssembler>(other).is_none() {
                continue;
            }
            if let Some(mut assembler) = world.get_mut::<UnitAssembler>(other) {
                assembler.module_tier = assembler.module_tier.max(tier);
            }
            let packed = world
                .get::<crate::entities::comp::Building>(other)
                .map(|building| building.tile.pack());
            if let Some(mut module) = world.get_mut::<super::UnitAssemblerModule>(e) {
                module.assembler = packed;
            }
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// RepairTower / RepairTurret
// ---------------------------------------------------------------------------

/// `RepairTowerBuild` behavior (6-tick target refresh; aura heal in `shields`).
pub struct RepairTowerBehavior {
    /// Resolved tower configuration.
    pub tower: RepairTower,
}

impl BuildingBehavior for RepairTowerBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.tower.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut tower) = world.entity_mut(e).take::<RepairTower>() else {
            return;
        };
        crate::world::blocks::units::update_repair_tower(world, e, &mut tower);
        world.entity_mut(e).insert(tower);
    }

    /// `RepairTowerBuild.shouldConsume`: `targets.size > 0`
    /// (RepairTower.java:88-91).
    fn should_consume(&self, world: &World, e: Entity) -> bool {
        world
            .get::<RepairTower>(e)
            .is_some_and(|tower| !tower.targets.is_empty())
    }
}

/// `RepairPointBuild` behavior (60-tick target reacquisition interval).
pub struct RepairTurretBehavior {
    /// Resolved turret configuration.
    pub turret: RepairTurret,
}

impl BuildingBehavior for RepairTurretBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.turret.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut turret) = world.entity_mut(e).take::<RepairTurret>() else {
            return;
        };
        crate::world::blocks::units::update_repair_turret(world, e, &mut turret);
        world.entity_mut(e).insert(turret);
    }

    /// `RepairTurret.shouldConsume`: `target != null && enabled`.
    fn should_consume(&self, world: &World, e: Entity) -> bool {
        world
            .get::<RepairTurret>(e)
            .is_some_and(|turret| turret.target.is_some())
    }

    /// `RepairPointBuild.version() = 1` (`RepairTurret.java:252`).
    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    /// `RepairPointBuild.write`: `super.write(write); write.f(rotation)`
    /// (`RepairTurret.java:243-248`; shared by repair-point and repair-turret).
    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        w.f(world
            .get::<RepairTurret>(e)
            .map(|turret| turret.rotation)
            .unwrap_or(0.0));
    }

    /// `RepairPointBuild.read`: `if(revision >= 1) rotation = read.f();`
    /// (`RepairTurret.java:250-254`).
    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8) {
        if revision < 1 {
            return;
        }
        let rotation = r.f().unwrap_or(0.0);
        if let Some(mut turret) = world.get_mut::<RepairTurret>(e) {
            turret.rotation = rotation;
        }
    }
}

// ---------------------------------------------------------------------------
// UnitCargoLoader / UnitCargoUnloadPoint
// ---------------------------------------------------------------------------

/// `UnitCargoLoaderBuild` behavior (tether unit spawn; `CargoAI` is plan 11 M2).
pub struct CargoLoaderBehavior {
    /// Resolved loader configuration.
    pub loader: UnitCargoLoader,
    /// Spawned `manifold` unit def keyed by raw [`UnitTypeId`].
    pub units: BTreeMap<u16, Arc<UnitTypeDef>>,
}

impl BuildingBehavior for CargoLoaderBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.loader.clone());
        if world.get::<super::CargoLoaderState>(e).is_none() {
            world
                .entity_mut(e)
                .insert(super::CargoLoaderState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<super::CargoLoaderState>() else {
            return;
        };
        // `UnitCargoLoaderBuild.updateTile` (`UnitCargoLoader.java:98-103`):
        // a unit id read from the save body re-tethers the unit on the next
        // update. Single-player/server always clears the pending id
        // (`unit != null || !net.client()`); the net-client keep-branch is
        // I08's relay concern.
        if state.read_unit_id != -1 {
            state.unit = crate::sim::unit_runtime::unit_by_id(world, state.read_unit_id);
            state.read_unit_id = -1;
        }
        let unit_alive = state.unit.is_some_and(|unit| {
            world
                .get::<crate::entities::comp::unit::UnitCore>(unit)
                .is_some_and(|core| !core.dead)
        });
        let delta = crate::world::update::edelta(world, e);
        if let Some(unit) = state.update(
            delta,
            self.loader.unit_build_time,
            self.loader.unit_type,
            unit_alive,
        ) && let Some(def) = self.units.get(&unit.raw())
            && let Some(pos) = world.get::<Pos>(e).copied()
        {
            let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
            let seq = next_unit_seq(world);
            let manifold = spawn_unit_def(world, seq, def, team, pos.x, pos.y, 90.0);
            // `UnitCargoLoaderBuild.updateTile`: `bt.building(this)` plus the
            // live-runtime adoption so the dispatched `CargoAI` updates it
            // (H01 T13). `Call.unitTetherBlockSpawned` also resolves the
            // spawner id on clients; both point at the loader here.
            world.entity_mut(manifold).insert(
                crate::entities::comp::unit::comp::BuildingTetherComp { building: Some(e) },
            );
            world
                .entity_mut(manifold)
                .insert(crate::entities::comp::unit::comp::UnitTetherComp { spawner: Some(e) });
            adopt_output_unit(world, manifold, &def.name);
            state.unit = Some(manifold);
        }
        world.entity_mut(e).insert(state);
    }

    /// `UnitCargoLoaderBuild.write` (`UnitCargoLoader.java:174-178`):
    /// `write.i(unit == null ? -1 : unit.id)` after `super.write`. The port's
    /// unit id is the [`crate::ecs::EntitySeq`] written by the unit chunk
    /// (`SaveVersion.writeEntity`), so the body is the same `i32`.
    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        let id = world
            .get::<super::CargoLoaderState>(e)
            .and_then(|state| state.unit)
            .and_then(|unit| world.get::<crate::ecs::EntitySeq>(unit))
            .map(|seq| seq.0 as i32)
            .unwrap_or(-1);
        w.i(id);
    }

    /// `UnitCargoLoaderBuild.read` (`UnitCargoLoader.java:181-185`):
    /// `readUnitId = read.i()`; `updateTile` resolves it via `unit_by_id`.
    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, _revision: u8) {
        let id = r.i().unwrap_or(-1);
        if let Some(mut state) = world.get_mut::<super::CargoLoaderState>(e) {
            state.read_unit_id = id;
        }
    }
}

/// `UnitCargoUnloadPointBuild` behavior (`updateTile` stale timer).
pub struct CargoUnloadPointBehavior {
    /// Resolved unload-point configuration.
    pub point: UnitCargoUnloadPoint,
}

impl BuildingBehavior for CargoUnloadPointBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.point.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let total = world
            .get::<ItemModule>(e)
            .map(|items| items.total)
            .unwrap_or(0);
        let capacity = transfer::item_capacity(world, e);
        let delta = crate::world::update::delta(world, e);
        if let Some(mut point) = world.get_mut::<UnitCargoUnloadPoint>(e) {
            let _ = point.update_stale(total, capacity, delta);
        }
    }

    /// `UnitCargoUnloadPointBuild.write` (`UnitCargoUnloadPoint.java:90-96`):
    /// `s item == null ? -1 : item.id`, then `bool stale`.
    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        let Some(point) = world.get::<UnitCargoUnloadPoint>(e) else {
            return;
        };
        w.s(point.item.map(|item| item.raw() as i16).unwrap_or(-1));
        w.bool(point.stale);
    }

    /// `UnitCargoUnloadPointBuild.read` (`UnitCargoUnloadPoint.java:99-103`):
    /// `item = content.item(read.s())` (`-1` = null), `stale = read.bool()`.
    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, _revision: u8) {
        let item = r.s().unwrap_or(-1);
        let stale = r.bool().unwrap_or(false);
        if let Some(mut point) = world.get_mut::<UnitCargoUnloadPoint>(e) {
            point.item = (item >= 0).then(|| ItemId::new(item as u16));
            point.stale = stale;
        }
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Registers every vanilla unit-block behavior into plan 07's registry.
///
/// Runs inside [`crate::world::blocks::default_registry`] so a placed unit block
/// updates through `world::update::update_buildings` with no harness override.
pub fn register(
    registry: &mut crate::world::behavior::BehaviorRegistry,
    content: &ContentRegistry,
) {
    for def in content.blocks() {
        let behavior: Option<Arc<dyn BuildingBehavior>> = match def.kind {
            BlockKind::UnitFactory => Some(Arc::new(UnitFactoryBehavior::from_def(def, content))),
            BlockKind::Reconstructor => {
                Some(Arc::new(ReconstructorBehavior::from_def(def, content)))
            }
            BlockKind::UnitAssembler => Some(Arc::new(AssemblerBehavior::from_def(def, content))),
            BlockKind::UnitAssemblerModule => Some(Arc::new(AssemblerModuleBehavior)),
            // `Blocks.java:6612-6621`: unit-repair-tower range 100, healAmount
            // 1.5 (class defaults 80 / 1, RepairTower.java:21,24).
            BlockKind::RepairTower => Some(Arc::new(RepairTowerBehavior {
                tower: match def.name.as_str() {
                    "unit-repair-tower" => RepairTower {
                        range: 100.0,
                        heal_amount: 1.5,
                        ..RepairTower::default()
                    },
                    _ => RepairTower::default(),
                },
            })),
            // `Blocks.java:6420-6443`: repair-point repairSpeed 0.45 /
            // repairRadius 60 (no coolant); repair-turret size 2, repairSpeed 3,
            // repairRadius 145, acceptCoolant with coolantMultiplier 1.6 (T12).
            BlockKind::RepairTurret => {
                let (repair_speed, repair_radius, accept_coolant, coolant_multiplier) =
                    match def.name.as_str() {
                        "repair-turret" => (3.0, 145.0, true, 1.6),
                        _ => (0.45, 60.0, false, 0.0),
                    };
                Some(Arc::new(RepairTurretBehavior {
                    turret: RepairTurret {
                        repair_speed,
                        repair_radius,
                        accept_coolant,
                        coolant_multiplier,
                        ..RepairTurret::default()
                    },
                }))
            }
            BlockKind::UnitCargoLoader => {
                // `Blocks.java unit-cargo-loader` spawns `UnitTypes.manifold`.
                let unit_type = content.unit_id("manifold").unwrap_or(UnitTypeId::new(0));
                Some(Arc::new(CargoLoaderBehavior {
                    loader: UnitCargoLoader {
                        unit_type,
                        unit_build_time: 480.0,
                    },
                    units: resolve_units(content, [unit_type]),
                }))
            }
            BlockKind::UnitCargoUnloadPoint => Some(Arc::new(CargoUnloadPointBehavior {
                point: UnitCargoUnloadPoint::default(),
            })),
            _ => None,
        };
        if let Some(behavior) = behavior {
            registry.register(def.id, behavior);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore};
    use crate::entities::comp::Unit;
    use crate::world::block::BlockTable;
    use crate::world::blocks::default_registry;
    use crate::world::limits::BuildRules;
    use crate::world::update::update_buildings;
    use bevy_ecs::world::World;

    fn content() -> ContentRegistry {
        let mut registry = crate::content::create_base_content(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        registry
    }

    #[test]
    fn default_registry_installs_unit_block_behaviors() {
        let content = content();
        let registry = default_registry(&content);
        for kind in [
            BlockKind::UnitFactory,
            BlockKind::Reconstructor,
            BlockKind::UnitAssembler,
            BlockKind::UnitAssemblerModule,
            BlockKind::RepairTower,
            BlockKind::RepairTurret,
            BlockKind::UnitCargoLoader,
            BlockKind::UnitCargoUnloadPoint,
        ] {
            let def = content
                .blocks()
                .iter()
                .find(|def| def.kind == kind)
                .unwrap_or_else(|| panic!("vanilla block for {kind:?}"));
            assert!(
                registry.get(def.id).is_some(),
                "override missing for {}",
                def.name
            );
        }
    }

    /// The arrangement target/angle for drone `index` (`UnitAssembler.java:509-513`).
    fn arrangement_target(world: &World, assembler: Entity, index: usize) -> (f32, f32, f32) {
        let pos = world
            .get::<crate::entities::comp::Pos>(assembler)
            .copied()
            .expect("assembler pos");
        let rotation = world
            .get::<crate::entities::comp::Building>(assembler)
            .map(|building| building.rotation)
            .unwrap_or(0);
        let (dx, dy) = crate::world::blocks::autotiler::d4(rotation);
        // The vanilla assemblers use `areaSize = 13` (`Blocks.java:6566,6581,6596`).
        let area = world
            .get::<UnitAssembler>(assembler)
            .map_or(11, |state| state.area_size);
        let len = crate::world::block::TILE_SIZE * (area + 5) as f32 / 2.0;
        let (sx, sy) = (pos.x + dx as f32 * len, pos.y + dy as f32 * len);
        let radius = area as f32 / 2.0 * std::f32::consts::SQRT_2 * crate::world::block::TILE_SIZE;
        let angle = index as f32 * 90.0 + 45.0;
        let radians = angle.to_radians();
        (
            sx + radians.cos() * radius,
            sy + radians.sin() * radius,
            angle + 180.0,
        )
    }

    /// Moves one drone onto its arranged spot and faces it the locked angle
    /// (a bare world has no unit-update pass to fly it there).
    fn place_drone_in_position(world: &mut World, assembler: Entity, drone: Entity, index: usize) {
        let (tx, ty, angle) = arrangement_target(world, assembler, index);
        if let Some(mut pos) = world.get_mut::<crate::entities::comp::Pos>(drone) {
            pos.x = tx;
            pos.y = ty;
        }
        if let Some(mut core) = world.get_mut::<crate::entities::comp::unit::comp::UnitCore>(drone)
        {
            core.rotation = angle;
        }
    }

    /// Runs `update_buildings` until the assembler holds four drones, forcing
    /// the power status like the existing assembler test does.
    fn build_assembler_drones(world: &mut World, entity: Entity) -> Vec<Entity> {
        use crate::world::modules::PowerModule;
        for _ in 0..4000 {
            if let Some(mut power) = world.get_mut::<PowerModule>(entity) {
                power.status = 1.0;
            }
            update_buildings(world);
            let state = world.get::<UnitAssembler>(entity).expect("assembler state");
            if state.units.len() >= 4 {
                return state.units.clone();
            }
        }
        panic!("assembler did not construct four drones");
    }

    /// gap5 GAP-4 / K-4 (I01 T11): an assembler reads `def.assembler_plans`,
    /// accepts the planned payloads, builds four `assembly-drone`s
    /// (`UnitAssembler.java:480-500`) and spawns the tier-0 unit once the
    /// drones hold their arranged spots (the `inPosition` efficiency term,
    /// `:516-517`).
    #[test]
    fn assembler_reads_plans_and_spawns_tier_unit() {
        use crate::entities::comp::unit::UnitTypeComp;
        use crate::world::PayloadRef;
        use crate::world::modules::{LiquidModule, PowerModule};

        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("tank-assembler")
            .expect("tank-assembler")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(6, 6),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        // Tier-0 plan is `vanquish`: stell x4 + tungsten-wall-large x10.
        let stell = content.unit_id("stell").expect("stell");
        let wall = content.block_id("tungsten-wall-large").expect("wall");
        for _ in 0..4 {
            inst.behavior.handle_payload(
                &mut world,
                entity,
                entity,
                PayloadRef {
                    entity: None,
                    content: stell.raw(),
                    is_block: false,
                },
            );
        }
        for _ in 0..10 {
            inst.behavior.handle_payload(
                &mut world,
                entity,
                entity,
                PayloadRef {
                    entity: None,
                    content: wall.raw(),
                    is_block: true,
                },
            );
        }
        // Keep power + cyanogen efficiency at 1 while building.
        if let Some(mut power) = world.get_mut::<PowerModule>(entity) {
            power.status = 1.0;
        }
        if let Some(cyanogen) = content.liquid_id("cyanogen")
            && let Some(mut liquids) = world.get_mut::<LiquidModule>(entity)
        {
            liquids.add(cyanogen, 1000.0, 1000.0);
        }
        // Phase 1: the four drones (`UnitAssembler.java:480-500`).
        let drones = build_assembler_drones(&mut world, entity);
        // Phase 2: place them on their spots; the product then makes progress.
        for (index, drone) in drones.iter().enumerate() {
            place_drone_in_position(&mut world, entity, *drone, index);
        }
        let vanquish = content.unit_id("vanquish").expect("vanquish");
        let mut spawned = false;
        for _ in 0..3200 {
            if let Some(mut power) = world.get_mut::<PowerModule>(entity) {
                power.status = 1.0;
            }
            update_buildings(&mut world);
            if world.iter_entities().any(|entity_ref| {
                entity_ref.get::<UnitTypeComp>().map(|comp| comp.type_id) == Some(vanquish)
            }) {
                spawned = true;
                break;
            }
        }
        assert!(spawned, "tank-assembler produced vanquish");
    }

    /// I01 T11: every drone gets its perimeter spot and locked angle
    /// (`UnitAssembler.java:509-513`).
    #[test]
    fn assembler_drones_take_positions() {
        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("mech-assembler")
            .expect("mech-assembler")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(6, 6),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        let drones = build_assembler_drones(&mut world, entity);
        for (index, drone) in drones.iter().enumerate() {
            let (expected_x, expected_y, expected_angle) =
                arrangement_target(&world, entity, index);
            let ai = world
                .get::<crate::ai::types::assembler::AssemblerAi>(*drone)
                .expect("drone AssemblerAI");
            assert!(
                (ai.target_x - expected_x).abs() < 1e-3 && (ai.target_y - expected_y).abs() < 1e-3,
                "drone {index} target ({}, {}) vs ({expected_x}, {expected_y})",
                ai.target_x,
                ai.target_y
            );
            assert_eq!(ai.target_angle, expected_angle, "drone {index} angle");
        }
    }

    /// I01 T11: the product's progress waits for the drones — with every drone
    /// off-position `eff = 0` (`UnitAssembler.java:516-517`), and advances once
    /// all four hold their spots.
    #[test]
    fn assembler_waits_for_drones_in_position() {
        use crate::world::PayloadRef;
        use crate::world::modules::PowerModule;

        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("tank-assembler")
            .expect("tank-assembler")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(6, 6),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        // Tier-0 requirements fully staged.
        let stell = content.unit_id("stell").expect("stell");
        let wall = content.block_id("tungsten-wall-large").expect("wall");
        for _ in 0..4 {
            inst.behavior.handle_payload(
                &mut world,
                entity,
                entity,
                PayloadRef {
                    entity: None,
                    content: stell.raw(),
                    is_block: false,
                },
            );
        }
        for _ in 0..10 {
            inst.behavior.handle_payload(
                &mut world,
                entity,
                entity,
                PayloadRef {
                    entity: None,
                    content: wall.raw(),
                    is_block: true,
                },
            );
        }
        let drones = build_assembler_drones(&mut world, entity);
        // Keep power + cyanogen efficiency at 1 like the spawn test.
        if let Some(mut power) = world.get_mut::<PowerModule>(entity) {
            power.status = 1.0;
        }
        if let Some(cyanogen) = content.liquid_id("cyanogen")
            && let Some(mut liquids) = world.get_mut::<crate::world::modules::LiquidModule>(entity)
        {
            liquids.add(cyanogen, 1000.0, 1000.0);
        }
        // The drones spawned at the building centre and never moved: no
        // efficiency, so no product progress even with requirements met.
        for _ in 0..600 {
            if let Some(mut power) = world.get_mut::<PowerModule>(entity) {
                power.status = 1.0;
            }
            update_buildings(&mut world);
        }
        let progress = world
            .get::<UnitAssembler>(entity)
            .expect("assembler")
            .progress;
        assert_eq!(progress, 0.0, "no progress while no drone is in position");
        // In position: progress starts advancing.
        for (index, drone) in drones.iter().enumerate() {
            place_drone_in_position(&mut world, entity, *drone, index);
        }
        if let Some(mut power) = world.get_mut::<PowerModule>(entity) {
            power.status = 1.0;
        }
        update_buildings(&mut world);
        let progress = world
            .get::<UnitAssembler>(entity)
            .expect("assembler")
            .progress;
        assert!(
            progress > 0.0,
            "progress advances with all drones in position ({progress})"
        );
    }

    /// PAR-0076: the additive reconstructor accepts its configured
    /// `ConsumeItems` stacks up to `2x` the requirement (`getMaximumAccepted`)
    /// and rejects every other item.
    #[test]
    fn additive_reconstructor_accepts_only_its_items() {
        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("additive-reconstructor")
            .expect("additive-reconstructor")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        let silicon = content.item_id("silicon").expect("silicon");
        let graphite = content.item_id("graphite").expect("graphite");
        let copper = content.item_id("copper").expect("copper");

        assert!(inst.behavior.accept_item(&world, entity, entity, silicon));
        assert!(inst.behavior.accept_item(&world, entity, entity, graphite));
        assert!(
            !inst.behavior.accept_item(&world, entity, entity, copper),
            "rejects copper"
        );
        assert_eq!(
            inst.behavior.get_maximum_accepted(&world, entity, silicon),
            80,
            "2x the silicon requirement"
        );

        // Filled to capacity: no more accepted; other stacks still are.
        if let Some(mut items) = world.get_mut::<ItemModule>(entity) {
            items.add(silicon, 80, 80);
        }
        assert!(!inst.behavior.accept_item(&world, entity, entity, silicon));
        assert_eq!(
            inst.behavior.accept_stack(&world, entity, silicon, 5, None),
            0
        );
        assert_eq!(
            inst.behavior
                .accept_stack(&world, entity, graphite, 7, None),
            7
        );
    }

    /// gap5 GAP-4 / K-4: `basic-assembler-module` registers its tier with the
    /// adjacent assembler (`UnitAssemblerModuleBuild.findLink`/`checkTier`).
    #[test]
    fn assembler_module_registers_tier() {
        use crate::world::BuildHarness;

        let mut harness = BuildHarness::new(20, 20, 7);
        let assembler = harness
            .content()
            .block_id("tank-assembler")
            .expect("tank-assembler");
        let module = harness
            .content()
            .block_id("basic-assembler-module")
            .expect("basic-assembler-module");
        assert!(harness.place(7, 7, assembler, 0, true));
        assert!(harness.place(12, 7, module, 0, true));
        let entity = harness.build_at(7, 7).expect("assembler");
        let tier = harness
            .world
            .get::<UnitAssembler>(entity)
            .map(|assembler| assembler.module_tier)
            .unwrap_or(0);
        assert_eq!(tier, 1, "module raised the assembler tier");
    }

    /// gap5 GAP-12 / K-8: `unit-cargo-loader` is configured with `manifold` and
    /// spawns it through the normal behavior tick.
    #[test]
    fn cargo_loader_spawns_manifold() {
        use crate::entities::comp::unit::UnitTypeComp;

        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("unit-cargo-loader")
            .expect("unit-cargo-loader")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        let manifold = content.unit_id("manifold").expect("manifold");
        assert_eq!(
            world
                .get::<UnitCargoLoader>(entity)
                .map(|loader| loader.unit_type),
            Some(manifold),
            "loader unit type is manifold"
        );
        // `unit-cargo-loader` consumes power (0.1333) and nitrogen (0.1667).
        if let Some(mut power) = world.get_mut::<crate::world::modules::PowerModule>(entity) {
            power.status = 1.0;
        }
        if let Some(nitrogen) = content.liquid_id("nitrogen")
            && let Some(mut liquids) = world.get_mut::<crate::world::modules::LiquidModule>(entity)
        {
            liquids.add(nitrogen, 1000.0, 1000.0);
        }
        let mut spawned = false;
        for _ in 0..600 {
            if let Some(mut power) = world.get_mut::<crate::world::modules::PowerModule>(entity) {
                power.status = 1.0;
            }
            update_buildings(&mut world);
            if world.iter_entities().any(|entity_ref| {
                entity_ref.get::<UnitTypeComp>().map(|comp| comp.type_id) == Some(manifold)
            }) {
                spawned = true;
                break;
            }
        }
        assert!(spawned, "unit-cargo-loader spawned manifold");
    }

    #[test]
    fn placed_factory_produces_unit_through_update_buildings() {
        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("ground-factory")
            .expect("ground-factory")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        assert!(
            world.get::<UnitFactoryBuild>(entity).is_some(),
            "factory state inserted by create_state"
        );
        // Feed the dagger plan (silicon 10 + lead 10).
        let silicon = content.item_id("silicon").expect("silicon");
        let lead = content.item_id("lead").expect("lead");
        if let Some(mut items) = world.get_mut::<ItemModule>(entity) {
            items.add(silicon, 20, 40);
            items.add(lead, 20, 40);
        }
        // `UnitFactoryBuild.created()` only auto-selects an *unlocked* plan; the
        // test registry has none, so select the dagger plan explicitly.
        if let Some(mut state) = world.get_mut::<UnitFactoryBuild>(entity) {
            state.current_plan = 0;
        }
        // `UnitFactory.updateTile` only advances when `efficiency > 0`
        // (`UnitFactory.java:406`); with no power graph in this harness the
        // consumer status is the efficiency input (`ConsumePower.efficiency`),
        // so mark the consumer satisfied as `PowerGraph.distributePower` would.
        if let Some(mut power) = world.get_mut::<crate::world::modules::PowerModule>(entity) {
            power.status = 1.0;
        }
        // `ground-factory` dagger plan time is 900 ticks.
        for _ in 0..1000 {
            update_buildings(&mut world);
            if world
                .iter_entities()
                .any(|entity_ref| entity_ref.contains::<Unit>())
            {
                break;
            }
        }
        assert!(
            world
                .iter_entities()
                .any(|entity_ref| entity_ref.contains::<Unit>()),
            "ground-factory produced a dagger through update_buildings"
        );
    }

    /// J05 T2: `UnitFactory`'s command config (`UnitFactory.java:58-82`, the
    /// command row's `configure(item)`): the row sets `command`, a plan change
    /// drops a command the new unit cannot use (`:64-66`, `:76-78`) and
    /// `configClear` clears only the command.
    #[test]
    fn factory_command_config_sets_and_drops_by_plan() {
        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("ground-factory")
            .expect("ground-factory")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        let behavior = inst.behavior.clone();
        // `nova` (plan 2) derives commands `[move, repair]`
        // (`UnitType.java:1066-1103`).
        let nova = content.unit_id("nova").expect("nova");
        behavior.configured(&mut world, entity, None, ConfigValue::Unit(nova));
        assert_eq!(
            world
                .get::<UnitFactoryBuild>(entity)
                .map(|state| state.current_plan),
            Some(2)
        );
        let repair = content.unit_command_by_name("repair").expect("repair").id;
        behavior.configured(
            &mut world,
            entity,
            None,
            ConfigValue::Content(crate::content::ContentType::UnitCommand, repair.raw()),
        );
        assert_eq!(
            world
                .get::<UnitFactoryBuild>(entity)
                .and_then(|state| state.command),
            Some(repair)
        );
        // The dagger plan (commands `[move]`) drops the unsupported command.
        let dagger = content.unit_id("dagger").expect("dagger");
        behavior.configured(&mut world, entity, None, ConfigValue::Unit(dagger));
        let state = world.get::<UnitFactoryBuild>(entity).expect("state");
        assert_eq!(state.current_plan, 0);
        assert_eq!(state.command, None, "unsupported command dropped");
        // `configClear` (`:82`) clears only the command; the plan stays.
        behavior.configured(&mut world, entity, None, ConfigValue::Unit(nova));
        behavior.configured(
            &mut world,
            entity,
            None,
            ConfigValue::Content(crate::content::ContentType::UnitCommand, repair.raw()),
        );
        behavior.configured(&mut world, entity, None, ConfigValue::None);
        let state = world.get::<UnitFactoryBuild>(entity).expect("state");
        assert_eq!(state.current_plan, 2, "plan kept by configClear");
        assert_eq!(state.command, None);
    }

    fn campaign_runtime(
        content: &ContentRegistry,
        rules: crate::game::rules::Rules,
    ) -> crate::game::runtime::CampaignRuntime {
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::universe::Campaign;

        let campaign = Campaign::from_registry(content, &EmptyNeighborhood);
        crate::game::runtime::CampaignRuntime::new(PlaySession::new(rules), campaign)
    }

    fn feed_factory(world: &mut World, entity: Entity, content: &ContentRegistry) {
        let silicon = content.item_id("silicon").expect("silicon");
        let lead = content.item_id("lead").expect("lead");
        if let Some(mut items) = world.get_mut::<ItemModule>(entity) {
            items.add(silicon, 400, 400);
            items.add(lead, 400, 400);
        }
        if let Some(mut state) = world.get_mut::<UnitFactoryBuild>(entity) {
            state.current_plan = 0;
        }
        if let Some(mut power) = world.get_mut::<crate::world::modules::PowerModule>(entity) {
            power.status = 1.0;
        }
    }

    /// G01 T12: `UnitPayload.dump` holds the finished payload when
    /// `Units.canCreate` is false (`rules.unit_cap = 1`).
    #[test]
    fn factory_respects_unit_cap() {
        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("ground-factory")
            .expect("ground-factory")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        world.insert_resource(campaign_runtime(
            &content,
            crate::game::rules::Rules {
                unit_cap: 1,
                unit_cap_variable: false,
                ..crate::game::rules::Rules::default()
            },
        ));
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        feed_factory(&mut world, entity, &content);
        for _ in 0..3000 {
            update_buildings(&mut world);
        }
        let daggers = world
            .iter_entities()
            .filter(|entity_ref| entity_ref.contains::<Unit>())
            .count();
        assert_eq!(daggers, 1, "the cap stops further production");
    }

    /// G01 T12: a `core-shard` raises the team's cap by `unitCapModifier` 8
    /// (`TeamData.unitCap`) and the 9th live dagger dies on spawn.
    #[test]
    fn core_modifier_raises_cap() {
        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let core = table.get_named("core-shard").expect("core-shard").clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        world.insert_resource(campaign_runtime(
            &content,
            crate::game::rules::Rules {
                unit_cap: 0,
                unit_cap_variable: true,
                ..crate::game::rules::Rules::default()
            },
        ));
        core.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        assert_eq!(
            crate::entities::comp::unit::queries::unit_cap(&world, 0),
            Some(8),
            "the core's unitCapModifier raises the cap"
        );
        for i in 0..9u64 {
            crate::entities::comp::unit::lifecycle::spawn_unit(
                &mut world,
                &content,
                100 + i,
                "dagger",
                0,
                64.0 + i as f32 * 8.0,
                64.0,
                0.0,
            );
        }
        let dagger = content.unit_by_name("dagger").expect("dagger").id;
        let alive = crate::entities::comp::unit::queries::live_type_count(&world, 0, dagger);
        assert_eq!(alive, 8, "the 9th dagger dies on spawn");
    }

    /// G01 T12: the wave team is uncapped outside PvP.
    #[test]
    fn wave_team_uncapped() {
        let content = content();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(campaign_runtime(
            &content,
            crate::game::rules::Rules {
                unit_cap: 0,
                unit_cap_variable: false,
                ..crate::game::rules::Rules::default()
            },
        ));
        assert_eq!(
            crate::entities::comp::unit::queries::unit_cap(&world, 2),
            None,
            "wave team uncapped"
        );
        for i in 0..5u64 {
            crate::entities::comp::unit::lifecycle::spawn_unit(
                &mut world,
                &content,
                100 + i,
                "dagger",
                2,
                64.0 + i as f32 * 8.0,
                64.0,
                0.0,
            );
        }
        let dagger = content.unit_by_name("dagger").expect("dagger").id;
        assert_eq!(
            crate::entities::comp::unit::queries::live_type_count(&world, 2, dagger),
            5
        );
    }
}
