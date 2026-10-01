// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla unit-command registry.
//!
//! Ported from `core/src/mindustry/ai/UnitCommand.java` (10 commands, exact
//! `loadAll()` order; behavior controllers land in plan 11).

use super::super::ctype::{Content, Mappable, ModContentInfo};
use super::super::id::{UnitCommandId, UnitStanceId};
use super::super::load::ContentRegistry;
use super::super::{ContentError, ContentType};

/// Controller kind tag (`controller` factory; plan 11 dispatches behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControllerKind {
    /// `null` controller — default unit behavior.
    None,
    /// `RepairAI`.
    Repair,
    /// `BuilderAI`.
    Builder,
    /// `BuilderAI` with `onlyAssist = true`.
    BuilderAssist,
    /// `MinerAI`.
    Miner,
}

impl ControllerKind {
    /// Java class-ish name.
    pub const fn name(self) -> &'static str {
        match self {
            ControllerKind::None => "None",
            ControllerKind::Repair => "RepairAI",
            ControllerKind::Builder => "BuilderAI",
            ControllerKind::BuilderAssist => "BuilderAI.assist",
            ControllerKind::Miner => "MinerAI",
        }
    }
}

/// Unit command record (`mindustry.ai.UnitCommand`, a `MappableContent`).
#[derive(Debug, Clone, PartialEq)]
pub struct UnitCommandDef {
    /// Dense id in the unit-command content space.
    pub id: UnitCommandId,
    /// Content name (parity ABI).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// UI icon name (`Icon` key).
    pub icon: String,
    /// Keybind name (`Binding` key), if any.
    pub keybind: Option<String>,
    /// Controller kind tag.
    pub controller: ControllerKind,
    /// Automatically switches to the move command when given a position.
    pub switch_to_move: bool,
    /// Whether to draw the movement/attack target.
    pub draw_target: bool,
    /// Whether to reset targets when switching to/from this command.
    pub reset_target: bool,
    /// Whether to snap the destination to ally buildings.
    pub snap_to_building: bool,
    /// Whether the unit arrives at the exact endpoint.
    pub exact_arrival: bool,
    /// Whether selecting this command refreshes the stance list.
    pub refresh_on_select: bool,
    /// Extra stances available when this command is selected.
    pub extra_stances: Vec<UnitStanceId>,
    /// Bundle key `command.<name>`.
    pub localized_key: String,
}

impl UnitCommandDef {
    /// Creates a command with upstream defaults (`UnitCommand(String, String, Func)`).
    pub fn new(name: &str, icon: &str, keybind: Option<&str>, controller: ControllerKind) -> Self {
        Self {
            id: UnitCommandId::new(0),
            name: name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            icon: icon.to_owned(),
            keybind: keybind.map(str::to_owned),
            controller,
            switch_to_move: true,
            draw_target: false,
            reset_target: true,
            snap_to_building: false,
            exact_arrival: false,
            refresh_on_select: false,
            extra_stances: Vec::new(),
            localized_key: format!("command.{name}"),
        }
    }
}

impl Content for UnitCommandDef {
    const TYPE: ContentType = ContentType::UnitCommand;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = UnitCommandId::new(id);
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
        "UnitCommand"
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }
}

impl Mappable for UnitCommandDef {
    fn name(&self) -> &str {
        &self.name
    }
}

/// Loads all 10 vanilla commands in `UnitCommand.loadAll()` order.
pub fn load(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    registry.add_unit_command({
        let mut command = UnitCommandDef::new(
            "move",
            "right",
            Some("unitCommandMove"),
            ControllerKind::None,
        );
        command.draw_target = true;
        command.reset_target = false;
        command
    })?;
    registry.add_unit_command(UnitCommandDef::new(
        "repair",
        "modeSurvival",
        Some("unitCommandRepair"),
        ControllerKind::Repair,
    ))?;
    registry.add_unit_command(UnitCommandDef::new(
        "rebuild",
        "hammer",
        Some("unitCommandRebuild"),
        ControllerKind::Builder,
    ))?;
    registry.add_unit_command(UnitCommandDef::new(
        "assist",
        "players",
        Some("unitCommandAssist"),
        ControllerKind::BuilderAssist,
    ))?;
    registry.add_unit_command({
        let mut command = UnitCommandDef::new(
            "mine",
            "production",
            Some("unitCommandMine"),
            ControllerKind::Miner,
        );
        command.refresh_on_select = true;
        command
    })?;
    registry.add_unit_command({
        let mut command = UnitCommandDef::new(
            "enterPayload",
            "downOpen",
            Some("unitCommandEnterPayload"),
            ControllerKind::None,
        );
        command.switch_to_move = false;
        command.draw_target = true;
        command.reset_target = false;
        command.snap_to_building = true;
        command
    })?;
    registry.add_unit_command({
        let mut command = UnitCommandDef::new(
            "loadUnits",
            "upload",
            Some("unitCommandLoadUnits"),
            ControllerKind::None,
        );
        command.switch_to_move = false;
        command.draw_target = true;
        command.reset_target = false;
        command
    })?;
    registry.add_unit_command({
        let mut command = UnitCommandDef::new(
            "loadBlocks",
            "up",
            Some("unitCommandLoadBlocks"),
            ControllerKind::None,
        );
        command.switch_to_move = false;
        command.draw_target = true;
        command.reset_target = false;
        command.exact_arrival = true;
        command
    })?;
    registry.add_unit_command({
        let mut command = UnitCommandDef::new(
            "unloadPayload",
            "download",
            Some("unitCommandUnloadPayload"),
            ControllerKind::None,
        );
        command.switch_to_move = false;
        command.draw_target = true;
        command.reset_target = false;
        command
    })?;
    registry.add_unit_command({
        let mut command = UnitCommandDef::new(
            "loopPayload",
            "resize",
            Some("unitCommandLoopPayload"),
            ControllerKind::None,
        );
        command.switch_to_move = false;
        command.draw_target = true;
        command.reset_target = false;
        command
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;
    use super::*;

    /// `commands::count_and_ids` (plan 02 §5 M2).
    #[test]
    fn count_and_ids() {
        let registry = test_registry();
        assert_eq!(registry.unit_commands().len(), 10, "command count");
        let names: Vec<&str> = registry
            .unit_commands()
            .iter()
            .map(|command| command.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "move",
                "repair",
                "rebuild",
                "assist",
                "mine",
                "enterPayload",
                "loadUnits",
                "loadBlocks",
                "unloadPayload",
                "loopPayload",
            ]
        );
        let move_command = registry.unit_command(UnitCommandId::new(0)).unwrap();
        assert!(move_command.draw_target);
        assert!(!move_command.reset_target);
        assert!(move_command.switch_to_move);
        assert_eq!(move_command.controller, ControllerKind::None);
        let enter = registry.unit_command_by_name("enterPayload").unwrap();
        assert!(!enter.switch_to_move);
        assert!(enter.snap_to_building);
        let rebuild = registry.unit_command_by_name("rebuild").unwrap();
        assert_eq!(rebuild.controller, ControllerKind::Builder);
        // `holdPosition` is added to the builder commands in `UnitStance.loadAll`.
        assert_eq!(rebuild.extra_stances.len(), 1);
    }
}
