// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla item registry.
//!
//! Ported from `core/src/mindustry/content/Items.java` (22 items, exact `load()`
//! order) and `core/src/mindustry/type/Item.java` (metadata half; logic in plans
//! 07/13). IDs are assigned in construction order and are parity/mod ABI.

use super::super::bundle::BundleView;
use super::super::color::Rgba;
use super::super::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use super::super::id::ItemId;
use super::super::load::ContentRegistry;
use super::super::settings_store::UnlockStore;
use super::super::{ContentError, ContentType};

/// Item content record (`mindustry.type.Item`).
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Dense id in the item content space.
    pub id: ItemId,
    /// Content name (parity ABI; already mod-prefixed).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Unlock/database fields.
    pub unlock: UnlockFields,
    /// Display color (`Item.color`).
    pub color: Rgba,
    /// How explosive this item is.
    pub explosiveness: f32,
    /// Flammability (>0.3 is eligible for item burners).
    pub flammability: f32,
    /// Radioactivity.
    pub radioactivity: f32,
    /// Electrical charge.
    pub charge: f32,
    /// Drill hardness.
    pub hardness: i32,
    /// Base material cost (1 cost = 1 tick of build time).
    pub cost: f32,
    /// Per-requirement health scaling added to a block's default health.
    pub health_scaling: f32,
    /// If true, this item is lowest priority for drills.
    pub low_priority: bool,
    /// If >0, the item is animated.
    pub frames: i32,
    /// Number of generated transition frames between animation frames.
    pub transition_frames: i32,
    /// Ticks between animation frames.
    pub frame_time: f32,
    /// If false, the item is incinerated in certain cores.
    pub buildable: bool,
    /// Hidden from most UI.
    pub hidden: bool,
}

impl Item {
    /// Creates an item with upstream defaults (`Item(String, Color)`).
    pub fn new(name: &str, color: Rgba, bundle: &dyn BundleView, store: &dyn UnlockStore) -> Self {
        Self {
            id: ItemId::new(0),
            name: name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            unlock: UnlockFields::new(ContentType::Item, name, bundle, store),
            color,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            hardness: 0,
            cost: 1.0,
            health_scaling: 0.0,
            low_priority: false,
            frames: 0,
            transition_frames: 0,
            frame_time: 5.0,
            buildable: true,
            hidden: false,
        }
    }

    /// `Item.isHidden()`.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
}

impl Content for Item {
    const TYPE: ContentType = ContentType::Item;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = ItemId::new(id);
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
        "Item"
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn unlock_fields(&self) -> Option<&UnlockFields> {
        Some(&self.unlock)
    }

    fn post_init(&mut self) -> Result<(), ContentError> {
        self.unlock.post_init();
        Ok(())
    }
}

impl Mappable for Item {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for Item {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

/// Loads all 22 vanilla items in `Items.load()` order.
pub fn load(
    registry: &mut ContentRegistry,
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
) -> Result<(), ContentError> {
    let copper = registry.add_item({
        let mut item = Item::new("copper", hex("d99d73"), bundle, store);
        item.hardness = 1;
        item.cost = 0.5;
        item.unlock.always_unlocked = true;
        item
    })?;

    let lead = registry.add_item({
        let mut item = Item::new("lead", hex("8c7fa9"), bundle, store);
        item.hardness = 1;
        item.cost = 0.7;
        item
    })?;

    let metaglass = registry.add_item({
        let mut item = Item::new("metaglass", hex("ebeef5"), bundle, store);
        item.cost = 1.5;
        item
    })?;

    let graphite = registry.add_item({
        let mut item = Item::new("graphite", hex("b2c6d2"), bundle, store);
        item.cost = 1.0;
        item
    })?;

    let sand = registry.add_item({
        let mut item = Item::new("sand", hex("f7cba4"), bundle, store);
        item.low_priority = true;
        item.buildable = false;
        item.unlock.always_unlocked = true;
        item
    })?;

    let coal = registry.add_item({
        let mut item = Item::new("coal", hex("272727"), bundle, store);
        item.explosiveness = 0.2;
        item.flammability = 1.0;
        item.hardness = 2;
        item.buildable = false;
        item
    })?;

    let titanium = registry.add_item({
        let mut item = Item::new("titanium", hex("8da1e3"), bundle, store);
        item.hardness = 3;
        item.cost = 1.0;
        item
    })?;

    let thorium = registry.add_item({
        let mut item = Item::new("thorium", hex("f9a3c7"), bundle, store);
        item.explosiveness = 0.2;
        item.hardness = 4;
        item.radioactivity = 1.0;
        item.cost = 1.1;
        item.health_scaling = 0.2;
        item
    })?;

    let scrap = registry.add_item({
        let mut item = Item::new("scrap", hex("777777"), bundle, store);
        item.cost = 0.5;
        item
    })?;

    let silicon = registry.add_item({
        let mut item = Item::new("silicon", hex("53565c"), bundle, store);
        item.cost = 0.8;
        item
    })?;

    let plastanium = registry.add_item({
        let mut item = Item::new("plastanium", hex("cbd97f"), bundle, store);
        item.flammability = 0.1;
        item.explosiveness = 0.2;
        item.cost = 1.3;
        item.health_scaling = 0.1;
        item
    })?;

    let phase_fabric = registry.add_item({
        let mut item = Item::new("phase-fabric", hex("f4ba6e"), bundle, store);
        item.cost = 1.3;
        item.radioactivity = 0.6;
        item.health_scaling = 0.25;
        item
    })?;

    let surge_alloy = registry.add_item({
        let mut item = Item::new("surge-alloy", hex("f3e979"), bundle, store);
        item.cost = 1.2;
        item.charge = 0.75;
        item.health_scaling = 0.25;
        item
    })?;

    let spore_pod = registry.add_item({
        let mut item = Item::new("spore-pod", hex("7457ce"), bundle, store);
        item.flammability = 1.15;
        item.buildable = false;
        item
    })?;

    let blast_compound = registry.add_item({
        let mut item = Item::new("blast-compound", hex("ff795e"), bundle, store);
        item.flammability = 0.4;
        item.explosiveness = 1.2;
        item.buildable = false;
        item
    })?;

    let pyratite = registry.add_item({
        let mut item = Item::new("pyratite", hex("ffaa5f"), bundle, store);
        item.flammability = 1.4;
        item.explosiveness = 0.4;
        item.buildable = false;
        item
    })?;

    let beryllium = registry.add_item({
        let mut item = Item::new("beryllium", hex("3a8f64"), bundle, store);
        item.hardness = 3;
        item.cost = 1.2;
        item.health_scaling = 0.6;
        item
    })?;

    let tungsten = registry.add_item({
        let mut item = Item::new("tungsten", hex("768a9a"), bundle, store);
        item.hardness = 5;
        item.cost = 1.5;
        item.health_scaling = 0.8;
        item
    })?;

    let oxide = registry.add_item({
        let mut item = Item::new("oxide", hex("e4ffd6"), bundle, store);
        item.cost = 1.2;
        item.health_scaling = 0.5;
        item
    })?;

    let carbide = registry.add_item({
        let mut item = Item::new("carbide", hex("89769a"), bundle, store);
        item.cost = 1.4;
        item.health_scaling = 1.1;
        item
    })?;

    let fissile_matter = registry.add_item({
        let mut item = Item::new("fissile-matter", hex("5e988d"), bundle, store);
        item.radioactivity = 1.5;
        item.hidden = true;
        item
    })?;

    let dormant_cyst = registry.add_item({
        let mut item = Item::new("dormant-cyst", hex("df824d"), bundle, store);
        item.flammability = 0.1;
        item.hidden = true;
        item
    })?;

    let serpulo = vec![
        scrap,
        copper,
        lead,
        graphite,
        coal,
        titanium,
        thorium,
        silicon,
        plastanium,
        phase_fabric,
        surge_alloy,
        spore_pod,
        sand,
        blast_compound,
        pyratite,
        metaglass,
    ];
    let erekir = vec![
        graphite,
        thorium,
        silicon,
        phase_fabric,
        surge_alloy,
        sand,
        beryllium,
        tungsten,
        oxide,
        carbide,
        fissile_matter,
        dormant_cyst,
    ];
    let erekir_only = erekir
        .iter()
        .copied()
        .filter(|item| !serpulo.contains(item))
        .collect();
    registry.set_item_lists(serpulo, erekir, erekir_only);
    Ok(())
}

/// Known-good color literal helper (fallback white; colors are static content data).
fn hex(value: &str) -> Rgba {
    Rgba::from_hex(value).unwrap_or(Rgba::WHITE)
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;
    use super::*;

    /// Source-derived golden table for `Items.load()` (name, color, hardness,
    /// cost, explosiveness, flammability, radioactivity, charge, health scaling,
    /// buildable, hidden, always unlocked, low priority).
    ///
    /// `items::matches_golden` (plan 02 §5 M1). The JVM golden is absent on this
    /// machine, so the table is transcribed from `Items.java` directly.
    struct ItemGolden {
        name: &'static str,
        color: &'static str,
        hardness: i32,
        cost: f32,
        explosiveness: f32,
        flammability: f32,
        radioactivity: f32,
        charge: f32,
        health_scaling: f32,
        buildable: bool,
        hidden: bool,
        always_unlocked: bool,
        low_priority: bool,
    }

    const GOLDEN: &[ItemGolden] = &[
        ItemGolden {
            name: "copper",
            color: "d99d73",
            hardness: 1,
            cost: 0.5,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: true,
            hidden: false,
            always_unlocked: true,
            low_priority: false,
        },
        ItemGolden {
            name: "lead",
            color: "8c7fa9",
            hardness: 1,
            cost: 0.7,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "metaglass",
            color: "ebeef5",
            hardness: 0,
            cost: 1.5,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "graphite",
            color: "b2c6d2",
            hardness: 0,
            cost: 1.0,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "sand",
            color: "f7cba4",
            hardness: 0,
            cost: 1.0,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: false,
            hidden: false,
            always_unlocked: true,
            low_priority: true,
        },
        ItemGolden {
            name: "coal",
            color: "272727",
            hardness: 2,
            cost: 1.0,
            explosiveness: 0.2,
            flammability: 1.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: false,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "titanium",
            color: "8da1e3",
            hardness: 3,
            cost: 1.0,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "thorium",
            color: "f9a3c7",
            hardness: 4,
            cost: 1.1,
            explosiveness: 0.2,
            flammability: 0.0,
            radioactivity: 1.0,
            charge: 0.0,
            health_scaling: 0.2,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "scrap",
            color: "777777",
            hardness: 0,
            cost: 0.5,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "silicon",
            color: "53565c",
            hardness: 0,
            cost: 0.8,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "plastanium",
            color: "cbd97f",
            hardness: 0,
            cost: 1.3,
            explosiveness: 0.2,
            flammability: 0.1,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.1,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "phase-fabric",
            color: "f4ba6e",
            hardness: 0,
            cost: 1.3,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.6,
            charge: 0.0,
            health_scaling: 0.25,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "surge-alloy",
            color: "f3e979",
            hardness: 0,
            cost: 1.2,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.75,
            health_scaling: 0.25,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "spore-pod",
            color: "7457ce",
            hardness: 0,
            cost: 1.0,
            explosiveness: 0.0,
            flammability: 1.15,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: false,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "blast-compound",
            color: "ff795e",
            hardness: 0,
            cost: 1.0,
            explosiveness: 1.2,
            flammability: 0.4,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: false,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "pyratite",
            color: "ffaa5f",
            hardness: 0,
            cost: 1.0,
            explosiveness: 0.4,
            flammability: 1.4,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: false,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "beryllium",
            color: "3a8f64",
            hardness: 3,
            cost: 1.2,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.6,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "tungsten",
            color: "768a9a",
            hardness: 5,
            cost: 1.5,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.8,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "oxide",
            color: "e4ffd6",
            hardness: 0,
            cost: 1.2,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.5,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "carbide",
            color: "89769a",
            hardness: 0,
            cost: 1.4,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 1.1,
            buildable: true,
            hidden: false,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "fissile-matter",
            color: "5e988d",
            hardness: 0,
            cost: 1.0,
            explosiveness: 0.0,
            flammability: 0.0,
            radioactivity: 1.5,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: true,
            hidden: true,
            always_unlocked: false,
            low_priority: false,
        },
        ItemGolden {
            name: "dormant-cyst",
            color: "df824d",
            hardness: 0,
            cost: 1.0,
            explosiveness: 0.0,
            flammability: 0.1,
            radioactivity: 0.0,
            charge: 0.0,
            health_scaling: 0.0,
            buildable: true,
            hidden: true,
            always_unlocked: false,
            low_priority: false,
        },
    ];

    fn close(a: f32, b: f32, label: &str, name: &str) {
        assert!(
            (a - b).abs() < 1e-6,
            "{name}: {label} expected {b}, got {a}"
        );
    }

    #[test]
    fn matches_golden() {
        let registry = test_registry();
        assert_eq!(registry.items().len(), 22, "item count");
        for (id, golden) in GOLDEN.iter().enumerate() {
            let item = registry.item(ItemId::new(id as u16)).unwrap();
            assert_eq!(item.name(), golden.name, "id {id}");
            assert_eq!(item.color, Rgba::from_hex(golden.color).unwrap());
            assert_eq!(item.hardness, golden.hardness);
            close(item.cost, golden.cost, "cost", golden.name);
            close(
                item.explosiveness,
                golden.explosiveness,
                "explosiveness",
                golden.name,
            );
            close(
                item.flammability,
                golden.flammability,
                "flammability",
                golden.name,
            );
            close(
                item.radioactivity,
                golden.radioactivity,
                "radioactivity",
                golden.name,
            );
            close(item.charge, golden.charge, "charge", golden.name);
            close(
                item.health_scaling,
                golden.health_scaling,
                "healthScaling",
                golden.name,
            );
            assert_eq!(
                item.buildable, golden.buildable,
                "{}: buildable",
                golden.name
            );
            assert_eq!(item.hidden, golden.hidden, "{}: hidden", golden.name);
            assert_eq!(
                item.unlock.always_unlocked, golden.always_unlocked,
                "{}: alwaysUnlocked",
                golden.name
            );
            assert_eq!(
                item.low_priority, golden.low_priority,
                "{}: lowPriority",
                golden.name
            );
            assert_eq!(
                item.unlock.localized_name, golden.name,
                "localized fallback"
            );
        }
    }

    #[test]
    fn campaign_item_lists() {
        let registry = test_registry();
        let names = |ids: &[ItemId]| -> Vec<String> {
            ids.iter()
                .map(|id| registry.item(*id).unwrap().name().to_owned())
                .collect()
        };
        assert_eq!(
            names(registry.serpulo_items()),
            vec![
                "scrap",
                "copper",
                "lead",
                "graphite",
                "coal",
                "titanium",
                "thorium",
                "silicon",
                "plastanium",
                "phase-fabric",
                "surge-alloy",
                "spore-pod",
                "sand",
                "blast-compound",
                "pyratite",
                "metaglass",
            ]
        );
        assert_eq!(registry.erekir_items().len(), 12);
        assert_eq!(
            names(registry.erekir_only_items()),
            vec![
                "beryllium",
                "tungsten",
                "oxide",
                "carbide",
                "fissile-matter",
                "dormant-cyst",
            ]
        );
    }
}
