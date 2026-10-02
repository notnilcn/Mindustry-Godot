// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! View capacity constants shared by `mind-core` benches and `mind-gdext`
//! pools (plan 17 §6.5).

/// Fixed effect-state pool capacity.
pub const EFFECT_POOL_CAPACITY: usize = 4096;
/// Fixed decal pool capacity (deviation #6: capped, oldest dropped).
pub const DECAL_CAPACITY: usize = 1024;
/// Delayed-spawn ring capacity.
pub const DELAYED_SPAWN_CAPACITY: usize = 256;
/// Default trail point length for engine trails.
pub const DEFAULT_TRAIL_LENGTH: usize = 20;
/// Decal default lifetime (`Effect.decal`).
pub const DECAL_LIFETIME: f32 = 3600.0;

/// `Effect.decal`/`DecalComp` fade start (plan 17 §3.10).
pub const DECAL_FADE_START: f32 = 0.98;
/// Decal cap policy marker (documented deviation).
pub const DECAL_CAP: usize = DECAL_CAPACITY;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacities_are_the_locked_values() {
        assert_eq!(EFFECT_POOL_CAPACITY, 4096);
        assert_eq!(DECAL_CAPACITY, 1024);
        assert_eq!(DELAYED_SPAWN_CAPACITY, 256);
    }
}
