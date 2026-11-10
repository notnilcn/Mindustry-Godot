// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! HUD state: the game-stats row, campaign-active flag and the `get_hud_state` payload.
//! Upstream: ui/fragments/HudFragment.java; ui/dialogs/GameOverDialog.java

use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// `GameOverDialog` stats rows (`state.stats`, G03 T16):
    /// `{"wave", "unitsCreated", "enemiesDestroyed", "built", "destroyed",
    /// "deconstructed", "waves"}` in Java label order.
    #[func]
    pub fn game_stats_json(&self) -> GString {
        // `GameStats::context_json` owns the key names + the `rules.waves` gate
        // (`GameOverDialog.java:78-84`); the dialog's `STAT_ROWS` mirror them.
        let value = self.session.stats.context_json(self.session.rules.waves);
        value_gstring(&value)
    }

    /// `state.isCampaign()`: a live campaign sector is loaded (the same
    /// condition `get_hud_state` reports as `campaign`). The J/N toggles read
    /// it (`DesktopInput.java:446-461`).
    #[func]
    pub fn campaign_active(&self) -> bool {
        self.session.is_campaign()
    }

    /// Live HUD/status fields (`HudFragment` read surface, gap3 C-10). Appended
    /// for `MindHud`; additive and read-only. Includes the campaign session's
    /// wave/enemy/timer/goal state the other getters do not expose.
    #[func]
    pub fn get_hud_state(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        dict.set(&key("campaign"), &self.session.is_campaign().to_variant());
        dict.set(&key("waves"), &self.session.rules.waves.to_variant());
        dict.set(
            &key("waveSending"),
            &self.session.rules.wave_sending.to_variant(),
        );
        dict.set(&key("attack"), &self.session.rules.attack_mode.to_variant());
        dict.set(&key("wave"), &self.session.wave.to_variant());
        dict.set(&key("enemies"), &self.session.enemies.to_variant());
        dict.set(&key("wavetime"), &self.session.wavetime.to_variant());
        dict.set(&key("winWave"), &self.session.rules.win_wave.to_variant());
        dict.set(
            &key("waveTimer"),
            &self.session.rules.wave_timer.to_variant(),
        );
        dict.set(
            &key("waitEnemies"),
            &self.session.rules.wait_enemies.to_variant(),
        );
        // `Logic.isWaitingWave` (`Logic.java:621-624`); G03 reads it for
        // `wave.waveInProgress`.
        dict.set(
            &key("waitingWave"),
            &mind_core::game::play::is_waiting_wave(&self.session).to_variant(),
        );
        dict.set(&key("gameOver"), &self.session.game_over.to_variant());
        dict.set(
            &key("afterGameOver"),
            &self.session.after_game_over.to_variant(),
        );
        dict.set(&key("won"), &self.session.won.to_variant());
        dict.set(&key("isSpawning"), &self.session.is_spawning.to_variant());
        dict.set(
            &key("hasCore"),
            &(self.session.player_core_count() > 0).to_variant(),
        );
        dict.set(
            &key("enemyCores"),
            &(self.session.wave_core_count() as i64).to_variant(),
        );
        if let Some((planet, sector)) = self.session.sector {
            let planet_name = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.planet(planet))
                .map(|planet| planet.name.clone())
                .unwrap_or_default();
            let preset_name = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(planet, sector))
                .and_then(|sector| sector.preset_name.clone());
            let was_captured = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(planet, sector))
                .map(|sector| sector.info.info.was_captured)
                .unwrap_or(false);
            dict.set(&key("planet"), &planet_name.as_str().to_variant());
            dict.set(&key("sector"), &(sector as i64).to_variant());
            dict.set(
                &key("sectorName"),
                &preset_name.unwrap_or(planet_name).as_str().to_variant(),
            );
            dict.set(&key("wasCaptured"), &was_captured.to_variant());
        }
        dict
    }
}
