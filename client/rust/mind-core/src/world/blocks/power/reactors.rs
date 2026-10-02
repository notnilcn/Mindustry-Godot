// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Distinctive generator/reaction formulas
//! (`{ThermalGenerator,SolarGenerator,ImpactReactor,NuclearReactor,
//! VariableReactor,HeaterGenerator}.java`).
//!
//! These are pure functions over the generator state so the reactor math is
//! testable without the ECS; behaviors call them with the matching
//! [`GeneratorConfig`] knobs. Plan 09 owns the trigger/conditions; plan 10/17 own
//! the explosion/damage effects.

/// Arc `Mathf.approachDelta(from, to, speed)` with an explicit `delta`.
pub fn approach_delta(from: f32, to: f32, speed: f32, delta: f32) -> f32 {
    if from < to {
        (from + speed * delta).min(to)
    } else {
        (from - speed * delta).max(to)
    }
}

/// `ThermalGenerator.updateTile`: `productionEfficiency = sum + attribute.env()`.
pub fn thermal_production_efficiency(sum: f32, attribute_env: f32) -> f32 {
    sum + attribute_env
}

/// `SolarGenerator.updateTile`.
pub fn solar_production_efficiency(
    enabled: bool,
    solar_multiplier: f32,
    light_env: f32,
    lighting: bool,
    ambient_light_alpha: f32,
) -> f32 {
    if !enabled {
        return 0.0;
    }
    let light = if lighting {
        1.0 - ambient_light_alpha
    } else {
        1.0
    };
    solar_multiplier * (light_env + light).max(0.0)
}

/// `ImpactReactorBuild.updateTile` warmup ramp.
pub fn impact_warmup_step(
    warmup: f32,
    efficiency: f32,
    power_status: f32,
    warmup_speed: f32,
    time_scale: f32,
    delta: f32,
) -> f32 {
    if efficiency >= 0.9999 && power_status >= 0.99 {
        let next = warmup + (1.0 - warmup) * (warmup_speed * time_scale * delta).min(1.0);
        if (next - 1.0).abs() < 0.001 {
            1.0
        } else {
            next
        }
    } else {
        warmup + (0.0 - warmup) * (0.01 * delta).min(1.0)
    }
}

/// `ImpactReactorBuild`: `productionEfficiency = warmup^5`.
pub fn impact_production_efficiency(warmup: f32) -> f32 {
    warmup.powi(5)
}

/// `NuclearReactor` coolant removal: `maxUsed = min(available, heat/coolantPower)`,
/// then `heat -= maxUsed * coolantPower`. Returns `(heat, used)`.
pub fn nuclear_coolant_removal(heat: f32, coolant_power: f32, available: f32) -> (f32, f32) {
    if heat <= 0.0 || coolant_power <= 0.0 {
        return (heat, 0.0);
    }
    let max_used = available.min(heat / coolant_power);
    (heat - max_used * coolant_power, max_used)
}

/// `NuclearReactorBuild.updateTile` heat accrual while fueled.
pub fn nuclear_heat_gain(fullness: f32, heating: f32, delta: f32) -> f32 {
    fullness * heating * delta.min(4.0)
}

/// `NuclearReactorBuild.heatFrac` (`HeatBlock`).
pub fn nuclear_heat_frac(heat_progress: f32, heat_output: f32) -> f32 {
    if heat_output > 0.0 {
        heat_progress / heat_output
    } else {
        0.0
    }
}

/// `VariableReactorBuild.updateEfficiencyMultiplier`. Returns
/// `(instability, efficiency)`.
pub fn variable_reactor_step(
    instability: f32,
    efficiency: f32,
    heat: f32,
    max_heat: f32,
    unstable_speed: f32,
    delta: f32,
) -> (f32, f32) {
    let target = if max_heat > 0.0 {
        (heat / max_heat).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let efficiency_met = if target == 0.0 {
        1.0
    } else {
        (efficiency / target).clamp(0.0, 1.0)
    };
    let met = efficiency_met >= 0.99999;
    let next = if met {
        approach_delta(instability, 0.0, 0.5, delta)
    } else {
        approach_delta(
            instability,
            1.0,
            unstable_speed * (1.0 - efficiency_met),
            delta,
        )
    };
    (next, efficiency * target)
}

/// `VariableReactorBuild.updateTile`: kill at instability `>= 1`.
pub fn variable_reactor_should_kill(instability: f32) -> bool {
    instability >= 1.0
}

/// `HeaterGeneratorBuild.updateTile`: heat approaches `heatOutput * efficiency`.
pub fn heater_heat_step(
    heat: f32,
    heat_output: f32,
    efficiency: f32,
    warmup_rate: f32,
    delta: f32,
) -> f32 {
    approach_delta(heat, heat_output * efficiency, warmup_rate, delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solar_matches_lighting_rule() {
        assert_eq!(solar_production_efficiency(true, 1.0, 0.0, false, 0.0), 1.0);
        assert_eq!(solar_production_efficiency(true, 1.0, 0.0, true, 0.5), 0.5);
        assert_eq!(
            solar_production_efficiency(false, 1.0, 1.0, false, 0.0),
            0.0
        );
    }

    #[test]
    fn impact_warmup_power_five() {
        assert_eq!(impact_production_efficiency(0.5), 0.03125);
        assert_eq!(impact_production_efficiency(1.0), 1.0);
    }

    #[test]
    fn nuclear_coolant_removal_is_exact() {
        // heat 1.0, coolantPower 0.5 -> up to 2.0 coolant used; 1.0 available.
        let (heat, used) = nuclear_coolant_removal(1.0, 0.5, 1.0);
        assert!((heat - 0.5).abs() < f32::EPSILON);
        assert_eq!(used, 1.0);
        // No coolant: unchanged.
        assert_eq!(nuclear_coolant_removal(1.0, 0.5, 0.0), (1.0, 0.0));
    }

    #[test]
    fn variable_reactor_instability_approaches_one() {
        let (instability, _eff) =
            variable_reactor_step(0.0, 0.0, 100.0, 100.0, 1.0 / 60.0 / 3.0, 1.0);
        assert!(instability > 0.0 && instability < 1.0);
        assert!(variable_reactor_should_kill(1.0));
    }

    #[test]
    fn heater_ramps_to_output() {
        let heat = heater_heat_step(0.0, 10.0, 1.0, 0.15, 1.0);
        assert!((heat - 0.15).abs() < f32::EPSILON);
        let heat = heater_heat_step(10.0, 10.0, 1.0, 0.15, 1.0);
        assert_eq!(heat, 10.0);
    }
}
