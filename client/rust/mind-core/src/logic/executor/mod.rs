// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LExecutor` — the mlog virtual machine (M0 core).
//!
//! Ported from `core/src/mindustry/logic/LExecutor.java`. This milestone lands
//! the executor core (`load`, `runOnce`, the instruction budget) and the
//! arithmetic/control/IO instruction subset; the world/unit instructions land in
//! later milestones behind the same [`Instruction`] enum.

pub mod draw;
pub mod radar;
pub mod unit_control;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use indexmap::IndexSet;

use crate::logic::access::LAccess;
use crate::logic::assembler::Assembler;
use crate::logic::enums::{BlockFlag, LLocate, LUnitControl, RadarSort, RadarTarget};
use crate::logic::ops::{ConditionOp, LogicOp};
use crate::logic::statement::Statement;
use crate::logic::value::{LVar, LogicObject, VarArena, VarId, VarRef};
use crate::math::ArcRand;

/// `LExecutor.maxInstructions`.
pub const MAX_INSTRUCTIONS: usize = 1000;
/// `LExecutor.maxGraphicsBuffer`.
pub const MAX_GRAPHICS_BUFFER: usize = 256;
/// `LExecutor.maxDisplayBuffer`.
pub const MAX_DISPLAY_BUFFER: usize = 1024;
/// `LExecutor.maxTextBuffer`.
pub const MAX_TEXT_BUFFER: usize = 400;
/// `LExecutor` instruction scale (`LogicBlock` `maxInstructionScale`).
pub const MAX_INSTRUCTION_SCALE: f32 = 5.0;

/// One compiled instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    /// `NoopI` / `InvalidStatement`.
    Noop,
    /// `SetI`.
    Set {
        /// Destination.
        to: VarRef,
        /// Source.
        from: VarRef,
    },
    /// `OpI`.
    Op {
        /// Operator.
        op: LogicOp,
        /// Destination.
        dest: VarRef,
        /// Left operand.
        a: VarRef,
        /// Right operand.
        b: VarRef,
    },
    /// `SelectI`.
    Select {
        /// Condition.
        op: ConditionOp,
        /// Result.
        result: VarRef,
        /// First compare operand.
        c0: VarRef,
        /// Second compare operand.
        c1: VarRef,
        /// True branch.
        a: VarRef,
        /// False branch.
        b: VarRef,
    },
    /// `JumpI`.
    Jump {
        /// Condition.
        op: ConditionOp,
        /// First operand.
        value: VarRef,
        /// Second operand.
        compare: VarRef,
        /// Destination index (`-1` = no jump).
        dest: i32,
    },
    /// `EndI`.
    End,
    /// `StopI`.
    Stop,
    /// `WaitI`.
    Wait {
        /// Seconds.
        value: VarRef,
        /// Elapsed sim seconds (`WaitI.curTime`).
        cur_time: f64,
    },
    /// `PrintI`.
    Print {
        /// Value.
        value: VarRef,
    },
    /// `PrintCharI`.
    PrintChar {
        /// Character value.
        value: VarRef,
    },
    /// `FormatI`.
    Format {
        /// Format string.
        value: VarRef,
    },
    /// `LookupI` (always null without `logicids.dat`).
    Lookup {
        /// Result.
        result: VarRef,
        /// Logic id.
        id: VarRef,
    },
    /// `PackColorI`.
    PackColor {
        /// Result.
        result: VarRef,
        /// Channels.
        r: VarRef,
        /// Green.
        g: VarRef,
        /// Blue.
        b: VarRef,
        /// Alpha.
        a: VarRef,
    },
    /// `UnpackColorI`.
    UnpackColor {
        /// Red.
        r: VarRef,
        /// Green.
        g: VarRef,
        /// Blue.
        b: VarRef,
        /// Alpha.
        a: VarRef,
        /// Packed color.
        value: VarRef,
    },
    /// `ReadI` (`LReadable` target, string char or query index fallback).
    Read {
        /// Output.
        output: VarRef,
        /// Target object.
        target: VarRef,
        /// Address/position.
        address: VarRef,
    },
    /// `WriteI` (`LWritable` target).
    Write {
        /// Input.
        input: VarRef,
        /// Target object.
        target: VarRef,
        /// Address/position.
        address: VarRef,
    },
    /// `GetLinkI`.
    GetLink {
        /// Output.
        output: VarRef,
        /// Link index.
        index: VarRef,
    },
    /// `DrawI` (`type` is the `GraphicsType` ordinal).
    Draw {
        /// Graphics type byte.
        type_: u8,
        /// X.
        x: VarRef,
        /// Y.
        y: VarRef,
        /// Parameter 1.
        p1: VarRef,
        /// Parameter 2.
        p2: VarRef,
        /// Parameter 3.
        p3: VarRef,
        /// Parameter 4.
        p4: VarRef,
    },
    /// `DrawFlushI`.
    DrawFlush {
        /// Target display.
        target: VarRef,
    },
    /// `PrintFlushI`.
    PrintFlush {
        /// Target message block.
        target: VarRef,
    },
    /// `SensorI`.
    Sensor {
        /// Output (numeric or object).
        to: VarRef,
        /// Target object.
        from: VarRef,
        /// Sensor selector (`LAccess` enum object or `Content` object).
        type_: VarRef,
    },
    /// `ControlI`.
    Control {
        /// Controlled accessor.
        type_: LAccess,
        /// Target object.
        target: VarRef,
        /// Parameter 1.
        p1: VarRef,
        /// Parameter 2.
        p2: VarRef,
        /// Parameter 3.
        p3: VarRef,
        /// Parameter 4.
        p4: VarRef,
    },
    /// `SetPropI`.
    SetProp {
        /// Property selector (`LAccess` or `Content` object).
        type_: VarRef,
        /// Target object.
        of: VarRef,
        /// Assigned value.
        value: VarRef,
    },
    /// `RadarI`.
    Radar {
        /// Target filters.
        targets: [RadarTarget; 3],
        /// Sort key.
        sort: RadarSort,
        /// Base `Ranged` object.
        radar: VarRef,
        /// Ascending flag.
        sort_order: VarRef,
        /// Output object.
        output: VarRef,
    },
    /// `UnitBindI`.
    UnitBind {
        /// Unit type `Content` object.
        type_: VarRef,
    },
    /// `UnitControlI`.
    UnitControl {
        /// Command.
        type_: LUnitControl,
        /// Parameter 1.
        p1: VarRef,
        /// Parameter 2.
        p2: VarRef,
        /// Parameter 3.
        p3: VarRef,
        /// Parameter 4.
        p4: VarRef,
        /// Parameter 5.
        p5: VarRef,
    },
    /// `UnitLocateI`.
    UnitLocate {
        /// Locate mode.
        locate: LLocate,
        /// Building flag filter.
        flag: BlockFlag,
        /// Enemy flag.
        enemy: VarRef,
        /// Ore content.
        ore: VarRef,
        /// Output x.
        out_x: VarRef,
        /// Output y.
        out_y: VarRef,
        /// Output found flag.
        out_found: VarRef,
        /// Output building.
        out_build: VarRef,
    },
}

impl Instruction {
    /// Executes one instruction against `exec`.
    ///
    /// Takes `&mut self` because `WaitI.cur_time` is per-instruction mutable
    /// state (upstream keeps it on the instruction instance).
    pub fn run(&mut self, exec: &mut Executor, world: &mut World) {
        match self {
            Instruction::Noop => {}
            Instruction::End => {
                let len = exec.instructions.len() as f64;
                exec.set_num(exec.counter, len);
            }
            Instruction::Stop => {
                let counter = exec.counter;
                let cur = exec.arena.get(counter).num as i32;
                exec.set_num(counter, (cur - 1) as f64);
                exec.yielded = true;
                exec.stopped = true;
            }
            Instruction::Set { to, from } => {
                let from = from.id();
                let to = to.id();
                if exec.is_constant(to) {
                    return;
                }
                let value = exec.arena.get(from).clone();
                exec.arena.get_mut(to).set_from(&value);
            }
            Instruction::Op { op, dest, a, b } => {
                if exec.is_constant(dest.id()) {
                    return;
                }
                let a = exec.arena.get(a.id()).clone();
                let b = exec.arena.get(b.id()).clone();
                let value = op.eval(&a, &b, &mut exec.rng).num();
                exec.set_num(dest.id(), value);
            }
            Instruction::Select {
                op,
                result,
                c0,
                c1,
                a,
                b,
            } => {
                if exec.is_constant(result.id()) {
                    return;
                }
                let vs = exec.arena.get(c0.id()).clone();
                let vo = exec.arena.get(c1.id()).clone();
                let chosen = if op.test(&vs, &vo) { a.id() } else { b.id() };
                let value = exec.arena.get(chosen).clone();
                exec.arena.get_mut(result.id()).set_from(&value);
            }
            Instruction::Jump {
                op,
                value,
                compare,
                dest,
            } => {
                if *dest != -1 {
                    let va = exec.arena.get(value.id()).clone();
                    let vb = exec.arena.get(compare.id()).clone();
                    if op.test(&va, &vb) {
                        exec.set_num(exec.counter, *dest as f64);
                    }
                }
            }
            Instruction::Wait { value, cur_time } => {
                let wait_for = exec.arena.get(value.id()).num();
                if wait_for <= 0.0 {
                    // Just yield without executing the wait again.
                    let counter = exec.counter;
                    let cur = exec.arena.get(counter).num;
                    exec.set_num(counter, cur - 1.0);
                    exec.yielded = true;
                    *cur_time = 0.0;
                } else if *cur_time >= wait_for {
                    *cur_time = 0.0;
                } else {
                    // Skip back to self.
                    let counter = exec.counter;
                    let cur = exec.arena.get(counter).num;
                    exec.set_num(counter, cur - 1.0);
                    exec.yielded = true;
                    // `Time.delta / 60f` with `Time.delta == 1` at the fixed step.
                    *cur_time += 1.0 / 60.0;
                }
            }
            Instruction::Print { value } => {
                let value = exec.arena.get(value.id()).clone();
                exec.append_print(&value);
            }
            Instruction::PrintChar { value } => {
                let value = exec.arena.get(value.id()).clone();
                exec.append_print_char(&value);
            }
            Instruction::Format { value } => {
                let value = exec.arena.get(value.id()).clone();
                exec.append_format(&value);
            }
            Instruction::Lookup { result, .. } => {
                let result = result.id();
                if !exec.is_constant(result) {
                    exec.arena.get_mut(result).set_obj(None);
                }
            }
            Instruction::PackColor { result, r, g, b, a } => {
                let v = pack_color(
                    exec.arena.get(r.id()).numf(),
                    exec.arena.get(g.id()).numf(),
                    exec.arena.get(b.id()).numf(),
                    exec.arena.get(a.id()).numf(),
                );
                exec.set_num(result.id(), v);
            }
            Instruction::UnpackColor { r, g, b, a, value } => {
                let bits = exec.arena.get(value.id()).num();
                let (rr, gg, bb, aa) = unpack_color(bits);
                exec.set_num(r.id(), rr);
                exec.set_num(g.id(), gg);
                exec.set_num(b.id(), bb);
                exec.set_num(a.id(), aa);
            }
            Instruction::GetLink { output, index } => {
                let address = exec.arena.get(index.id()).numi();
                let linked = if address >= 0 {
                    exec.links.get(address as usize).copied()
                } else {
                    None
                };
                exec.set_obj(
                    output.id(),
                    linked.map(crate::logic::value::LogicObject::Building),
                );
            }
            Instruction::Read {
                output,
                target,
                address,
            } => {
                let target_obj = exec.arena.get(target.id()).value_obj().cloned();
                crate::logic::blocks::io::read_target(
                    world,
                    exec,
                    target_obj.as_ref(),
                    *address,
                    *output,
                );
            }
            Instruction::Write {
                input,
                target,
                address,
            } => {
                let target_obj = exec.arena.get(target.id()).value_obj().cloned();
                crate::logic::blocks::io::write_target(
                    world,
                    exec,
                    target_obj.as_ref(),
                    *address,
                    *input,
                );
            }
            Instruction::Draw {
                type_,
                x,
                y,
                p1,
                p2,
                p3,
                p4,
            } => {
                draw::pack_draw(exec, *type_, *x, *y, *p1, *p2, *p3, *p4);
            }
            Instruction::DrawFlush { target } => {
                let target_obj = exec.arena.get(target.id()).value_obj().cloned();
                crate::logic::blocks::io::flush_draw(world, exec, target_obj.as_ref());
                exec.graphics_buffer.clear();
            }
            Instruction::PrintFlush { target } => {
                let target_obj = exec.arena.get(target.id()).value_obj().cloned();
                crate::logic::blocks::io::flush_print(world, exec, target_obj.as_ref());
                exec.text_buffer.clear();
            }
            Instruction::Sensor { to, from, type_ } => {
                let target = exec.arena.get(from.id()).value_obj().cloned();
                let selector = exec.arena.get(type_.id()).clone();
                let sensed = match (&target, selector.value_obj()) {
                    (Some(t), Some(LogicObject::Enum(name))) => LAccess::from_name(name)
                        .map(|a| crate::logic::access::sense(world, t, a))
                        .unwrap_or(crate::logic::access::Sensed::Obj(None)),
                    (Some(t), Some(LogicObject::Content(c))) => crate::logic::access::Sensed::Num(
                        crate::logic::access::sense_content(world, t, *c),
                    ),
                    _ => crate::logic::access::Sensed::Obj(None),
                };
                let to = to.id();
                if exec.is_constant(to) {
                    return;
                }
                match sensed {
                    crate::logic::access::Sensed::Num(n) => exec.arena.get_mut(to).set_num(n),
                    crate::logic::access::Sensed::Obj(o) => exec.arena.get_mut(to).set_obj(o),
                }
            }
            Instruction::Control {
                type_,
                target,
                p1,
                p2,
                p3,
                p4,
            } => {
                let target_obj = exec.arena.get(target.id()).value_obj().cloned();
                let Some(target_obj) = target_obj else { return };
                let params = [
                    exec.arena.get(p1.id()).clone(),
                    exec.arena.get(p2.id()).clone(),
                    exec.arena.get(p3.id()).clone(),
                    exec.arena.get(p4.id()).clone(),
                ];
                let refs: Vec<&LVar> = params.iter().collect();
                crate::logic::access::control(world, &target_obj, *type_, &refs);
            }
            Instruction::SetProp { type_, of, value } => {
                let target = exec.arena.get(of.id()).value_obj().cloned();
                let selector = exec.arena.get(type_.id()).clone();
                let Some(target) = target else { return };
                match selector.value_obj() {
                    Some(LogicObject::Enum(name)) => {
                        if let Some(a) = LAccess::from_name(name) {
                            let value = exec.arena.get(value.id()).clone();
                            if let Some(obj) = value.value_obj().cloned() {
                                crate::logic::access::set_prop_obj(world, &target, a, &obj);
                            } else {
                                crate::logic::access::set_prop_num(world, &target, a, value.num());
                            }
                        }
                    }
                    Some(LogicObject::Content(c)) => {
                        let amount = exec.arena.get(value.id()).num();
                        crate::logic::access::set_prop_content(world, &target, *c, amount);
                    }
                    _ => {}
                }
            }
            Instruction::Radar {
                targets,
                sort,
                radar,
                sort_order,
                output,
            } => {
                let base = exec.arena.get(radar.id()).value_obj().cloned();
                let (base_x, base_y, team) = base_team_pos(world, base.as_ref());
                let order = exec.arena.get(sort_order.id()).as_bool();
                let found = radar::find(
                    &radar::candidates(world),
                    team,
                    *targets,
                    *sort,
                    order,
                    base_x,
                    base_y,
                );
                let output = output.id();
                if !exec.is_constant(output) {
                    exec.arena
                        .get_mut(output)
                        .set_obj(found.map(LogicObject::Unit));
                }
            }
            Instruction::UnitBind { type_ } => {
                let selector = exec.arena.get(type_.id()).clone();
                let type_id = match selector.value_obj() {
                    Some(LogicObject::Content(c)) => Some(c.id),
                    _ => None,
                };
                let bound = type_id.and_then(|id| {
                    let index = id as usize;
                    if exec.binds.len() <= index {
                        exec.binds.resize(index + 1, 0);
                    }
                    let cursor = &mut exec.binds[index];
                    let team = exec.team;
                    unit_control::bind_next(world, id, team, cursor)
                });
                let unit_var = exec.unit;
                if !exec.is_constant(unit_var) {
                    exec.arena
                        .get_mut(unit_var)
                        .set_obj(bound.map(LogicObject::Unit));
                }
            }
            Instruction::UnitControl {
                type_,
                p1,
                p2,
                p3,
                p4,
                p5: _,
            } => {
                if !unit_control::logic_unit_control_enabled(crate::logic::blocks::rules_ref(world))
                {
                    return;
                }
                let bound = exec.arena.get(exec.unit).value_obj().cloned();
                let Some(LogicObject::Unit(unit)) = bound else {
                    return;
                };
                let privileged = exec.privileged;
                let team = exec.team;
                if unit_control::check_logic_ai(world, team, privileged, unit, Some(unit), true)
                    .is_none()
                {
                    return;
                }
                unit_control::refresh_control_timer(world, unit);
                let params: Vec<LVar> = [*p1, *p2, *p3, *p4]
                    .iter()
                    .map(|v| exec.arena.get(v.id()).clone())
                    .collect();
                unit_control::apply_control(world, unit, *type_, &params);
            }
            Instruction::UnitLocate {
                out_x,
                out_y,
                out_found,
                out_build,
                ..
            } => {
                // Ore/building/spawn/damaged scans need plan 06/11 world queries
                // (quadtree/ore index); report "not found" until those land.
                set_output_num(exec, *out_x, 0.0);
                set_output_num(exec, *out_y, 0.0);
                set_output_num(exec, *out_found, 0.0);
                set_output_obj(exec, *out_build, None);
            }
        }
    }
}

/// Base `(x, y, team)` for a radar/locate instruction.
fn base_team_pos(world: &World, base: Option<&LogicObject>) -> (f32, f32, u8) {
    match base {
        Some(LogicObject::Building(e)) => {
            let pos = world.get::<crate::entities::comp::Pos>(*e);
            let team = world
                .get::<crate::entities::comp::TeamComp>(*e)
                .map(|t| t.team)
                .unwrap_or(0);
            (
                pos.map(|p| p.x).unwrap_or(0.0),
                pos.map(|p| p.y).unwrap_or(0.0),
                team,
            )
        }
        Some(LogicObject::Unit(e)) => {
            let pos = world.get::<crate::entities::comp::Pos>(*e);
            let team = world
                .get::<crate::entities::comp::TeamComp>(*e)
                .map(|t| t.team)
                .unwrap_or(0);
            (
                pos.map(|p| p.x).unwrap_or(0.0),
                pos.map(|p| p.y).unwrap_or(0.0),
                team,
            )
        }
        _ => (0.0, 0.0, 0),
    }
}

fn set_output_num(exec: &mut Executor, var: VarRef, value: f64) {
    let id = var.id();
    if !exec.is_constant(id) {
        exec.arena.get_mut(id).set_num(value);
    }
}

fn set_output_obj(exec: &mut Executor, var: VarRef, value: Option<LogicObject>) {
    let id = var.id();
    if !exec.is_constant(id) {
        exec.arena.get_mut(id).set_obj(value);
    }
}

/// `Color.toDoubleBits` for clamped `[0,1]` channel inputs (`PackColorI`).
pub fn pack_color(r: f32, g: f32, b: f32, a: f32) -> f64 {
    let clamp = |v: f32| v.clamp(0.0, 1.0);
    let bits = (((clamp(r) * 255.0) as i32) << 24)
        | (((clamp(g) * 255.0) as i32) << 16)
        | (((clamp(b) * 255.0) as i32) << 8)
        | ((clamp(a) * 255.0) as i32);
    f64::from_bits(bits as u32 as u64)
}

/// `Color.fromDouble` inverse for `UnpackColorI` (returns `0..1` channels).
pub fn unpack_color(bits: f64) -> (f64, f64, f64, f64) {
    let value = bits.to_bits() as u32;
    let r = ((value >> 24) & 0xff) as f64 / 255.0;
    let g = ((value >> 16) & 0xff) as f64 / 255.0;
    let b = ((value >> 8) & 0xff) as f64 / 255.0;
    let a = (value & 0xff) as f64 / 255.0;
    (r, g, b, a)
}

/// The mlog virtual machine.
#[derive(Debug)]
pub struct Executor {
    /// Variable arena (moved from the assembler on `load`).
    pub arena: VarArena,
    /// Compiled instructions.
    pub instructions: Vec<Instruction>,
    /// Visible non-constant variable ids.
    pub var_ids: Vec<VarId>,
    /// `@counter`.
    pub counter: VarId,
    /// `@unit`.
    pub unit: VarId,
    /// `@this`.
    pub thisv: VarId,
    /// `@ipt`.
    pub ipt: VarId,
    /// `@queries`.
    pub query_result: Option<VarId>,
    /// Privileged (world processor) executor.
    pub privileged: bool,
    /// A `wait`/`stop` yielded this tick.
    pub yielded: bool,
    /// `stop` halted the program.
    pub stopped: bool,
    /// Packed draw command buffer (cap 256).
    pub graphics_buffer: Vec<u64>,
    /// Deviation 7: when set, `draw` packing is skipped (headless default).
    pub skip_draw_pack: bool,
    /// Text print buffer (cap 400).
    pub text_buffer: String,
    /// Global logic RNG stream.
    pub rng: ArcRand,
    /// Owning build (for `@this`/ipt).
    pub build: Option<Entity>,
    /// Owning build's instructions-per-tick (`build.ipt`).
    pub build_ipt: i32,
    /// Team of the owning build (`LogicBuild.team`).
    pub team: u8,
    /// Links (valid only), in link declaration order.
    pub links: Vec<Entity>,
    /// Building ids of valid links (`LogicBuild.executor.linkIds`).
    pub link_ids: IndexSet<i32>,
    /// Per-unit-type binding cursor (`LExecutor.binds`).
    pub binds: Vec<u32>,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor {
    /// Creates an empty executor.
    pub fn new() -> Self {
        Self {
            arena: VarArena::new(),
            instructions: Vec::new(),
            var_ids: Vec::new(),
            counter: 0,
            unit: 0,
            thisv: 0,
            ipt: 0,
            query_result: None,
            privileged: false,
            yielded: false,
            stopped: false,
            graphics_buffer: Vec::new(),
            skip_draw_pack: false,
            text_buffer: String::new(),
            rng: ArcRand::new(0),
            build: None,
            build_ipt: 0,
            team: 0,
            links: Vec::new(),
            link_ids: IndexSet::new(),
            binds: Vec::new(),
        }
    }

    /// `LExecutor.initialized`.
    pub fn initialized(&self) -> bool {
        !self.instructions.is_empty()
    }

    /// `LExecutor.load`.
    pub fn load(&mut self, asm: Assembler) {
        self.arena = asm.arena;
        self.instructions = asm.instructions;
        self.privileged = asm.privileged;
        self.stopped = false;
        self.yielded = false;
        self.text_buffer.clear();
        self.graphics_buffer.clear();
        self.binds.clear();

        // Keep non-constant vars plus link constants (names not starting with `_`/`@`).
        self.var_ids = self
            .arena
            .cells
            .iter()
            .filter(|v| !v.constant || (!v.name.starts_with('_') && !v.name.starts_with('@')))
            .map(|v| v.id as VarId)
            .collect();

        self.counter = match self.arena.get_id("@counter") {
            Some(id) => id,
            None => self.arena.put_var("@counter"),
        };
        self.unit = match self.arena.get_id("@unit") {
            Some(id) => id,
            None => self.arena.put_obj_const("@unit", None),
        };
        self.thisv = match self.arena.get_id("@this") {
            Some(id) => id,
            None => self.arena.put_obj_const("@this", None),
        };
        // `ipt = builder.putConst("@ipt", build != null ? build.ipt : 0)`.
        let ipt_value = self.build_ipt as f64;
        self.ipt = self.arena.put_num_const("@ipt", ipt_value);
        if self.privileged {
            self.query_result = Some(self.arena.put_num_const("@queries", 0.0));
        }
    }

    fn is_constant(&self, id: VarId) -> bool {
        self.arena.get(id).constant
    }

    fn set_num(&mut self, id: VarId, value: f64) {
        self.arena.get_mut(id).set_num(value);
    }

    fn set_obj(&mut self, id: VarId, value: Option<LogicObject>) {
        self.arena.get_mut(id).set_obj(value);
    }

    /// Reads the numeric program counter.
    pub fn counter_value(&self) -> f64 {
        self.arena.get(self.counter).num
    }

    /// `LExecutor.runOnce`.
    pub fn run_once(&mut self, world: &mut World) {
        let len = self.instructions.len() as f64;
        let mut counter = self.arena.get(self.counter).num;
        if counter >= len || counter < 0.0 {
            counter = 0.0;
        }
        if counter < len {
            self.arena.get_mut(self.counter).is_obj = false;
            let index = counter as usize;
            self.arena.get_mut(self.counter).num = counter + 1.0;
            // Swap the instruction out so its per-instance mutable state
            // (`WaitI.cur_time`) can be updated without aliasing `self`.
            let mut instruction =
                std::mem::replace(&mut self.instructions[index], Instruction::Noop);
            instruction.run(self, world);
            self.instructions[index] = instruction;
        }
    }

    /// Runs up to `max` instructions (bounded).
    pub fn run(&mut self, world: &mut World, max: usize) {
        for _ in 0..max {
            let counter = self.arena.get(self.counter).num;
            if self.stopped
                || self.yielded
                || counter < 0.0
                || counter >= self.instructions.len() as f64
            {
                break;
            }
            self.run_once(world);
        }
    }

    /// Instruction-budget driver (`LogicBuild.updateTile` inner loop).
    pub fn run_budget(&mut self, world: &mut World, accumulator: &mut f32, edelta: f32, ipt: f32) {
        let max_scale = MAX_INSTRUCTION_SCALE;
        if *accumulator > max_scale * ipt {
            *accumulator = max_scale * ipt;
        }
        while *accumulator >= 1.0 {
            self.run_once(world);
            if self.yielded {
                self.yielded = false;
                break;
            }
            *accumulator -= 1.0;
        }
        *accumulator += edelta * ipt;
    }

    fn append_print(&mut self, value: &LVar) {
        let text = format_value(value);
        push_text(&mut self.text_buffer, &text);
    }

    fn append_print_char(&mut self, value: &LVar) {
        let code = value.numi();
        let text = if value.is_obj {
            match value.value_obj() {
                Some(LogicObject::Content(content)) => format!("content:{}", content.id),
                _ => String::new(),
            }
        } else if let Some(ch) = char::from_u32(code as u32) {
            ch.to_string()
        } else {
            String::new()
        };
        push_text(&mut self.text_buffer, &text);
    }

    fn append_format(&mut self, value: &LVar) {
        let text = format_value(value);
        // Replace the lowest `{N}` placeholder with the current buffer; upstream
        // `FormatI` substitutes into the text buffer.
        let replacement = self.text_buffer.clone();
        self.text_buffer = substitute_format(&text, &replacement);
        truncate_text(&mut self.text_buffer);
    }

    /// `executor.vars` name lookup (`optionalVar`).
    pub fn optional_var(&self, name: &str) -> Option<VarId> {
        self.var_ids
            .iter()
            .copied()
            .find(|id| self.arena.get(*id).name == name)
    }
}

/// `PrintI` value → string (`null`, string, content name, `[object]`).
pub fn format_value(value: &LVar) -> String {
    if !value.is_obj {
        let n = value.num();
        // integer rendering when within 1e-5 of an integer
        if (n - n.round()).abs() < 1e-5 {
            return format!("{}", n.round() as i64);
        }
        let text = format!("{n}");
        return text;
    }
    match &value.obj {
        None => "null".to_owned(),
        Some(LogicObject::Str(s)) => s.clone(),
        Some(LogicObject::Content(c)) => format!("content:{}", c.id),
        Some(LogicObject::Enum(name)) => (*name).to_owned(),
        Some(LogicObject::Team(id)) => format!("team:{id}"),
        Some(_) => "[object]".to_owned(),
    }
}

/// Appends text honoring the 400-char cap (`maxTextBuffer`).
pub fn push_text(buffer: &mut String, text: &str) {
    buffer.push_str(text);
    truncate_text(buffer);
}

fn truncate_text(buffer: &mut String) {
    if buffer.chars().count() > MAX_TEXT_BUFFER {
        let truncated: String = buffer.chars().take(MAX_TEXT_BUFFER).collect();
        *buffer = truncated;
    }
}

/// `FormatI`: replaces the lowest `{N}` placeholder with `replacement`.
fn substitute_format(text: &str, replacement: &str) -> String {
    let mut result = text.to_owned();
    for n in 0..10 {
        let token = format!("{{{n}}}");
        if let Some(pos) = result.find(&token) {
            result.replace_range(pos..pos + token.len(), replacement);
            break;
        }
    }
    result
}

/// Builds one instruction from a statement (`LStatement.build`).
pub fn build_statement(statement: &Statement, asm: &mut Assembler) -> Option<Instruction> {
    Some(match statement {
        Statement::Invalid {} => Instruction::Noop,
        Statement::Set { to, from } => {
            let from = asm.var(from);
            let to = asm.var(to);
            Instruction::Set { to, from }
        }
        Statement::Operation { op, dest, a, b } => {
            let a = asm.var(a);
            let b = asm.var(b);
            let dest = asm.var(dest);
            Instruction::Op {
                op: *op,
                dest,
                a,
                b,
            }
        }
        Statement::Select {
            op,
            result,
            comp0,
            comp1,
            a,
            b,
        } => {
            let c0 = asm.var(comp0);
            let c1 = asm.var(comp1);
            let av = asm.var(a);
            let bv = asm.var(b);
            let result = asm.var(result);
            Instruction::Select {
                op: *op,
                result,
                c0,
                c1,
                a: av,
                b: bv,
            }
        }
        Statement::Jump {
            dest_index,
            op,
            value,
            compare,
        } => {
            let value = asm.var(value);
            let compare = asm.var(compare);
            Instruction::Jump {
                op: *op,
                value,
                compare,
                dest: *dest_index,
            }
        }
        Statement::End {} => Instruction::End,
        Statement::Stop {} => Instruction::Stop,
        Statement::Wait { value } => {
            let value = asm.var(value);
            Instruction::Wait {
                value,
                cur_time: 0.0,
            }
        }
        Statement::Print { value } => Instruction::Print {
            value: asm.var(value),
        },
        Statement::PrintChar { value } => Instruction::PrintChar {
            value: asm.var(value),
        },
        Statement::Format { value } => Instruction::Format {
            value: asm.var(value),
        },
        Statement::Lookup { result, id, .. } => {
            let result = asm.var(result);
            let id = asm.var(id);
            Instruction::Lookup { result, id }
        }
        Statement::PackColor { result, r, g, b, a } => {
            let result = asm.var(result);
            let r = asm.var(r);
            let g = asm.var(g);
            let b = asm.var(b);
            let a = asm.var(a);
            Instruction::PackColor { result, r, g, b, a }
        }
        Statement::UnpackColor { r, g, b, a, value } => {
            let r = asm.var(r);
            let g = asm.var(g);
            let b = asm.var(b);
            let a = asm.var(a);
            let value = asm.var(value);
            Instruction::UnpackColor { r, g, b, a, value }
        }
        Statement::Read {
            output,
            target,
            address,
        } => {
            let output = asm.var(output);
            let target = asm.var(target);
            let address = asm.var(address);
            Instruction::Read {
                output,
                target,
                address,
            }
        }
        Statement::Write {
            input,
            target,
            address,
        } => {
            let input = asm.var(input);
            let target = asm.var(target);
            let address = asm.var(address);
            Instruction::Write {
                input,
                target,
                address,
            }
        }
        Statement::GetLink { output, address } => {
            let output = asm.var(output);
            let index = asm.var(address);
            Instruction::GetLink { output, index }
        }
        Statement::Draw {
            type_,
            x,
            y,
            p1,
            p2,
            p3,
            p4,
        } => {
            let x = asm.var(x);
            let y = asm.var(y);
            let p1 = asm.var(p1);
            let p2 = asm.var(p2);
            let p3 = asm.var(p3);
            let p4 = asm.var(p4);
            Instruction::Draw {
                type_: type_.ordinal() as u8,
                x,
                y,
                p1,
                p2,
                p3,
                p4,
            }
        }
        Statement::DrawFlush { target } => Instruction::DrawFlush {
            target: asm.var(target),
        },
        Statement::PrintFlush { target } => Instruction::PrintFlush {
            target: asm.var(target),
        },
        Statement::Sensor { to, from, type_ } => Instruction::Sensor {
            to: asm.var(to),
            from: asm.var(from),
            type_: asm.var(type_),
        },
        Statement::Control {
            type_,
            target,
            p1,
            p2,
            p3,
            p4,
        } => Instruction::Control {
            type_: *type_,
            target: asm.var(target),
            p1: asm.var(p1),
            p2: asm.var(p2),
            p3: asm.var(p3),
            p4: asm.var(p4),
        },
        Statement::SetProp { type_, of, value } => Instruction::SetProp {
            type_: asm.var(type_),
            of: asm.var(of),
            value: asm.var(value),
        },
        Statement::Radar {
            target1,
            target2,
            target3,
            sort,
            radar,
            sort_order,
            output,
        }
        | Statement::UnitRadar {
            target1,
            target2,
            target3,
            sort,
            radar,
            sort_order,
            output,
        } => Instruction::Radar {
            targets: [*target1, *target2, *target3],
            sort: *sort,
            radar: asm.var(radar),
            sort_order: asm.var(sort_order),
            output: asm.var(output),
        },
        Statement::UnitBind { type_ } => Instruction::UnitBind {
            type_: asm.var(type_),
        },
        Statement::UnitControl {
            type_,
            p1,
            p2,
            p3,
            p4,
            p5,
        } => Instruction::UnitControl {
            type_: *type_,
            p1: asm.var(p1),
            p2: asm.var(p2),
            p3: asm.var(p3),
            p4: asm.var(p4),
            p5: asm.var(p5),
        },
        Statement::UnitLocate {
            locate,
            flag,
            enemy,
            ore,
            out_x,
            out_y,
            out_found,
            out_build,
        } => Instruction::UnitLocate {
            locate: *locate,
            flag: *flag,
            enemy: asm.var(enemy),
            ore: asm.var(ore),
            out_x: asm.var(out_x),
            out_y: asm.var(out_y),
            out_found: asm.var(out_found),
            out_build: asm.var(out_build),
        },
        // Later-milestone instructions compile to a no-op for now so program
        // structure (indices/jumps) is preserved.
        _ => Instruction::Noop,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec(code: &str) -> Executor {
        let mut e = Executor::new();
        e.privileged = true;
        e.load(Assembler::assemble(code, true).unwrap());
        e
    }

    fn var_num(e: &Executor, name: &str) -> f64 {
        e.optional_var(name)
            .map(|id| e.arena.get(id).num())
            .unwrap_or(f64::NAN)
    }

    fn run(e: &mut Executor, max: usize) {
        let mut world = World::new();
        e.run(&mut world, max);
    }

    #[test]
    fn set_op_select_jump_run() {
        let mut e = exec("set result 5\nop add result result 2\n");
        run(&mut e, 10);
        assert_eq!(var_num(&e, "result"), 7.0);
    }

    #[test]
    fn jump_skips() {
        let mut e = exec("set x 0\njump skip always\nset x 1\nskip:\nset x 2\n");
        // label `skip` is statement index 3
        run(&mut e, 20);
        assert_eq!(var_num(&e, "x"), 2.0);
    }

    #[test]
    fn budget_clamping_limits_instructions() {
        // Build a straight-line program and clamp the accumulator.
        let mut code = String::new();
        for _ in 0..20 {
            code.push_str("op add n n 1\n");
        }
        let mut e = exec(&code);
        let mut acc = 1000.0f32;
        let mut world = World::new();
        e.run_budget(&mut world, &mut acc, 0.0, 2.0);
        // <= 5 * 2 = 10 instructions executed
        assert!(var_num(&e, "n") <= 10.0);
        assert!(acc <= 5.0 * 2.0 + 0.0);
    }

    #[test]
    fn pack_unpack_color_roundtrip() {
        let mut e = exec("packcolor c 1 0 0 1\nunpackcolor r g b a c\n");
        run(&mut e, 10);
        assert!((var_num(&e, "r") - 1.0).abs() < 1e-9);
        assert!((var_num(&e, "g") - 0.0).abs() < 1e-9);
        assert!((var_num(&e, "a") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn print_and_format() {
        let mut e = exec("print 5\nprint \"x\"\n");
        run(&mut e, 10);
        assert_eq!(e.text_buffer, "5x");
    }
}
