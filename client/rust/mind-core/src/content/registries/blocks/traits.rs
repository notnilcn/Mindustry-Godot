// SPDX-License-Identifier: GPL-3.0-only

//! `Content`, `Mappable` and `Unlockable` implementations for `BlockDef`.
//! Upstream: core/src/mindustry/world/Block.java (`init`/`postInit`)

use super::*;

use crate::content::category::Category;
use crate::content::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use crate::content::id::BlockId;
use crate::content::{ContentError, ContentType};

use super::{BlockDef, BlockGroup, TILE_SIZE};

impl Content for BlockDef {
    const TYPE: ContentType = ContentType::Block;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = BlockId::new(id);
    }

    fn minfo(&self) -> &ModContentInfo {
        &self.minfo
    }

    fn minfo_mut(&mut self) -> &mut ModContentInfo {
        &mut self.minfo
    }

    fn removed(&self) -> bool {
        self.removed
    }

    fn set_removed(&mut self, removed: bool) {
        self.removed = removed;
    }

    fn kind_name(&self) -> &'static str {
        self.kind.name()
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn unlock_fields(&self) -> Option<&UnlockFields> {
        Some(&self.unlock)
    }

    /// `Block.init()` metadata derivations (`world/Block.java:1357+`).
    fn init_self(&mut self) -> Result<(), ContentError> {
        // `Block.displayShadow(tile)` is `hasShadow` for every vanilla block
        // (no overrides), kept as a field for the render/metadata ABI.
        self.display_shadow = self.has_shadow;

        // `Floor.init` (`Floor.java:187-190`): `blendId = blendGroup.id`; the
        // self default resolves to this block's own id. `Floor.load` (`:170`)
        // exposes `<name>-edge` only for the floors the upstream `edges`
        // generator covers: non-overlay floors with `blendGroup == this` and
        // `drawEdgeOut` (`tools/Generators.java:827-847`).
        self.has_edge = is_floor_kind(self.kind)
            && self.kind != BlockKind::AirBlock
            && !is_overlay_floor_kind(self.kind)
            && self.blend_group.is_none()
            && self.draw_edge_out;

        if self.fog_radius > 0 && !self.flags.contains(&BlockFlag::HasFogRadius) {
            self.flags.push(BlockFlag::HasFogRadius);
        }

        // `CoreBlock.java:158-159`: `lightRadius = 30f + 20f * size;
        // fogRadius = Math.max(fogRadius, (int)(lightRadius / 8f * 3f) + 13)`.
        if self.kind == BlockKind::CoreBlock {
            // `lightRadius` is populated by [`java_light_defaults`] (PAR-0467).
            self.fog_radius = self
                .fog_radius
                .max((self.light_radius / 8.0 * 3.0) as i32 + 13);
        }
        // `BaseTurret.java:59` (`init()`): `fogRadius = max(round(range / 8 *
        // fogRadiusMultiplier), fogRadius)` — the runtime range lives on
        // `TurretConfig`, so turrets only get the flag here (the radius is
        // computed by `crate::sim::fog::building_fog_radius`).
        if matches!(
            self.kind,
            BlockKind::BuildTurret
                | BlockKind::ItemTurret
                | BlockKind::LiquidTurret
                | BlockKind::PowerTurret
                | BlockKind::TractorBeamTurret
                | BlockKind::PointDefenseTurret
                | BlockKind::LaserTurret
                | BlockKind::ContinuousLiquidTurret
                | BlockKind::ContinuousTurret
                | BlockKind::RepairTurret
        ) && !self.flags.contains(&BlockFlag::HasFogRadius)
        {
            self.flags.push(BlockFlag::HasFogRadius);
        }

        // `Block.baseExplosiveness`/`explosivenessScale`/`flammabilityScale`
        // (`Block.java:197-201`): every `PowerGenerator` subclass gets
        // `baseExplosiveness = 5` (`PowerGenerator.java:60`), batteries override
        // it (`Blocks.java:2513,2520`) and the conduit family scales down
        // (`Blocks.java:2327-2458`, written upstream as
        // `explosivenessScale = flammabilityScale = A/B` -> both `A/B`).
        if matches!(
            self.kind,
            BlockKind::ConsumeGenerator
                | BlockKind::ThermalGenerator
                | BlockKind::SolarGenerator
                | BlockKind::NuclearReactor
                | BlockKind::ImpactReactor
                | BlockKind::VariableReactor
                | BlockKind::HeaterGenerator
        ) {
            self.base_explosiveness = 5.0;
        }
        let (base, explosiveness_scale, flammability_scale) = match self.name.as_str() {
            "battery" => (Some(1.0), None, None),
            "battery-large" => (Some(5.0), None, None),
            "conduit" => (None, Some(10.0 / 20.0), Some(10.0 / 20.0)),
            "pulse-conduit" => (None, Some(16.0 / 40.0), Some(16.0 / 40.0)),
            "plated-conduit" => (None, Some(16.0 / 50.0), Some(16.0 / 50.0)),
            "liquid-router" => (None, Some(20.0 / 120.0), Some(20.0 / 120.0)),
            "bridge-conduit" => (None, Some(20.0 / 100.0), Some(20.0 / 100.0)),
            "phase-conduit" => (None, Some(20.0 / 100.0), Some(20.0 / 100.0)),
            "reinforced-conduit" => (None, Some(20.0 / 50.0), Some(20.0 / 50.0)),
            "reinforced-bridge-conduit" => (None, Some(20.0 / 120.0), Some(20.0 / 120.0)),
            "reinforced-liquid-router" => (None, Some(40.0 / 150.0), Some(40.0 / 150.0)),
            _ => (None, None, None),
        };
        if let Some(base) = base {
            self.base_explosiveness = base;
        }
        if let Some(scale) = explosiveness_scale {
            self.explosiveness_scale = scale;
        }
        if let Some(scale) = flammability_scale {
            self.flammability_scale = scale;
        }

        if self.health == -1 {
            let mut round = false;
            if self.scaled_health < 0.0 {
                self.scaled_health = 40.0;
                let mut scaling = 1.0f32;
                for stack in &self.requirements {
                    scaling += self
                        .item_health_scaling
                        .get(stack.item.index())
                        .copied()
                        .unwrap_or(0.0);
                }
                self.scaled_health *= scaling;
                round = true;
            }
            if round {
                let size = self.size as f32;
                self.health = round_to(size * size * self.scaled_health, 5.0) as i32;
            } else {
                let size = self.size as f32;
                self.health = (size * size * self.scaled_health) as i32;
            }
        }

        self.offset = ((self.size + 1) % 2) as f32 * TILE_SIZE / 2.0;
        self.size_offset = -((self.size - 1) / 2);

        if !self.requirements.is_empty() && self.build_time < 0.0 {
            self.build_time = 0.0;
            for stack in &self.requirements {
                let cost = self
                    .item_costs
                    .get(stack.item.index())
                    .copied()
                    .unwrap_or(1.0);
                self.build_time += stack.amount as f32 * cost;
            }
        }
        if self.build_time < 0.0 {
            self.build_time = 20.0;
        }
        self.build_time *= self.build_cost_multiplier;

        if self.liquid_capacity < 0.0 {
            let mut consume_amount = 1.0f32;
            for spec in &self.consumes {
                match &spec.consume {
                    Consume::Liquid { amount, .. } => {
                        consume_amount = consume_amount.max(amount * 60.0);
                    }
                    Consume::Liquids(stacks) => {
                        for stack in stacks {
                            consume_amount = consume_amount.max(stack.amount * 60.0);
                        }
                    }
                    Consume::Coolant { amount, .. } => {
                        consume_amount = consume_amount.max(amount * 60.0);
                    }
                    _ => {}
                }
            }
            self.liquid_capacity = round_to(10.0 * consume_amount, 1.0);
        }

        if self.group == BlockGroup::Transportation || self.category == Category::Distribution {
            self.accepts_items = true;
        }

        self.optional_consumers = self
            .consumes
            .iter()
            .enumerate()
            .filter(|(_, spec)| spec.optional && !spec.ignore)
            .map(|(index, _)| index)
            .collect();
        self.non_optional_consumers = self
            .consumes
            .iter()
            .enumerate()
            .filter(|(_, spec)| !spec.optional && !spec.ignore)
            .map(|(index, _)| index)
            .collect();
        self.update_consumers = self
            .consumes
            .iter()
            .enumerate()
            .filter(|(_, spec)| spec.update && !spec.ignore)
            .map(|(index, _)| index)
            .collect();
        self.cons_power = self
            .consumes
            .iter()
            .position(|spec| matches!(spec.consume, Consume::Power { .. }));

        // `Consume.apply(Block)` flags: liquid consumers imply `hasLiquids`
        // (`ConsumeLiquidBase`/`ConsumeLiquids`/`ConsumeLiquidFilter`) and
        // `ConsumePower` implies `hasPower`. `ConsumeItems.apply` sets
        // `hasItems`/`acceptsItems`.
        for spec in &self.consumes {
            match &spec.consume {
                Consume::Items(_) => {
                    self.has_items = true;
                    self.accepts_items = true;
                }
                Consume::Liquid { .. } | Consume::Liquids(_) | Consume::Coolant { .. } => {
                    self.has_liquids = true;
                }
                Consume::Power { .. } => {
                    self.has_power = true;
                }
            }
        }

        // `Block.init()`: every liquid block with `drawLiquidLight` emits light.
        if self.has_liquids {
            self.emit_light = true;
        }
        Ok(())
    }

    /// `UnlockableContent.postInit` + `Block.postInit` database defaults.
    fn post_init(&mut self) -> Result<(), ContentError> {
        if self
            .unlock
            .database_tag
            .as_deref()
            .is_none_or(str::is_empty)
        {
            self.unlock.database_tag = Some(self.category.name().to_owned());
        }
        self.unlock.post_init();
        Ok(())
    }

    fn after_patch(&mut self) -> Result<(), ContentError> {
        self.init_self()
    }
}

impl Mappable for BlockDef {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for BlockDef {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

/// `UI.roundAmount` (`core/src/mindustry/core/UI.java:736`).
///
/// The `>= 1000` and `>= 100` branches are byte-identical upstream; kept as-is
/// for source fidelity.
#[allow(clippy::if_same_then_else)]
pub fn ui_round_amount(number: i32) -> i32 {
    if number >= 1_000_000_000 {
        round_to(number as f32, 100_000_000.0) as i32
    } else if number >= 1_000_000 {
        round_to(number as f32, 100_000.0) as i32
    } else if number >= 10_000 {
        round_to(number as f32, 1000.0) as i32
    } else if number >= 1000 {
        round_to(number as f32, 100.0) as i32
    } else if number >= 100 {
        round_to(number as f32, 100.0) as i32
    } else if number >= 10 {
        round_to(number as f32, 10.0) as i32
    } else {
        number
    }
}

/// `Mathf.round(value, step)` — Arc truncates toward zero
/// (`(int)(value / step) * step`), it does **not** round to nearest.
pub fn round_to(value: f32, step: f32) -> f32 {
    ((value / step) as i32) as f32 * step
}
