// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla unit-stance registry.
//!
//! Ported from `core/src/mindustry/ai/UnitStance.java` and `ItemUnitStance.java`.
//! Incompatible bit sets are built in the registry `link()` pass (plan 02 §3.8),
//! in stance id order, exactly like `UnitStance.init()`.

use super::super::ctype::{Content, Mappable, ModContentInfo};
use super::super::id::{ItemId, UnitCommandId, UnitStanceId};
use super::super::load::ContentRegistry;
use super::super::{ContentError, ContentType};

/// Unit stance record (`mindustry.ai.UnitStance` / `ItemUnitStance`).
#[derive(Debug, Clone, PartialEq)]
pub struct UnitStanceDef {
    /// Dense id in the unit-stance content space.
    pub id: UnitStanceId,
    /// Content name (parity ABI; `item-<item>` for item stances).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// UI icon name (`Icon` key or `item-<name>`).
    pub icon: String,
    /// Keybind name (`Binding` key), if any.
    pub keybind: Option<String>,
    /// Whether this stance can be toggled on/off.
    pub toggle: bool,
    /// Commands mutually exclusive with this stance (authoring list).
    pub incompatible_commands: Vec<UnitCommandId>,
    /// Stances mutually exclusive with this stance (authoring list).
    pub incompatible_stances: Vec<UnitStanceId>,
    /// Incompatible stances as a bitset (`UnitStance.incompatibleStanceBits`).
    pub incompatible_stance_bits: u32,
    /// Incompatible commands as a bitset (`UnitStance.incompatibleCommandBits`).
    pub incompatible_command_bits: u32,
    /// Set for `ItemUnitStance` records.
    pub item: Option<ItemId>,
    /// Bundle key `stance.<name>` (`stance.mine` for item stances).
    pub localized_key: String,
}

impl UnitStanceDef {
    /// Creates a stance with upstream defaults (`UnitStance(String, String, KeyBind[, boolean])`).
    pub fn new(name: &str, icon: &str, keybind: Option<&str>, toggle: bool) -> Self {
        Self {
            id: UnitStanceId::new(0),
            name: name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            icon: icon.to_owned(),
            keybind: keybind.map(str::to_owned),
            toggle,
            incompatible_commands: Vec::new(),
            incompatible_stances: Vec::new(),
            incompatible_stance_bits: 0,
            incompatible_command_bits: 0,
            item: None,
            localized_key: format!("stance.{name}"),
        }
    }

    /// Creates an item stance (`new ItemUnitStance(item)`).
    pub fn new_item(item: ItemId, item_name: &str) -> Self {
        let mut stance = Self::new(
            &format!("item-{item_name}"),
            &format!("item-{item_name}"),
            None,
            true,
        );
        stance.item = Some(item);
        stance.localized_key = String::from("stance.mine");
        stance
    }

    /// Whether this stance is compatible with `command` (`UnitStance.isCompatible`).
    pub fn is_compatible(&self, command: Option<UnitCommandId>) -> bool {
        match command {
            None => true,
            Some(command) => self.incompatible_command_bits & (1 << command.raw()) == 0,
        }
    }
}

impl Content for UnitStanceDef {
    const TYPE: ContentType = ContentType::UnitStance;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = UnitStanceId::new(id);
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
        if self.item.is_some() {
            "ItemUnitStance"
        } else {
            "UnitStance"
        }
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }
}

impl Mappable for UnitStanceDef {
    fn name(&self) -> &str {
        &self.name
    }
}

/// Loads the 8 core stances plus one `ItemUnitStance` per item (plan 02 M2).
pub fn load(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    let stop = registry.add_unit_stance(UnitStanceDef::new(
        "stop",
        "cancel",
        Some("cancelOrders"),
        false,
    ))?;
    let hold_fire = registry.add_unit_stance(UnitStanceDef::new(
        "holdfire",
        "none",
        Some("unitStanceHoldFire"),
        true,
    ))?;
    let pursue_target = registry.add_unit_stance(UnitStanceDef::new(
        "pursuetarget",
        "right",
        Some("unitStancePursueTarget"),
        true,
    ))?;
    let patrol = registry.add_unit_stance(UnitStanceDef::new(
        "patrol",
        "refresh",
        Some("unitStancePatrol"),
        true,
    ))?;
    let ram = registry.add_unit_stance(UnitStanceDef::new(
        "ram",
        "rightOpen",
        Some("unitStanceRam"),
        true,
    ))?;
    let boost = registry.add_unit_stance(UnitStanceDef::new(
        "boost",
        "up",
        Some("unitStanceBoost"),
        true,
    ))?;
    let hold_position = registry.add_unit_stance(UnitStanceDef::new(
        "holdposition",
        "effect",
        Some("unitStanceHoldPosition"),
        true,
    ))?;
    let mine_auto =
        registry.add_unit_stance(UnitStanceDef::new("mineauto", "settings", None, false))?;

    // patrol.incompatibleCommands = repair, assist, rebuild
    let repair = registry.unit_command_by_name("repair").map(|c| c.id);
    let assist = registry.unit_command_by_name("assist").map(|c| c.id);
    let rebuild = registry.unit_command_by_name("rebuild").map(|c| c.id);
    let enter_payload = registry.unit_command_by_name("enterPayload").map(|c| c.id);
    if let (Some(repair), Some(assist), Some(rebuild), Some(enter_payload)) =
        (repair, assist, rebuild, enter_payload)
    {
        if let Some(patrol) = registry.unit_stance_mut(patrol) {
            patrol
                .incompatible_commands
                .extend([repair, assist, rebuild]);
        }
        if let Some(boost) = registry.unit_stance_mut(boost) {
            boost
                .incompatible_commands
                .extend([rebuild, repair, assist, enter_payload]);
        }
    }

    // Item stances for every loaded item, in item id order.
    let items: Vec<(ItemId, String)> = registry
        .items()
        .iter()
        .map(|item| (item.id, item.name.clone()))
        .collect();
    for (item, name) in items {
        let stance = registry.add_unit_stance(UnitStanceDef::new_item(item, &name))?;
        // `ItemUnitStance`: incompatibleStances = mineAuto + mineAuto's stances.
        if let Some(record) = registry.unit_stance_mut(stance) {
            record.incompatible_stances.push(mine_auto);
        }
    }

    // The builder commands get holdPosition as an extra stance.
    for command in [repair, assist, rebuild].into_iter().flatten() {
        if let Some(command) = registry.unit_command_mut(command)
            && !command.extra_stances.contains(&hold_position)
        {
            command.extra_stances.push(hold_position);
        }
    }
    let _ = (stop, hold_fire, pursue_target, ram, mine_auto);
    Ok(())
}

/// `UnitStance.loadAfterMods`: adds stances for mod items (plan 20 calls this).
pub fn load_after_mods(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    let missing: Vec<(ItemId, String)> = registry
        .items()
        .iter()
        .filter(|item| {
            !registry
                .unit_stances()
                .iter()
                .any(|stance| stance.item == Some(item.id))
        })
        .map(|item| (item.id, item.name.clone()))
        .collect();
    for (item, name) in missing {
        let mine_auto = registry
            .unit_stance_by_name("mineauto")
            .map(|stance| stance.id);
        let stance = registry.add_unit_stance(UnitStanceDef::new_item(item, &name))?;
        if let (Some(mine_auto), Some(stance)) = (mine_auto, registry.unit_stance_mut(stance)) {
            stance.incompatible_stances.push(mine_auto);
        }
    }
    Ok(())
}

/// Builds incompatible bits for all stances (`UnitStance.init`), in id order.
pub(crate) fn link(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    let stances = registry.unit_stances_mut();
    let len = stances.len();
    for index in 0..len {
        let self_id = index as u16;
        let incompatible: Vec<UnitStanceId> = stances[index]
            .incompatible_stances
            .iter()
            .copied()
            .filter(|stance| stance.index() != index && stance.index() < len)
            .collect();
        let commands: Vec<UnitCommandId> = stances[index].incompatible_commands.clone();

        for stance in incompatible {
            let other = stance.index();
            stances[index].incompatible_stance_bits |= 1 << stance.raw();
            stances[other].incompatible_stance_bits |= 1 << self_id;
        }
        for command in commands {
            stances[index].incompatible_command_bits |= 1 << command.raw();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;
    use super::*;

    /// `stances::incompatible_bits` (plan 02 §5 M2).
    #[test]
    fn incompatible_bits() {
        let registry = test_registry();
        assert_eq!(
            registry.unit_stances().len(),
            30,
            "8 core + 22 item stances"
        );
        let id = |name: &str| registry.unit_stance_by_name(name).unwrap().id;

        let patrol = registry.unit_stance(id("patrol")).unwrap();
        let boost = registry.unit_stance(id("boost")).unwrap();
        // Symmetric stance exclusion: item stances disable mineauto both ways.
        let item_copper = registry.unit_stance(id("item-copper")).unwrap();
        let mine_auto = registry.unit_stance(id("mineauto")).unwrap();
        assert_eq!(
            item_copper.incompatible_stances,
            vec![mine_auto.id],
            "ItemUnitStance(mineAuto)"
        );
        assert!(item_copper.incompatible_stance_bits & (1 << mine_auto.id.raw()) != 0);
        // `UnitStance.init` sets the bit symmetrically.
        assert!(mine_auto.incompatible_stance_bits & (1 << item_copper.id.raw()) != 0);

        // Command bits.
        let repair = registry.unit_command_by_name("repair").unwrap().id;
        let assist = registry.unit_command_by_name("assist").unwrap().id;
        let rebuild = registry.unit_command_by_name("rebuild").unwrap().id;
        for command in [repair, assist, rebuild] {
            assert!(patrol.incompatible_command_bits & (1 << command.raw()) != 0);
            assert!(boost.incompatible_command_bits & (1 << command.raw()) != 0);
        }
        assert!(!patrol.is_compatible(Some(repair)));
        assert!(patrol.is_compatible(None));
        assert!(
            registry
                .unit_stance(id("stop"))
                .unwrap()
                .is_compatible(Some(repair))
        );
    }

    /// Name order: 8 core stances then item stances in item id order.
    #[test]
    fn load_order() {
        let registry = test_registry();
        let names: Vec<&str> = registry
            .unit_stances()
            .iter()
            .take(8)
            .map(|stance| stance.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "stop",
                "holdfire",
                "pursuetarget",
                "patrol",
                "ram",
                "boost",
                "holdposition",
                "mineauto",
            ]
        );
        assert_eq!(registry.unit_stances()[8].name, "item-copper");
        assert_eq!(registry.unit_stances()[8].item, Some(ItemId::COPPER));
    }
}
