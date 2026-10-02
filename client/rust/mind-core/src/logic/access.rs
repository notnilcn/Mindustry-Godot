// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LAccess` and the logic capability trait definitions.
//!
//! Ported from `core/src/mindustry/logic/{LAccess,Senseable,Settable,Controllable,
//! Ranged,LReadable,LWritable,LPrintable,LDrawable}.java`. Definitions live here;
//! the concrete `sense`/`control`/`setProp` implementations land with the owning
//! plans (07/08/10/11/12) behind these traits.

use bevy_ecs::entity::Entity;

use crate::content::ContentRef;
use crate::logic::enums::{FieldKind, MlogField, logic_enum};
use crate::logic::value::LogicObject;

logic_enum!(
    /// `LAccess` — sensor/settable/controllable property.
    ///
    /// **Ordinals are ABI**: append only, never reorder.
    LAccess {
        TotalItems => "totalItems",
        FirstItem => "firstItem",
        TotalLiquids => "totalLiquids",
        TotalPower => "totalPower",
        ItemCapacity => "itemCapacity",
        LiquidCapacity => "liquidCapacity",
        PowerCapacity => "powerCapacity",
        PowerNetStored => "powerNetStored",
        PowerNetCapacity => "powerNetCapacity",
        PowerNetIn => "powerNetIn",
        PowerNetOut => "powerNetOut",
        Ammo => "ammo",
        AmmoCapacity => "ammoCapacity",
        CurrentAmmoType => "currentAmmoType",
        MemoryCapacity => "memoryCapacity",
        Health => "health",
        MaxHealth => "maxHealth",
        Heat => "heat",
        Shield => "shield",
        Armor => "armor",
        Efficiency => "efficiency",
        Progress => "progress",
        Timescale => "timescale",
        Rotation => "rotation",
        X => "x",
        Y => "y",
        VelocityX => "velocityX",
        VelocityY => "velocityY",
        ShootX => "shootX",
        ShootY => "shootY",
        CameraX => "cameraX",
        CameraY => "cameraY",
        CameraWidth => "cameraWidth",
        CameraHeight => "cameraHeight",
        DisplayWidth => "displayWidth",
        DisplayHeight => "displayHeight",
        BufferSize => "bufferSize",
        Operations => "operations",
        Size => "size",
        Solid => "solid",
        Dead => "dead",
        Range => "range",
        Shooting => "shooting",
        Boosting => "boosting",
        MineX => "mineX",
        MineY => "mineY",
        Mining => "mining",
        BuildX => "buildX",
        BuildY => "buildY",
        PingX => "pingX",
        PingY => "pingY",
        PingText => "pingText",
        Building => "building",
        Breaking => "breaking",
        Speed => "speed",
        Team => "team",
        Type => "type",
        Flag => "flag",
        Flying => "flying",
        Controlled => "controlled",
        Controller => "controller",
        Name => "name",
        PayloadCount => "payloadCount",
        PayloadType => "payloadType",
        TotalPayload => "totalPayload",
        PayloadCapacity => "payloadCapacity",
        MaxUnits => "maxUnits",
        Id => "id",
        SelectedBlock => "selectedBlock",
        SelectedRotation => "selectedRotation",
        BulletLifetime => "bulletLifetime",
        BulletTime => "bulletTime",
        Enabled => "enabled",
        Shoot => "shoot",
        Shootp => "shootp",
        Config => "config",
        Color => "color"
    }
);

impl LAccess {
    /// Parameter names (`LAccess.params`).
    pub const fn params(self) -> &'static [&'static str] {
        match self {
            LAccess::Enabled | LAccess::Config | LAccess::Color => &["to"],
            LAccess::Shoot => &["x", "y", "shoot"],
            LAccess::Shootp => &["unit", "shoot"],
            _ => &[],
        }
    }

    /// Object-valued accessor (`LAccess.isObj`).
    pub const fn is_obj(self) -> bool {
        matches!(self, LAccess::Shootp | LAccess::Config)
    }

    /// Privileged accessor (`LAccess.privilegedAccess`).
    pub const fn privileged(self) -> bool {
        matches!(
            self,
            LAccess::CameraX | LAccess::CameraY | LAccess::CameraWidth | LAccess::CameraHeight
        )
    }

    /// `LAccess.senseable`.
    pub fn senseable() -> Vec<LAccess> {
        LAccess::ALL
            .iter()
            .copied()
            .filter(|a| a.params().len() <= 1 && !a.privileged())
            .collect()
    }

    /// `LAccess.senseablePrivileged`.
    pub fn senseable_privileged() -> Vec<LAccess> {
        LAccess::ALL
            .iter()
            .copied()
            .filter(|a| a.params().len() <= 1)
            .collect()
    }

    /// `LAccess.controls`.
    pub fn controls() -> Vec<LAccess> {
        LAccess::ALL
            .iter()
            .copied()
            .filter(|a| !a.params().is_empty())
            .collect()
    }

    /// `LAccess.settable`.
    pub const SETTABLE: &'static [LAccess] = &[
        LAccess::X,
        LAccess::Y,
        LAccess::VelocityX,
        LAccess::VelocityY,
        LAccess::Rotation,
        LAccess::Speed,
        LAccess::Armor,
        LAccess::Health,
        LAccess::Shield,
        LAccess::Team,
        LAccess::Flag,
        LAccess::TotalPower,
        LAccess::PayloadType,
        LAccess::BulletTime,
        LAccess::BulletLifetime,
    ];
}

/// `Senseable`: numeric and object sensors plus content lookup.
pub trait LogicSense {
    /// Numeric `sense` (`Senseable.sense`).
    fn sense(&self, e: Entity, a: LAccess) -> f64;
    /// Object `senseObject` (`Senseable.senseObject`).
    fn sense_object(&self, e: Entity, a: LAccess) -> Option<LogicObject>;
    /// Content `sense` (`Senseable.sense(Content)`).
    fn sense_content(&self, e: Entity, c: ContentRef) -> f64;
}

/// `Settable`: `setProp` dispatch.
pub trait LogicSettle {
    /// Numeric assignment.
    fn set_prop_num(&mut self, e: Entity, a: LAccess, v: f64);
    /// Object assignment.
    fn set_prop_obj(&mut self, e: Entity, a: LAccess, v: LogicObject);
    /// Content-count assignment.
    fn set_prop_content(&mut self, e: Entity, c: ContentRef, v: f64);
}

/// `Controllable`: `control` dispatch.
pub trait LogicControl {
    /// Numeric control.
    fn control_num(&mut self, e: Entity, a: LAccess, p1: f64, p2: f64, p3: f64, p4: f64);
    /// Object control.
    fn control_obj(&mut self, e: Entity, a: LAccess, p1: LogicObject, p2: f64, p3: f64, p4: f64);
    /// Owning team of the target.
    fn logic_team(&self, e: Entity) -> u8;
}

/// `Ranged`: radar/locate range source.
pub trait LogicRanged {
    /// Range in world units (`Ranged.range`).
    fn range(&self, e: Entity) -> f32;
}

/// `LReadable`: a logic-readable target (memory, processor variables).
pub trait LogicRead {
    /// Reads `address` from the target.
    fn logic_read(&mut self, e: Entity, address: &super::value::LVar) -> super::value::LVar;
}

/// `LWritable`: a logic-writable target.
pub trait LogicWrite {
    /// Writes `value` at `address`.
    fn logic_write(&mut self, e: Entity, address: &super::value::LVar, value: &super::value::LVar);
}

/// `LPrintable`: a target that accepts printed text.
pub trait LogicPrint {
    /// Prints `value` to the target.
    fn logic_print(&mut self, e: Entity, value: &str);
}

/// `LDrawable`: a target that accepts packed draw commands.
pub trait LogicDraw {
    /// Appends the packed `DisplayCmd` buffer and advances `operations`.
    fn logic_draw(&mut self, e: Entity, buffer: &[u64]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_access_table_matches_upstream() {
        assert_eq!(LAccess::ALL.len(), 77);
        assert_eq!(LAccess::TotalItems.name(), "totalItems");
        assert_eq!(LAccess::Enabled.params(), &["to"]);
        assert!(LAccess::Config.is_obj());
        assert!(LAccess::CameraX.privileged());
        assert!(!LAccess::senseable().contains(&LAccess::CameraX));
        assert!(LAccess::senseable_privileged().contains(&LAccess::CameraX));
        assert!(LAccess::controls().contains(&LAccess::Shoot));
        assert!(LAccess::SETTABLE.contains(&LAccess::Health));
    }
}
