// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `CrawlComp` segment rotation (plan 11 §3.6).
//!
//! Ported from `core/src/mindustry/entities/comp/CrawlComp.java`: the crawl
//! body trails the movement direction within `segmentMaxRot`. The area scan for
//! solids/deeps/damage and `crawlDust` are plan-06/17 seams (marked).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::unit::comp::CrawlComp;

/// `CrawlComp.update`: rotate the crawl segment toward the movement direction.
pub fn update_crawl(world: &mut World, entity: Entity, delta: (f32, f32), max_rot: f32) {
    let speed = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    if speed <= 0.001 {
        return;
    }
    let target = delta.1.atan2(delta.0).to_degrees();
    if let Some(mut crawl) = world.get_mut::<CrawlComp>(entity) {
        crawl.segment_rot = rotate_toward(crawl.segment_rot, target, max_rot.max(1.0));
    }
    // TODO(plan 06): area scan for solids/deeps and per-tile crawl damage/slowdown.
}

fn rotate_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let mut diff = (target - current) % 360.0;
    if diff > 180.0 {
        diff -= 360.0;
    } else if diff < -180.0 {
        diff += 360.0;
    }
    current + diff.clamp(-max_step, max_step)
}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::CrawlComp;
    use crate::entities::comp::unit::movement::update_kinematics;

    #[test]
    fn crawl_segment_rotates_toward_motion() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("latum", 0, 64.0, 64.0, 0.0).expect("latum");
        for _ in 0..60 {
            update_kinematics(
                &mut harness.build.world,
                &harness.build.content,
                unit,
                (0.0, 2.0),
            );
        }
        let crawl = harness.build.world.get::<CrawlComp>(unit).unwrap();
        // Segment rotated toward +y (90 degrees).
        assert!(crawl.segment_rot > 0.0);
    }
}
