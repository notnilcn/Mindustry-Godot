// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Fires/puddles, the combat context guard, delayed explosions and the runtime
//! checksum.
//! Upstream: core/src/mindustry/core/Logic.java (updateEntities)

use super::*;

impl Sim {
    /// `Groups.fires`/`Groups.puddles` update half of `Logic.updateEntities`:
    /// advances every live fire and puddle one tick against the authoritative
    /// tile grid (`Sim.grid`; the runtime snapshot is not on this path).
    ///
    /// `water_attr` is the active weathers' water attribute
    /// (`weather_water_attr`; `0.0` with no weather).
    pub fn update_fires_and_puddles(&mut self) {
        let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() else {
            return;
        };
        let fx = runtime.fx.clone();
        // `state.envAttrs.get(Attribute.water)` from the active weathers
        // (PAR-0127); `0.0` when no weather is running.
        let water_attr = crate::game::runtime::weather_water_attr(&self.ecs.0, &runtime.content);
        let fireballs = crate::combat::fires::update_fires(
            &mut self.ecs.0,
            &mut self.grid.tiles,
            &runtime.content,
            &mut runtime.rng,
            fx.as_ref(),
            water_attr,
        );
        crate::combat::puddles::update_puddles(
            &mut self.ecs.0,
            &mut self.grid.tiles,
            &runtime.content,
            &mut runtime.rng,
            fx.as_ref(),
        );
        self.ecs.0.insert_non_send(runtime);
        // `FireComp.update` fireballs (`Bullets.fireball.createNet`, team
        // derelict, random angle, type damage) plus the puddle-reaction
        // fireballs collected by `Puddles.reactPuddle`.
        let pending_puddle_fireballs = self
            .ecs
            .0
            .get_resource_mut::<crate::combat::puddles::PendingFireballs>()
            .map(|mut pending| std::mem::take(&mut pending.0))
            .unwrap_or_default();
        if !fireballs.is_empty() || !pending_puddle_fireballs.is_empty() {
            self.with_combat_ctx(|ctx| {
                for (x, y) in fireballs.into_iter().chain(pending_puddle_fireballs) {
                    let angle = ctx
                        .rng
                        .range(crate::determinism::RngStream::Sim, 0.0, 360.0);
                    let _ = ctx.spawn(&crate::combat::bullet::BulletSpawn {
                        def: crate::content::registries::bullets::FIREBALL,
                        team: 0,
                        x,
                        y,
                        angle,
                        damage: -1.0,
                        ..crate::combat::bullet::BulletSpawn::default()
                    });
                }
            });
        }
        // H09 T7: due `Damage.dynamicExplosion` delayed actions (lightning,
        // fireballs, staggered waves).
        self.drain_delayed_explosions();
    }

    /// Runs `f` with a live [`crate::combat::bullet::CombatCtx`] (unit runtime
    /// + audio sink).
    ///
    /// Bullets spawned through the context join `UnitRuntime.bullets`; `None`
    /// without a unit runtime.
    pub(crate) fn with_combat_ctx<R>(
        &mut self,
        f: impl FnOnce(&mut crate::combat::bullet::CombatCtx<'_>) -> R,
    ) -> Option<R> {
        let mut runtime = self.ecs.0.remove_non_send::<UnitRuntime>()?;
        let audio = self
            .ecs
            .0
            .remove_resource::<crate::audio::AudioSinkRes>()
            .unwrap_or_else(crate::audio::AudioSinkRes::noop);
        let mut spawned: Vec<Entity> = Vec::new();
        let result = {
            let UnitRuntime {
                content,
                grid,
                rng,
                fx,
                seq,
                ..
            } = &mut runtime;
            let mut ctx = crate::combat::bullet::CombatCtx {
                world: &mut self.ecs.0,
                content,
                grid,
                rng,
                fx: fx.as_ref(),
                audio: &audio,
                seq,
                spawned: &mut spawned,
            };
            f(&mut ctx)
        };
        self.ecs.0.insert_resource(audio);
        runtime.bullets_created += spawned.len() as u64;
        runtime.bullets.extend(spawned);
        self.ecs.0.insert_non_send(runtime);
        Some(result)
    }

    /// Drains due `DelayedExplosionActions` (the `Time.run` delays of
    /// `Damage.dynamicExplosion`; H09 T7). Entries count down one per tick and
    /// run in insertion order.
    pub(super) fn drain_delayed_explosions(&mut self) {
        use crate::combat::damage::explosion::{DelayedAction, DelayedExplosionActions};
        let due = {
            let Some(mut actions) = self.ecs.0.get_resource_mut::<DelayedExplosionActions>() else {
                return;
            };
            let mut due: Vec<DelayedAction> = Vec::new();
            let mut index = 0;
            while index < actions.0.len() {
                actions.0[index].0 -= 1.0;
                if actions.0[index].0 <= 0.0 {
                    due.push(actions.0.remove(index).1);
                } else {
                    index += 1;
                }
            }
            due
        };
        for action in due {
            self.with_combat_ctx(|ctx| {
                crate::combat::damage::explosion::run_delayed_action(ctx, &action);
            });
        }
    }

    /// Deterministic digest of the live unit runtime; empty when not installed.
    pub fn unit_runtime_checksum(&self) -> Checksum {
        match self.ecs.0.get_non_send::<UnitRuntime>() {
            Some(runtime) => runtime.checksum(&self.ecs.0),
            None => Checksummer::new().finish(),
        }
    }
}
