// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Global logic variables and constants.
//!
//! Ported from `core/src/mindustry/logic/GlobalVars.java`. Registers the
//! state-independent constants (math, team control ids, `LAccess` names, align
//! names, named colors), the content/`@sfx-*`/time-client constants, the
//! `logicids.dat` lookup tables (shipped asset copied into
//! `mind-core/assets/logicids.dat`) and the per-tick `update()`.

use indexmap::{IndexMap, IndexSet};

use crate::content::registries::sound_meta::SOUNDS;
use crate::content::{ContentRef, ContentRegistry, ContentType};
use crate::logic::access::LAccess;
use crate::logic::value::{LVar, LogicObject, VarArena, VarId, VarRef};
use crate::math::ArcRand;

/// `GlobalVars.ctrlProcessor`.
pub const CTRL_PROCESSOR: i32 = 1;
/// `GlobalVars.ctrlPlayer`.
pub const CTRL_PLAYER: i32 = 2;
/// `GlobalVars.ctrlCommand`.
pub const CTRL_COMMAND: i32 = 3;

/// `GlobalVars.lookableContent`.
pub const LOOKABLE_CONTENT: [ContentType; 5] = [
    ContentType::Block,
    ContentType::Unit,
    ContentType::Item,
    ContentType::Liquid,
    ContentType::Team,
];

/// `GlobalVars.writableLookableContent`.
pub const WRITABLE_LOOKABLE_CONTENT: [ContentType; 4] = [
    ContentType::Block,
    ContentType::Unit,
    ContentType::Item,
    ContentType::Liquid,
];

/// A documented global variable entry (`GlobalVars.VarEntry`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarEntry {
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Icon name.
    pub icon: String,
    /// Privileged.
    pub privileged: bool,
}

/// Per-tick inputs to [`GlobalVars::update`] (`GlobalVars.update`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlobalUpdate {
    /// `state.tick` (fixed 60 Hz).
    pub tick: u64,
    /// `state.wave`.
    pub wave: i32,
    /// `state.wavetime` (ticks).
    pub wavetime: f32,
    /// `world.width()`.
    pub map_width: i32,
    /// `world.height()`.
    pub map_height: i32,
    /// `net.server() || !net.active()`.
    pub server: bool,
    /// `net.client()`.
    pub client: bool,
    /// `player.locale()`.
    pub client_locale: Option<String>,
    /// `player.unit()`.
    pub client_unit: Option<bevy_ecs::entity::Entity>,
    /// `player.name()`.
    pub client_name: Option<String>,
    /// `player.team().id`.
    pub client_team: u8,
    /// `mobile`.
    pub client_mobile: bool,
    /// `control.sound.isPlaying()`.
    pub client_music_playing: bool,
    /// `state.data.getAudioName(music.file)` or the file stem.
    pub client_current_music: Option<String>,
}

/// `GlobalVars` — logic constant/variable table.
#[derive(Clone, Debug, bevy_ecs::prelude::Resource)]
pub struct GlobalVars {
    /// Constant cells by name (lookup only).
    pub cells: IndexMap<String, LVar>,
    /// Privileged names.
    pub privileged_names: IndexSet<String>,
    /// Documentation entries (plan 14).
    pub entries: Vec<VarEntry>,
    /// Global random stream (`GlobalVars.rand`).
    pub rand: ArcRand,
    /// `logicIdToContent` per `writableLookableContent`
    /// (`0 block, 1 unit, 2 item, 3 liquid`; `None` = unknown name).
    pub logic_to_content: [Vec<Option<u16>>; 4],
    /// `contentIdToLogicId` per type (`-1` = absent).
    pub content_to_logic: [Vec<i32>; 4],
    /// `Content.emojiChar()` icon codes per dense id
    /// (`[Block, Unit, Item, Liquid]`; `0` = no icon).
    pub icon_codes: [Vec<u32>; 4],
}

/// The shipped `assets/icons/icons.properties` (content icon codes;
/// `Iconc`/`Content.emojiChar` parity ABI).
const ICON_PROPERTIES: &str = include_str!("../../assets/icons.properties");

/// The shipped `logicids.dat` (copied from the Java assets; `GlobalVars.init`).
const LOGIC_IDS: &[u8] = include_bytes!("../../assets/logicids.dat");

/// Parses `logicids.dat`: for each of `writableLookableContent` a big-endian
/// `short` count then `count` `DataInputStream.readUTF` strings.
pub fn parse_logic_ids(bytes: &[u8]) -> Option<[Vec<String>; 4]> {
    let mut out: [Vec<String>; 4] = std::array::from_fn(|_| Vec::new());
    let mut offset = 0usize;
    for table in out.iter_mut() {
        let amount = i16::from_be_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]);
        offset += 2;
        if amount < 0 {
            return None;
        }
        for _ in 0..amount {
            let len = u16::from_be_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]) as usize;
            offset += 2;
            let name = std::str::from_utf8(bytes.get(offset..offset + len)?).ok()?;
            offset += len;
            table.push(name.to_owned());
        }
    }
    if offset != bytes.len() {
        return None;
    }
    Some(out)
}

impl Default for GlobalVars {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalVars {
    /// Creates and initializes the state-independent constants.
    pub fn new() -> Self {
        let mut globals = Self {
            cells: IndexMap::new(),
            privileged_names: IndexSet::new(),
            entries: Vec::new(),
            rand: ArcRand::new(0),
            logic_to_content: std::array::from_fn(|_| Vec::new()),
            content_to_logic: std::array::from_fn(|_| Vec::new()),
            icon_codes: std::array::from_fn(|_| Vec::new()),
        };
        globals.init_static();
        // Plan 18 seeds the full `Sounds` table unconditionally, so `@sfx-*`
        // constants are always available (upstream guards on loaded assets).
        globals.init_sfx();
        globals.reindex();
        globals
    }

    /// Assigns every cell its stable global-arena id (insertion index).
    fn reindex(&mut self) {
        for (i, var) in self.cells.values_mut().enumerate() {
            var.id = i as i32;
        }
    }

    fn put_entry(&mut self, name: &str, value: Option<LogicObject>, privileged: bool) {
        let mut var = LVar::new(name);
        var.constant = true;
        match value {
            Some(obj) => {
                var.is_obj = true;
                var.obj = Some(obj);
            }
            None => {
                var.is_obj = true;
                var.obj = None;
            }
        }
        self.cells.insert(name.to_owned(), var);
        if privileged {
            self.privileged_names.insert(name.to_owned());
        }
        self.entries.push(VarEntry {
            name: name.to_owned(),
            description: String::new(),
            icon: String::new(),
            privileged,
        });
    }

    fn put_num(&mut self, name: &str, value: f64, privileged: bool) {
        let mut var = LVar::num_const(name, value);
        var.constant = true;
        self.cells.insert(name.to_owned(), var);
        if privileged {
            self.privileged_names.insert(name.to_owned());
        }
        self.entries.push(VarEntry {
            name: name.to_owned(),
            description: String::new(),
            icon: String::new(),
            privileged,
        });
    }

    /// State-independent constants (`GlobalVars.init` minus content/audio).
    fn init_static(&mut self) {
        self.put_entry("the end", None, true);
        self.put_num("false", 0.0, false);
        self.put_num("true", 1.0, false);
        self.put_entry("null", None, true);

        // math
        self.put_num("@pi", std::f64::consts::PI, false);
        self.put_num("π", std::f64::consts::PI, false);
        self.put_num("@e", std::f64::consts::E, false);
        self.put_num("@degToRad", std::f64::consts::PI / 180.0, false);
        self.put_num("@radToDeg", 180.0 / std::f64::consts::PI, false);

        // control ids
        self.put_num("@ctrlProcessor", CTRL_PROCESSOR as f64, false);
        self.put_num("@ctrlPlayer", CTRL_PLAYER as f64, false);
        self.put_num("@ctrlCommand", CTRL_COMMAND as f64, false);

        // time / map state (mutable; `update` refreshes from sim time)
        self.put_num("@time", 0.0, false);
        self.put_num("@tick", 0.0, false);
        self.put_num("@second", 0.0, false);
        self.put_num("@minute", 0.0, false);
        self.put_num("@waveNumber", 0.0, false);
        self.put_num("@waveTime", 0.0, false);
        self.put_num("@mapw", 0.0, false);
        self.put_num("@maph", 0.0, false);
        self.put_entry("@wait", None, true);

        // network / client variables (privileged, desynced-client values)
        self.put_num("@server", 1.0, true);
        self.put_num("@client", 0.0, true);
        self.put_entry("@clientLocale", None, true);
        self.put_entry("@clientUnit", None, true);
        self.put_entry("@clientName", None, true);
        self.put_num("@clientTeam", 0.0, true);
        self.put_num("@clientMobile", 0.0, true);
        self.put_num("@clientMusicPlaying", 0.0, true);
        self.put_entry("@clientCurrentMusic", None, true);

        // sensor constants
        for sensor in LAccess::ALL {
            let name = format!("@{}", sensor.name());
            let mut var = LVar::new(&name);
            var.constant = true;
            var.is_obj = true;
            var.obj = Some(LogicObject::Enum(sensor.name()));
            self.cells.insert(name, var);
        }

        // align constants
        for name in [
            "center",
            "top",
            "bottom",
            "left",
            "right",
            "topLeft",
            "topRight",
            "bottomLeft",
            "bottomRight",
        ] {
            let value = crate::logic::statement::name_to_align(name).unwrap_or(0);
            let key = format!("@{name}");
            // Upstream `GlobalVars.init`: `put("@" + name, align)` where `align`
            // is an `Integer`; `DrawI` reads it with `p1.numi()`.
            let var = LVar::num_const(key, value as f64);
            self.cells.insert(var.name.clone(), var);
        }

        // named colors: @color<Name> where Name = capitalize(lowercased name)
        for (name, color) in NAMED_COLORS {
            let capitalized = capitalize(name);
            let key = format!("@color{capitalized}");
            self.put_num(&key, rgba_to_double_bits(*color), false);
        }
    }

    /// Registers `@<name>` content constants (`GlobalVars.init` tail).
    ///
    /// Items/liquids/blocks/units/weathers register under `@<name>`; status
    /// effects under `@status-<name>`; teams under `@<name>` by index. Blocks
    /// that share an item name are skipped (upstream sand special-case). Sound
    /// constants (`@sfx-*`) need plan 18's `SoundId` table and are omitted; the
    /// `@<type>Count` constants are absent without `logicids.dat` (HLP §9).
    pub fn init_content(&mut self, content: &ContentRegistry) {
        for team in crate::game::team::Team::base_teams() {
            let key = format!("@{}", team.name);
            let mut var = LVar::new(&key);
            var.constant = true;
            var.is_obj = true;
            var.obj = Some(LogicObject::Team(team.id));
            self.cells.insert(key, var);
        }
        for type_ in [
            ContentType::Item,
            ContentType::Liquid,
            ContentType::Block,
            ContentType::Unit,
            ContentType::Weather,
        ] {
            for entry in content.entries(type_) {
                let Some(name) = entry.name else { continue };
                if type_ == ContentType::Block {
                    if content.get_by_name(ContentType::Item, name).is_some() {
                        continue;
                    }
                    // `GlobalVars.init` skips `LegacyBlock`s (the three legacy
                    // block kinds registered in `blocks/legacy.rs`).
                    if content.block_by_name(name).is_some_and(|def| {
                        matches!(
                            def.kind,
                            crate::content::registries::blocks::BlockKind::LegacyMechPad
                                | crate::content::registries::blocks::BlockKind::LegacyUnitFactory
                                | crate::content::registries::blocks::BlockKind::LegacyCommandCenter
                        )
                    }) {
                        continue;
                    }
                }
                // `GlobalVars.init` skips `type.internal` units.
                if type_ == ContentType::Unit
                    && content.unit_by_name(name).is_some_and(|def| def.internal)
                {
                    continue;
                }
                let key = format!("@{name}");
                self.put_content(&key, type_, entry.id, false);
            }
        }
        // `Content.emojiChar()` -> `Iconc` code by content name
        // (`icons.properties`); the PrintCharI table.
        {
            let iconc = crate::assets::icons::Iconc::from_properties(ICON_PROPERTIES);
            for (slot, type_) in [
                (0usize, ContentType::Block),
                (1, ContentType::Unit),
                (2, ContentType::Item),
                (3, ContentType::Liquid),
            ] {
                for entry in content.entries(type_) {
                    let Some(name) = entry.name else { continue };
                    let Some(code) = iconc.code(name) else {
                        continue;
                    };
                    let table = &mut self.icon_codes[slot];
                    let index = entry.id as usize;
                    if table.len() <= index {
                        table.resize(index + 1, 0);
                    }
                    table[index] = code;
                }
            }
        }
        for entry in content.entries(ContentType::Status) {
            let Some(name) = entry.name else { continue };
            let key = format!("@status-{name}");
            self.put_content(&key, ContentType::Status, entry.id, false);
        }
        self.init_logic_ids(content);
        self.init_sfx();
        self.reindex();
    }

    /// `GlobalVars.init` logic-id pass: parse the shipped `logicids.dat`, map
    /// each name to the content records and register `@<type>Count`.
    fn init_logic_ids(&mut self, content: &ContentRegistry) {
        let Some(tables) = parse_logic_ids(LOGIC_IDS) else {
            return;
        };
        for (index, type_) in WRITABLE_LOOKABLE_CONTENT.iter().enumerate() {
            let type_name = match type_ {
                ContentType::Block => "block",
                ContentType::Unit => "unit",
                ContentType::Item => "item",
                ContentType::Liquid => "liquid",
                _ => continue,
            };
            let names = &tables[index];
            let mut to_content = vec![None; names.len()];
            for (logic_id, name) in names.iter().enumerate() {
                if let Some(record) = content.get_by_name(*type_, name) {
                    to_content[logic_id] = Some(record.id);
                }
            }
            let mut to_logic = vec![-1i32; content.type_len(*type_)];
            for (logic_id, record) in to_content.iter().enumerate() {
                if let Some(id) = record {
                    to_logic[*id as usize] = logic_id as i32;
                }
            }
            self.logic_to_content[index] = to_content;
            self.content_to_logic[index] = to_logic;
            self.put_num(&format!("@{type_name}Count"), names.len() as f64, false);
        }
    }

    /// Registers `@sfx-<file>` sound-id constants (`GlobalVars.init` audio block).
    ///
    /// Upstream iterates the loaded `Sound` assets, skips `Sounds.none`/`unset`
    /// and uses `sound.file.nameWithoutExtension()` as the key with
    /// `Sounds.getSoundId(sound)` as the value. Plan 18's seeded `SOUNDS` table
    /// mirrors the generated `Sounds` order, so the raw index is that id.
    pub fn init_sfx(&mut self) {
        for (i, meta) in SOUNDS.iter().enumerate().skip(2) {
            let file = meta.asset.rsplit('/').next().unwrap_or(meta.name);
            let key = format!("@sfx-{file}");
            if self.cells.contains_key(&key) {
                continue;
            }
            let mut var = LVar::num_const(key.clone(), i as f64);
            var.id = i as i32;
            self.cells.insert(key, var);
        }
    }

    /// Convenience: static constants plus content + sound constants.
    pub fn with_content(content: &ContentRegistry) -> Self {
        let mut globals = Self::new();
        globals.init_content(content);
        globals
    }

    /// `Content.emojiChar()` icon code for a content record (0 when absent).
    pub fn content_icon_code(&self, type_: ContentType, id: u16) -> u32 {
        let slot = match type_ {
            ContentType::Block => 0,
            ContentType::Unit => 1,
            ContentType::Item => 2,
            ContentType::Liquid => 3,
            _ => return 0,
        };
        self.icon_codes[slot].get(id as usize).copied().unwrap_or(0)
    }

    /// Registers a content constant object under `name`.
    fn put_content(&mut self, name: &str, type_: ContentType, id: u16, privileged: bool) {
        let mut var = LVar::new(name);
        var.constant = true;
        var.is_obj = true;
        var.obj = Some(LogicObject::Content(ContentRef::new(type_, id)));
        self.cells.insert(name.to_owned(), var);
        if privileged {
            self.privileged_names.insert(name.to_owned());
        }
    }

    /// Installs the content-initialized arena as the world's logic-globals
    /// resource (`GlobalVars.init` at content-init time).
    ///
    /// Plan 02/content-init calls this once per content load. Executors then
    /// observe live updates through [`update_world`](Self::update_world).
    pub fn install_world(world: &mut bevy_ecs::world::World, content: &ContentRegistry) -> Self {
        let globals = Self::with_content(content);
        world.insert_resource(globals.clone());
        if world.get_resource::<LogicContentInfo>().is_none() {
            world.insert_resource(LogicContentInfo::from_content(content));
        }
        globals
    }

    /// Applies a [`GlobalUpdate`] to the installed resource, if present.
    pub fn update_world(world: &mut bevy_ecs::world::World, update: &GlobalUpdate) {
        if let Some(mut globals) = world.get_resource_mut::<Self>() {
            globals.update(update);
        }
    }

    /// `GlobalVars.update` — refreshes the tick/map/network/client variables.
    ///
    /// Running executors observe these values through their lowered
    /// [`VarRef::Global`](crate::logic::value::VarRef::Global) mirror cells each
    /// tick (plan 13 §3.2 deviation 2).
    pub fn update(&mut self, update: &GlobalUpdate) {
        self.set_num_raw("@time", update.tick as f64 / 60.0 * 1000.0);
        self.set_num_raw("@tick", update.tick as f64);
        self.set_num_raw("@second", update.tick as f64 / 60.0);
        self.set_num_raw("@minute", update.tick as f64 / 3600.0);
        self.set_num_raw("@waveNumber", update.wave as f64);
        // `@wave` is not a Java constant (only `@waveNumber` is; GlobalVars.java
        // has no `put("@wave", ...)` call).
        self.set_num_raw("@waveTime", update.wavetime as f64 / 60.0);
        self.set_num_raw("@mapw", update.map_width as f64);
        self.set_num_raw("@maph", update.map_height as f64);
        self.set_num_raw("@server", if update.server { 1.0 } else { 0.0 });
        self.set_num_raw("@client", if update.client { 1.0 } else { 0.0 });
        self.set_obj_raw(
            "@clientLocale",
            update.client_locale.clone().map(LogicObject::Str),
        );
        self.set_obj_raw("@clientUnit", update.client_unit.map(LogicObject::Unit));
        self.set_obj_raw(
            "@clientName",
            update.client_name.clone().map(LogicObject::Str),
        );
        self.set_num_raw("@clientTeam", update.client_team as f64);
        self.set_num_raw(
            "@clientMobile",
            if update.client_mobile { 1.0 } else { 0.0 },
        );
        self.set_num_raw(
            "@clientMusicPlaying",
            if update.client_music_playing {
                1.0
            } else {
                0.0
            },
        );
        self.set_obj_raw(
            "@clientCurrentMusic",
            update.client_current_music.clone().map(LogicObject::Str),
        );
    }

    /// Sets a numeric constant's raw value regardless of the `constant` flag.
    fn set_num_raw(&mut self, name: &str, value: f64) {
        if let Some(cell) = self.cells.get_mut(name) {
            cell.is_obj = false;
            cell.obj = None;
            cell.num = value;
        }
    }

    /// Sets an object constant's raw value regardless of the `constant` flag.
    fn set_obj_raw(&mut self, name: &str, value: Option<LogicObject>) {
        if let Some(cell) = self.cells.get_mut(name) {
            cell.is_obj = true;
            cell.obj = value;
        }
    }

    /// `GlobalVars.get(name, privileged)`.
    pub fn get(&self, name: &str, privileged: bool) -> Option<LVar> {
        if !privileged && self.privileged_names.contains(name) {
            return self.cells.get("null").cloned();
        }
        self.cells.get(name).cloned()
    }

    /// Returns the global-arena reference for `name` (`GlobalVars.get`).
    ///
    /// Mirrors [`get`](Self::get): privileged names resolve to the `null` cell
    /// for non-privileged callers.
    pub fn get_ref(&self, name: &str, privileged: bool) -> Option<VarRef> {
        if !privileged && self.privileged_names.contains(name) {
            return self
                .cells
                .get("null")
                .map(|v| VarRef::Global(v.id.max(0) as VarId));
        }
        self.cells
            .get(name)
            .map(|v| VarRef::Global(v.id.max(0) as VarId))
    }

    /// Global cell by arena id.
    pub fn global_cell(&self, id: VarId) -> Option<&LVar> {
        self.cells.get_index(id as usize).map(|(_, v)| v)
    }

    /// Mutable global cell by arena id.
    pub fn global_cell_mut(&mut self, id: VarId) -> Option<&mut LVar> {
        self.cells.get_index_mut(id as usize).map(|(_, v)| v)
    }

    /// `GlobalVars.waitVar` (`@wait`).
    pub fn wait_var(&self) -> Option<&LVar> {
        self.cells.get("@wait")
    }

    /// `GlobalVars.lookupContent`: teams by index (0..255), writable-lookable
    /// content through the `logicids.dat` tables, otherwise `None`.
    pub fn lookup_content(&self, type_: ContentType, id: i32) -> Option<LogicObject> {
        if type_ == ContentType::Team {
            if (0..256).contains(&id) {
                return Some(LogicObject::Team(id as u8));
            }
            return None;
        }
        let index = WRITABLE_LOOKABLE_CONTENT
            .iter()
            .position(|candidate| *candidate == type_)?;
        let logic_id = usize::try_from(id).ok()?;
        let content_id = *self.logic_to_content[index].get(logic_id)?;
        content_id.map(|id| LogicObject::Content(ContentRef::new(type_, id)))
    }

    /// `GlobalVars.lookupLogicId`: the logic id of a content record, `-1` when
    /// absent (`Block/Item/Liquid/UnitType.getLogicId`).
    pub fn lookup_logic_id(&self, content: ContentRef) -> i32 {
        let Some(index) = WRITABLE_LOOKABLE_CONTENT
            .iter()
            .position(|candidate| *candidate == content.type_)
        else {
            return -1;
        };
        self.content_to_logic[index]
            .get(content.id as usize)
            .copied()
            .unwrap_or(-1)
    }

    /// `GlobalVars.remove(name)` (data-patch reset).
    pub fn remove(&mut self, name: &str) {
        self.cells.shift_remove(name);
    }
}

/// Content names by type and dense id, for the executor's `print`/`format`
/// (`PrintI.toString` needs content names and the VM has no registry access).
///
/// Built next to [`GlobalVars`] by [`GlobalVars::install_world`].
#[derive(Clone, Debug, bevy_ecs::prelude::Resource)]
pub struct LogicContentInfo {
    /// `names[type as usize][dense id]`; empty string = no such record.
    names: Vec<Vec<String>>,
    /// `rows[type as usize][dense id]` (the content `sense` tables).
    rows: Vec<Vec<ContentSenseRow>>,
}

/// Host-injected client identity used by the `GlobalVars` update
/// (`@clientLocale`/`@clientName`; `GlobalVars.java:211-216` reads
/// `player.locale()`/`player.name()`).
///
/// The gdext host fills this from the settings store; core defaults to
/// `"en"`/`""`.
#[derive(Clone, Debug, Default, PartialEq, bevy_ecs::prelude::Resource)]
pub struct LogicClientInfo {
    /// `player.locale()`.
    pub locale: String,
    /// `player.name()`.
    pub name: String,
}

/// Scalar rows of the content `sense` tables
/// (`Block.sense`, `UnitType.sense`, `Item.sense`, `Liquid.sense`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContentSenseRow {
    /// `health`/`maxHealth`.
    pub health: f32,
    /// Unit `armor` / block `armor`.
    pub armor: f32,
    /// Block `solid`.
    pub solid: bool,
    /// Block `size` / unit `hitSize / 8`.
    pub size: f32,
    /// `itemCapacity`.
    pub item_capacity: f32,
    /// `liquidCapacity`.
    pub liquid_capacity: f32,
    /// Buffered power capacity (`consPower.capacity`).
    pub power_capacity: f32,
    /// `UnitType.maxRange` (raw, matching `World.conv(maxRange)`).
    pub range: f32,
    /// `type.speed * 60 / 8`.
    pub speed: f32,
    /// `type.flying`.
    pub flying: bool,
    /// `payloadCapacity / tilesize²`.
    pub payload_capacity: f32,
    /// Packed RGBA8888 (`Color.toDoubleBits`).
    pub color: u32,
}

/// Packs an `Rgba` (0..1) into RGBA8888 with Java's truncation
/// (`Color.rgba8888`).
fn pack_rgba(color: crate::content::Rgba) -> u32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
    (c(color.r) << 24) | (c(color.g) << 16) | (c(color.b) << 8) | c(color.a)
}

impl Default for LogicContentInfo {
    fn default() -> Self {
        Self {
            names: vec![Vec::new(); ContentType::ALL.len()],
            rows: vec![Vec::new(); ContentType::ALL.len()],
        }
    }
}

impl LogicContentInfo {
    /// Builds the name tables from the registry (`GlobalVars.init` content pass).
    pub fn from_content(content: &ContentRegistry) -> Self {
        let mut info = Self::default();
        for type_ in ContentType::ALL {
            let entries = content.entries(type_);
            let len = entries
                .iter()
                .map(|entry| entry.id as usize + 1)
                .max()
                .unwrap_or(0);
            let table = &mut info.names[type_ as usize];
            table.resize(len, String::new());
            for entry in entries {
                if let Some(name) = entry.name {
                    table[entry.id as usize] = name.to_owned();
                }
            }
        }
        // Block rows (`Block.sense(LAccess)`, J/world/Block.java:1674-1708).
        {
            let rows = &mut info.rows[ContentType::Block as usize];
            for def in content.blocks() {
                let index = def.id.raw() as usize;
                if rows.len() <= index {
                    rows.resize(index + 1, ContentSenseRow::default());
                }
                let power_capacity =
                    def.consumes
                        .iter()
                        .find_map(|c| match &c.consume {
                            crate::content::registries::blocks::Consume::Power {
                                buffered, ..
                            } if *buffered > 0.0 => Some(*buffered),
                            _ => None,
                        })
                        .unwrap_or(0.0);
                rows[index] = ContentSenseRow {
                    health: def.health as f32,
                    armor: def.armor,
                    solid: def.solid,
                    size: def.size as f32,
                    item_capacity: def.item_capacity as f32,
                    liquid_capacity: def.liquid_capacity,
                    power_capacity,
                    color: def.map_color.map(pack_rgba).unwrap_or(0),
                    ..ContentSenseRow::default()
                };
            }
        }
        // Unit rows (`UnitType.sense`, J/type/UnitType.java:1430-1449).
        {
            let rows = &mut info.rows[ContentType::Unit as usize];
            let tile_payload = (crate::config::TILESIZE * crate::config::TILESIZE) as f32;
            for def in content.units() {
                let index = def.id.raw() as usize;
                if rows.len() <= index {
                    rows.resize(index + 1, ContentSenseRow::default());
                }
                rows[index] = ContentSenseRow {
                    health: def.health,
                    armor: def.armor,
                    size: def.hit_size / crate::world::block::TILE_SIZE,
                    item_capacity: def.item_capacity.max(0) as f32,
                    range: def.max_range,
                    speed: def.speed * 60.0 / crate::world::block::TILE_SIZE,
                    flying: def.flying,
                    payload_capacity: if def.payload_capacity > 0.0 {
                        def.payload_capacity / tile_payload
                    } else {
                        0.0
                    },
                    ..ContentSenseRow::default()
                };
            }
        }
        // Item/Liquid rows (`Item.sense`/`Liquid.sense`: color only).
        {
            let rows = &mut info.rows[ContentType::Item as usize];
            for def in content.items() {
                let index = def.id.raw() as usize;
                if rows.len() <= index {
                    rows.resize(index + 1, ContentSenseRow::default());
                }
                rows[index] = ContentSenseRow {
                    color: pack_rgba(def.color),
                    ..ContentSenseRow::default()
                };
            }
        }
        {
            let rows = &mut info.rows[ContentType::Liquid as usize];
            for def in content.liquids() {
                let index = def.id.raw() as usize;
                if rows.len() <= index {
                    rows.resize(index + 1, ContentSenseRow::default());
                }
                rows[index] = ContentSenseRow {
                    color: pack_rgba(def.color),
                    ..ContentSenseRow::default()
                };
            }
        }
        info
    }

    /// Scalar sense row for a content record, when known.
    pub fn row(&self, type_: ContentType, id: u16) -> Option<&ContentSenseRow> {
        self.rows.get(type_ as usize)?.get(id as usize)
    }

    /// Name for a content record (`Content.name`), when known.
    pub fn name(&self, type_: ContentType, id: u16) -> Option<&str> {
        self.names
            .get(type_ as usize)?
            .get(id as usize)
            .map(String::as_str)
            .filter(|name| !name.is_empty())
    }
}

/// Capitalizes the first character (`Strings.capitalize` for ASCII names).
fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Red/green/blue/alpha bytes (`Color` 0–255 components).
pub type Rgba8 = (u8, u8, u8, u8);

/// `Color.rgba8888(r,g,b,a)` → `Double.longBitsToDouble(rgba8888 & 0xffffffffL)`.
pub fn rgba_to_double_bits(color: Rgba8) -> f64 {
    let (r, g, b, a) = color;
    let packed = ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32);
    f64::from_bits(packed as u64)
}

/// `Color.toDoubleBits(int r,g,b,a)`.
pub fn rgba_u8_to_double_bits(r: i32, g: i32, b: i32, a: i32) -> f64 {
    rgba_to_double_bits((r as u8, g as u8, b as u8, a as u8))
}

/// Arc named colors (lowercase keys; `Colors.reset()`), `(name, (r,g,b,a))`.
#[rustfmt::skip]
pub const NAMED_COLORS: &[(&str, Rgba8)] = &[
    ("clear", (0, 0, 0, 0)),
    ("black", (0, 0, 0, 255)),
    ("white", (255, 255, 255, 255)),
    ("lightgray", (204, 204, 204, 255)),
    ("gray", (128, 128, 128, 255)),
    ("darkgray", (64, 64, 64, 255)),
    ("blue", (64, 64, 230, 255)),
    ("navy", (0, 0, 128, 255)),
    ("royal", (64, 64, 230, 255)),
    ("slate", (112, 128, 144, 255)),
    ("sky", (135, 206, 235, 255)),
    ("cyan", (0, 255, 255, 255)),
    ("teal", (0, 128, 128, 255)),
    ("green", (56, 214, 103, 255)),
    ("acid", (127, 255, 0, 255)),
    ("lime", (0, 255, 0, 255)),
    ("forest", (34, 139, 34, 255)),
    ("olive", (128, 128, 0, 255)),
    ("yellow", (255, 255, 0, 255)),
    ("gold", (255, 215, 0, 255)),
    ("goldenrod", (218, 165, 32, 255)),
    ("orange", (255, 165, 0, 255)),
    ("brown", (165, 42, 42, 255)),
    ("tan", (210, 180, 140, 255)),
    ("brick", (178, 34, 34, 255)),
    ("red", (229, 84, 84, 255)),
    ("scarlet", (255, 36, 0, 255)),
    ("crimson", (220, 20, 60, 255)),
    ("coral", (255, 127, 80, 255)),
    ("salmon", (250, 128, 114, 255)),
    ("pink", (255, 192, 203, 255)),
    ("magenta", (255, 0, 255, 255)),
    ("purple", (128, 0, 128, 255)),
    ("violet", (238, 130, 238, 255)),
    ("maroon", (128, 0, 0, 255)),
];

/// `Colors.get(name)` for the lowercase named colors.
pub fn named_color(name: &str) -> Option<Rgba8> {
    NAMED_COLORS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
}

/// Folds the constants into an arena (assemble-time copy); returns the cell id.
pub fn insert_into(arena: &mut VarArena, var: &LVar) -> crate::logic::value::VarId {
    let id = arena.put_var(&var.name);
    let cell = arena.get_mut(id);
    cell.is_obj = var.is_obj;
    cell.obj = var.obj.clone();
    cell.num = var.num;
    cell.constant = true;
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_constants_resolve() {
        let g = GlobalVars::new();
        assert_eq!(g.get("null", false).unwrap().obj, None);
        assert!(g.get("null", false).unwrap().is_obj);
        assert_eq!(g.get("true", false).unwrap().num, 1.0);
        assert_eq!(g.get("@pi", false).unwrap().num, std::f64::consts::PI);
        assert!(g.get("@ctrlProcessor", false).is_some());
        assert!(g.get("the end", false).is_some());
        // privileged constants fall back to null for non-privileged executors
        assert_eq!(g.get("the end", false).unwrap().obj, None);
    }

    #[test]
    fn named_color_red_matches_arc() {
        let red = named_color("red").unwrap();
        assert_eq!(red, (229, 84, 84, 255));
        assert_eq!(rgba_to_double_bits(red), f64::from_bits(0xe5_54_54_ff_u64));
    }

    #[test]
    fn content_constants_register() {
        use crate::world::harness::BuildHarness;
        let content = BuildHarness::load_content();
        let g = GlobalVars::with_content(&content);
        let copper = g.get("@copper", false).expect("@copper");
        assert!(
            matches!(copper.obj, Some(LogicObject::Content(c)) if c.type_ == ContentType::Item)
        );
        assert!(g.get("@dagger", false).is_some());
        assert!(g.get("@rain", false).is_some());
        assert!(g.get("@status-wet", false).is_some());
        assert!(g.get("@sharded", false).is_some());
        // `GlobalVars.init`: legacy blocks and internal units do not register.
        assert!(g.get("@legacy-mech-pad", false).is_none());
        assert!(g.get("@block", false).is_none());
        assert!(g.get("@dummy", false).is_none());
        // `@<type>Count` constants come from the shipped `logicids.dat`.
        assert_eq!(g.get("@itemCount", false).unwrap().num, 20.0);
    }

    #[test]
    fn update_math_matches_upstream() {
        let mut g = GlobalVars::new();
        let update = GlobalUpdate {
            tick: 180,
            wave: 4,
            wavetime: 300.0,
            map_width: 16,
            map_height: 24,
            server: true,
            ..GlobalUpdate::default()
        };
        g.update(&update);
        assert_eq!(g.get("@time", false).unwrap().num, 3000.0);
        assert_eq!(g.get("@tick", false).unwrap().num, 180.0);
        assert_eq!(g.get("@second", false).unwrap().num, 3.0);
        assert_eq!(g.get("@minute", false).unwrap().num, 0.05);
        assert_eq!(g.get("@waveNumber", false).unwrap().num, 4.0);
        // `@wave` is not a Java global (only `@waveNumber`).
        assert!(g.get("@wave", false).is_none());
        assert_eq!(g.get("@waveTime", false).unwrap().num, 5.0);
        assert_eq!(g.get("@mapw", false).unwrap().num, 16.0);
        assert_eq!(g.get("@maph", false).unwrap().num, 24.0);
        // Privileged fallback: non-privileged `@server` resolves to null.
        assert_eq!(g.get("@server", true).unwrap().num, 1.0);
        assert_eq!(g.get("@server", false).unwrap().obj, None);
    }

    #[test]
    fn executor_observes_live_global_update() {
        use crate::logic::assembler::Assembler;
        use crate::logic::executor::Executor;

        let mut globals = GlobalVars::new();
        let asm = Assembler::assemble_with("set t @tick\nend\n", true, globals.clone())
            .expect("assemble");
        let mut exec = Executor::new();
        exec.load(asm);

        // Insert the live arena resource and advance @tick between runs.
        let mut world = bevy_ecs::world::World::new();
        globals.update(&GlobalUpdate {
            tick: 42,
            ..GlobalUpdate::default()
        });
        world.insert_resource(globals.clone());
        exec.run_once(&mut world);
        let t = exec.optional_var("t").expect("t");
        assert_eq!(exec.arena.get(t).num(), 42.0);

        globals.update(&GlobalUpdate {
            tick: 99,
            ..GlobalUpdate::default()
        });
        world.insert_resource(globals);
        exec.run_once(&mut world);
        assert_eq!(
            exec.arena.get(t).num(),
            42.0,
            "counter past end resets to 0"
        );
    }

    #[test]
    fn sfx_constants_register_from_sound_table() {
        let content = crate::world::harness::BuildHarness::load_content();
        let g = GlobalVars::with_content(&content);
        // `Sounds.none`/`unset` are skipped; the first real sound is index 2.
        assert!(g.get("@sfx-acceleratorCharge", false).is_some());
        assert_eq!(g.get("@sfx-acceleratorCharge", false).unwrap().num, 2.0);
        assert!(g.get("@sfx-none", false).is_none());
        assert_eq!(g.get("@sfx-shoot", false).unwrap().num, {
            let idx = SOUNDS
                .iter()
                .position(|m| m.asset.ends_with("shoot") || m.name == "shoot");
            idx.unwrap_or(0) as f64
        });
    }

    #[test]
    fn lookup_maps_logic_ids() {
        use crate::world::harness::BuildHarness;
        let content = BuildHarness::load_content();
        let g = GlobalVars::with_content(&content);
        // File-derived counts (`logicids.dat`): 262 blocks / 56 units /
        // 20 items / 11 liquids.
        assert_eq!(g.get("@blockCount", false).unwrap().num, 262.0);
        assert_eq!(g.get("@unitCount", false).unwrap().num, 56.0);
        assert_eq!(g.get("@itemCount", false).unwrap().num, 20.0);
        assert_eq!(g.get("@liquidCount", false).unwrap().num, 11.0);
        // `lookup item 0` -> copper; `sensor @copper @id` -> 0.
        let copper = content.item_id("copper").expect("copper");
        let copper_ref = ContentRef::new(ContentType::Item, copper.raw());
        assert_eq!(
            g.lookup_content(ContentType::Item, 0),
            Some(LogicObject::Content(copper_ref))
        );
        assert_eq!(g.lookup_logic_id(copper_ref), 0);
        // First/last block entries.
        let press = content.block_id("graphite-press").expect("graphite-press");
        assert_eq!(
            g.lookup_content(ContentType::Block, 0),
            Some(LogicObject::Content(ContentRef::new(
                ContentType::Block,
                press.raw()
            )))
        );
        let dummy = content.block_id("target-dummy").expect("target-dummy");
        let dummy_ref = ContentRef::new(ContentType::Block, dummy.raw());
        assert_eq!(
            g.lookup_content(ContentType::Block, 261),
            Some(LogicObject::Content(dummy_ref))
        );
        assert_eq!(g.lookup_logic_id(dummy_ref), 261);
        // Teams stay index-based.
        assert_eq!(
            g.lookup_content(ContentType::Team, 3),
            Some(LogicObject::Team(3))
        );
        // `GlobalVars::new()` (no content) keeps empty tables.
        let empty = GlobalVars::new();
        assert_eq!(empty.lookup_content(ContentType::Item, 0), None);
        assert_eq!(
            empty.lookup_logic_id(ContentRef::new(ContentType::Item, 0)),
            -1
        );
    }
}
