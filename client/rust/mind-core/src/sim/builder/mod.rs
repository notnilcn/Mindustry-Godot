// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live builder-unit construction: the player unit's build-plan queue and the
//! per-tick `BuilderComp.updateBuildLogic` pass that drives
//! `ConstructBlock.ConstructBuild` progress (PAR-0098).
//!
//! Ported from `core/src/mindustry/entities/comp/BuilderComp.java`
//! (`validatePlans`, `updateBuildLogic`, `shouldSkip`, `addBuild`),
//! `core/src/mindustry/world/Build.java` (`beginPlace`/`beginBreak`) and
//! `core/src/mindustry/world/blocks/ConstructBlock.java`
//! (`construct`/`deconstruct`/`constructFinish`/`deconstructFinish`).
//!
//! Player input (`SimCommand::Place`/`Break` carrying `player: Some(_)`)
//! queues plans on the player unit instead of mutating tiles; the builder pass
//! then swaps the footprint to a `build<size>` construct block, advances it at
//! `1 / buildCost * buildSpeed * buildSpeedMultiplier * rules.buildSpeed(team)`
//! per tick while the unit is within `buildRange`, draws the requirements from
//! the team core as progress accrues (refunding on deconstruction), and sets
//! the real block when done. API placements (`player: None`, MCP/tests/AI)
//! keep the instant-build convention.

mod checks;
mod queue;
#[cfg(test)]
mod tests;
mod update;

pub(crate) use checks::no_unit_overlap;
pub use checks::unit_overlap_at;

use std::collections::VecDeque;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind, ContentRegistry};
use crate::ecs::{BuildingComp, EntitySeq};
use crate::entities::comp::unit::comp::{HitboxComp, StatusComp, UnitTypeComp};
use crate::entities::comp::{Building, Pos, TeamComp};
use crate::event::{BlockBrokenEvent, BlockPlacedEvent, SimEvent};
use crate::world::blocks::distribution::item_bridge::ItemBridgeBuild;
use crate::world::blocks::storage::core_block::CoreBuild;
use crate::world::construct::{BuildEconomy, CoreInventory};
use crate::world::modules::ItemModule;
use crate::world::{BlockTable, ConstructState, TilePos, WorldGrid};

use super::Sim;
use super::runtime::SimRuntime;

/// Team id of `Team.derelict`.
pub(super) const DERELICT: u8 = 0;

/// `Block.bounds(x, y)` (`Block.java:1023-1025`): the world rect a block
/// footprint covers, centered on `(x, y) * tilesize + offset`.
pub(super) fn block_bounds(
    content: Option<&crate::content::ContentRegistry>,
    x: i32,
    y: i32,
    block: BlockId,
) -> (f32, f32, f32, f32) {
    let (size, offset) = content
        .and_then(|content| content.block(block))
        .map(|def| (def.size, def.offset))
        .unwrap_or((1, 0.0));
    let unit = crate::config::TILESIZE as f32;
    let s = size as f32 * unit;
    (
        x as f32 * unit + offset - s / 2.0,
        y as f32 * unit + offset - s / 2.0,
        s,
        s,
    )
}

/// ARC `Rect.overlaps`: strict edge comparison (`Rect.java`).
pub(super) fn rects_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 < b.0 + b.2 && a.0 + a.2 > b.0 && a.1 < b.1 + b.3 && a.1 + a.3 > b.1
}

/// `Logic.checkOverlappingPlans` (`Logic.java:255-266`): a finished non-breaking
/// placement removes team ghost plans whose bounds overlap its own.
pub(super) fn check_overlapping_ghosts(
    world: &mut World,
    content: &crate::content::ContentRegistry,
    team: u8,
    x: i32,
    y: i32,
    block: BlockId,
) {
    let Some(mut campaign) = world.get_resource_mut::<crate::game::runtime::CampaignRuntime>()
    else {
        return;
    };
    let bounds = block_bounds(Some(content), x, y, block);
    let data = campaign.session.teams.get(crate::ecs::TeamId(team));
    for plan in data.plans.iter_mut() {
        if rects_overlap(
            bounds,
            block_bounds(Some(content), plan.x as i32, plan.y as i32, plan.block),
        ) {
            plan.removed = true;
        }
    }
    data.plans.retain(|plan| !plan.removed);
}

/// One queued build plan (`BuildPlan` with its transient builder fields).
#[derive(Debug, Clone, PartialEq)]
pub struct UnitBuildPlan {
    /// Target tile x (block centre).
    pub x: i16,
    /// Target tile y (block centre).
    pub y: i16,
    /// Placement rotation.
    pub rotation: u8,
    /// Block to place (`AIR` for a deconstruction).
    pub block: BlockId,
    /// `BuildPlan.config`; applied on finish
    /// (`ConstructBlock.constructFinish`, `ConstructBlock.java:72-118`).
    pub config: crate::world::config::ConfigValue,
    /// Whether this is a deconstruction order.
    pub breaking: bool,
    /// `BuildPlan.initialized`: the construct block was begun.
    pub initialized: bool,
    /// `BuildPlan.stuck`: no progress on the last attempt.
    pub stuck: bool,
    /// `BuildPlan.progress` mirror of the construct progress.
    pub progress: f32,
}

impl UnitBuildPlan {
    /// A placement plan.
    pub fn place(x: i16, y: i16, rotation: u8, block: BlockId) -> Self {
        Self {
            x,
            y,
            rotation,
            block,
            config: crate::world::config::ConfigValue::None,
            breaking: false,
            initialized: false,
            stuck: false,
            progress: 0.0,
        }
    }

    /// A deconstruction plan.
    pub fn break_plan(x: i16, y: i16) -> Self {
        Self {
            breaking: true,
            ..Self::place(x, y, 0, BlockId::AIR)
        }
    }

    fn tile(&self) -> TilePos {
        TilePos::new(self.x, self.y)
    }
}

/// `BuilderComp.plans` + `buildCounter` on a builder unit.
#[derive(Debug, Clone, Component)]
pub struct BuilderPlans {
    /// The plan queue; the head is the active plan.
    pub plans: VecDeque<UnitBuildPlan>,
    /// `buildCounter` (ticks of build work banked, capped at 10).
    pub build_counter: f32,
    /// `BuilderComp.updateBuilding`: whether the queue advances at all
    /// (`BuilderComp.updateBuildLogic` returns early when false; PAR-0098 §2b).
    pub update_building: bool,
    /// `ItemBridge.lastBuild`: the last bridge this builder placed (T11).
    pub last_bridge: Option<(BlockId, TilePos)>,
}

impl Default for BuilderPlans {
    fn default() -> Self {
        Self {
            plans: VecDeque::new(),
            build_counter: 0.0,
            // `BuilderComp.updateBuilding = true` upstream.
            update_building: true,
            last_bridge: None,
        }
    }
}

impl BuilderPlans {
    /// `BuilderComp.addBuild(place, tail)`: replaces a same-position plan.
    pub fn add_build(&mut self, plan: UnitBuildPlan, tail: bool) {
        self.plans
            .retain(|other| other.x != plan.x || other.y != plan.y);
        if tail {
            self.plans.push_back(plan);
        } else {
            self.plans.push_front(plan);
        }
    }
}
