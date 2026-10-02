// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Controller framework (plan 11 §3.5): `UnitController`, the transient
//! `ControllerSlot` storage, and controller selection.
//!
//! Ported from `entities/units/UnitController.java` and
//! `entities/units/AIController.java`. Plan 11 deviation 3: no `Prov`/closure
//! selection — a data [`AiKind`] plus free-function dispatch. M0 implements
//! `NoAi`/`Ground`/`Flying`; the remaining kinds are declared so the selection
//! matrix is stable and append-only.

use bevy_ecs::component::Component;
use bevy_ecs::world::World;

use crate::content::registries::units::{AiControllerKind, UnitTypeDef};
use crate::world::TilePos;

/// Runtime controller kind (plan 11 §3.5; append-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AiKind {
    /// Inert controller (`NoAI`).
    NoAi,
    /// Ground pathfinding controller (`GroundAI`, default for walkers).
    Ground,
    /// Melee "hug" controller (`HugAI`; raycast approach, closes to contact).
    Hug,
    /// Flying flag controller (`FlyingAI`, default for air units).
    #[default]
    Flying,
    /// Flying follow controller (`FlyingFollowAI`; follows an allied unit).
    FlyingFollow,
    /// RTS command controller (`CommandAI`).
    Command,
    /// Logic controller (`LogicAI`).
    Logic,
    /// Homing missile controller (`MissileAI`).
    Missile,
    /// Builder controller (`BuilderAI`).
    Builder,
    /// Miner controller (`MinerAI`).
    Miner,
    /// Cargo shuttle controller (`CargoAI`).
    Cargo,
    /// Defender controller (`DefenderAI`).
    Defender,
    /// Suicide ram controller (`SuicideAI`).
    Suicide,
    /// RTS boost controller (`BoostAI`).
    Boost,
    /// Assembler drone controller (`AssemblerAI`).
    Assembler,
    /// Experimental wave pre-build controller (`PrebuildAI`).
    Prebuild,
    /// Repair/rebuild-assist controller (`RepairAI`; append-only).
    Repair,
    /// Player possession bridge (`Player`; plan 15 owns input).
    Player,
}

impl AiKind {
    /// Java class-ish name (codec/audit ABI).
    pub const fn name(self) -> &'static str {
        match self {
            AiKind::NoAi => "NoAI",
            AiKind::Ground => "GroundAI",
            AiKind::Hug => "HugAI",
            AiKind::Flying => "FlyingAI",
            AiKind::FlyingFollow => "FlyingFollowAI",
            AiKind::Command => "CommandAI",
            AiKind::Logic => "LogicAI",
            AiKind::Missile => "MissileAI",
            AiKind::Builder => "BuilderAI",
            AiKind::Miner => "MinerAI",
            AiKind::Cargo => "CargoAI",
            AiKind::Defender => "DefenderAI",
            AiKind::Suicide => "SuicideAI",
            AiKind::Boost => "BoostAI",
            AiKind::Assembler => "AssemblerAI",
            AiKind::Prebuild => "PrebuildAI",
            AiKind::Repair => "RepairAI",
            AiKind::Player => "Player",
        }
    }
}

/// Transient controller slot stored on every unit (`Unit.controller`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct ControllerSlot {
    /// Active controller kind.
    pub kind: AiKind,
    /// Move target tile (`CommandAI.targetPos` / M0 test hook).
    pub target: Option<TilePos>,
    /// Stuck/progress timer in ticks (`stuckTimer`-adjacent; M3 uses it).
    pub timer: f32,
}

impl ControllerSlot {
    /// Creates a slot for `kind` with no target.
    pub const fn new(kind: AiKind) -> Self {
        Self {
            kind,
            target: None,
            timer: 0.0,
        }
    }
}

/// Seam trait for object controllers (plan 11 §3.5). M0 stores [`AiKind`] on the
/// entity and dispatches through free functions; later milestones attach state.
pub trait UnitController: Send {
    /// The controlled unit, if any.
    fn unit(&self) -> Option<bevy_ecs::entity::Entity>;
    /// Binds the controller to a unit.
    fn set_unit(&mut self, unit: bevy_ecs::entity::Entity);
    /// Called when the controller becomes active.
    fn init(&mut self, _world: &mut World) {}
    /// Per-tick update.
    fn update_unit(&mut self, _world: &mut World) {}
    /// Whether this controller survives a save/load (`keepState`).
    fn keep_state(&self) -> bool {
        false
    }
}

/// Selects the AI controller for a unit type (`UnitType.aiController`).
///
/// Mirrors `UnitType.controller`/`aiController`: an explicit content
/// `ControllerKind` (assembler/builder/cargo/no/missile) wins; otherwise the
/// `AiControllerKind` preset selects the AI, defaulting to `FlyingAI` for air
/// units and `GroundAI` for walkers.
pub fn select_ai(unit: &UnitTypeDef) -> AiKind {
    use crate::content::registries::units::ControllerKind;
    match unit.controller {
        ControllerKind::Assembler => AiKind::Assembler,
        ControllerKind::Builder { .. } => AiKind::Builder,
        // AI team default (`BuilderOrCommand` resolves to `CommandAI` for player
        // teams; the harness spawns AI teams, so use the builder behavior).
        ControllerKind::BuilderOrCommand => AiKind::Builder,
        ControllerKind::Cargo => AiKind::Cargo,
        ControllerKind::No => AiKind::NoAi,
        ControllerKind::Missile => AiKind::Missile,
        ControllerKind::Default => match unit.ai_controller {
            AiControllerKind::Defender => AiKind::Defender,
            AiControllerKind::FlyingFollow => AiKind::FlyingFollow,
            AiControllerKind::Hug => AiKind::Hug,
            AiControllerKind::Suicide => AiKind::Suicide,
            AiControllerKind::Default => {
                if unit.flying {
                    AiKind::Flying
                } else {
                    AiKind::Ground
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_kind_names_are_unique() {
        let kinds = [
            AiKind::NoAi,
            AiKind::Ground,
            AiKind::Hug,
            AiKind::Flying,
            AiKind::FlyingFollow,
            AiKind::Command,
            AiKind::Logic,
            AiKind::Missile,
            AiKind::Builder,
            AiKind::Miner,
            AiKind::Cargo,
            AiKind::Defender,
            AiKind::Suicide,
            AiKind::Boost,
            AiKind::Assembler,
            AiKind::Prebuild,
            AiKind::Repair,
            AiKind::Player,
        ];
        for (i, a) in kinds.iter().enumerate() {
            for b in &kinds[i + 1..] {
                assert_ne!(a.name(), b.name());
            }
        }
    }
}
