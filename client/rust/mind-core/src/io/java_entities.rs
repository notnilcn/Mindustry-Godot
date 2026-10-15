// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Upstream (`MSAV`) save entity decoder: unit, fire and puddle chunks.
//!
//! `SaveVersion.writeWorldEntities` (`J/io/SaveVersion.java:425-448`) writes
//! every serializable entity as `u8 class id` + `i32 id` + the generated
//! `EntityIO.write` payload (`s revision` + fields in manifest order,
//! `JA/src/main/java/mindustry/annotations/entity/EntityIO.java:83-96`). The
//! native `MGRS` chain stores its own revisions in `sim_io`; this module is the
//! one-way reader for the upstream class ids a Java save carries: the vanilla
//! unit classes, `FireComp` (id 10) and `PuddleComp` (id 13).
//!
//! The field tables are the committed revision manifests
//! (`JA/resources/revisions/<class>/<N>.json`) minus the `@NoSerialize` fields
//! (`WeaponsComp.aimX/aimY` are in every unit manifest but never written), in
//! manifest order (which is the generated `write` order). Every table was
//! verified against the generated `write` bytecode of the matching
//! `mindustry.gen` class (2026-10-15). The class ids are the live slots of the
//! generated `EntityMapping` static initializer (stale `classids.properties`
//! rows such as `vela=25`/`vanquish=40` have no entity class and never appear
//! in a save).

use crate::ai::controller::AiKind;
use crate::content::{ContentRegistry, ContentType, ItemId, LiquidId, StatusId, UnitTypeId};
use crate::entities::comp::unit::comp::StatusEntry;
use crate::io::IoResult;
use crate::io::sim_io::{DecodedFire, DecodedPuddle, DecodedUnit};
use crate::io::wire::WireReader;

/// Upstream `FireComp` class id (`classids.properties`).
pub const JAVA_FIRE_CLASS_ID: u8 = 10;
/// Upstream `PuddleComp` class id (`classids.properties`).
pub const JAVA_PUDDLE_CLASS_ID: u8 = 13;

/// One unit class layout: the save class id, the revision that class writes,
/// and the wire field names in write order.
pub(crate) struct JavaUnitClass {
    class_id: u8,
    revision: u16,
    fields: &'static [&'static str],
}

/// Standard `UnitEntity`/`UnitWaterMove`/`LegsUnit`/`TankUnit`/`CrawlUnit`/
/// `ElevationMoveUnit` layout (manifest minus `aimX`/`aimY`).
const UNIT_STANDARD: &[&str] = &[
    "abilities",
    "controller",
    "elevation",
    "flag",
    "health",
    "isShooting",
    "mineTile",
    "mounts",
    "plans",
    "rotation",
    "shield",
    "spawnedByCore",
    "stack",
    "statuses",
    "team",
    "type",
    "updateBuilding",
    "vel",
    "x",
    "y",
];

/// `MechUnit` layout: `baseRotation` sits between `abilities` and
/// `controller`.
const UNIT_BASE_ROTATION: &[&str] = &[
    "abilities",
    "baseRotation",
    "controller",
    "elevation",
    "flag",
    "health",
    "isShooting",
    "mineTile",
    "mounts",
    "plans",
    "rotation",
    "shield",
    "spawnedByCore",
    "stack",
    "statuses",
    "team",
    "type",
    "updateBuilding",
    "vel",
    "x",
    "y",
];

/// `PayloadUnit` layout: `payloads` sits between `mounts` and `plans`.
const UNIT_PAYLOAD: &[&str] = &[
    "abilities",
    "controller",
    "elevation",
    "flag",
    "health",
    "isShooting",
    "mineTile",
    "mounts",
    "payloads",
    "plans",
    "rotation",
    "shield",
    "spawnedByCore",
    "stack",
    "statuses",
    "team",
    "type",
    "updateBuilding",
    "vel",
    "x",
    "y",
];

/// `BuildingTetherPayloadUnit` layout (`manifold`/`assembly-drone`):
/// `building` after `abilities`, `payloads` after `mounts`.
const UNIT_TETHER_PAYLOAD: &[&str] = &[
    "abilities",
    "building",
    "controller",
    "elevation",
    "flag",
    "health",
    "isShooting",
    "mineTile",
    "mounts",
    "payloads",
    "plans",
    "rotation",
    "shield",
    "spawnedByCore",
    "stack",
    "statuses",
    "team",
    "type",
    "updateBuilding",
    "vel",
    "x",
    "y",
];

/// `TargetDummyUnit` layout: `building` after `abilities`.
const UNIT_TETHER: &[&str] = &[
    "abilities",
    "building",
    "controller",
    "elevation",
    "flag",
    "health",
    "isShooting",
    "mineTile",
    "mounts",
    "plans",
    "rotation",
    "shield",
    "spawnedByCore",
    "stack",
    "statuses",
    "team",
    "type",
    "updateBuilding",
    "vel",
    "x",
    "y",
];

/// `TimedKillUnit` layout (`missile`): `lifetime` after `isShooting`, `time`
/// after `team`.
const UNIT_TIMED_KILL: &[&str] = &[
    "abilities",
    "controller",
    "elevation",
    "flag",
    "health",
    "isShooting",
    "lifetime",
    "mineTile",
    "mounts",
    "plans",
    "rotation",
    "shield",
    "spawnedByCore",
    "stack",
    "statuses",
    "team",
    "time",
    "type",
    "updateBuilding",
    "vel",
    "x",
    "y",
];

/// Live vanilla unit classes (generated `EntityMapping` idMap slots, 2026-10-15).
const JAVA_UNIT_CLASSES: &[JavaUnitClass] = &[
    JavaUnitClass {
        class_id: 0,
        revision: 5,
        fields: UNIT_STANDARD,
    }, // alpha
    JavaUnitClass {
        class_id: 2,
        revision: 9,
        fields: UNIT_STANDARD,
    }, // block
    JavaUnitClass {
        class_id: 3,
        revision: 9,
        fields: UNIT_STANDARD,
    }, // flare family
    JavaUnitClass {
        class_id: 4,
        revision: 9,
        fields: UNIT_BASE_ROTATION,
    }, // mace family
    JavaUnitClass {
        class_id: 5,
        revision: 7,
        fields: UNIT_PAYLOAD,
    }, // mega family
    JavaUnitClass {
        class_id: 16,
        revision: 8,
        fields: UNIT_STANDARD,
    }, // mono
    JavaUnitClass {
        class_id: 17,
        revision: 7,
        fields: UNIT_BASE_ROTATION,
    }, // nova
    JavaUnitClass {
        class_id: 18,
        revision: 7,
        fields: UNIT_STANDARD,
    }, // poly
    JavaUnitClass {
        class_id: 19,
        revision: 5,
        fields: UNIT_BASE_ROTATION,
    }, // pulsar
    JavaUnitClass {
        class_id: 20,
        revision: 9,
        fields: UNIT_STANDARD,
    }, // risso family
    JavaUnitClass {
        class_id: 21,
        revision: 8,
        fields: UNIT_STANDARD,
    }, // spiroct
    JavaUnitClass {
        class_id: 23,
        revision: 8,
        fields: UNIT_PAYLOAD,
    }, // quad
    JavaUnitClass {
        class_id: 24,
        revision: 9,
        fields: UNIT_STANDARD,
    }, // corvus family
    JavaUnitClass {
        class_id: 26,
        revision: 7,
        fields: UNIT_PAYLOAD,
    }, // oct
    JavaUnitClass {
        class_id: 29,
        revision: 5,
        fields: UNIT_STANDARD,
    }, // arkyid
    JavaUnitClass {
        class_id: 30,
        revision: 5,
        fields: UNIT_STANDARD,
    }, // beta
    JavaUnitClass {
        class_id: 31,
        revision: 5,
        fields: UNIT_STANDARD,
    }, // gamma
    JavaUnitClass {
        class_id: 32,
        revision: 5,
        fields: UNIT_BASE_ROTATION,
    }, // quasar
    JavaUnitClass {
        class_id: 33,
        revision: 5,
        fields: UNIT_STANDARD,
    }, // toxopid
    JavaUnitClass {
        class_id: 36,
        revision: 3,
        fields: UNIT_TETHER_PAYLOAD,
    }, // manifold family
    JavaUnitClass {
        class_id: 39,
        revision: 3,
        fields: UNIT_TIMED_KILL,
    }, // missile
    JavaUnitClass {
        class_id: 43,
        revision: 2,
        fields: UNIT_STANDARD,
    }, // stell family
    JavaUnitClass {
        class_id: 45,
        revision: 2,
        fields: UNIT_STANDARD,
    }, // elude
    JavaUnitClass {
        class_id: 46,
        revision: 2,
        fields: UNIT_STANDARD,
    }, // latum family
    JavaUnitClass {
        class_id: 49,
        revision: 0,
        fields: UNIT_TETHER,
    }, // dummy
];

/// The layout for one upstream unit class id, if the class is a vanilla unit
/// this build can decode.
pub(crate) fn java_unit_class(class_id: u8) -> Option<&'static JavaUnitClass> {
    JAVA_UNIT_CLASSES
        .iter()
        .find(|class| class.class_id == class_id)
}

/// Whether an upstream class id is one of the decodable entity classes
/// (`LiveEntitySink` dispatch).
pub(crate) fn supports_java_class(class_id: u8) -> bool {
    class_id == JAVA_FIRE_CLASS_ID
        || class_id == JAVA_PUDDLE_CLASS_ID
        || java_unit_class(class_id).is_some()
}

/// Decodes one upstream unit chunk payload (after class id + entity id).
///
/// Returns `Ok(None)` for an unknown class/revision, a field this build cannot
/// consume, or a unit it must drop (non-empty payload stack); the chunk is
/// length-bounded, so a partial read is safe. `UnitComp.afterRead`
/// (`UnitComp.java:585-593`) is applied by the host
/// (`Sim::reset_loaded_controllers`).
pub(crate) fn read_java_unit(
    class_id: u8,
    id: i32,
    r: &mut WireReader,
    content: &ContentRegistry,
) -> IoResult<Option<DecodedUnit>> {
    let Some(class) = java_unit_class(class_id) else {
        return Ok(None);
    };
    let revision = r.us()?;
    if revision != class.revision {
        return Ok(None);
    }
    let mut name: Option<String> = None;
    let mut team: u8 = 0;
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut rotation = 0.0f32;
    let mut health = 0.0f32;
    let mut elevation = 0.0f32;
    let mut shield = 0.0f32;
    let mut spawned_by_core = false;
    let mut stack: Option<(ItemId, i32)> = None;
    let mut statuses: Vec<StatusEntry> = Vec::new();
    let mut controller: Option<AiKind> = None;
    for field in class.fields {
        match *field {
            "abilities" => {
                // `TypeIO.readAbilities` (mutator): `b len` + `f data` each.
                let len = r.ub()? as usize;
                r.skip(len * 4)?;
            }
            "baseRotation" | "elevation" | "health" | "lifetime" | "rotation" | "shield"
            | "time" | "x" | "y" => {
                let value = r.f()?;
                match *field {
                    "elevation" => elevation = value,
                    "health" => health = value,
                    "rotation" => rotation = value,
                    "shield" => shield = value,
                    "x" => x = value,
                    "y" => y = value,
                    _ => {}
                }
            }
            "building" => {
                // `TypeIO.readBuilding`: `i` packed tile pos (-1 = none).
                let _ = r.i()?;
            }
            "controller" => {
                controller = read_java_controller(r)?;
            }
            "flag" => {
                // `f64` (`arc.util.io.Writes.d`).
                let _ = r.d()?;
            }
            "isShooting" | "updateBuilding" => {
                let _ = r.bool()?;
            }
            "spawnedByCore" => {
                spawned_by_core = r.bool()?;
            }
            "mineTile" => {
                // `TypeIO.readTile`: `i` packed tile pos (-1 = none).
                let _ = r.i()?;
            }
            "mounts" => {
                // `TypeIO.readMounts` (mutator): `b len` + (`b state`, `f x`, `f y`).
                let len = r.ub()? as usize;
                r.skip(len * 9)?;
            }
            "payloads" => {
                // `Seq<Payload>`: `i` size + payloads. A non-empty stack is not
                // representable here; drop the unit (length-bounded chunk).
                if r.i()? != 0 {
                    return Ok(None);
                }
            }
            "plans" => {
                // `TypeIO.readPlansQueue`: `i` used (-1 = null) + `readPlan`.
                let _ = crate::io::typeio::codecs::read_plans_queue(r)?;
            }
            "stack" => {
                // `TypeIO.readItems` (mutator): `s item` (-1 = none) + `i amount`.
                let raw = r.s()?;
                let amount = r.i()?;
                if raw >= 0 {
                    stack = content
                        .get_by_id(ContentType::Item, i32::from(raw))
                        .map(|item| (ItemId::new(item.id), amount));
                }
            }
            "statuses" => {
                let Some(decoded) = read_java_statuses(r, content)? else {
                    return Ok(None);
                };
                statuses = decoded;
            }
            "team" => {
                team = r.ub()?;
            }
            "type" => {
                // `Content`: `s` save-side id, resolved through the temporary
                // mapper installed by the content-header region.
                let raw = r.s()?;
                let Some(mapped) = content.get_by_id(ContentType::Unit, i32::from(raw)) else {
                    return Ok(None);
                };
                let Some(def) = content.unit(UnitTypeId::new(mapped.id)) else {
                    return Ok(None);
                };
                name = Some(def.name.clone());
            }
            "vel" => {
                // `TypeIO.readVec2` (mutator): `f x`, `f y`.
                let _ = r.f()?;
                let _ = r.f()?;
            }
            _ => {
                // A field this build has no decoder for: the chunk length
                // bounds the damage, drop the unit.
                return Ok(None);
            }
        }
    }
    let Some(name) = name else {
        return Ok(None);
    };
    Ok(Some(DecodedUnit {
        // The host applies the revision-3 application path
        // (`apply_decoded_entities`): the upstream chunk carries the real
        // `spawnedByCore` flag, so the v1/v2 name fallback must not run.
        revision: 3,
        // `SaveVersion.writeEntity` wrote the real `entity.id()`; the unit
        // chunk restores it (I01 T10).
        saved_id: id,
        name,
        team,
        x,
        y,
        rotation,
        health,
        payloads: Vec::new(),
        spawned_by_core,
        elevation,
        shield,
        stack,
        statuses,
        command: None,
        controller,
        plans: Vec::new(),
    }))
}

/// `TypeIO.readStatus` entries: `s effect`, `f time`, then for a `dynamic`
/// effect a `b` bitset with one `f` per set flag.
fn read_java_statuses(
    r: &mut WireReader,
    content: &ContentRegistry,
) -> IoResult<Option<Vec<StatusEntry>>> {
    let count = r.i()?;
    if !(0..=1024).contains(&count) {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let raw = r.s()?;
        let duration = r.f()?;
        let Some(mapped) = content.get_by_id(ContentType::Status, i32::from(raw)) else {
            return Ok(None);
        };
        let Some(def) = content.status(StatusId::new(mapped.id)) else {
            return Ok(None);
        };
        if def.dynamic {
            let flags = r.ub()?;
            for bit in 0..7u8 {
                if flags & (1 << bit) != 0 {
                    let _ = r.f()?;
                }
            }
        }
        out.push(StatusEntry::new(StatusId::new(mapped.id), duration));
    }
    Ok(Some(out))
}

/// `TypeIO.readController` type byte: only the state-keeping kinds survive a
/// load (`LogicAI`, `CommandAI`; `UnitComp.java:585-593`). Player/formation/
/// assembler/legacy kinds decode to `None` (the host re-derives them).
///
/// `CommandAI` (types 4/6/7/8/9): attack/pos flags, attack ref, command byte
/// (type >= 6), command queue (type >= 7), stances (type 8 single, type 9
/// counted). `Player` (0)/formation (1) carry an `i`; `LogicAI` (3) an `i`.
fn read_java_controller(r: &mut WireReader) -> IoResult<Option<AiKind>> {
    let kind = r.ub()?;
    match kind {
        0 | 1 => {
            let _ = r.i()?;
            Ok(None)
        }
        3 => {
            let _ = r.i()?;
            Ok(Some(AiKind::Logic))
        }
        4 | 6 | 7 | 8 | 9 => {
            let has_attack = r.bool()?;
            let has_pos = r.bool()?;
            if has_pos {
                let _ = r.f()?;
                let _ = r.f()?;
            }
            if has_attack {
                let _ = r.ub()?; // target kind (1 = building)
                let _ = r.i()?; // building pos / unit id
            }
            if kind >= 6 {
                let _ = r.b()?; // unit command id (-1 = none)
            }
            if kind >= 7 {
                let len = r.ub()? as usize;
                for _ in 0..len {
                    match r.ub()? {
                        0 | 1 => {
                            let _ = r.i()?;
                        }
                        2 => {
                            let _ = r.f()?;
                            let _ = r.f()?;
                        }
                        _ => {}
                    }
                }
            }
            if kind == 8 {
                let _ = r.ub()?; // single stance
            } else if kind == 9 {
                let stances = r.ub()? as usize;
                for _ in 0..stances {
                    let _ = r.ub()?;
                }
            }
            Ok(Some(AiKind::Command))
        }
        // AssemblerAI (5) and the legacy/ground replacement kinds: re-derived.
        _ => Ok(None),
    }
}

/// Decodes one upstream `Fire` chunk payload (after class id + entity id).
///
/// Rev 0: `f baseFlammability, s block, f lifetime, f puddleFlammability,
/// i tile, f time, f x, f y`; rev 1: `f lifetime, i tile, f time, f x, f y`
/// (`JA/resources/revisions/FireComp/{0,1}.json`; only the sim fields the
/// port's `FireState` keeps are returned).
pub(crate) fn read_java_fire(r: &mut WireReader) -> IoResult<Option<DecodedFire>> {
    let revision = r.us()?;
    match revision {
        0 => {
            let _ = r.f()?;
            let _ = r.s()?;
            let lifetime = r.f()?;
            let _ = r.f()?;
            let tile = r.i()?;
            let time = r.f()?;
            let _ = r.f()?;
            let _ = r.f()?;
            Ok(java_tile(tile).map(|tile| DecodedFire {
                tile,
                time,
                lifetime,
            }))
        }
        1 => {
            let lifetime = r.f()?;
            let tile = r.i()?;
            let time = r.f()?;
            let _ = r.f()?;
            let _ = r.f()?;
            Ok(java_tile(tile).map(|tile| DecodedFire {
                tile,
                time,
                lifetime,
            }))
        }
        _ => Ok(None),
    }
}

/// Decodes one upstream `Puddle` chunk payload (after class id + entity id).
///
/// Rev 0: `f amount, i generation, s liquid, i tile, f x, f y`; rev 1:
/// `f amount, s liquid, i tile, f x, f y`
/// (`JA/resources/revisions/PuddleComp/{0,1}.json`).
pub(crate) fn read_java_puddle(
    r: &mut WireReader,
    content: &ContentRegistry,
) -> IoResult<Option<DecodedPuddle>> {
    let revision = r.us()?;
    let (amount, raw_liquid, tile) = match revision {
        0 => {
            let amount = r.f()?;
            let _ = r.i()?; // generation
            let liquid = r.s()?;
            let tile = r.i()?;
            let _ = r.f()?;
            let _ = r.f()?;
            (amount, liquid, tile)
        }
        1 => {
            let amount = r.f()?;
            let liquid = r.s()?;
            let tile = r.i()?;
            let _ = r.f()?;
            let _ = r.f()?;
            (amount, liquid, tile)
        }
        _ => return Ok(None),
    };
    let Some(tile) = java_tile(tile) else {
        return Ok(None);
    };
    let Some(mapped) = content.get_by_id(ContentType::Liquid, i32::from(raw_liquid)) else {
        return Ok(None);
    };
    Ok(Some(DecodedPuddle {
        tile,
        liquid: LiquidId::new(mapped.id),
        amount,
    }))
}

/// `Point2` unpack with the `-1` empty-tile convention (`TypeIO.writeTile`).
fn java_tile(pack: i32) -> Option<(i16, i16)> {
    let x = crate::io::typeio::unpack_point2_x(pack);
    let y = crate::io::typeio::unpack_point2_y(pack);
    if x < 0 || y < 0 {
        return None;
    }
    Some((i16::try_from(x).ok()?, i16::try_from(y).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{BlockId, MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::io::wire::WireWriter;

    fn registry() -> ContentRegistry {
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        registry.init().expect("init");
        registry.post_init().expect("post init");
        registry
    }

    fn dagger_id(registry: &ContentRegistry) -> i16 {
        registry.unit_id("dagger").expect("dagger").raw() as i16
    }

    /// H04 T10: a hand-built `MechUnit` (`mace` family, class id 4) revision-9
    /// chunk decodes to a `DecodedUnit` (the field order matches the manifest
    /// and the generated `write` bytecode).
    #[test]
    fn java_unit_chunk_decodes() {
        let registry = registry();
        let mut out = Vec::new();
        {
            let mut w = WireWriter::new(&mut out);
            w.us(9); // revision
            w.ub(0); // abilities
            w.f(0.0); // baseRotation
            // controller: CommandAI (type 9) with no attack/pos/command/queue.
            w.ub(9);
            w.bool(false);
            w.bool(false);
            w.b(-1); // command = null
            w.ub(0); // command queue
            w.ub(0); // stances
            w.f(0.0); // elevation
            w.d(0.0); // flag
            w.f(120.0); // health
            w.bool(false); // isShooting
            w.i(-1); // mineTile
            w.ub(0); // mounts
            w.i(0); // plans
            w.f(90.0); // rotation
            w.f(0.0); // shield
            w.bool(false); // spawnedByCore
            w.s(-1); // stack item: none
            w.i(0); // stack amount
            w.i(0); // statuses
            w.ub(2); // team
            w.s(dagger_id(&registry)); // type
            w.bool(false); // updateBuilding
            w.f(0.0); // vel x
            w.f(0.0); // vel y
            w.f(100.0); // x
            w.f(200.0); // y
        }
        let mut r = WireReader::new(&out);
        let unit = read_java_unit(4, 7, &mut r, &registry)
            .expect("decode")
            .expect("dagger decoded");
        assert_eq!(unit.name, "dagger");
        assert_eq!(unit.saved_id, 7, "chunk id carried (I01 T10)");
        assert_eq!(unit.team, 2);
        assert_eq!((unit.x, unit.y), (100.0, 200.0));
        assert_eq!(unit.rotation, 90.0);
        assert_eq!(unit.health, 120.0);
        assert_eq!(unit.controller, Some(AiKind::Command));
        assert!(!unit.spawned_by_core);
        assert_eq!(r.pos(), out.len(), "chunk fully consumed");
    }

    /// H04 T10: an unknown class or an unsupported revision skips the unit
    /// (the chunk stays length-bounded).
    #[test]
    fn java_unit_unknown_revision_skips() {
        let registry = registry();
        let mut out = Vec::new();
        {
            let mut w = WireWriter::new(&mut out);
            w.us(4); // an old `mace` revision this build does not decode
            w.ub(0); // abilities would follow -- never read
        }
        let mut r = WireReader::new(&out);
        assert!(
            read_java_unit(4, 0, &mut r, &registry)
                .expect("decode")
                .is_none()
        );
        // Unknown class id: skipped before any read.
        let mut r = WireReader::new(&out);
        assert!(
            read_java_unit(1, 0, &mut r, &registry)
                .expect("decode")
                .is_none()
        );
    }

    /// H04 T10: upstream `FireComp` rev 1 and `PuddleComp` rev 1 chunks decode
    /// to the sim fields the port restores.
    #[test]
    fn java_fire_and_puddle_chunks_decode() {
        let registry = registry();
        // Fire: lifetime f, tile i (Point2.pack(13, 14)), time f, x f, y f.
        let mut out = Vec::new();
        {
            let mut w = WireWriter::new(&mut out);
            w.us(1);
            w.f(600.0); // lifetime
            w.i(crate::io::typeio::pack_point2(13, 14));
            w.f(120.0); // time
            w.f(104.0); // x
            w.f(112.0); // y
        }
        let mut r = WireReader::new(&out);
        let fire = read_java_fire(&mut r).expect("decode").expect("fire");
        assert_eq!(fire.tile, (13, 14));
        assert_eq!(fire.lifetime, 600.0);
        assert_eq!(fire.time, 120.0);

        // Puddle: amount f, liquid s, tile i, x f, y f.
        let water = registry.liquid_id("water").expect("water").raw() as i16;
        let mut out = Vec::new();
        {
            let mut w = WireWriter::new(&mut out);
            w.us(1);
            w.f(50.0); // amount
            w.s(water);
            w.i(crate::io::typeio::pack_point2(20, 30));
            w.f(160.0); // x
            w.f(240.0); // y
        }
        let mut r = WireReader::new(&out);
        let puddle = read_java_puddle(&mut r, &registry)
            .expect("decode")
            .expect("puddle");
        assert_eq!(puddle.tile, (20, 30));
        assert_eq!(puddle.amount, 50.0);
        assert_eq!(puddle.liquid, registry.liquid_id("water").expect("water"));

        // An unknown revision skips.
        let mut out = Vec::new();
        {
            let mut w = WireWriter::new(&mut out);
            w.us(9);
        }
        let mut r = WireReader::new(&out);
        assert!(read_java_fire(&mut r).expect("decode").is_none());
    }

    /// H04 T10: the class table stays clear of the native `0xF0..=0xF2` ids
    /// and covers the fixture's alpha class (0).
    #[test]
    fn java_class_table_reserved_against_native_ids() {
        assert!(java_unit_class(0).is_some(), "alpha family");
        assert!(java_unit_class(4).is_some(), "mace family");
        assert!(java_unit_class(5).is_some(), "mega family");
        assert!(JAVA_UNIT_CLASSES.iter().all(|class| class.class_id < 0xF0));
        assert!(supports_java_class(JAVA_FIRE_CLASS_ID));
        assert!(supports_java_class(JAVA_PUDDLE_CLASS_ID));
        assert!(!supports_java_class(6), "buildings are not entity chunks");
        assert!(!supports_java_class(7), "bullets are not imported");
        assert_eq!(BlockId::AIR.raw(), 0);
    }
}
