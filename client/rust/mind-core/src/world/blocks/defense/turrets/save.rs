// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret save fields (plan 10 §6.3; `TurretBuild`/`ItemTurretBuild`/
//! `ContinuousTurretBuild` `write`/`read`).
//!
//! Plan 07's `BuildingCodec` owns the building chunk framing; this module owns
//! the per-class turret payload. Revision semantics are append-only and match
//! `6.3`: v1 `reloadCounter`+`rotation`, v2 adds item ammo, v3 adds
//! `lastLength`. Reads are revision-tolerant (unknown/missing fields keep
//! defaults).

use crate::content::{BulletKind, ItemId};
use crate::io::IoError;
use crate::world::blocks::payloads::PayloadKind;
use crate::world::{BuildingReader, BuildingWriter};

use super::{AmmoEntry, PayloadContent, PayloadStack, TurretAmmo, TurretKind, TurretState};

/// `Building.version()` for a turret kind (`6.3`).
pub const fn version(kind: TurretKind) -> u8 {
    match kind {
        TurretKind::Item | TurretKind::PayloadAmmo => 2,
        TurretKind::Continuous | TurretKind::Laser => 3,
        _ => 1,
    }
}

/// Writes the turret payload (`TurretBuild.write` chain).
pub fn write(w: &mut BuildingWriter, state: &TurretState) -> Result<(), IoError> {
    // v1: `TurretBuild.write`.
    w.f(state.reload_counter);
    w.f(state.rotation);
    // v2: `ItemTurretBuild.write` / `PayloadTurretBuild.write`.
    match &state.config.ammo {
        TurretAmmo::Item(_) => {
            w.ub(state.ammo.len() as u8);
            for entry in &state.ammo {
                w.us(entry.item.raw());
                w.s(entry.amount as i16);
            }
        }
        TurretAmmo::Payload(_) => {
            w.ub(state.payloads.len() as u8);
            for stack in &state.payloads {
                w.b(stack.content.kind.tag() as i8);
                w.us(stack.content.id);
                w.s(stack.count as i16);
            }
        }
        _ => {}
    }
    // v3: `ContinuousTurretBuild.write` / `LaserTurretBuild.write`.
    if matches!(
        state.config.kind,
        TurretKind::Continuous | TurretKind::Laser
    ) {
        w.f(state.last_length);
    }
    Ok(())
}

/// Reads the turret payload (revision-tolerant; `TurretBuild.read` chain).
pub fn read(r: &mut BuildingReader, state: &mut TurretState, revision: u8) -> Result<(), IoError> {
    state.reload_counter = r.f()?;
    state.rotation = r.f()?;
    if revision >= 2 {
        match &state.config.ammo {
            TurretAmmo::Item(_) => {
                let count = r.ub()? as usize;
                state.ammo.clear();
                for _ in 0..count {
                    let item = ItemId::new(r.us()?);
                    let amount = r.s()? as i32;
                    let bullet = state
                        .config
                        .ammo
                        .item_bullet(item)
                        .unwrap_or_else(|| state.config.ammo.first_bullet());
                    state.ammo.push(AmmoEntry {
                        item,
                        bullet,
                        amount,
                    });
                }
                state.total_ammo = state.ammo.iter().map(|entry| entry.amount).sum();
            }
            TurretAmmo::Payload(_) => {
                let count = r.ub()? as usize;
                state.payloads.clear();
                for _ in 0..count {
                    let kind = PayloadKind::from_tag(r.b()? as u8).unwrap_or(PayloadKind::Build);
                    let id = r.us()?;
                    let amount = r.s()? as i32;
                    state.payloads.push(PayloadStack {
                        content: PayloadContent { kind, id },
                        count: amount,
                    });
                }
                state.total_ammo = state.payloads.iter().map(|stack| stack.count).sum();
            }
            _ => {}
        }
    }
    if revision >= 3 {
        state.last_length = r.f()?;
    }
    Ok(())
}

impl TurretAmmo {
    /// Bullet fired by an accepted item (`ammoTypes` lookup).
    fn item_bullet(&self, item: ItemId) -> Option<crate::content::BulletId> {
        match self {
            TurretAmmo::Item(entries) => entries
                .iter()
                .find(|entry| entry.item == item)
                .map(|entry| entry.bullet),
            _ => None,
        }
    }

    /// First ammo bullet (fallback when a saved item is unknown).
    fn first_bullet(&self) -> crate::content::BulletId {
        match self {
            TurretAmmo::Item(entries) => entries
                .first()
                .map(|entry| entry.bullet)
                .unwrap_or_default(),
            TurretAmmo::Liquid(entries) => entries
                .first()
                .map(|entry| entry.bullet)
                .unwrap_or_default(),
            TurretAmmo::Power(bullet) => *bullet,
            TurretAmmo::Payload(entries) => entries
                .first()
                .map(|entry| entry.bullet)
                .unwrap_or_default(),
        }
    }
}

/// Convenience: the bullet kind that a turret's ammo resolves to (view/tests).
pub fn ammo_bullet_kind(
    state: &TurretState,
    content: &crate::content::ContentRegistry,
) -> BulletKind {
    let bullet =
        state
            .ammo
            .first()
            .map(|entry| entry.bullet)
            .or_else(|| match &state.config.ammo {
                TurretAmmo::Power(bullet) => Some(*bullet),
                TurretAmmo::Item(entries) => entries.first().map(|entry| entry.bullet),
                TurretAmmo::Liquid(entries) => entries.first().map(|entry| entry.bullet),
                TurretAmmo::Payload(entries) => entries.first().map(|entry| entry.bullet),
            });
    bullet
        .and_then(|id| content.bullet(id))
        .map(|def| def.kind)
        .unwrap_or(BulletKind::Basic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turret_versions_match_plan() {
        assert_eq!(version(TurretKind::Item), 2);
        assert_eq!(version(TurretKind::Continuous), 3);
        assert_eq!(version(TurretKind::Laser), 3);
        assert_eq!(version(TurretKind::Power), 1);
        assert_eq!(version(TurretKind::PointDefense), 1);
        assert_eq!(version(TurretKind::TractorBeam), 1);
    }
}
