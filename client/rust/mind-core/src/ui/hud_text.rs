// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/HudFragment.java (the
//         `table.labelWrap(() -> …)` status builder + `IntFormat` definitions).

//! HUD status/objective text composition (plan 14 §3.5/M2).
//!
//! Godot-free: takes a fixture [`HudStatus`] (the read-only sim surface the
//! GDScript `HudFragment` binds to) plus the plan-03 [`Bundle`]/[`Iconc`], and
//! produces the exact status text upstream composes. Kept separate from
//! `MindHud` so `cargo test -p mind-core` and `mind-headless ui hud-text` can
//! assert it without Godot.

use crate::assets::bundle::Bundle;
use crate::assets::icons::Iconc;
use crate::ui::text::{format_icons, format_time};

/// Read-only inputs for one status-text composition.
#[derive(Debug, Clone, Default)]
pub struct HudStatus {
    /// `state.rules.mission` (overrides everything when non-empty).
    pub mission: String,
    /// Qualified, non-hidden objective texts (`obj.text()`), in order.
    pub objectives: Vec<String>,
    /// Ticks until unit-factory activation, when the player team cannot activate
    /// factories yet (`None` = active / not applicable).
    pub unit_activation_remaining: Option<f32>,
    /// `state.rules.waves`.
    pub waves: bool,
    /// `state.rules.attackMode`.
    pub attack_mode: bool,
    /// Enemy core count for the attack-mode branch.
    pub enemy_cores: i32,
    /// `state.afterGameOver && state.isCampaign()`.
    pub after_game_over_campaign: bool,
    /// `state.isCampaign()`.
    pub campaign: bool,
    /// `state.rules.winWave`.
    pub win_wave: i32,
    /// `state.wave`.
    pub wave: i32,
    /// `state.enemies`.
    pub enemies: i32,
    /// `state.rules.waveTimer`.
    pub wave_timer: bool,
    /// `logic.isWaitingWave()`.
    pub waiting_wave: bool,
    /// `state.wavetime` in ticks.
    pub wavetime_ticks: f32,
}

/// Composes the status label text (`HudFragment` status builder).
pub fn status_text(status: &HudStatus, bundle: &Bundle, iconc: &Iconc) -> String {
    let mut out = String::new();

    // Mission overrides everything.
    if !status.mission.is_empty() {
        out.push_str(&status.mission);
        return out;
    }

    // Objectives override mission when at least one produced text.
    if !status.objectives.is_empty() {
        let mut first = true;
        for objective in &status.objectives {
            if objective.is_empty() {
                continue;
            }
            if !first {
                out.push_str("\n\n[white]");
            }
            out.push_str(&format_icons(objective, iconc));
            first = false;
        }
        if !out.is_empty() {
            return out;
        }
    }

    if let Some(remaining) = status.unit_activation_remaining {
        let remaining = remaining.max(0.0);
        let value = format!("[accent]{}", format_time(remaining));
        let msg = bundle.format("rules.unitfactoryactivation.objective", &[value.as_str()]);
        out.push_str("[lightgray]");
        out.push_str(&msg);
        out.push_str("[white]\n");
    }

    // Attack mode (no waves): enemy core counter.
    if !status.waves && status.attack_mode {
        let sum = status.enemy_cores.max(1);
        out.push_str(&enemy_core_text(sum, bundle));
        return out;
    }

    // No status after game over in campaign.
    if status.after_game_over_campaign {
        return out;
    }

    if !status.waves && status.campaign {
        out.push_str("[lightgray]");
        out.push_str(bundle.get("sector.curcapture"));
    }

    if !status.waves {
        return out;
    }

    out.push_str(&wave_text(status.wave, status.win_wave, bundle));
    out.push('\n');

    if status.enemies > 0 {
        let key = if status.enemies == 1 {
            "wave.enemy"
        } else {
            "wave.enemies"
        };
        out.push_str(&bundle.format(key, &[&status.enemies.to_string()]));
        out.push('\n');
    }

    if status.wave_timer {
        if status.waiting_wave {
            out.push_str(bundle.get("wave.waveInProgress"));
        } else {
            out.push_str(&waiting_time(status.wavetime_ticks));
        }
    } else if status.enemies == 0 {
        out.push_str(bundle.get("waiting"));
    }

    out
}

/// Wave text: `wave.cap` when `winWave > 1 && winWave >= wave`, else `wave`.
pub fn wave_text(wave: i32, win_wave: i32, bundle: &Bundle) -> String {
    if win_wave > 1 && win_wave >= wave {
        bundle.format("wave.cap", &[&wave.to_string(), &win_wave.to_string()])
    } else {
        bundle.format("wave", &[&wave.to_string()])
    }
}

/// Attack-mode enemy-core text (`wave.enemycore(s)`), always with the count arg.
pub fn enemy_core_text(sum: i32, bundle: &Bundle) -> String {
    if sum > 1 {
        bundle.format("wave.enemycores", &[&sum.to_string()])
    } else {
        bundle.format("wave.enemycore", &[&sum.to_string()])
    }
}

/// `wave.waiting` timer: `m:ss` when minutes > 0, else `s`.
pub fn waiting_time(ticks: f32) -> String {
    let seconds = (ticks / 60.0) as i32;
    let m = seconds / 60;
    let s = seconds % 60;
    if m > 0 {
        format!("{m}:{s:02}")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::bundle::parse_properties;

    fn bundle() -> Bundle {
        Bundle::from_layers(vec![parse_properties(
            "wave=Wave {0}\nwave.cap=Wave {0} / {1}\nwave.enemy=Enemy {0}\nwave.enemies=Enemies {0}\nwave.enemycore=Enemy Core {0}\nwave.enemycores=Enemy Cores {0}\nwave.waiting=Next wave in {0}\nwave.waveInProgress=Wave in progress\nwaiting=Waiting\nsector.curcapture=Capturing sector\nrules.unitfactoryactivation.objective=Activate a unit factory in {0}\n",
        )])
    }

    fn iconc() -> Iconc {
        Iconc::from_properties("63743=spawn|block-spawn-ui\n")
    }

    #[test]
    fn wave_variants() {
        let status = HudStatus {
            waves: true,
            wave: 5,
            win_wave: 0,
            enemies: 0,
            wave_timer: true,
            wavetime_ticks: 300.0,
            ..Default::default()
        };
        let text = status_text(&status, &bundle(), &iconc());
        assert_eq!(text, "Wave 5\n5");
    }

    #[test]
    fn win_wave_cap() {
        let bundle = bundle();
        assert_eq!(wave_text(3, 10, &bundle), "Wave 3 / 10");
        assert_eq!(wave_text(10, 10, &bundle), "Wave 10 / 10");
        assert_eq!(wave_text(11, 10, &bundle), "Wave 11");
        assert_eq!(wave_text(1, 1, &bundle), "Wave 1");
    }

    #[test]
    fn enemy_counts() {
        let status = HudStatus {
            waves: true,
            wave: 2,
            enemies: 1,
            wave_timer: false,
            ..Default::default()
        };
        assert_eq!(
            status_text(&status, &bundle(), &iconc()),
            "Wave 2\nEnemy 1\n"
        );

        let status = HudStatus {
            waves: true,
            wave: 2,
            enemies: 3,
            wave_timer: false,
            ..Default::default()
        };
        assert_eq!(
            status_text(&status, &bundle(), &iconc()),
            "Wave 2\nEnemies 3\n"
        );

        // `waiting` only shows when the timer is off AND there are no enemies.
        let idle = HudStatus {
            waves: true,
            wave: 2,
            enemies: 0,
            wave_timer: false,
            ..Default::default()
        };
        assert_eq!(status_text(&idle, &bundle(), &iconc()), "Wave 2\nWaiting");

        let attack = HudStatus {
            waves: false,
            attack_mode: true,
            enemy_cores: 2,
            ..Default::default()
        };
        assert_eq!(status_text(&attack, &bundle(), &iconc()), "Enemy Cores 2");
        let one = HudStatus {
            waves: false,
            attack_mode: true,
            enemy_cores: 1,
            ..Default::default()
        };
        assert_eq!(status_text(&one, &bundle(), &iconc()), "Enemy Core 1");
    }

    #[test]
    fn waiting_timer() {
        assert_eq!(waiting_time(0.0), "0");
        assert_eq!(waiting_time(300.0), "5");
        assert_eq!(waiting_time(3900.0), "1:05");
        assert_eq!(waiting_time(3600.0), "1:00");
        let status = HudStatus {
            waves: true,
            wave: 1,
            wave_timer: true,
            wavetime_ticks: 600.0,
            ..Default::default()
        };
        assert!(status_text(&status, &bundle(), &iconc()).ends_with("10"));

        let waiting = HudStatus {
            waves: true,
            wave: 4,
            waiting_wave: true,
            wave_timer: true,
            ..Default::default()
        };
        let text = status_text(&waiting, &bundle(), &iconc());
        assert!(text.contains("Wave in progress"));
    }

    #[test]
    fn objectives_compose() {
        let status = HudStatus {
            mission: String::from("Mission X"),
            objectives: vec![String::from("do :spawn:")],
            ..Default::default()
        };
        // Mission wins when non-empty.
        assert_eq!(status_text(&status, &bundle(), &iconc()), "Mission X");

        let status = HudStatus {
            objectives: vec![String::from("first"), String::from("second")],
            ..Default::default()
        };
        assert_eq!(
            status_text(&status, &bundle(), &iconc()),
            "first\n\n[white]second"
        );
    }

    #[test]
    fn unit_activation_countdown() {
        let status = HudStatus {
            unit_activation_remaining: Some(3600.0),
            ..Default::default()
        };
        let text = status_text(&status, &bundle(), &iconc());
        assert!(text.starts_with("[lightgray]"));
        assert!(text.contains("[accent]1:00"));
    }
}
