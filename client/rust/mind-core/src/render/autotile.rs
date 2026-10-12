// SPDX-License-Identifier: GPL-3.0-only
//! `Conveyor.draw` belt sprite-frame math (view-only).
//!
//! Ported from `core/src/mindustry/world/blocks/distribution/Conveyor.java`
//! (`ConveyorBuild.draw`): the 4-frame scrolling animation index. Belts animate
//! only while enabled and unclogged (`clogHeat <= 0.5`); the editor and
//! disabled buildings pin frame 0.

/// `Conveyor.draw` frame index:
/// `(int)((Time.time * speed * 8f * timeScale * efficiency) % 4)`, forced to
/// `0` while disabled or clogged (`clogHeat > 0.5`) —
/// `Conveyor.java:144-145`. `sim_time` is the simulation tick counter (Java
/// `Time.time`); `timeScale` is always 1 in the campaign.
pub fn conveyor_frame(
    sim_time: f32,
    speed: f32,
    efficiency: f32,
    enabled: bool,
    clog_heat: f32,
) -> i32 {
    if !enabled || clog_heat > 0.5 {
        return 0;
    }
    ((sim_time * speed * 8.0 * efficiency) % 4.0) as i32
}

#[cfg(test)]
mod tests {
    use super::conveyor_frame;

    const SPEED: f32 = 0.035;

    /// Every 4 frames the belt completes one texture cycle; the index advances
    /// as `(ticks * speed * 8) % 4` (`Conveyor.java:144`).
    #[test]
    fn frame_advances_on_the_java_cadence() {
        assert_eq!(conveyor_frame(0.0, SPEED, 1.0, true, 0.0), 0);
        assert_eq!(conveyor_frame(25.0, SPEED, 1.0, true, 0.0), 3);
        assert_eq!(conveyor_frame(50.0, SPEED, 1.0, true, 0.0), 2);
        assert_eq!(conveyor_frame(75.0, SPEED, 1.0, true, 0.0), 1);
        assert_eq!(conveyor_frame(100.0, SPEED, 1.0, true, 0.0), 0);
    }

    /// `efficiency` is a multiplier on the phase, not on the index.
    #[test]
    fn efficiency_scales_the_phase() {
        assert_eq!(conveyor_frame(100.0, SPEED, 0.5, true, 0.0), 2);
        assert_eq!(conveyor_frame(100.0, SPEED, 0.25, true, 0.0), 3);
    }

    /// Disabled and clogged belts hold frame 0 (`enabled && clogHeat <= 0.5`).
    #[test]
    fn disabled_or_clogged_pins_frame_zero() {
        assert_eq!(conveyor_frame(25.0, SPEED, 1.0, false, 0.0), 0);
        assert_eq!(conveyor_frame(25.0, SPEED, 1.0, true, 0.6), 0);
        assert_eq!(conveyor_frame(25.0, SPEED, 1.0, true, 0.5), 3);
    }
}
