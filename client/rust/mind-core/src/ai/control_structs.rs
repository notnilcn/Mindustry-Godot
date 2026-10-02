// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ControlPathfinder` bit-packed value types (plan 11 §3.7/§6.4, frozen).
//!
//! Ported from the `@Struct` declarations at the end of
//! `core/src/mindustry/ai/ControlPathfinder.java`:
//! `FieldIndexStruct{ int pos; @StructField(8) costId; @StructField(8) team }`,
//! `IntraEdgeStruct{ @StructField(8) dir; @StructField(8) portal; float cost }`
//! and `NodeIndexStruct{ @StructField(22) cluster; @StructField(2) dir;
//! @StructField(8) portal }`.
//!
//! `@Struct` packs fields from the least-significant bit in declaration order,
//! so these layouts are a save/A* ABI: do not reorder or resize them.

/// `FieldIndex` — packed `(pos, cost_id, team)` long key.
///
/// Layout: `pos` bits `0..32`, `cost_id` bits `32..40`, `team` bits `40..48`.
pub struct FieldIndex;

impl FieldIndex {
    /// Packs the key (`FieldIndex.get(pos, costId, team)`).
    pub fn get(pos: i32, cost_id: i32, team: i32) -> i64 {
        (pos as u32 as i64) | (((cost_id as i64) & 0xff) << 32) | (((team as i64) & 0xff) << 40)
    }

    /// Unpacks the world array position.
    pub fn pos(value: i64) -> i32 {
        value as u32 as i32
    }

    /// Unpacks the path-cost id.
    pub fn cost_id(value: i64) -> i32 {
        ((value >> 32) & 0xff) as i32
    }

    /// Unpacks the team id.
    pub fn team(value: i64) -> i32 {
        ((value >> 40) & 0xff) as i32
    }
}

/// `IntraEdge` — packed `(dir, portal, cost)` long.
///
/// Layout: `dir` bits `0..8`, `portal` bits `8..16`, `cost` f32 bits `16..48`.
pub struct IntraEdge;

impl IntraEdge {
    /// Packs an intra-cluster edge (`IntraEdge.get(dir, portal, cost)`).
    pub fn get(dir: i32, portal: i32, cost: f32) -> i64 {
        ((dir as i64) & 0xff)
            | (((portal as i64) & 0xff) << 8)
            | (((cost.to_bits() as i64) & 0xffff_ffff) << 16)
    }

    /// Unpacks the edge direction (portal index on the shared side).
    pub fn dir(value: i64) -> i32 {
        (value & 0xff) as i32
    }

    /// Unpacks the portal index.
    pub fn portal(value: i64) -> i32 {
        ((value >> 8) & 0xff) as i32
    }

    /// Unpacks the traversal cost.
    pub fn cost(value: i64) -> f32 {
        f32::from_bits((value >> 16) as u32)
    }
}

/// `NodeIndex` — packed `(cluster, dir, portal)` int.
///
/// Layout: `cluster` bits `0..22`, `dir` bits `22..24`, `portal` bits `24..32`.
pub struct NodeIndex;

impl NodeIndex {
    /// Packs a cluster node address (`NodeIndex.get(cluster, dir, portal)`).
    pub fn get(cluster: i32, dir: i32, portal: i32) -> i32 {
        (cluster & 0x003f_ffff) | ((dir & 0x3) << 22) | ((portal & 0xff) << 24)
    }

    /// Unpacks the cluster index.
    pub fn cluster(value: i32) -> i32 {
        value & 0x003f_ffff
    }

    /// Unpacks the side direction.
    pub fn dir(value: i32) -> i32 {
        (value >> 22) & 0x3
    }

    /// Unpacks the portal index.
    pub fn portal(value: i32) -> i32 {
        (value >> 24) & 0xff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_index_round_trips() {
        let key = FieldIndex::get(123_456, 5, 200);
        assert_eq!(FieldIndex::pos(key), 123_456);
        assert_eq!(FieldIndex::cost_id(key), 5);
        assert_eq!(FieldIndex::team(key), 200);
        // Negative positions (world array packing) survive.
        let key = FieldIndex::get(-1, 0, 0);
        assert_eq!(FieldIndex::pos(key), -1);
    }

    #[test]
    fn intra_edge_round_trips_and_keeps_cost_bits() {
        for cost in [0.0f32, 1.0, 1.5, -3.25, f32::MAX] {
            let edge = IntraEdge::get(2, 17, cost);
            assert_eq!(IntraEdge::dir(edge), 2);
            assert_eq!(IntraEdge::portal(edge), 17);
            assert_eq!(IntraEdge::cost(edge).to_bits(), cost.to_bits());
        }
    }

    #[test]
    fn node_index_packing_is_frozen() {
        let node = NodeIndex::get(0x2f_1234 & 0x003f_ffff, 2, 255);
        assert_eq!(NodeIndex::cluster(node), 0x2f_1234 & 0x003f_ffff);
        assert_eq!(NodeIndex::dir(node), 2);
        assert_eq!(NodeIndex::portal(node), 255);
        // Max cluster fits in 22 bits; dir in 2; portal in 8.
        let s = NodeIndex::get(0x003f_ffff, 3, 255);
        assert_eq!(NodeIndex::cluster(s), 0x003f_ffff);
        assert_eq!(NodeIndex::dir(s), 3);
        assert_eq!(NodeIndex::portal(s), 255);
    }
}
