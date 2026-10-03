// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Layer`, `CacheLayer` and `BuildingCacheLayer` constant tables.
//!
//! Ported from `graphics/Layer.java`, `graphics/CacheLayer.java` and
//! `graphics/BuildingCacheLayer.java`. All three are **append-only ABI**: new
//! values append, existing values never renumber. The `CacheLayer` order is the
//! plan-06 §3.12 / plan-16 §3.4 frozen ABI.

/// One `Layer` value (plan 16 §3.4). The integer discriminant is only an
/// ordering aid; [`Layer::z`] is the authoritative f32 value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    /// `Layer.min` (`-11`).
    Min,
    /// `Layer.background` (`-10`).
    Background,
    /// `Layer.floor` (`0`).
    Floor,
    /// `Layer.scorch` (`10`).
    Scorch,
    /// `Layer.debris` (`20`).
    Debris,
    /// `Layer.blockUnder` (`29.5`).
    BlockUnder,
    /// `Layer.block` (`30`).
    Block,
    /// `Layer.blockCracks` (`30.1`).
    BlockCracks,
    /// `Layer.blockAfterCracks` (`30.2`).
    BlockAfterCracks,
    /// `Layer.blockAdditive` (`31`).
    BlockAdditive,
    /// `Layer.blockProp` (`32`).
    BlockProp,
    /// `Layer.blockOver` (`35`).
    BlockOver,
    /// `Layer.blockBuilding` (`40`).
    BlockBuilding,
    /// `Layer.turret` (`50`).
    Turret,
    /// `Layer.turretHeat` (`50.1`).
    TurretHeat,
    /// `Layer.groundUnit` (`60`).
    GroundUnit,
    /// `Layer.power` (`70`).
    Power,
    /// `Layer.legUnit` (`75`).
    LegUnit,
    /// `Layer.darkness` (`80`).
    Darkness,
    /// `Layer.plans` (`85`).
    Plans,
    /// `Layer.flyingUnitLow` (`90`).
    FlyingUnitLow,
    /// `Layer.bullet` (`100`).
    Bullet,
    /// `Layer.effect` (`110`).
    Effect,
    /// `Layer.flyingUnit` (`115`).
    FlyingUnit,
    /// `Layer.overlayUI` (`120`).
    OverlayUi,
    /// `Layer.buildBeam` (`122`).
    BuildBeam,
    /// `Layer.shields` (`125`).
    Shields,
    /// `Layer.weather` (`130`).
    Weather,
    /// `Layer.light` (`140`).
    Light,
    /// `Layer.playerName` (`150`).
    PlayerName,
    /// `Layer.fogOfWar` (`155`).
    FogOfWar,
    /// `Layer.space` (`160`).
    Space,
    /// `Layer.end` (`200`).
    End,
    /// `Layer.endPixeled` (`210`).
    EndPixeled,
    /// `Layer.max` (`220`).
    Max,
}

impl Layer {
    /// Every layer in ascending z order (the band iteration order).
    pub const ALL: [Layer; 35] = [
        Layer::Min,
        Layer::Background,
        Layer::Floor,
        Layer::Scorch,
        Layer::Debris,
        Layer::BlockUnder,
        Layer::Block,
        Layer::BlockCracks,
        Layer::BlockAfterCracks,
        Layer::BlockAdditive,
        Layer::BlockProp,
        Layer::BlockOver,
        Layer::BlockBuilding,
        Layer::Turret,
        Layer::TurretHeat,
        Layer::GroundUnit,
        Layer::Power,
        Layer::LegUnit,
        Layer::Darkness,
        Layer::Plans,
        Layer::FlyingUnitLow,
        Layer::Bullet,
        Layer::Effect,
        Layer::FlyingUnit,
        Layer::OverlayUi,
        Layer::BuildBeam,
        Layer::Shields,
        Layer::Weather,
        Layer::Light,
        Layer::PlayerName,
        Layer::FogOfWar,
        Layer::Space,
        Layer::End,
        Layer::EndPixeled,
        Layer::Max,
    ];

    /// The `Layer.java` float value.
    pub const fn z(self) -> f32 {
        match self {
            Layer::Min => -11.0,
            Layer::Background => -10.0,
            Layer::Floor => 0.0,
            Layer::Scorch => 10.0,
            Layer::Debris => 20.0,
            Layer::BlockUnder => 29.5,
            Layer::Block => 30.0,
            Layer::BlockCracks => 30.1,
            Layer::BlockAfterCracks => 30.2,
            Layer::BlockAdditive => 31.0,
            Layer::BlockProp => 32.0,
            Layer::BlockOver => 35.0,
            Layer::BlockBuilding => 40.0,
            Layer::Turret => 50.0,
            Layer::TurretHeat => 50.1,
            Layer::GroundUnit => 60.0,
            Layer::Power => 70.0,
            Layer::LegUnit => 75.0,
            Layer::Darkness => 80.0,
            Layer::Plans => 85.0,
            Layer::FlyingUnitLow => 90.0,
            Layer::Bullet => 100.0,
            Layer::Effect => 110.0,
            Layer::FlyingUnit => 115.0,
            Layer::OverlayUi => 120.0,
            Layer::BuildBeam => 122.0,
            Layer::Shields => 125.0,
            Layer::Weather => 130.0,
            Layer::Light => 140.0,
            Layer::PlayerName => 150.0,
            Layer::FogOfWar => 155.0,
            Layer::Space => 160.0,
            Layer::End => 200.0,
            Layer::EndPixeled => 210.0,
            Layer::Max => 220.0,
        }
    }

    /// The Java identifier (`LayerName` parity ABI).
    pub const fn name(self) -> &'static str {
        match self {
            Layer::Min => "min",
            Layer::Background => "background",
            Layer::Floor => "floor",
            Layer::Scorch => "scorch",
            Layer::Debris => "debris",
            Layer::BlockUnder => "blockUnder",
            Layer::Block => "block",
            Layer::BlockCracks => "blockCracks",
            Layer::BlockAfterCracks => "blockAfterCracks",
            Layer::BlockAdditive => "blockAdditive",
            Layer::BlockProp => "blockProp",
            Layer::BlockOver => "blockOver",
            Layer::BlockBuilding => "blockBuilding",
            Layer::Turret => "turret",
            Layer::TurretHeat => "turretHeat",
            Layer::GroundUnit => "groundUnit",
            Layer::Power => "power",
            Layer::LegUnit => "legUnit",
            Layer::Darkness => "darkness",
            Layer::Plans => "plans",
            Layer::FlyingUnitLow => "flyingUnitLow",
            Layer::Bullet => "bullet",
            Layer::Effect => "effect",
            Layer::FlyingUnit => "flyingUnit",
            Layer::OverlayUi => "overlayUI",
            Layer::BuildBeam => "buildBeam",
            Layer::Shields => "shields",
            Layer::Weather => "weather",
            Layer::Light => "light",
            Layer::PlayerName => "playerName",
            Layer::FogOfWar => "fogOfWar",
            Layer::Space => "space",
            Layer::End => "end",
            Layer::EndPixeled => "endPixeled",
            Layer::Max => "max",
        }
    }

    /// Parses a Java layer identifier (debug API / render-list `layer` names).
    pub fn from_name(name: &str) -> Option<Layer> {
        Layer::ALL.iter().copied().find(|l| l.name() == name)
    }

    /// Resolves the nearest named [`Layer`] for an arbitrary `z` (descriptor
    /// overrides such as `block + 0.01` fold onto their base layer).
    pub fn from_z(z: f32) -> Layer {
        let mut best = Layer::Block;
        let mut best_delta = f32::MAX;
        for layer in Layer::ALL {
            let delta = (layer.z() - z).abs();
            if delta < best_delta {
                best_delta = delta;
                best = layer;
            }
        }
        best
    }
}

/// A `CacheLayer` id. The numeric value is the frozen ABI order
/// `water,mud,tar,slag,arkycite,cryofluid,space,normal,walls` (plan 06 §3.12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CacheLayerId {
    /// `CacheLayer.water`.
    Water,
    /// `CacheLayer.mud`.
    Mud,
    /// `CacheLayer.tar`.
    Tar,
    /// `CacheLayer.slag`.
    Slag,
    /// `CacheLayer.arkycite`.
    Arkycite,
    /// `CacheLayer.cryofluid`.
    Cryofluid,
    /// `CacheLayer.space`.
    Space,
    /// `CacheLayer.normal`.
    Normal,
    /// `CacheLayer.walls`.
    Walls,
}

impl CacheLayerId {
    /// Every cache layer in frozen ABI order.
    pub const ALL: [CacheLayerId; 9] = [
        CacheLayerId::Water,
        CacheLayerId::Mud,
        CacheLayerId::Tar,
        CacheLayerId::Slag,
        CacheLayerId::Arkycite,
        CacheLayerId::Cryofluid,
        CacheLayerId::Space,
        CacheLayerId::Normal,
        CacheLayerId::Walls,
    ];

    /// Frozen ABI id (`CacheLayer.id`).
    pub const fn id(self) -> u8 {
        match self {
            CacheLayerId::Water => 0,
            CacheLayerId::Mud => 1,
            CacheLayerId::Tar => 2,
            CacheLayerId::Slag => 3,
            CacheLayerId::Arkycite => 4,
            CacheLayerId::Cryofluid => 5,
            CacheLayerId::Space => 6,
            CacheLayerId::Normal => 7,
            CacheLayerId::Walls => 8,
        }
    }

    /// Java identifier.
    pub const fn name(self) -> &'static str {
        match self {
            CacheLayerId::Water => "water",
            CacheLayerId::Mud => "mud",
            CacheLayerId::Tar => "tar",
            CacheLayerId::Slag => "slag",
            CacheLayerId::Arkycite => "arkycite",
            CacheLayerId::Cryofluid => "cryofluid",
            CacheLayerId::Space => "space",
            CacheLayerId::Normal => "normal",
            CacheLayerId::Walls => "walls",
        }
    }

    /// `CacheLayer.liquid` (shader-animated liquid surface).
    pub const fn is_liquid(self) -> bool {
        matches!(
            self,
            CacheLayerId::Water
                | CacheLayerId::Mud
                | CacheLayerId::Tar
                | CacheLayerId::Slag
                | CacheLayerId::Arkycite
                | CacheLayerId::Cryofluid
        )
    }

    /// The `ShaderLayer` shader registry key, if any (`ShaderLayer.shader`).
    pub const fn shader_key(self) -> Option<&'static str> {
        match self {
            CacheLayerId::Water => Some("water"),
            CacheLayerId::Mud => Some("mud"),
            CacheLayerId::Tar => Some("tar"),
            CacheLayerId::Slag => Some("slag"),
            CacheLayerId::Arkycite => Some("arkycite"),
            CacheLayerId::Cryofluid => Some("cryofluid"),
            CacheLayerId::Space => Some("space"),
            CacheLayerId::Normal | CacheLayerId::Walls => None,
        }
    }

    /// Parses a Java cache-layer identifier.
    pub fn from_name(name: &str) -> Option<CacheLayerId> {
        CacheLayerId::ALL.iter().copied().find(|l| l.name() == name)
    }
}

/// `BuildingCacheLayer` values (`Layer.block - 0.5` / `Layer.block`).
pub mod building_cache_layer {
    use super::Layer;

    /// `BuildingCacheLayer.under`.
    pub const UNDER: f32 = Layer::BlockUnder.z();
    /// `BuildingCacheLayer.normal`.
    pub const NORMAL: f32 = Layer::Block.z();
    /// `BuildingCacheLayer.all` order.
    pub const ALL: [f32; 2] = [UNDER, NORMAL];
    /// `BuildingCacheLayer.amount`.
    pub const AMOUNT: usize = ALL.len();
}

/// Alias kept for the plan's `BuildingCacheLayer` name.
pub use building_cache_layer as BuildingCacheLayer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_values_parity() {
        // Exact `graphics/Layer.java` values, in order.
        assert_eq!(Layer::ALL.len(), 35);
        let expected = [
            (-11.0_f32, "min"),
            (-10.0, "background"),
            (0.0, "floor"),
            (10.0, "scorch"),
            (20.0, "debris"),
            (29.5, "blockUnder"),
            (30.0, "block"),
            (30.1, "blockCracks"),
            (30.2, "blockAfterCracks"),
            (31.0, "blockAdditive"),
            (32.0, "blockProp"),
            (35.0, "blockOver"),
            (40.0, "blockBuilding"),
            (50.0, "turret"),
            (50.1, "turretHeat"),
            (60.0, "groundUnit"),
            (70.0, "power"),
            (75.0, "legUnit"),
            (80.0, "darkness"),
            (85.0, "plans"),
            (90.0, "flyingUnitLow"),
            (100.0, "bullet"),
            (110.0, "effect"),
            (115.0, "flyingUnit"),
            (120.0, "overlayUI"),
            (122.0, "buildBeam"),
            (125.0, "shields"),
            (130.0, "weather"),
            (140.0, "light"),
            (150.0, "playerName"),
            (155.0, "fogOfWar"),
            (160.0, "space"),
            (200.0, "end"),
            (210.0, "endPixeled"),
            (220.0, "max"),
        ];
        for (layer, (z, name)) in Layer::ALL.iter().zip(expected) {
            assert_eq!(layer.z(), z, "layer {} z", name);
            assert_eq!(layer.name(), name);
            assert_eq!(Layer::from_name(name), Some(*layer));
        }
    }

    #[test]
    fn cache_layer_order_parity() {
        // Frozen plan-06 §3.12 / plan-16 §3.4 ABI order.
        let expected = [
            "water",
            "mud",
            "tar",
            "slag",
            "arkycite",
            "cryofluid",
            "space",
            "normal",
            "walls",
        ];
        for (i, name) in expected.iter().enumerate() {
            let layer = CacheLayerId::from_name(name).unwrap();
            assert_eq!(layer.id() as usize, i, "cache layer {name}");
            assert_eq!(layer.name(), *name);
        }
        assert_eq!(CacheLayerId::ALL.len(), 9);
        assert!(CacheLayerId::Water.is_liquid());
        assert!(!CacheLayerId::Space.is_liquid());
        assert!(!CacheLayerId::Walls.is_liquid());
    }

    #[test]
    fn building_cache_layer_values() {
        assert_eq!(BuildingCacheLayer::UNDER, 29.5);
        assert_eq!(BuildingCacheLayer::NORMAL, 30.0);
        assert_eq!(BuildingCacheLayer::AMOUNT, 2);
        assert_eq!(BuildingCacheLayer::ALL, [29.5, 30.0]);
    }
}
