// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MessageBlock`/`MessageBuild` (plan 13 M4).
//!
//! Ported from `core/src/mindustry/world/blocks/logic/MessageBlock.java`. The
//! message is raw text; `print` truncates to `maxTextLength`, `read` returns a
//! char (NaN out of range) and config trims/clamps newlines.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::io::entity::{EntityReader, EntityWriter};
use crate::world::behavior::BuildingBehavior;
use crate::world::config::ConfigValue;

use super::message_def_of;

/// Runtime message text (`MessageBuild.message`).
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct MessageBlockState {
    /// Stored message.
    pub message: String,
}

impl MessageBlockState {
    /// `MessageBuild.read(position, output)`: NaN out of range.
    pub fn read_char(&self, address: i32) -> f64 {
        if address < 0 || address as usize >= self.message.chars().count() {
            f64::NAN
        } else {
            self.message
                .chars()
                .nth(address as usize)
                .map(u32::from)
                .unwrap_or(0) as f64
        }
    }

    /// `MessageBuild.print(text)`: truncate to `max_text`.
    pub fn print(&mut self, text: &str, max_text: usize) {
        self.message = text.chars().take(max_text).collect();
    }
}

/// `MessageBlock.MessageBuild` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct MessageBehavior;

impl BuildingBehavior for MessageBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(MessageBlockState::default());
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        let def = message_def_of(world, e).unwrap_or_default();
        let ConfigValue::String(text) = value else {
            return;
        };
        if !super::accessible(def.privileged, super::rules_ref(world)) {
            return;
        }
        if text.chars().count() > def.max_text.max(0) as usize {
            return;
        }
        let mut message = String::new();
        let mut count = 0;
        for c in text.trim().chars() {
            if c == '\n' {
                if count <= def.max_newlines {
                    message.push('\n');
                }
                count += 1;
            } else {
                message.push(c);
            }
        }
        if let Some(mut state) = world.get_mut::<MessageBlockState>(e) {
            state.message = message;
        }
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        world
            .get::<MessageBlockState>(e)
            .map(|state| ConfigValue::String(state.message.clone()))
            .unwrap_or(ConfigValue::None)
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        0
    }

    fn write(&self, world: &World, e: Entity, w: &mut EntityWriter) {
        let message = world
            .get::<MessageBlockState>(e)
            .map(|state| state.message.clone())
            .unwrap_or_default();
        let _ = w.str(&message);
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut EntityReader, _revision: u8) {
        let Ok(message) = r.str() else {
            return;
        };
        if let Some(mut state) = world.get_mut::<MessageBlockState>(e) {
            state.message = message;
        }
    }
}
