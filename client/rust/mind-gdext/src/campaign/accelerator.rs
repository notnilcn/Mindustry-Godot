// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Interplanetary Accelerator cutscene: the launch state machine, FX cues and the
//! pending-launch record.
//! Upstream: world/blocks/campaign/Accelerator.java

use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// The planet-launch picker's confirm for the accelerator at tile `(x, y)`
    /// (`AcceleratorBuild.buildConfiguration` callback, Accelerator.java:
    /// 262-284): the sim half (`MindSimHost.accelerator_launch`: consume,
    /// carry, buffer, `progress = 0`), `renderer.showLaunch(this)` (the
    /// camera cutscene) with `beginLaunch`'s `coreLaunchConstruct` +
    /// charge/construct sounds (:338,356-357), then after `launchDuration() -
    /// 6` ticks the sector switch in `pump_accelerator_launch`. Returns false
    /// when `canLaunch()` no longer holds.
    #[func]
    pub fn accelerator_launch_to(&mut self, x: i32, y: i32, planet: GString, sector: i32) -> bool {
        if self.pending_accelerator.is_some() || sector < 0 {
            return false;
        }
        let Some(mut host) = self.host() else {
            return false;
        };
        let resources = host.bind_mut().accelerator_launch(x, y).to_string();
        if resources.is_empty() {
            return false;
        }
        let world = host
            .bind()
            .building_center_world(x, y)
            .try_to::<Vector2>()
            .map(|center| (center.x, center.y))
            .unwrap_or((x as f32 * 8.0, y as f32 * 8.0));
        if let Some(mut camera) = wiring::optional(&self.camera) {
            camera.bind_mut().show_accelerator_launch(world.0, world.1);
        }
        // `Fx.coreLaunchConstruct.at(x, y, launchBlock.size)` (core-nucleus
        // size 5) + `chargeSound`/`constructSound` (Accelerator.java:338,
        // 356-357).
        self.accelerator_fx("coreLaunchConstruct", world, 5.0);
        self.accelerator_sound("acceleratorCharge", world);
        self.accelerator_sound("acceleratorConstruct", world);
        self.pending_accelerator = Some(PendingAcceleratorLaunch {
            tile: (x, y),
            world,
            planet: planet.to_string(),
            sector,
            resources,
            core: "core-nucleus".to_owned(),
            elapsed: 0.0,
            charged: false,
            next_dust_ring: 0,
            white_fade: false,
            // A cutscene RNG seeded from the launch site: a replay of the same
            // launch draws the same bolts/dust.
            rand: mind_core::math::ArcRand::new(
                ((world.0 as i64 as u64) << 32) ^ (world.1 as i64 as u64),
            ),
            bolt_seed: (world.0 as i64).wrapping_mul(73_856_093)
                ^ (world.1 as i64).wrapping_mul(19_349_663),
            extra_bolts: 0,
        });
        log::info!("accelerator launch to `{planet}:{sector}`");
        true
    }

    /// Whether an accelerator launch is in flight (the planet-launch flow).
    #[func]
    pub fn accelerator_launch_pending(&self) -> bool {
        self.pending_accelerator.is_some()
    }
}

impl MindCampaign {
    fn accelerator_fx(&self, name: &str, world: (f32, f32), rotation: f32) {
        if let Some(mut fx) = wiring::optional(&self.fx) {
            fx.bind_mut()
                .spawn_effect(GString::from(name), world.0, world.1, rotation);
        }
    }

    fn accelerator_sound(&self, name: &str, world: (f32, f32)) {
        if let Some(mut audio) = wiring::optional(&self.audio) {
            audio.bind_mut().play_at_ui(
                GString::from(name),
                f64::from(world.0),
                f64::from(world.1),
                1.0,
                1.0,
            );
        }
    }

    /// Spawns the `Fx.lightning` polylines of a launch bolt burst
    /// (`Accelerator.java:364-367,423-429`; `Lightning.create`'s view event).
    ///
    /// The sim-side branches (damage 40) need a `CombatCtx`; the host renderer
    /// has no sim handle, so this is the cutscene's visual half (residual noted
    /// on PAR-0602: a `MindSimHost::spawn_lightning` seam would cover damage).
    fn accelerator_lightning(&self, bolts: &[mind_core::render::accelerator::LaunchBolt]) {
        let Some(fx) = wiring::optional(&self.fx) else {
            return;
        };
        let bus = fx.bind().bus_handle();
        // `lightningColor = Pal.accent` (Accelerator.java:62).
        let color = mind_core::content::Rgba::new(
            mind_core::render::drawf::pal::ACCENT[0],
            mind_core::render::drawf::pal::ACCENT[1],
            mind_core::render::drawf::pal::ACCENT[2],
            mind_core::render::drawf::pal::ACCENT[3],
        );
        for bolt in bolts {
            if bolt.points.len() < 2 {
                continue;
            }
            let points: smallvec::SmallVec<[(f32, f32); 8]> = bolt.points.iter().copied().collect();
            bus.push(mind_core::fx::FxEvent::EffectWithData {
                effect: mind_core::content::registries::fx_meta::EffectRef::Named(
                    mind_core::content::EffectId::LIGHTNING,
                ),
                x: bolt.x,
                y: bolt.y,
                rotation: bolt.angle,
                color,
                data: mind_core::fx::EffectData::Positions(points),
            });
        }
    }

    /// Spawns one `beginLaunch` dust ring's `Fx.coreLandDust` rays
    /// (`Accelerator.java:373-384`): the ray snaps to its tile
    /// (`Tile.worldx()` = tile corner) and is tinted by
    /// `t.floor().mapColor * (1.7 + range(0.15))`.
    fn accelerator_dust(&self, rays: &[mind_core::render::accelerator::DustRay]) {
        if rays.is_empty() {
            return;
        }
        let Some(registry) = self.registry.as_ref() else {
            return;
        };
        let tile_size = mind_core::config::TILESIZE as f32;
        // Resolve every tile's floor colour before borrowing `MindFx`.
        let floors: Vec<Option<mind_core::content::Rgba>> = match self.host() {
            Some(host) => {
                let guard = host.bind();
                let grid = guard.grid();
                rays.iter()
                    .map(|ray| {
                        let tx = i16::try_from((ray.x / tile_size).floor() as i32).ok()?;
                        let ty = i16::try_from((ray.y / tile_size).floor() as i32).ok()?;
                        grid.floor_at(mind_core::world::TilePos::new(tx, ty))
                            .and_then(|floor| registry.block(floor))
                            .and_then(|def| def.map_color)
                    })
                    .collect()
            }
            None => vec![None; rays.len()],
        };
        let Some(mut fx) = wiring::optional(&self.fx) else {
            return;
        };
        let mut bound = fx.bind_mut();
        for (ray, floor) in rays.iter().zip(floors) {
            let Some(floor) = floor else {
                continue;
            };
            let color = mind_core::content::Rgba::new(
                (floor.r * ray.brightness).clamp(0.0, 1.0),
                (floor.g * ray.brightness).clamp(0.0, 1.0),
                (floor.b * ray.brightness).clamp(0.0, 1.0),
                floor.a,
            );
            // `t.worldx()`/`t.worldy()`: the tile corner, not a centre.
            let x = (ray.x / tile_size).floor() * tile_size;
            let y = (ray.y / tile_size).floor() * tile_size;
            bound.spawn_view_effect(
                mind_core::content::EffectId::CORE_LAND_DUST,
                x,
                y,
                ray.rotation,
                color,
            );
        }
    }

    /// `beginLaunch`'s cover image (`Accelerator.java:343-354`): the HUD's
    /// white cover fades in over `duration` seconds at the end of the launch.
    fn accelerator_white_fade(&self, duration: f64) {
        let Some(ui_root) = wiring::optional(&self.ui_root) else {
            return;
        };
        let fade = {
            let root = ui_root.bind();
            root.fade_in_node()
        };
        let Some(mut fade) = fade else {
            return;
        };
        if fade.has_method("start_white") {
            let _ = fade.call("start_white", &[duration.to_variant()]);
        }
    }

    /// Advances an accelerator launch: the `Time.run(chargeDuration)` burst
    /// (`coreLaunchConstruct` + `launchAccelerator` + `launchSound`,
    /// Accelerator.java:358-362), `beginLaunch`'s 20 `Lightning.create` bolts
    /// and 13 `coreLandDust` rings (:364-387), `updateLaunch`'s extra bolt
    /// (:423-429), the white cover image (:343-354), and the
    /// `Time.runTask(launchDuration() - 6f)` switch (:273-283):
    /// `launching = false`, `unlockedOnLand`, `universe.clearLoadoutInfo()`,
    /// `updateLaunchResources(resources)`, `updateLoadout((CoreBlock)launchBlock)`,
    /// `control.playSector(sector)`.
    pub(super) fn pump_accelerator_launch(&mut self, delta: f64) {
        let Some(mut pending) = self.pending_accelerator.take() else {
            return;
        };
        let sequence = accelerator_sequence();
        // Arc `Time.delta` (clamped like the camera rig's frame delta).
        let frame_ticks = ((delta * 60.0) as f32).clamp(0.0001, 4.0);
        pending.elapsed += frame_ticks;
        if !pending.charged && pending.elapsed >= sequence.charge_tick() {
            pending.charged = true;
            self.accelerator_fx("coreLaunchConstruct", pending.world, 5.0);
            self.accelerator_fx("launchAccelerator", pending.world, 0.0);
            self.accelerator_sound("acceleratorLaunch", pending.world);
            if let Some(mut camera) = wiring::optional(&self.camera) {
                // `Effect.shake(10f, 14f, this)`.
                camera.bind_mut().apply_shake_event(10.0, 14.0);
            }
            // `for(int i = 0; i < launchLightning; i++)
            //  Lightning.create(team, lightningColor, lightningDamage, ...)`
            // (:364-367). The sim branch is host-owned (residual); the view
            // half is the `Fx.lightning` polyline the renderer draws.
            let bolts = mind_core::render::accelerator::launch_bolts(
                pending.world.0,
                pending.world.1,
                &mut pending.rand,
                pending.bolt_seed,
            );
            self.accelerator_lightning(&bolts);
        }
        // `Time.run(i * 2f)` dust rings (`Accelerator.java:369-386`): ring `i`
        // spawns at `chargeDuration + i * 2` ticks.
        while pending.next_dust_ring < ACCELERATOR_DUST_RINGS
            && pending.elapsed >= dust_ring_at(&sequence, pending.next_dust_ring)
        {
            let ring = pending.next_dust_ring;
            let rays = mind_core::render::accelerator::dust_rays(
                &sequence,
                5,
                ring,
                pending.world.0,
                pending.world.1,
                &mut pending.rand,
            );
            self.accelerator_dust(&rays);
            pending.next_dust_ring += 1;
        }
        // `updateLaunch` (`Accelerator.java:423-429`): after the charge, each
        // tick rolls `chanceDelta(lightningLaunchChance * pow3In(chargeFout))`.
        if pending.elapsed > sequence.charge_tick() {
            let fin_in = (1.0 - pending.elapsed / sequence.total()).clamp(0.0, 1.0);
            let phase = mind_core::render::accelerator::LaunchPhase::from_land_time_in(
                fin_in,
                sequence.launch_duration,
                sequence.charge_duration,
            );
            let chance = sequence.lightning_chance(&phase) * f64::from(frame_ticks);
            if chance > 0.0 && f64::from(pending.rand.next_float()) < chance {
                pending.extra_bolts = pending.extra_bolts.saturating_add(1);
                let bolt = mind_core::render::accelerator::launch_bolt(
                    pending.world.0,
                    pending.world.1,
                    &mut pending.rand,
                    pending
                        .bolt_seed
                        .wrapping_add(1000)
                        .wrapping_add(i64::from(pending.extra_bolts)),
                );
                self.accelerator_lightning(&[bolt]);
            }
        }
        // `beginLaunch`'s cover image (`Accelerator.java:343-354`): delayed
        // `(launchDuration() - 30) / 60` s, then faded in over `30 / 60` s.
        if !pending.white_fade && pending.elapsed >= sequence.total() - 30.0 {
            pending.white_fade = true;
            self.accelerator_white_fade(30.0 / 60.0);
        }
        if pending.elapsed < sequence.switch_tick() {
            self.pending_accelerator = Some(pending);
            return;
        }
        if let Some(mut host) = self.host() {
            host.bind_mut()
                .accelerator_end_launch(pending.tile.0, pending.tile.1);
        }
        let stacks: Vec<mind_core::io::json::content_serde::JsonItemStack> =
            serde_json::from_str(&pending.resources).unwrap_or_default();
        let amounts: Option<Vec<i32>> = self.registry.as_ref().map(|registry| {
            let mut amounts = vec![0; registry.items().len()];
            for stack in &stacks {
                if let Some(id) = stack
                    .item
                    .as_deref()
                    .and_then(|name| registry.item_id(name))
                {
                    amounts[id.index()] += stack.amount;
                }
            }
            amounts
        });
        let core = pending.core.clone();
        let _ = self.with_campaign_mut(|campaign, settings| {
            campaign.universe.clear_loadout_info(settings);
            if let Some(amounts) = amounts {
                let _ = campaign.universe.update_launch_resources(settings, amounts);
            }
            // `updateLoadout(CoreBlock)`: an unnamed schematic -> empty file.
            campaign.universe.update_loadout(settings, &core, None);
        });
        self.persist();
        // `Control.playSector` (`Control.java:437-441`): write the sector being
        // left before its world is replaced by the destination load
        // (CG-SECTOR-02).
        self.save_outgoing_slot();
        // `control.playSector(sector)` = the planet dialog's `_launch` order:
        // prime the generator, stage the loadout, load the world, then start.
        self.prime_sector_load(GString::from(pending.planet.as_str()), pending.sector);
        if let Some(mut host) = self.host() {
            host.bind_mut().set_pending_launch_core(
                GString::from(core.as_str()),
                GString::from(pending.resources.as_str()),
            );
            host.bind_mut()
                .load_sector(GString::from(pending.planet.as_str()), pending.sector);
        }
        let payload = serde_json::json!({
            "launch_core": core,
            "stacks": serde_json::from_str::<serde_json::Value>(&pending.resources)
                .unwrap_or(serde_json::Value::Array(Vec::new())),
        });
        let started = self.start_sector_with_loadout(
            GString::from(pending.planet.as_str()),
            pending.sector,
            value_gstring(&payload),
        );
        log::info!(
            "accelerator launch landed on `{}:{}`: {started}",
            pending.planet,
            pending.sector
        );
    }
}

/// An accelerator launch in flight (`AcceleratorBuild.buildConfiguration`'s
/// callback, Accelerator.java:262-284).
pub(super) struct PendingAcceleratorLaunch {
    /// Accelerator anchor tile.
    tile: (i32, i32),
    /// Accelerator world centre (fx/sound origin).
    world: (f32, f32),
    /// Destination planet name and sector id.
    planet: String,
    sector: i32,
    /// Carried resources `[{item, amount}]`.
    resources: String,
    /// The launch core (`launchBlock`).
    core: String,
    /// Ticks since the launch began (Arc `Time.delta`, real frames).
    elapsed: f32,
    /// The `chargeDuration` burst (Accelerator.java:358-362) already played.
    charged: bool,
    /// Next `beginLaunch` dust ring to spawn (`Time.run(i * 2f)`,
    /// Accelerator.java:369-386).
    next_dust_ring: usize,
    /// The white cover image started (Accelerator.java:343-354).
    white_fade: bool,
    /// Launch-cutscene RNG (bolt angles/lengths, dust chances).
    rand: mind_core::math::ArcRand,
    /// `Lightning.create` polyline seed of the charge burst.
    bolt_seed: i64,
    /// `updateLaunch` bolts spawned so far (varies their polyline seeds).
    extra_bolts: u32,
}

/// `AcceleratorBuild`'s launch timeline (`launchDuration` 120 + `chargeDuration`
/// 220, Accelerator.java:51-52) shared with the render pass.
fn accelerator_sequence() -> mind_core::world::blocks::campaign::accelerator::LaunchSequence {
    mind_core::world::blocks::campaign::accelerator::LaunchSequence::vanilla()
}

/// `beginLaunch`'s 13 dust rings (`for(int i = 0; i < 13; i++)`,
/// Accelerator.java:370).
const ACCELERATOR_DUST_RINGS: usize = mind_core::world::blocks::campaign::accelerator::DUST_RINGS;

/// Launch tick at which `beginLaunch` dust ring `ring` spawns:
/// `chargeDuration + i * 2` (`Time.run(i * 2f)`, Accelerator.java:370-371).
fn dust_ring_at(
    sequence: &mind_core::world::blocks::campaign::accelerator::LaunchSequence,
    ring: usize,
) -> f32 {
    sequence.charge_tick() + sequence.dust_ring(5, ring).at
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PAR-0602: the accelerator cutscene timeline is the shared
    /// `LaunchSequence` (charge 220, switch 334, total 340) and the 13 dust
    /// rings straddle the charge in `i * 2` tick steps.
    #[test]
    fn accelerator_cutscene_timeline_matches_the_java_schedule() {
        let sequence = accelerator_sequence();
        assert_eq!(sequence.total(), 340.0);
        assert_eq!(sequence.charge_tick(), 220.0);
        assert_eq!(sequence.switch_tick(), 334.0);
        assert_eq!(ACCELERATOR_DUST_RINGS, 13);

        assert_eq!(
            dust_ring_at(&sequence, 0),
            220.0,
            "first ring at the charge"
        );
        assert_eq!(dust_ring_at(&sequence, 1), 222.0);
        assert_eq!(dust_ring_at(&sequence, 12), 244.0, "last ring at i * 2");
        // Every ring is due before the sector switch fires.
        assert!(dust_ring_at(&sequence, 12) < sequence.switch_tick());

        // The ring geometry is the block-size based `Accelerator` one:
        // radius 2.5 + 1 + 144 = 147.5, rays ceil(147.5 * 2PI / 6) = 155.
        let last = sequence.dust_ring(5, 12);
        assert!((last.radius - (5.0 / 2.0 + 1.0 + 12.0 * 12.0)).abs() < 1e-4);
        assert_eq!(last.rays, 155, "ceil(147.5 * 2PI / 6)");
        assert!((last.chance - (0.7 - 0.24)).abs() < 1e-9);
    }

    /// PAR-0602: the charge-tick burst is 20 bolts, each with a >= 2-point
    /// `Fx.lightning` polyline (length 5-25 -> 2-12 points), and the
    /// `updateLaunch` roll only becomes possible after the charge.
    #[test]
    fn accelerator_burst_builds_twenty_lightning_polylines() {
        let sequence = accelerator_sequence();
        let mut rand = mind_core::math::ArcRand::new(0x5EED);
        let bolts = mind_core::render::accelerator::launch_bolts(64.0, 64.0, &mut rand, 3);
        assert_eq!(bolts.len(), 20);
        for bolt in &bolts {
            assert!(bolt.points.len() >= 2, "length {}", bolt.length);
            assert!(bolt.points.len() <= 12);
        }
        // `updateLaunch` only rolls after the charge (`rawTime > charge`).
        let charge = mind_core::render::accelerator::LaunchPhase::from_land_time_in(
            (340.0 - 220.0) / 340.0,
            120.0,
            220.0,
        );
        assert_eq!(sequence.lightning_chance(&charge), 0.0);
        let mid = mind_core::render::accelerator::LaunchPhase::from_land_time_in(
            (340.0 - 280.0) / 340.0,
            120.0,
            220.0,
        );
        assert!(sequence.lightning_chance(&mid) > 0.0);
    }
}
