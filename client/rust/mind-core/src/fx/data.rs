// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Typed replacement for Java's `Object Effect.data` (plan 17 §3.3).
//!
//! Java passes live `Posc`/`Block`/`Unit` objects and reads them every frame.
//! The port stores content ids / positions / entity refs in [`EffectData`] and
//! resolves live reads through a [`ViewSnapshot`]. Entities that despawn
//! mid-effect freeze at their last snapshot (plan 17 §8 R-17-6).

use smallvec::SmallVec;

use crate::content::{BlockId, ItemId, Rgba, TeamEntryId, UnitTypeId};

/// Opaque view-side entity handle (never a sim id; resolved by `ViewSnapshot`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ViewEntityId(pub u64);

/// A resolved world pose.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pose {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
}

/// Identifies a live trail channel (`EffectData::Trail`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrailChannelId(pub u32);

/// `LegDestroy` payload (`Fx.legDestroy`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct LegDestroyData {
    /// Base x.
    pub x: f32,
    /// Base y.
    pub y: f32,
    /// Leg length.
    pub length: f32,
    /// Joint offset.
    pub joint: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Whether the leg is a "flip" leg.
    pub flip: bool,
}

/// Typed effect payload. `Copy`-friendly except the position-list variants.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum EffectData {
    /// No payload.
    #[default]
    None,
    /// A unit type only (`unitSpawn`, `unitDespawn(type)`).
    UnitType(UnitTypeId),
    /// A live unit reference (`unitControl`, `unitShieldBreak`).
    Unit {
        /// View handle.
        id: ViewEntityId,
        /// Unit type at emit time.
        kind: UnitTypeId,
    },
    /// A block type.
    Block(BlockId),
    /// An item type.
    Item(ItemId),
    /// A fixed position (`itemTransfer`, `unitSpirit`).
    Position {
        /// World x.
        x: f32,
        /// World y.
        y: f32,
    },
    /// A list of positions (`debugLine`).
    Positions(SmallVec<[(f32, f32); 8]>),
    /// A live trail channel (`trailFade`).
    Trail(TrailChannelId),
    /// A single float (`healWaveDynamic`).
    Float(f32),
    /// Shield break payload.
    Shield {
        /// Team.
        team: TeamEntryId,
        /// Polygon side count.
        sides: u8,
        /// Rotation in degrees.
        rotation: f32,
        /// Radius.
        radius: f32,
    },
    /// `legDestroy`.
    LegDestroy(LegDestroyData),
    /// A vector-array payload.
    Vec2Array(SmallVec<[(f32, f32); 8]>),
    /// A rect payload.
    Rect {
        /// Center x.
        x: f32,
        /// Center y.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
    },
    /// Mod/scripting extension (`u32` discriminant).
    Custom(u32),
}

impl EffectData {
    /// Whether this payload can supply a position for `followParent`.
    pub fn position(&self, snapshot: &dyn ViewSnapshot) -> Option<Pose> {
        match self {
            EffectData::None
            | EffectData::UnitType(_)
            | EffectData::Block(_)
            | EffectData::Item(_)
            | EffectData::Trail(_)
            | EffectData::Float(_)
            | EffectData::Shield { .. }
            | EffectData::LegDestroy(_)
            | EffectData::Vec2Array(_)
            | EffectData::Rect { .. }
            | EffectData::Custom(_) => None,
            EffectData::Position { x, y } => Some(Pose {
                x: *x,
                y: *y,
                rotation: 0.0,
            }),
            EffectData::Positions(points) => points.first().map(|p| Pose {
                x: p.0,
                y: p.1,
                rotation: 0.0,
            }),
            EffectData::Unit { id, .. } => snapshot.unit_pose(*id),
        }
    }
}

/// Read-only view into live sim state for effect bodies. Implemented by the
/// gdext interpolated snapshot and by headless test fixtures.
pub trait ViewSnapshot: Send + Sync {
    /// Unit pose, or `None` if the unit is gone.
    fn unit_pose(&self, id: ViewEntityId) -> Option<Pose> {
        let _ = id;
        None
    }

    /// Unit type at the current tick, if known.
    fn unit_type(&self, id: ViewEntityId) -> Option<UnitTypeId> {
        let _ = id;
        None
    }

    /// Building pose.
    fn building_pose(&self, id: ViewEntityId) -> Option<Pose> {
        let _ = id;
        None
    }

    /// Building block type.
    fn building_block(&self, id: ViewEntityId) -> Option<BlockId> {
        let _ = id;
        None
    }

    /// Resolved team color.
    fn team_color(&self, team: TeamEntryId) -> Rgba {
        let _ = team;
        Rgba::WHITE
    }
}

/// A snapshot that resolves nothing (headless default).
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptySnapshot;

impl ViewSnapshot for EmptySnapshot {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_data_resolves() {
        let snap = EmptySnapshot;
        let data = EffectData::Position { x: 3.0, y: 4.0 };
        let pose = data.position(&snap).unwrap();
        assert_eq!((pose.x, pose.y), (3.0, 4.0));
    }

    #[test]
    fn missing_unit_freezes_to_none() {
        let snap = EmptySnapshot;
        let data = EffectData::Unit {
            id: ViewEntityId(7),
            kind: UnitTypeId::new(0),
        };
        assert!(data.position(&snap).is_none());
    }
}
