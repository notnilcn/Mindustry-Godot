// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `CommandAI` state machine (plan 11 §3.8/§4.2).
//!
//! Ported from `core/src/mindustry/ai/types/CommandAI.java`. This module owns the
//! serializable command/stance/target/queue state and the deterministic
//! transitions plans 13/15/21 consume (`command`, `commandPosition`,
//! `commandTarget`, `setStance`, queue advance). The full movement/formation
//! behavior (`defaultBehavior`, `finishPath`, blocked-unit yield) is driven by
//! [`crate::ai::UnitGroup`]/`ControlPathfinder` and is added incrementally; the
//! state + queue math here is the frozen serialization surface (plan 11 §3.8).

use crate::content::ContentRegistry;
use crate::content::id::{UnitCommandId, UnitStanceId};
use crate::content::registries::commands::ControllerKind;
use crate::content::registries::units::UnitTypeDef;

use super::super::controller::AiKind;
use super::super::unit_command_runtime::{allows_command, default_command};
use super::super::unit_stance_runtime::{
    StanceBits, disable_stance as bits_disable_stance, set_stance as bits_set_stance,
};

/// Maximum queued waypoints (`CommandAI.maxCommandQueueSize`).
pub const MAX_COMMAND_QUEUE: usize = 50;

/// One queued waypoint (`CommandAI` `Position`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandQueueEntry {
    /// A building tile position (`packedPos`).
    Building(i32),
    /// A unit id.
    Unit(i32),
    /// A world-pixel position.
    Position(f32, f32),
}

/// Attack target (`CommandAI.attackTarget`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttackTarget {
    /// A building tile position.
    Building(i32),
    /// A unit id.
    Unit(i32),
}

impl AttackTarget {
    /// Serialized kind byte (`TypeIO.writeController` entityType).
    pub const fn kind(self) -> u8 {
        match self {
            AttackTarget::Building(_) => 1,
            AttackTarget::Unit(_) => 0,
        }
    }
}

/// `CommandAI` runtime state (serialized subset per plan 11 §6.3).
#[derive(Debug, Clone, PartialEq)]
pub struct CommandAiState {
    /// Current move target position (`targetPos`).
    pub target_pos: Option<(f32, f32)>,
    /// Attack target (`attackTarget`).
    pub attack_target: Option<AttackTarget>,
    /// Active command (`command`).
    pub command: Option<UnitCommandId>,
    /// Queued waypoints (`commandQueue`).
    pub command_queue: Vec<CommandQueueEntry>,
    /// Active stances (`stances`).
    pub stances: StanceBits,
    /// Controller kind installed by the active command (transient).
    pub controller: AiKind,
    /// Last command, used to detect command switches (transient).
    pub last_command: Option<UnitCommandId>,
    /// Whether to stop at the final waypoint (`stopAtTarget`).
    pub stop_at_target: bool,
    /// Whether to always move to the exact final point (`alwaysArrive`).
    pub always_arrive: bool,
}

impl Default for CommandAiState {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandAiState {
    /// Creates an empty command state.
    pub fn new() -> Self {
        Self {
            target_pos: None,
            attack_target: None,
            command: None,
            command_queue: Vec::new(),
            stances: StanceBits::new(),
            controller: AiKind::Command,
            last_command: None,
            stop_at_target: false,
            always_arrive: false,
        }
    }

    /// `CommandAI.init()`: select the default command and install its controller.
    pub fn init(&mut self, unit: &UnitTypeDef, content: &ContentRegistry) {
        if self.command.is_none()
            && let Some(def) = default_command(unit, content)
        {
            self.command = Some(def.id);
            self.controller = ai_kind_from_controller(def.controller);
        }
    }

    /// `CommandAI.command(UnitCommand)`: validates against the unit's
    /// capabilities, clears the queue and installs the command's controller.
    ///
    /// Returns `true` when the command was accepted.
    pub fn command(
        &mut self,
        unit: &UnitTypeDef,
        content: &ContentRegistry,
        command: UnitCommandId,
    ) -> bool {
        if !allows_command(unit, command) {
            return false;
        }
        let Some(def) = content.unit_command(command) else {
            return false;
        };
        self.last_command = self.command;
        self.command = Some(command);
        self.command_queue.clear();
        self.stop_at_target = false;
        self.controller = ai_kind_from_controller(def.controller);
        true
    }

    /// Sets the move target (`CommandAI.commandPosition`).
    pub fn command_position(&mut self, x: f32, y: f32) {
        self.target_pos = Some((x, y));
        self.attack_target = None;
    }

    /// Sets an explicit attack target (`CommandAI.commandTarget`).
    pub fn command_target(&mut self, target: AttackTarget) {
        self.attack_target = Some(target);
        self.stop_at_target = true;
    }

    /// Appends a waypoint, capped at [`MAX_COMMAND_QUEUE`].
    ///
    /// Mirrors `CommandAI.commandQueue`/`addCommand`: the head of the queue is
    /// the current target. Returns `true` when appended.
    pub fn command_queue(&mut self, entry: CommandQueueEntry) -> bool {
        if self.command_queue.len() >= MAX_COMMAND_QUEUE {
            return false;
        }
        self.command_queue.push(entry);
        true
    }

    /// Clears the queue and move/attack target (`CommandAI.clearCommands`).
    pub fn clear_commands(&mut self) {
        self.command_queue.clear();
        self.target_pos = None;
        self.attack_target = None;
        self.stop_at_target = false;
    }

    /// The front of the queue, if any.
    pub fn next_waypoint(&self) -> Option<CommandQueueEntry> {
        self.command_queue.first().copied()
    }

    /// Advances past the front waypoint, returning it.
    ///
    /// `loop_queue` re-appends the consumed waypoint (the `loopPayload`/patrol
    /// behavior); plan 15/13 drive the policy.
    pub fn advance_queue(&mut self, loop_queue: bool) -> Option<CommandQueueEntry> {
        if self.command_queue.is_empty() {
            return None;
        }
        let entry = self.command_queue.remove(0);
        if loop_queue {
            self.command_queue.push(entry);
        }
        Some(entry)
    }

    /// `CommandAI.setStance(stance)`: drop incompatible stances, then set.
    pub fn set_stance(&mut self, content: &ContentRegistry, stance: UnitStanceId) -> bool {
        bits_set_stance(&mut self.stances, content, stance)
    }

    /// `CommandAI.disableStance(stance)`.
    pub fn disable_stance(&mut self, stance: UnitStanceId) -> bool {
        bits_disable_stance(&mut self.stances, stance)
    }

    /// `CommandAI.hasStance(stance)`.
    pub fn has_stance(&self, stance: UnitStanceId) -> bool {
        self.stances.get(stance)
    }

    /// Whether the queue is empty and there is no target.
    pub fn is_idle(&self) -> bool {
        self.command_queue.is_empty() && self.target_pos.is_none() && self.attack_target.is_none()
    }
}

/// Maps a command's `ControllerKind` to the runtime [`AiKind`].
pub const fn ai_kind_from_controller(kind: ControllerKind) -> AiKind {
    match kind {
        ControllerKind::None => AiKind::Command,
        ControllerKind::Repair => AiKind::Repair,
        ControllerKind::Builder | ControllerKind::BuilderAssist => AiKind::Builder,
        ControllerKind::Miner => AiKind::Miner,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore};

    fn content() -> ContentRegistry {
        let mut registry = crate::content::create_base_content(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        registry
    }

    #[test]
    fn command_validation_and_controller_selection() {
        let content = content();
        let dagger = content.unit_by_name("dagger").expect("dagger");
        let mut state = CommandAiState::new();
        state.init(dagger, &content);
        assert_eq!(
            state.command,
            default_command(dagger, &content).map(|d| d.id)
        );

        // `mine` is not a dagger capability.
        let mine = content.unit_command_by_name("mine").expect("mine");
        assert!(!state.command(dagger, &content, mine.id));
        // `move` is allowed and installs the default (command) controller.
        let move_cmd = content.unit_command_by_name("move").expect("move");
        assert!(state.command(dagger, &content, move_cmd.id));
        assert_eq!(state.controller, AiKind::Command);
        // A miner command maps to `MinerAI` on a mining unit.
        let mono = content.unit_by_name("mono").expect("mono");
        assert!(state.command(mono, &content, mine.id));
        assert_eq!(state.controller, AiKind::Miner);
    }

    #[test]
    fn command_switch_clears_queue() {
        let content = content();
        let dagger = content.unit_by_name("dagger").expect("dagger");
        let move_cmd = content.unit_command_by_name("move").expect("move");
        let mut state = CommandAiState::new();
        assert!(state.command(dagger, &content, move_cmd.id));
        assert!(state.command_queue(CommandQueueEntry::Position(10.0, 20.0)));
        assert!(!state.is_idle());
        // Re-issuing a command clears the queue.
        assert!(state.command(dagger, &content, move_cmd.id));
        assert!(state.command_queue.is_empty());
        assert!(state.is_idle());
        // `clearCommands` also empties targets.
        state.command_position(1.0, 2.0);
        state.clear_commands();
        assert!(state.is_idle());
    }

    #[test]
    fn queue_is_capped_and_advances_fifo() {
        let mut state = CommandAiState::new();
        for i in 0..MAX_COMMAND_QUEUE {
            assert!(state.command_queue(CommandQueueEntry::Position(i as f32, 0.0)));
        }
        assert!(!state.command_queue(CommandQueueEntry::Position(1.0, 1.0)));
        assert_eq!(
            state.next_waypoint(),
            Some(CommandQueueEntry::Position(0.0, 0.0))
        );
        assert_eq!(
            state.advance_queue(false),
            Some(CommandQueueEntry::Position(0.0, 0.0))
        );
        assert_eq!(
            state.next_waypoint(),
            Some(CommandQueueEntry::Position(1.0, 0.0))
        );
        assert_eq!(state.command_queue.len(), MAX_COMMAND_QUEUE - 1);
    }

    #[test]
    fn stance_transitions_respect_incompatibility() {
        let content = content();
        let patrol = content.unit_stance_by_name("patrol").expect("patrol");
        let mut state = CommandAiState::new();
        assert!(state.set_stance(&content, patrol.id));
        assert!(state.has_stance(patrol.id));
        assert!(!state.set_stance(&content, patrol.id), "idempotent");
        assert!(state.disable_stance(patrol.id));
        assert!(!state.has_stance(patrol.id));
    }

    #[test]
    fn controller_kind_mapping_matches_table() {
        assert_eq!(
            ai_kind_from_controller(ControllerKind::None),
            AiKind::Command
        );
        assert_eq!(
            ai_kind_from_controller(ControllerKind::Repair),
            AiKind::Repair
        );
        assert_eq!(
            ai_kind_from_controller(ControllerKind::Miner),
            AiKind::Miner
        );
        assert_eq!(
            ai_kind_from_controller(ControllerKind::BuilderAssist),
            AiKind::Builder
        );
    }
}
