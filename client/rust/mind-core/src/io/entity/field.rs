// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Field wire types for the entity codec (`EntityIO` field read/write calls).
//!
//! Every field type that can appear in an entity def implements [`IoField`]:
//! a manifest type name + byte size (for the revision manifests) and the
//! big-endian read/write pair. Ported from the Arc `Writes`/`Reads` calls the
//! upstream generated code emitted per field type. Additional types (modules,
//! stacks, `Option<T>` with an explicit default) are added here as plans
//! 05/07/11 introduce their components.

use super::super::IoResult;
use super::super::wire::{WireReader, WireWriter};
use crate::content::{
    BlockId, BulletId, EffectId, ItemId, LiquidId, Rgba, StatusId, TeamEntryId, UnitCommandId,
    UnitStanceId, UnitTypeId, WeatherId,
};
use crate::ecs::TeamId;
use crate::world::TilePos;

/// One entity-codec field type.
pub trait IoField: Sized {
    /// Manifest type name (`revisions/<def>/<N>.json` `type`).
    const TYPE_NAME: &'static str;
    /// Fixed byte size, or `-1` for variable-length/complex values.
    const SIZE: i32;
    /// Writes the field value.
    fn write_field(&self, w: &mut WireWriter) -> IoResult<()>;
    /// Reads the field value into `self`.
    fn read_field(&mut self, r: &mut WireReader) -> IoResult<()>;
}

macro_rules! io_field_primitive {
    ($ty:ty, $name:literal, $size:expr, $write:ident, $read:ident) => {
        impl IoField for $ty {
            const TYPE_NAME: &'static str = $name;
            const SIZE: i32 = $size;

            fn write_field(&self, w: &mut WireWriter) -> IoResult<()> {
                w.$write(*self);
                Ok(())
            }

            fn read_field(&mut self, r: &mut WireReader) -> IoResult<()> {
                *self = r.$read()?;
                Ok(())
            }
        }
    };
}

io_field_primitive!(u8, "u8", 1, ub, ub);
io_field_primitive!(i8, "i8", 1, b, b);
io_field_primitive!(u16, "u16", 2, us, us);
io_field_primitive!(i16, "i16", 2, s, s);
io_field_primitive!(u32, "u32", 4, u, u);
io_field_primitive!(i32, "i32", 4, i, i);
io_field_primitive!(i64, "i64", 8, l, l);
io_field_primitive!(f32, "f32", 4, f, f);
io_field_primitive!(f64, "f64", 8, d, d);
io_field_primitive!(bool, "bool", 1, bool, bool);

macro_rules! io_field_content_id {
    ($ty:ty, $name:literal) => {
        impl IoField for $ty {
            const TYPE_NAME: &'static str = $name;
            const SIZE: i32 = 2;

            fn write_field(&self, w: &mut WireWriter) -> IoResult<()> {
                w.us(self.raw());
                Ok(())
            }

            fn read_field(&mut self, r: &mut WireReader) -> IoResult<()> {
                *self = <$ty>::new(r.us()?);
                Ok(())
            }
        }
    };
}

io_field_content_id!(BlockId, "BlockId");
io_field_content_id!(ItemId, "ItemId");
io_field_content_id!(LiquidId, "LiquidId");
io_field_content_id!(StatusId, "StatusId");
io_field_content_id!(BulletId, "BulletId");
io_field_content_id!(UnitTypeId, "UnitTypeId");
io_field_content_id!(WeatherId, "WeatherId");
io_field_content_id!(UnitCommandId, "UnitCommandId");
io_field_content_id!(UnitStanceId, "UnitStanceId");
io_field_content_id!(TeamEntryId, "TeamEntryId");

impl IoField for EffectId {
    const TYPE_NAME: &'static str = "EffectId";
    const SIZE: i32 = 2;

    fn write_field(&self, w: &mut WireWriter) -> IoResult<()> {
        w.us(self.raw());
        Ok(())
    }

    fn read_field(&mut self, r: &mut WireReader) -> IoResult<()> {
        *self = EffectId(r.us()?);
        Ok(())
    }
}

impl IoField for TilePos {
    const TYPE_NAME: &'static str = "TilePos";
    const SIZE: i32 = 4;

    fn write_field(&self, w: &mut WireWriter) -> IoResult<()> {
        w.s(self.0);
        w.s(self.1);
        Ok(())
    }

    fn read_field(&mut self, r: &mut WireReader) -> IoResult<()> {
        *self = TilePos::new(r.s()?, r.s()?);
        Ok(())
    }
}

impl IoField for TeamId {
    const TYPE_NAME: &'static str = "TeamId";
    const SIZE: i32 = 1;

    fn write_field(&self, w: &mut WireWriter) -> IoResult<()> {
        w.ub(self.0);
        Ok(())
    }

    fn read_field(&mut self, r: &mut WireReader) -> IoResult<()> {
        *self = TeamId(r.ub()?);
        Ok(())
    }
}

impl IoField for Rgba {
    const TYPE_NAME: &'static str = "Rgba";
    const SIZE: i32 = 4;

    fn write_field(&self, w: &mut WireWriter) -> IoResult<()> {
        w.u(self.to_rgba8888());
        Ok(())
    }

    fn read_field(&mut self, r: &mut WireReader) -> IoResult<()> {
        *self = Rgba::from_rgba8888(r.u()?);
        Ok(())
    }
}

impl IoField for String {
    const TYPE_NAME: &'static str = "String";
    const SIZE: i32 = -1;

    fn write_field(&self, w: &mut WireWriter) -> IoResult<()> {
        w.str(self)
    }

    fn read_field(&mut self, r: &mut WireReader) -> IoResult<()> {
        *self = r.str()?;
        Ok(())
    }
}

impl IoField for Vec<u8> {
    const TYPE_NAME: &'static str = "Vec<u8>";
    const SIZE: i32 = -1;

    fn write_field(&self, w: &mut WireWriter) -> IoResult<()> {
        crate::io::typeio::codecs::write_bytes(w, self)
    }

    fn read_field(&mut self, r: &mut WireReader) -> IoResult<()> {
        *self = crate::io::typeio::codecs::read_bytes(r)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_roundtrips_and_sizes() {
        let mut buf = Vec::new();
        {
            let mut w = WireWriter::new(&mut buf);
            5u8.write_field(&mut w).unwrap();
            (-3i16).write_field(&mut w).unwrap();
            7.5f32.write_field(&mut w).unwrap();
            true.write_field(&mut w).unwrap();
            TilePos::new(-4, 9).write_field(&mut w).unwrap();
            BlockId::STONE_WALL.write_field(&mut w).unwrap();
            TeamId::SHARDED.write_field(&mut w).unwrap();
            "héllo".to_owned().write_field(&mut w).unwrap();
            vec![1u8, 2].write_field(&mut w).unwrap();
        }
        let mut r = WireReader::new(&buf);
        let mut u = 0u8;
        u.read_field(&mut r).unwrap();
        assert_eq!(u, 5);
        let mut i = 0i16;
        i.read_field(&mut r).unwrap();
        assert_eq!(i, -3);
        let mut f = 0.0f32;
        f.read_field(&mut r).unwrap();
        assert_eq!(f, 7.5);
        let mut b = false;
        b.read_field(&mut r).unwrap();
        assert!(b);
        let mut pos = TilePos::new(0, 0);
        pos.read_field(&mut r).unwrap();
        assert_eq!(pos, TilePos::new(-4, 9));
        let mut block = BlockId::AIR;
        block.read_field(&mut r).unwrap();
        assert_eq!(block, BlockId::STONE_WALL);
        let mut team = TeamId(9);
        team.read_field(&mut r).unwrap();
        assert_eq!(team, TeamId::SHARDED);
        let mut text = String::new();
        text.read_field(&mut r).unwrap();
        assert_eq!(text, "héllo");
        let mut bytes = Vec::new();
        bytes.read_field(&mut r).unwrap();
        assert_eq!(bytes, vec![1u8, 2]);
        assert_eq!(r.remaining(), 0);

        assert_eq!(u8::SIZE, 1);
        assert_eq!(TilePos::SIZE, 4);
        assert_eq!(String::SIZE, -1);
        assert_eq!(BlockId::TYPE_NAME, "BlockId");
    }
}
