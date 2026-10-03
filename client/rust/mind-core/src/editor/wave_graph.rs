// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Wave-graph numeric series (plan 19 §3.9/§2.3.7; `editor/WaveGraph.java`).
//!
//! `WaveGraph` computes a per-wave `values[wave][unitTypeId]` table plus the
//! `max`/`maxTotal`/`maxHealth` normalizers and draws it in Arc. The port keeps
//! the numeric series in `mind-core` (deterministic, headless-testable) and lets
//! GDScript draw it (`mind-gdext`/scenes). `SpawnGroup` comes from plan 12.

use crate::game::spawn_group::SpawnGroup;

/// The three graph modes (`WaveGraph.Mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaveGraphMode {
    /// Per-unit spawn counts.
    Counts,
    /// Total spawns per wave.
    Totals,
    /// Total spawned unit health per wave.
    Health,
}

impl WaveGraphMode {
    /// All modes in upstream order.
    pub const ALL: [WaveGraphMode; 3] = [
        WaveGraphMode::Counts,
        WaveGraphMode::Totals,
        WaveGraphMode::Health,
    ];

    /// Bundle key suffix (`@wavemode.<name>`).
    pub fn name(self) -> &'static str {
        match self {
            WaveGraphMode::Counts => "counts",
            WaveGraphMode::Totals => "totals",
            WaveGraphMode::Health => "health",
        }
    }
}

/// One unit type participating in the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct WaveGraphUnit {
    /// Content id (`UnitType.id`).
    pub id: u16,
    /// Content name (bundle/emoji lookup in the view).
    pub name: String,
    /// Unit health (`UnitType.health`).
    pub health: f32,
}

/// The computed series (`WaveGraph.rebuild` state).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WaveGraphData {
    /// First wave index (`from`).
    pub from: i32,
    /// Last wave index (`to`).
    pub to: i32,
    /// Participating unit types sorted by id (`used.orderedItems().sort()`).
    pub units: Vec<WaveGraphUnit>,
    /// `series[wave-from][unit-index]` spawn counts.
    pub series: Vec<Vec<i32>>,
    /// Total spawn count per wave.
    pub totals: Vec<i32>,
    /// Total spawned health per wave.
    pub health: Vec<f32>,
    /// Maximum single-unit-type count across the window (`max`).
    pub max: i32,
    /// Maximum total count across the window (`maxTotal`).
    pub max_total: i32,
    /// Maximum total health across the window (`maxHealth`).
    pub max_health: f32,
}

impl WaveGraphData {
    /// Computes the series for `groups` over waves `from..=to`.
    ///
    /// `resolve` maps a spawn-group unit name to `(id, health)`; unknown units are
    /// skipped exactly like an unresolved `UnitType`.
    pub fn compute<F>(groups: &[SpawnGroup], from: i32, to: i32, resolve: F) -> Self
    where
        F: Fn(&str) -> Option<(u16, f32)>,
    {
        let to = to.max(from);
        let waves = (to - from + 1) as usize;

        // Collect every used unit, sorted by id (upstream `used.orderedItems().sort()`).
        let mut used: Vec<WaveGraphUnit> = Vec::new();
        for group in groups {
            if let Some((id, health)) = resolve(&group.unit)
                && !used.iter().any(|u| u.id == id)
            {
                used.push(WaveGraphUnit {
                    id,
                    name: group.unit.clone(),
                    health,
                });
            }
        }
        used.sort_by_key(|unit| unit.id);

        let mut series = vec![vec![0i32; used.len()]; waves];
        let mut totals = vec![0i32; waves];
        let mut health = vec![0f32; waves];
        let mut max = 1i32;
        let mut max_total = 1i32;
        let mut max_health = 1f32;

        for (wave_index, wave) in (from..=to).enumerate() {
            let mut sum = 0i32;
            let mut health_sum = 0f32;
            for group in groups {
                let Some(unit_index) = used.iter().position(|u| u.name == group.unit) else {
                    continue;
                };
                let spawned = group.get_spawned(wave);
                if spawned <= 0 {
                    continue;
                }
                series[wave_index][unit_index] += spawned;
                max = max.max(series[wave_index][unit_index]);
                health_sum += spawned as f32 * used[unit_index].health;
                sum += spawned;
            }
            totals[wave_index] = sum;
            health[wave_index] = health_sum;
            max_total = max_total.max(sum);
            max_health = max_health.max(health_sum);
        }

        Self {
            from,
            to,
            units: used,
            series,
            totals,
            health,
            max,
            max_total,
            max_health,
        }
    }

    /// Number of wave columns.
    pub fn len(&self) -> usize {
        self.series.len()
    }

    /// Whether the window is empty.
    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }

    /// The y-axis maximum for a mode (`max`/`maxTotal`/`maxHealth`).
    pub fn mode_max(&self, mode: WaveGraphMode) -> f32 {
        match mode {
            WaveGraphMode::Counts => self.max as f32,
            WaveGraphMode::Totals => self.max_total as f32,
            WaveGraphMode::Health => self.max_health,
        }
    }

    /// The drawn y-axis upper bound (`WaveGraph.nextStep(max)`).
    pub fn max_y(&self, mode: WaveGraphMode) -> i32 {
        next_step(self.mode_max(mode))
    }

    /// The per-wave value for one unit type in `Counts` mode.
    pub fn unit_series(&self, column: usize, unit_index: usize) -> i32 {
        self.series
            .get(column)
            .and_then(|row| row.get(unit_index))
            .copied()
            .unwrap_or(0)
    }
}

/// `WaveGraph.nextStep`: the next `1/2/5/10`-style axis tick ≥ `value`.
pub fn next_step(value: f32) -> i32 {
    let value = value.max(1.0);
    let mut order = 1i32;
    while (order as f32) < value {
        if order * 2 > value as i32 {
            return order * 2;
        }
        if order * 5 > value as i32 {
            return order * 5;
        }
        if order * 10 > value as i32 {
            return order * 10;
        }
        order *= 10;
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(name: &str) -> SpawnGroup {
        SpawnGroup::new(name)
    }

    fn resolve(name: &str) -> Option<(u16, f32)> {
        match name {
            "dagger" => Some((1, 150.0)),
            "mace" => Some((2, 600.0)),
            _ => None,
        }
    }

    #[test]
    fn series_and_normalizers() {
        let mut d = unit("dagger");
        d.unit_amount = 2;
        d.unit_scaling = 2.0;
        d.max = 40;
        let mut m = unit("mace");
        m.begin = 2;
        m.unit_amount = 1;
        let groups = vec![d, m];
        let data = WaveGraphData::compute(&groups, 0, 5, resolve);
        assert_eq!(data.units.len(), 2);
        assert_eq!(data.units[0].name, "dagger");
        // wave 0: dagger 2; wave 2: dagger 3 + mace 1; wave 4: dagger 4 + mace 1.
        assert_eq!(data.series[0], vec![2, 0]);
        assert_eq!(data.series[2], vec![3, 1]);
        assert_eq!(data.series[4], vec![4, 1]);
        assert_eq!(data.totals[2], 4);
        assert_eq!(data.max, 4);
        assert_eq!(data.max_total, 5);
        // health at wave 4: 4*150 + 1*600 = 1200.
        assert_eq!(data.health[4], 1200.0);
        assert_eq!(data.max_health, 1200.0);
    }

    #[test]
    fn unknown_units_are_skipped() {
        let groups = vec![unit("ghost")];
        let data = WaveGraphData::compute(&groups, 0, 3, resolve);
        assert!(data.units.is_empty());
        assert!(data.totals.iter().all(|t| *t == 0));
    }

    #[test]
    fn next_step_sequence() {
        assert_eq!(next_step(1.0), 1);
        assert_eq!(next_step(3.0), 5);
        assert_eq!(next_step(6.0), 10);
        assert_eq!(next_step(11.0), 20);
        assert_eq!(next_step(1200.0), 2000);
    }

    #[test]
    fn series_is_deterministic() {
        let groups = vec![unit("dagger"), unit("mace")];
        let a = WaveGraphData::compute(&groups, 0, 10, resolve);
        let b = WaveGraphData::compute(&groups, 0, 10, resolve);
        assert_eq!(a, b);
    }
}
