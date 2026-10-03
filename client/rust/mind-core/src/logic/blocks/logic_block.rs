// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LogicBlock`/`LogicBuild` (plan 13 M3).
//!
//! Ported from `core/src/mindustry/world/blocks/logic/LogicBlock.java`: link
//! fixpoint, config compression, the revision-5 save codec, the privilege
//! surface and the per-tick executor budget driver.

use std::io::{Cursor, Read, Write};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use indexmap::IndexSet;

use crate::content::BlockKind;
use crate::entities::comp::{Building, Pos, TeamComp};
use crate::io::entity::{EntityReader, EntityWriter};
use crate::logic::assembler::Assembler;
use crate::logic::executor::{Executor, Instruction};
use crate::logic::value::{LogicObject, VarRef, conv};
use crate::world::behavior::BuildingBehavior;
use crate::world::config::ConfigValue;
use crate::world::update::edelta;

use super::io::{CellValue, write_variable};
use super::{
    LogicLink, block_kind_of, block_name_of, block_size_of, build_at, is_valid_building,
    link_name_for, logic_def_of, privileged_of,
};

/// `LogicBlock.maxByteLen`.
pub const MAX_BYTE_LEN: i32 = 1024 * 100;
/// `LogicBlock.maxCompressedLen`.
pub const MAX_COMPRESSED_LEN: usize = 16_000;
/// `LogicBlock.maxLinks`.
pub const MAX_LINKS: i32 = 6000;
/// `LogicBlock.maxNameLength`.
pub const MAX_NAME_LENGTH: usize = 32;

/// Runtime state of one logic processor (`LogicBuild`).
#[derive(Component, Debug)]
pub struct LogicBlockState {
    /// Program source.
    pub code: String,
    /// Per-processor VM.
    pub executor: Executor,
    /// Instruction accumulation (`LogicBuild.accumulator`).
    pub accumulator: f32,
    /// Links.
    pub links: Vec<LogicLink>,
    /// Link name → index cache.
    pub link_map: Option<indexmap::IndexMap<String, usize>>,
    /// Duplicate-link check has run.
    pub checked_duplicates: bool,
    /// Instructions per tick.
    pub ipt: i32,
    /// World-processor tag (max 32 chars).
    pub tag: Option<String>,
    /// World-processor icon tag.
    pub icon_tag: char,
    /// `@links` variable.
    pub links_var: Option<VarRef>,
}

impl LogicBlockState {
    /// Creates a processor state with an empty program.
    pub fn new(ipt: i32) -> Self {
        Self {
            code: String::new(),
            executor: Executor::new(),
            accumulator: 0.0,
            links: Vec::new(),
            link_map: None,
            checked_duplicates: false,
            ipt,
            tag: None,
            icon_tag: '\0',
            links_var: None,
        }
    }

    /// `LogicBuild.optionalLink(name)`.
    pub fn optional_link(&self, name: &str) -> Option<Entity> {
        if name.is_empty() {
            return None;
        }
        let last = name.as_bytes()[name.len() - 1];
        if !last.is_ascii_digit() {
            return None;
        }
        self.links
            .iter()
            .find(|link| link.name == name && link.valid)
            .and_then(|link| link.last_build)
    }

    /// `LogicBuild.findLinkName(block)` using the current link list.
    pub fn find_link_name_for(&self, world: &World, other: Entity) -> String {
        let bname = block_name_of(world, other)
            .map(|name| link_name_for(&name))
            .unwrap_or_default();
        find_link_name(&self.links, &bname)
    }

    /// `LogicBuild.relativeConnections()`.
    pub fn relative_connections(&self, world: &World, e: Entity) -> Vec<LogicLink> {
        let (ox, oy) = world
            .get::<Building>(e)
            .map(|b| (b.tile.x() as i32, b.tile.y() as i32))
            .unwrap_or((0, 0));
        self.links
            .iter()
            .map(|link| {
                let mut copy = link.copy();
                copy.x -= ox;
                copy.y -= oy;
                copy
            })
            .collect()
    }

    /// `LogicBuild.updateCode(str, keep, assemble)` (assemble hook omitted; the
    /// plan-06 filter hook is a later milestone).
    pub fn update_code(&mut self, world: &mut World, e: Entity, keep: bool) {
        self.link_map = None;
        let def = logic_def_of(world, e).unwrap_or_default();
        let privileged = def.privileged;
        let ipt = self.ipt;
        let code = self.code.clone();
        // Content/sound constants live in the optional `GlobalVars` resource
        // (installed at content init); fall back to static globals otherwise.
        let globals = world
            .get_resource::<crate::logic::globals::GlobalVars>()
            .cloned()
            .unwrap_or_default();
        let mut asm = match Assembler::assemble_with(&code, privileged, globals) {
            Ok(asm) => asm,
            Err(_) => {
                self.code = String::new();
                Assembler::new()
            }
        };

        // Store connections as object constants.
        for link in self.links.iter_mut() {
            let cur = build_at(world, link.x, link.y);
            link.valid = valid_link(world, e, privileged, cur);
            if link.valid
                && let Some(build) = cur
            {
                link.logic_var =
                    Some(asm.put_obj_const(&link.name, Some(LogicObject::Building(build))));
            }
        }

        // Store link objects.
        self.executor.links.clear();
        self.executor.link_ids.clear();
        for link in &self.links {
            if link.valid
                && let Some(build) = build_at(world, link.x, link.y)
            {
                self.executor.links.push(build);
                self.executor.link_ids.insert(build.index().index() as i32);
            }
        }

        self.links_var = Some(asm.put_num_const("@links", self.executor.links.len() as f64));
        asm.put_num_const("@ipt", ipt as f64);

        if keep {
            for id in self.executor.var_ids.clone() {
                let old = self.executor.arena.get(id).clone();
                if old.constant {
                    continue;
                }
                if let Some(dest) = asm.get_var(&old.name)
                    && !asm.arena.get(dest.id()).constant
                {
                    asm.arena.get_mut(dest.id()).set_from(&old);
                }
            }
        }

        // `@this` / `@thisx` / `@thisy`.
        if let Some(this_id) = asm.arena.get_id("@this") {
            asm.arena
                .get_mut(this_id)
                .set_const(Some(LogicObject::Building(e)));
        }
        if let Some(building) = world.get::<Building>(e) {
            let tx = building.tile.x() as f32;
            let ty = building.tile.y() as f32;
            asm.put_num_const("@thisx", conv(tx) as f64);
            asm.put_num_const("@thisy", conv(ty) as f64);
        }

        self.executor.build = Some(e);
        self.executor.build_ipt = ipt;
        self.executor.load(asm);

        // `@unit` object survives a code reload (`executor.unit.objval = oldUnit`).
        // The assembler bootstrap leaves `@unit` null, so nothing to restore here.
    }

    /// `LogicBuild.updateLinks()`.
    pub fn update_links(&mut self, world: &World) {
        let Some(links_var) = self.links_var else {
            return;
        };
        let valids = self.links.iter().filter(|link| link.valid).count();
        self.executor.links.clear();
        self.executor.link_ids.clear();
        for link in &self.links {
            if link.valid
                && let Some(build) = build_at(world, link.x, link.y)
            {
                self.executor.links.push(build);
                self.executor.link_ids.insert(build.index().index() as i32);
            }
        }
        self.executor.arena.get_mut(links_var.id()).num = valids as f64;
    }

    /// `LogicBlock.configure(Integer/Point2)` link toggle.
    pub fn toggle_link(&mut self, world: &mut World, e: Entity, target: Entity) {
        let privileged = privileged_of(world, e);
        if !valid_link(world, e, privileged, Some(target)) {
            return;
        }
        let mut removed = false;
        let mut i = 0;
        while i < self.links.len() {
            if build_at(world, self.links[i].x, self.links[i].y) == Some(target) {
                self.links[i].try_set(&mut self.executor, None);
                self.links.remove(i);
                removed = true;
            } else {
                i += 1;
            }
        }
        if !removed {
            let name = self.find_link_name_for(world, target);
            let (x, y) = world
                .get::<Building>(target)
                .map(|b| (b.tile.x() as i32, b.tile.y() as i32))
                .unwrap_or((0, 0));
            let mut link = LogicLink::new(name, x, y);
            link.valid = true;
            link.last_build = Some(target);
            link.try_set(&mut self.executor, Some(target));
            self.links.push(link);
        }
        self.update_links(world);
    }

    /// `LogicBuild` one-time duplicate-link removal.
    fn remove_duplicate_links(&mut self, world: &World) {
        let mut seen = IndexSet::new();
        let mut remove = Vec::new();
        for (i, link) in self.links.iter().enumerate() {
            if let Some(build) = build_at(world, link.x, link.y)
                && !seen.insert(build.index())
            {
                remove.push(i);
            }
        }
        for i in remove.into_iter().rev() {
            self.links.remove(i);
        }
    }

    /// `LogicBuild.updateTile` link fixpoint. Returns whether links changed.
    fn fixpoint_links(&mut self, world: &World, e: Entity, privileged: bool) -> bool {
        let mut changed = false;
        let mut updates = true;
        while updates {
            updates = false;
            let mut i = 0;
            while i < self.links.len() {
                let (x, y) = (self.links[i].x, self.links[i].y);
                let cur = build_at(world, x, y);
                let valid = valid_link(world, e, privileged, cur);
                let last_block = self.links[i]
                    .last_build
                    .and_then(|build| block_kind_of(world, build));
                if self.links[i].last_build.is_none() {
                    self.links[i].last_build = cur;
                }
                if valid != self.links[i].valid || self.links[i].last_build != cur {
                    self.links[i].last_build = cur;
                    changed = true;
                    self.links[i].valid = valid;
                    self.links[i].try_set(&mut self.executor, None);

                    if valid && let Some(build) = cur {
                        let block = block_kind_of(world, build);
                        let name_mismatch = (last_block.is_some() && block != last_block)
                            || (last_block.is_none()
                                && !self.links[i].name.starts_with(&link_name_for(
                                    &block_name_of(world, build).unwrap_or_default(),
                                )));
                        if name_mismatch {
                            self.links[i].logic_var = None;
                            let new_name = self.find_link_name_for(world, build);
                            self.links[i].name = new_name;
                        }

                        // Remove redundant links to the same building.
                        let mut j = 0;
                        while j < self.links.len() {
                            let same = j != i
                                && build_at(world, self.links[j].x, self.links[j].y) == Some(build);
                            if same {
                                self.links[j].try_set(&mut self.executor, None);
                                self.links.remove(j);
                                if j < i {
                                    i -= 1;
                                }
                            } else {
                                j += 1;
                            }
                        }

                        self.links[i].try_set(&mut self.executor, Some(build));
                        updates = true;
                        break;
                    }
                }
                i += 1;
            }
        }
        changed
    }

    /// `LogicBuild.readCompressed(data, relative)`.
    pub fn read_compressed(
        &mut self,
        world: &mut World,
        e: Entity,
        data: &[u8],
        relative: bool,
    ) -> Result<(), std::io::Error> {
        let mut decoder = ZlibDecoder::new(Cursor::new(data));
        let mut version = [0u8; 1];
        decoder.read_exact(&mut version)?;

        let mut len_bytes = [0u8; 4];
        decoder.read_exact(&mut len_bytes)?;
        let byte_len = i32::from_be_bytes(len_bytes);
        if byte_len > MAX_BYTE_LEN {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Malformed logic data",
            ));
        }
        let mut bytes = vec![0u8; byte_len.max(0) as usize];
        decoder.read_exact(&mut bytes)?;

        let mut count_bytes = [0u8; 4];
        decoder.read_exact(&mut count_bytes)?;
        let total = i32::from_be_bytes(count_bytes).clamp(0, MAX_LINKS);

        self.links.clear();
        if version[0] == 0 {
            for _ in 0..total {
                let mut skip = [0u8; 4];
                decoder.read_exact(&mut skip)?;
            }
        } else {
            let mut used = IndexSet::new();
            let (tx, ty) = world
                .get::<Building>(e)
                .map(|b| (b.tile.x() as i32, b.tile.y() as i32))
                .unwrap_or((0, 0));
            for _ in 0..total {
                let mut name_len = [0u8; 2];
                decoder.read_exact(&mut name_len)?;
                let name_len = u16::from_be_bytes(name_len) as usize;
                let mut name_bytes = vec![0u8; name_len];
                decoder.read_exact(&mut name_bytes)?;
                let mut name = String::from_utf8_lossy(&name_bytes).into_owned();

                let mut xy = [0u8; 4];
                decoder.read_exact(&mut xy)?;
                let mut x = i16::from_be_bytes([xy[0], xy[1]]) as i32;
                let mut y = i16::from_be_bytes([xy[2], xy[3]]) as i32;
                if relative {
                    x += tx;
                    y += ty;
                }

                if let Some(build) = build_at(world, x, y) {
                    if !used.insert(build.index()) {
                        continue;
                    }
                    let best = block_name_of(world, build)
                        .map(|n| link_name_for(&n))
                        .unwrap_or_default();
                    if !name.starts_with(&best) {
                        name = find_link_name(&self.links, &best);
                    }
                }
                self.links.push(LogicLink::new(name, x, y));
            }
        }

        self.code = String::from_utf8_lossy(&bytes).into_owned();
        self.update_code(world, e, false);
        Ok(())
    }

    /// `LogicBlock.compress(code, links)`.
    pub fn compressed_config(&self, world: &World, e: Entity) -> Vec<u8> {
        compress(&self.code, &self.relative_connections(world, e))
    }
}

/// `LogicBlock.findLinkName(block)`: lowest free `bname{n}` (`n >= 1`).
pub fn find_link_name(links: &[LogicLink], bname: &str) -> String {
    let mut taken = IndexSet::new();
    let mut max = 1;
    for link in links {
        if let Some(rest) = link.name.strip_prefix(bname)
            && let Ok(value) = rest.parse::<i32>()
        {
            taken.insert(value);
            max = max.max(value);
        }
    }
    for i in 1..(max + 2) {
        if !taken.contains(&i) {
            return format!("{bname}{i}");
        }
    }
    format!("{bname}0")
}

/// `LogicBlock.validLink(other)` predicate.
pub fn valid_link(world: &World, this: Entity, privileged: bool, other: Option<Entity>) -> bool {
    let Some(other) = other else {
        return false;
    };
    if !is_valid_building(world, other) {
        return false;
    }
    if block_kind_of(world, other) == Some(BlockKind::ConstructBlock) {
        return false;
    }
    if privileged {
        // `!(privileged && !worldProcessorPlayerLink && other.team == defaultTeam)`;
        // `worldProcessorPlayerLink` defaults to true (plan 12).
        return true;
    }
    if privileged_of(world, other) {
        return false;
    }
    let this_team = world.get::<TeamComp>(this).map(|t| t.team);
    let other_team = world.get::<TeamComp>(other).map(|t| t.team);
    if this_team != other_team {
        return false;
    }
    let range = logic_def_of(world, this)
        .map(|def| def.range)
        .unwrap_or(0.0);
    within(world, this, other, range)
}

fn within(world: &World, this: Entity, other: Entity, range: f32) -> bool {
    let (Some(a), Some(b)) = (world.get::<Pos>(this), world.get::<Pos>(other)) else {
        return false;
    };
    let size = block_size_of(world, other).unwrap_or(1) as f32;
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt() <= range + size * crate::world::block::TILE_SIZE / 2.0
}

/// `LogicBlock.compress(byte[], links)` (`DeflaterOutputStream`, zlib level 6).
pub fn compress(code: &str, links: &[LogicLink]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = ZlibEncoder::new(&mut out, Compression::new(6));
        // version 1
        let _ = encoder.write_all(&[1u8]);
        let bytes = code.as_bytes();
        let _ = encoder.write_all(&(bytes.len() as i32).to_be_bytes());
        let _ = encoder.write_all(bytes);
        let _ = encoder.write_all(&(links.len() as i32).to_be_bytes());
        for link in links {
            let name = link.name.as_bytes();
            let _ = encoder.write_all(&(name.len() as u16).to_be_bytes());
            let _ = encoder.write_all(name);
            let _ = encoder.write_all(&(link.x as i16).to_be_bytes());
            let _ = encoder.write_all(&(link.y as i16).to_be_bytes());
        }
        let _ = encoder.finish();
    }
    out
}

/// `LogicBlock.LogicBuild` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct LogicBlockBehavior;

impl BuildingBehavior for LogicBlockBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        let def = logic_def_of(world, e).unwrap_or_default();
        let mut state = LogicBlockState::new(def.ipt);
        state.executor.privileged = def.privileged;
        state.executor.build = Some(e);
        world.entity_mut(e).insert(state);
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        let privileged = privileged_of(world, e);
        let accessible = super::accessible(privileged, super::rules_ref(world));
        match value {
            ConfigValue::Bytes(data) => {
                if !accessible {
                    return;
                }
                if let Some(mut state) = world.entity_mut(e).take::<LogicBlockState>() {
                    let _ = state.read_compressed(world, e, &data, true);
                    world.entity_mut(e).insert(state);
                }
            }
            ConfigValue::String(tag) => {
                if !accessible || !privileged {
                    return;
                }
                if tag.len() < MAX_NAME_LENGTH
                    && let Some(mut state) = world.get_mut::<LogicBlockState>(e)
                {
                    state.tag = Some(tag);
                }
            }
            ConfigValue::Number(number) => {
                if !accessible || !privileged {
                    return;
                }
                if let Some(mut state) = world.get_mut::<LogicBlockState>(e) {
                    state.icon_tag = char::from_u32(number as u32).unwrap_or('\0');
                }
            }
            ConfigValue::Point2(x, y) => {
                if !accessible {
                    return;
                }
                if let Some(target) = build_at(world, x, y)
                    && let Some(mut state) = world.entity_mut(e).take::<LogicBlockState>()
                {
                    state.toggle_link(world, e, target);
                    world.entity_mut(e).insert(state);
                }
            }
            ConfigValue::Building(target) => {
                if !accessible {
                    return;
                }
                if let Some(mut state) = world.entity_mut(e).take::<LogicBlockState>() {
                    state.toggle_link(world, e, target);
                    world.entity_mut(e).insert(state);
                }
            }
            _ => {}
        }
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        let Some(state) = world.get::<LogicBlockState>(e) else {
            return ConfigValue::Bytes(Default::default());
        };
        ConfigValue::Bytes(state.compressed_config(world, e).into())
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(def) = logic_def_of(world, e) else {
            return;
        };
        let Some(mut state) = world.entity_mut(e).take::<LogicBlockState>() else {
            return;
        };

        state.executor.team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);

        if !state.checked_duplicates {
            state.checked_duplicates = true;
            state.remove_duplicate_links(world);
        }

        let changed = state.fixpoint_links(world, e, def.privileged);
        if changed {
            state.update_links(world);
        }

        // `state.rules.disableWorldProcessors && privileged` gate (plan 12).
        let disabled_world_processors = world
            .get_resource::<super::LogicRulesRes>()
            .map(|rules| rules.0.disable_world_processors())
            .unwrap_or(false);
        if disabled_world_processors && def.privileged {
            world.entity_mut(e).insert(state);
            return;
        }

        let enabled = world.get::<Building>(e).is_some_and(|b| b.enabled);
        if enabled && state.executor.initialized() {
            let step = edelta(world, e);
            let ipt = state.ipt as f32;
            state
                .executor
                .run_budget(world, &mut state.accumulator, step, ipt);
        }

        world.entity_mut(e).insert(state);
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        5
    }

    fn write(&self, world: &World, e: Entity, w: &mut EntityWriter) {
        let Some(state) = world.get::<LogicBlockState>(e) else {
            return;
        };
        let def = logic_def_of(world, e).unwrap_or_default();

        let compressed = compress(&state.code, &state.links);
        w.i(compressed.len() as i32);
        w.bytes(&compressed);

        let unit = state
            .executor
            .arena
            .get_id("@unit")
            .map(|id| state.executor.arena.get(id));
        let write_unit = unit.is_some_and(|var| var.is_obj && var.obj.is_some());
        let count = state
            .executor
            .var_ids
            .iter()
            .filter(|id| {
                let var = state.executor.arena.get(**id);
                !(var.is_obj && var.obj.is_none())
            })
            .count()
            + usize::from(write_unit);
        w.i(count as i32);

        if write_unit {
            let _ = w.str("@unit");
            if let Some(var) = unit
                && let Some(obj) = &var.obj
            {
                let _ = super::io::write_object(w, world, obj);
            }
        }

        for id in &state.executor.var_ids {
            let var = state.executor.arena.get(*id);
            if var.is_obj && var.obj.is_none() {
                continue;
            }
            let cell = CellValue::from_lvar(var);
            let _ = write_variable(w, world, &var.name, &cell);
        }

        // Legacy memory region (unused).
        w.i(0);

        if def.privileged {
            w.s((state.ipt.clamp(1, def.max_ipt)) as i16);
        } else {
            let base = def.ipt.max(1);
            w.s(if state.ipt == base {
                0
            } else {
                state.ipt.clamp(1, base) as i16
            });
        }

        match &state.tag {
            Some(tag) => {
                w.b(1);
                let _ = w.str(tag);
            }
            None => w.b(0),
        }
        w.s(state.icon_tag as i16);

        let waits: Vec<(usize, f32)> = state
            .executor
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(i, instruction)| match instruction {
                Instruction::Wait { cur_time, .. } => Some((i, *cur_time as f32)),
                _ => None,
            })
            .collect();
        w.s(waits.len() as i16);
        for (index, value) in waits {
            w.s(index as i16);
            w.f(value);
        }
        w.f(state.accumulator);
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut EntityReader, revision: u8) {
        let Some(mut state) = world.entity_mut(e).take::<LogicBlockState>() else {
            return;
        };

        if revision >= 1 {
            if let Ok(comp_len) = r.i()
                && let Ok(bytes) = r.bytes(comp_len.max(0) as usize)
            {
                let bytes = bytes.to_vec();
                let _ = state.read_compressed(world, e, &bytes, false);
            }
        } else {
            let mut code = String::new();
            let mut total = 0i16;
            if let Ok(value) = r.str() {
                code = value;
            }
            if let Ok(value) = r.s() {
                total = value;
            }
            for _ in 0..total.max(0) {
                let _ = r.i();
            }
            state.code = code;
            state.links.clear();
            state.update_code(world, e, false);
        }

        // Variables.
        let var_count = r.i().unwrap_or(0).max(0);
        let mut values: Vec<(String, CellValue)> = Vec::new();
        for _ in 0..var_count {
            match super::io::read_variable(r, world) {
                Ok(pair) => values.push(pair),
                Err(_) => break,
            }
        }

        // Legacy memory region skip.
        let memory = r.i().unwrap_or(0).max(0);
        let _ = r.skip((memory as usize) * 8);

        let def = logic_def_of(world, e).unwrap_or_default();
        if def.privileged && revision >= 2 {
            let raw = r.s().unwrap_or(state.ipt as i16) as i32;
            state.ipt = raw.clamp(1, def.max_ipt);
        }
        if !def.privileged && revision >= 5 {
            let raw = r.s().unwrap_or(0) as i32;
            if raw != 0 {
                state.ipt = raw.clamp(1, def.ipt.max(1));
            }
        }
        if revision >= 3 {
            let exists = r.b().unwrap_or(0);
            state.tag = if exists != 0 { r.str().ok() } else { None };
            state.icon_tag = char::from_u32(r.us().unwrap_or(0) as u32).unwrap_or('\0');
        }

        let mut waits: Vec<(usize, f32)> = Vec::new();
        let mut accumulator = 0.0f32;
        if revision >= 4 {
            let waits_count = r.us().unwrap_or(0) as usize;
            for _ in 0..waits_count {
                let index = r.us().unwrap_or(0) as usize;
                let value = r.f().unwrap_or(0.0);
                waits.push((index, value));
            }
            accumulator = r.f().unwrap_or(0.0);
        }

        // Apply stored variables by name.
        for (name, cell) in &values {
            if let Some(id) = state.executor.arena.get_id(name) {
                let constant = state.executor.arena.get(id).constant;
                if !constant || name == "@unit" {
                    cell.apply(state.executor.arena.get_mut(id));
                }
            }
        }
        // Apply wait timers by instruction index.
        for (index, value) in waits {
            if let Some(Instruction::Wait { cur_time, .. }) =
                state.executor.instructions.get_mut(index)
            {
                *cur_time = value as f64;
            }
        }
        state.accumulator = accumulator;

        world.entity_mut(e).insert(state);
    }
}
