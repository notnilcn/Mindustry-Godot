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
use crate::logic::enums::{
    BlockFlag, CutsceneAction, FetchType, LLocate, LMarkerControl, LUnitControl, MessageType,
    QueryShape, QueryType, RadarSort, RadarTarget, TileLayer,
};
use crate::logic::fx::EffectEntry;
use crate::logic::ops::{ConditionOp, LogicOp};
use crate::logic::statement::{LogicRule, Statement};
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
    /// `QueryI` (privileged).
    Query {
        /// Query shape.
        shape: QueryShape,
        /// Query type.
        type_: QueryType,
        /// Team filter.
        team: VarRef,
        /// World x.
        x: VarRef,
        /// World y.
        y: VarRef,
        /// Width/radius.
        w: VarRef,
        /// Height.
        h: VarRef,
    },
    /// `FetchI` (privileged).
    Fetch {
        /// Fetch kind.
        type_: FetchType,
        /// Output.
        result: VarRef,
        /// Team.
        team: VarRef,
        /// Index.
        index: VarRef,
        /// Content filter.
        extra: VarRef,
    },
    /// `GetBlockI` (privileged).
    GetBlock {
        /// Layer.
        layer: TileLayer,
        /// Output.
        result: VarRef,
        /// Tile x.
        x: VarRef,
        /// Tile y.
        y: VarRef,
    },
    /// `SetBlockI` (privileged, host-gated).
    SetBlock {
        /// Layer.
        layer: TileLayer,
        /// Target block.
        block: VarRef,
        /// Tile x.
        x: VarRef,
        /// Tile y.
        y: VarRef,
        /// Team.
        team: VarRef,
        /// Rotation.
        rotation: VarRef,
    },
    /// `SpawnUnitI` (privileged, host-gated).
    SpawnUnit {
        /// Unit type.
        type_: VarRef,
        /// World x.
        x: VarRef,
        /// World y.
        y: VarRef,
        /// Facing degrees.
        rotation: VarRef,
        /// Team.
        team: VarRef,
        /// Output unit.
        result: VarRef,
        /// Spawn-effect toggle.
        effect: VarRef,
    },
    /// `SpawnBulletI` (privileged).
    SpawnBullet {
        /// Output bullet.
        result: VarRef,
        /// Bullet source.
        from: VarRef,
        /// Weapon/ammo selector.
        index: VarRef,
        /// World x.
        x: VarRef,
        /// World y.
        y: VarRef,
        /// Facing degrees.
        rotation: VarRef,
        /// Team.
        team: VarRef,
        /// Owner.
        owner: VarRef,
        /// Damage override.
        damage: VarRef,
        /// Velocity scale.
        velocity_scl: VarRef,
        /// Lifetime scale.
        life_scl: VarRef,
        /// Aim x.
        aim_x: VarRef,
        /// Aim y.
        aim_y: VarRef,
    },
    /// `ApplyEffectI` (privileged, host-gated).
    ApplyStatus {
        /// Clear instead of apply.
        clear: bool,
        /// Status effect.
        effect: VarRef,
        /// Target unit.
        unit: VarRef,
        /// Duration seconds.
        duration: VarRef,
    },
    /// `SenseWeatherI` (privileged).
    WeatherSense {
        /// Output flag.
        to: VarRef,
        /// Weather content.
        weather: VarRef,
    },
    /// `SetWeatherI` (privileged).
    WeatherSet {
        /// Weather content.
        weather: VarRef,
        /// Desired state.
        state: VarRef,
    },
    /// `SpawnWaveI` (privileged, host-gated).
    SpawnWave {
        /// Tile x.
        x: VarRef,
        /// Tile y.
        y: VarRef,
        /// Natural (skip-wave) toggle.
        natural: VarRef,
    },
    /// `SetRuleI` (privileged).
    SetRule {
        /// Rule.
        rule: LogicRule,
        /// Value.
        value: VarRef,
        /// Param 1.
        p1: VarRef,
        /// Param 2.
        p2: VarRef,
        /// Param 3.
        p3: VarRef,
        /// Param 4.
        p4: VarRef,
    },
    /// `FlushMessageI` (privileged).
    FlushMessage {
        /// Message kind.
        type_: MessageType,
        /// Duration seconds.
        duration: VarRef,
        /// Success output (`1`/`0`).
        out_success: VarRef,
    },
    /// `CutsceneI` (privileged).
    Cutscene {
        /// Action.
        action: CutsceneAction,
        /// Param 1.
        p1: VarRef,
        /// Param 2.
        p2: VarRef,
        /// Param 3.
        p3: VarRef,
        /// Param 4.
        p4: VarRef,
    },
    /// `EffectI` (privileged).
    Effect {
        /// Resolved `LogicFx` entry (`None` = no-op).
        effect: Option<EffectEntry>,
        /// World x.
        x: VarRef,
        /// World y.
        y: VarRef,
        /// Rotation.
        rotation: VarRef,
        /// Color bits.
        color: VarRef,
        /// Data object.
        data: VarRef,
    },
    /// `ExplosionI` (privileged, host-gated).
    Explosion {
        /// Team.
        team: VarRef,
        /// World x.
        x: VarRef,
        /// World y.
        y: VarRef,
        /// Radius.
        radius: VarRef,
        /// Damage.
        damage: VarRef,
        /// Air toggle.
        air: VarRef,
        /// Ground toggle.
        ground: VarRef,
        /// Pierce toggle.
        pierce: VarRef,
        /// Effect toggle.
        effect: VarRef,
    },
    /// `GetFlagI` (privileged).
    GetFlag {
        /// Output flag.
        result: VarRef,
        /// Flag key.
        flag: VarRef,
    },
    /// `SetFlagI` (privileged).
    SetFlag {
        /// Flag key.
        flag: VarRef,
        /// Value.
        value: VarRef,
    },
    /// `SetMarkerI` (privileged).
    SetMarker {
        /// Control.
        type_: LMarkerControl,
        /// Marker id.
        id: VarRef,
        /// Param 1.
        p1: VarRef,
        /// Param 2.
        p2: VarRef,
        /// Param 3.
        p3: VarRef,
    },
    /// `MakeMarkerI` (privileged).
    MakeMarker {
        /// Marker type name (compile-time string).
        type_: String,
        /// Marker id.
        id: VarRef,
        /// Tile x.
        x: VarRef,
        /// Tile y.
        y: VarRef,
        /// Replace toggle.
        replace: VarRef,
    },
    /// `PlaySoundI` (privileged).
    PlaySound {
        /// Positional toggle.
        positional: bool,
        /// Sound id.
        id: VarRef,
        /// Volume.
        volume: VarRef,
        /// Pitch.
        pitch: VarRef,
        /// Pan.
        pan: VarRef,
        /// World x.
        x: VarRef,
        /// World y.
        y: VarRef,
        /// Limit toggle.
        limit: VarRef,
    },
    /// `PlayMusicI` (privileged).
    PlayMusic {
        /// Music name.
        name: VarRef,
        /// Interrupt toggle.
        interrupt: VarRef,
    },
    /// `LocalePrintI` (privileged).
    LocalePrint {
        /// Key.
        name: VarRef,
    },
    /// `SyncI` (privileged).
    Sync {
        /// Variable.
        variable: VarRef,
    },
    /// `ClientDataI` (privileged, gated by `allow_logic_data` at build).
    ClientData {
        /// Channel.
        channel: VarRef,
        /// Value.
        value: VarRef,
        /// Reliable toggle.
        reliable: VarRef,
    },
}

impl Instruction {
    /// Visits every [`VarRef`] field (used to lower `Global` refs at load).
    pub(crate) fn for_each_var_ref_mut(&mut self, f: &mut dyn FnMut(&mut VarRef)) {
        match self {
            Instruction::Noop | Instruction::End | Instruction::Stop => {}
            Instruction::Set { to, from } => {
                f(to);
                f(from);
            }
            Instruction::Op { dest, a, b, .. } => {
                f(dest);
                f(a);
                f(b);
            }
            Instruction::Select {
                result,
                c0,
                c1,
                a,
                b,
                ..
            } => {
                f(result);
                f(c0);
                f(c1);
                f(a);
                f(b);
            }
            Instruction::Jump { value, compare, .. } => {
                f(value);
                f(compare);
            }
            Instruction::Wait { value, .. }
            | Instruction::Print { value }
            | Instruction::PrintChar { value }
            | Instruction::Format { value } => f(value),
            Instruction::Lookup { result, id } => {
                f(result);
                f(id);
            }
            Instruction::PackColor { result, r, g, b, a } => {
                f(result);
                f(r);
                f(g);
                f(b);
                f(a);
            }
            Instruction::UnpackColor { r, g, b, a, value } => {
                f(r);
                f(g);
                f(b);
                f(a);
                f(value);
            }
            Instruction::Read {
                output,
                target,
                address,
            } => {
                f(output);
                f(target);
                f(address);
            }
            Instruction::Write {
                input,
                target,
                address,
            } => {
                f(input);
                f(target);
                f(address);
            }
            Instruction::GetLink { output, index } => {
                f(output);
                f(index);
            }
            Instruction::Draw {
                x,
                y,
                p1,
                p2,
                p3,
                p4,
                ..
            } => {
                f(x);
                f(y);
                f(p1);
                f(p2);
                f(p3);
                f(p4);
            }
            Instruction::DrawFlush { target } | Instruction::PrintFlush { target } => f(target),
            Instruction::Sensor { to, from, type_ } => {
                f(to);
                f(from);
                f(type_);
            }
            Instruction::Control {
                target,
                p1,
                p2,
                p3,
                p4,
                ..
            } => {
                f(target);
                f(p1);
                f(p2);
                f(p3);
                f(p4);
            }
            Instruction::SetProp { type_, of, value } => {
                f(type_);
                f(of);
                f(value);
            }
            Instruction::Radar {
                radar,
                sort_order,
                output,
                ..
            } => {
                f(radar);
                f(sort_order);
                f(output);
            }
            Instruction::UnitBind { type_ } => f(type_),
            Instruction::UnitControl {
                p1, p2, p3, p4, p5, ..
            } => {
                f(p1);
                f(p2);
                f(p3);
                f(p4);
                f(p5);
            }
            Instruction::UnitLocate {
                enemy,
                ore,
                out_x,
                out_y,
                out_found,
                out_build,
                ..
            } => {
                f(enemy);
                f(ore);
                f(out_x);
                f(out_y);
                f(out_found);
                f(out_build);
            }
            Instruction::Query {
                team, x, y, w, h, ..
            } => {
                f(team);
                f(x);
                f(y);
                f(w);
                f(h);
            }
            Instruction::Fetch {
                result,
                team,
                index,
                extra,
                ..
            } => {
                f(result);
                f(team);
                f(index);
                f(extra);
            }
            Instruction::GetBlock { result, x, y, .. } => {
                f(result);
                f(x);
                f(y);
            }
            Instruction::SetBlock {
                block,
                x,
                y,
                team,
                rotation,
                ..
            } => {
                f(block);
                f(x);
                f(y);
                f(team);
                f(rotation);
            }
            Instruction::SpawnUnit {
                type_,
                x,
                y,
                rotation,
                team,
                result,
                effect,
            } => {
                f(type_);
                f(x);
                f(y);
                f(rotation);
                f(team);
                f(result);
                f(effect);
            }
            Instruction::SpawnBullet {
                result,
                from,
                index,
                x,
                y,
                rotation,
                team,
                owner,
                damage,
                velocity_scl,
                life_scl,
                aim_x,
                aim_y,
            } => {
                f(result);
                f(from);
                f(index);
                f(x);
                f(y);
                f(rotation);
                f(team);
                f(owner);
                f(damage);
                f(velocity_scl);
                f(life_scl);
                f(aim_x);
                f(aim_y);
            }
            Instruction::ApplyStatus {
                effect,
                unit,
                duration,
                ..
            } => {
                f(effect);
                f(unit);
                f(duration);
            }
            Instruction::WeatherSense { to, weather } => {
                f(to);
                f(weather);
            }
            Instruction::WeatherSet { weather, state } => {
                f(weather);
                f(state);
            }
            Instruction::SpawnWave { x, y, natural } => {
                f(x);
                f(y);
                f(natural);
            }
            Instruction::SetRule {
                value,
                p1,
                p2,
                p3,
                p4,
                ..
            } => {
                f(value);
                f(p1);
                f(p2);
                f(p3);
                f(p4);
            }
            Instruction::FlushMessage {
                duration,
                out_success,
                ..
            } => {
                f(duration);
                f(out_success);
            }
            Instruction::Cutscene { p1, p2, p3, p4, .. } => {
                f(p1);
                f(p2);
                f(p3);
                f(p4);
            }
            Instruction::Effect {
                x,
                y,
                rotation,
                color,
                data,
                ..
            } => {
                f(x);
                f(y);
                f(rotation);
                f(color);
                f(data);
            }
            Instruction::Explosion {
                team,
                x,
                y,
                radius,
                damage,
                air,
                ground,
                pierce,
                effect,
            } => {
                f(team);
                f(x);
                f(y);
                f(radius);
                f(damage);
                f(air);
                f(ground);
                f(pierce);
                f(effect);
            }
            Instruction::GetFlag { result, flag } => {
                f(result);
                f(flag);
            }
            Instruction::SetFlag { flag, value } => {
                f(flag);
                f(value);
            }
            Instruction::SetMarker { id, p1, p2, p3, .. } => {
                f(id);
                f(p1);
                f(p2);
                f(p3);
            }
            Instruction::MakeMarker {
                id, x, y, replace, ..
            } => {
                f(id);
                f(x);
                f(y);
                f(replace);
            }
            Instruction::PlaySound {
                id,
                volume,
                pitch,
                pan,
                x,
                y,
                limit,
                ..
            } => {
                f(id);
                f(volume);
                f(pitch);
                f(pan);
                f(x);
                f(y);
                f(limit);
            }
            Instruction::PlayMusic { name, interrupt } => {
                f(name);
                f(interrupt);
            }
            Instruction::LocalePrint { name } => f(name),
            Instruction::Sync { variable } => f(variable),
            Instruction::ClientData {
                channel,
                value,
                reliable,
            } => {
                f(channel);
                f(value);
                f(reliable);
            }
        }
    }

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
            Instruction::Query {
                shape,
                type_,
                team,
                x,
                y,
                w,
                h,
            } => {
                crate::logic::world::run_query(exec, world, *shape, *type_, *team, *x, *y, *w, *h);
            }
            Instruction::Fetch {
                type_,
                result,
                team,
                index,
                extra,
            } => {
                crate::logic::world::run_fetch(exec, world, *type_, *result, *team, *index, *extra);
            }
            Instruction::GetBlock {
                layer,
                result,
                x,
                y,
            } => {
                crate::logic::world::run_get_block(exec, world, *layer, *result, *x, *y);
            }
            Instruction::SetBlock {
                layer,
                block,
                x,
                y,
                team,
                rotation,
            } => {
                crate::logic::world::run_set_block(
                    exec, world, *layer, *block, *x, *y, *team, *rotation,
                );
            }
            Instruction::SpawnUnit {
                type_,
                x,
                y,
                rotation,
                team,
                result,
                effect,
            } => {
                crate::logic::world::run_spawn_unit(
                    exec, world, *type_, *x, *y, *rotation, *team, *result, *effect,
                );
            }
            Instruction::SpawnBullet {
                result,
                from,
                index,
                x,
                y,
                rotation,
                team,
                owner,
                damage,
                velocity_scl,
                life_scl,
                aim_x,
                aim_y,
            } => {
                crate::logic::world::run_spawn_bullet(
                    exec,
                    world,
                    *result,
                    *from,
                    *index,
                    *x,
                    *y,
                    *rotation,
                    *team,
                    *owner,
                    *damage,
                    *velocity_scl,
                    *life_scl,
                    *aim_x,
                    *aim_y,
                );
            }
            Instruction::ApplyStatus {
                clear,
                effect,
                unit,
                duration,
            } => {
                crate::logic::world::run_apply_status(
                    exec, world, *clear, *effect, *unit, *duration,
                );
            }
            Instruction::WeatherSense { to, weather } => {
                crate::logic::world::run_weather_sense(exec, world, *to, *weather);
            }
            Instruction::WeatherSet { weather, state } => {
                crate::logic::world::run_weather_set(exec, world, *weather, *state);
            }
            Instruction::SpawnWave { x, y, natural } => {
                crate::logic::world::run_spawn_wave(exec, world, *x, *y, *natural);
            }
            Instruction::SetRule {
                rule,
                value,
                p1,
                p2,
                p3,
                p4,
            } => {
                crate::logic::world::run_set_rule(exec, world, *rule, *value, *p1, *p2, *p3, *p4);
            }
            Instruction::FlushMessage {
                type_,
                duration,
                out_success,
            } => {
                crate::logic::world::run_flush_message(
                    exec,
                    world,
                    *type_,
                    *duration,
                    *out_success,
                );
            }
            Instruction::Cutscene {
                action,
                p1,
                p2,
                p3,
                p4,
            } => {
                crate::logic::world::run_cutscene(exec, world, *action, *p1, *p2, *p3, *p4);
            }
            Instruction::Effect {
                effect,
                x,
                y,
                rotation,
                color,
                data,
            } => {
                crate::logic::world::run_effect(
                    exec, world, *effect, *x, *y, *rotation, *color, *data,
                );
            }
            Instruction::Explosion {
                team,
                x,
                y,
                radius,
                damage,
                air,
                ground,
                pierce,
                effect,
            } => {
                crate::logic::world::run_explosion(
                    exec, world, *team, *x, *y, *radius, *damage, *air, *ground, *pierce, *effect,
                );
            }
            Instruction::GetFlag { result, flag } => {
                crate::logic::world::run_get_flag(exec, world, *result, *flag);
            }
            Instruction::SetFlag { flag, value } => {
                crate::logic::world::run_set_flag(exec, world, *flag, *value);
            }
            Instruction::SetMarker {
                type_,
                id,
                p1,
                p2,
                p3,
            } => {
                crate::logic::world::run_set_marker(exec, world, *type_, *id, *p1, *p2, *p3);
            }
            Instruction::MakeMarker {
                type_,
                id,
                x,
                y,
                replace,
            } => {
                crate::logic::world::run_make_marker(exec, world, type_, *id, *x, *y, *replace);
            }
            Instruction::PlaySound {
                positional,
                id,
                volume,
                pitch,
                pan,
                x,
                y,
                limit,
            } => {
                crate::logic::world::run_play_sound(
                    exec,
                    world,
                    *positional,
                    *id,
                    *volume,
                    *pitch,
                    *pan,
                    *x,
                    *y,
                    *limit,
                );
            }
            Instruction::PlayMusic { name, interrupt } => {
                crate::logic::world::run_play_music(exec, world, *name, *interrupt);
            }
            Instruction::LocalePrint { name } => {
                crate::logic::world::run_locale_print(exec, world, *name);
            }
            Instruction::Sync { variable } => {
                crate::logic::world::run_sync(exec, world, *variable);
            }
            Instruction::ClientData {
                channel,
                value,
                reliable,
            } => {
                crate::logic::world::run_client_data(exec, world, *channel, *value, *reliable);
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
    /// `@queries` result arenas (deviation 3); index 0 is the privileged arena.
    pub queries: Vec<Vec<LogicObject>>,
    /// Global arena snapshot from the assembler (deviation 2).
    pub globals: crate::logic::globals::GlobalVars,
    /// `(local mirror id, global arena id)` for live `@time`-style reads.
    pub global_mirrors: Vec<(VarId, VarId)>,
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
            queries: Vec::new(),
            globals: crate::logic::globals::GlobalVars::new(),
            global_mirrors: Vec::new(),
        }
    }

    /// `LExecutor.initialized`.
    pub fn initialized(&self) -> bool {
        !self.instructions.is_empty()
    }

    /// `LExecutor.load`.
    pub fn load(&mut self, mut asm: Assembler) {
        self.globals = std::mem::take(&mut asm.globals);
        self.arena = asm.arena;
        self.instructions = asm.instructions;
        self.privileged = asm.privileged;
        self.stopped = false;
        self.yielded = false;
        self.text_buffer.clear();
        self.graphics_buffer.clear();
        self.binds.clear();
        self.queries.clear();
        self.queries.push(Vec::new());

        // Lower `@time`-style global refs into local mirror cells so the hot VM
        // path stays index-based while reads observe `GlobalVars::update`.
        self.lower_globals();

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
            self.query_result = Some(
                self.arena
                    .put_obj_const("@queries", Some(LogicObject::Query(0))),
            );
        }
    }

    /// Whether a variable id is constant (`LVar.constant`).
    pub(crate) fn is_constant(&self, id: VarId) -> bool {
        self.arena.get(id).constant
    }

    pub(crate) fn set_num(&mut self, id: VarId, value: f64) {
        self.arena.get_mut(id).set_num(value);
    }

    pub(crate) fn set_obj(&mut self, id: VarId, value: Option<LogicObject>) {
        self.arena.get_mut(id).set_obj(value);
    }

    /// Reads the numeric program counter.
    pub fn counter_value(&self) -> f64 {
        self.arena.get(self.counter).num
    }

    /// Lowers [`VarRef::Global`] instruction fields to local mirror cells.
    ///
    /// Mirrors are marked constant so they never leak into variable dumps or
    /// checksums (upstream filters `@`-constants out of `executor.vars`).
    fn lower_globals(&mut self) {
        self.global_mirrors.clear();
        if self.globals.cells.is_empty() {
            return;
        }
        for index in 0..self.instructions.len() {
            let mut instr = std::mem::replace(&mut self.instructions[index], Instruction::Noop);
            {
                let arena = &mut self.arena;
                let globals = &self.globals;
                let mirrors = &mut self.global_mirrors;
                let mut resolve = |r: &mut VarRef| {
                    if let VarRef::Global(gid) = *r {
                        let local = match mirrors.iter().find(|(_, g)| *g == gid) {
                            Some((local, _)) => *local,
                            None => {
                                let (is_obj, obj, num) = globals
                                    .global_cell(gid)
                                    .map(|c| (c.is_obj, c.obj.clone(), c.num))
                                    .unwrap_or((true, None, 0.0));
                                let name = globals
                                    .global_cell(gid)
                                    .map(|c| c.name.clone())
                                    .unwrap_or_default();
                                let id = arena.put_var(&name);
                                let cell = arena.get_mut(id);
                                cell.is_obj = is_obj;
                                cell.obj = obj;
                                cell.num = num;
                                cell.constant = true;
                                mirrors.push((id, gid));
                                id
                            }
                        };
                        *r = VarRef::Local(local);
                    }
                };
                instr.for_each_var_ref_mut(&mut resolve);
            }
            self.instructions[index] = instr;
        }
    }

    /// Copies live global values into the local mirror cells for this tick.
    ///
    /// Reads the [`GlobalVars`](crate::logic::globals::GlobalVars) resource when
    /// installed (the harness/host path) and falls back to the load-time
    /// snapshot otherwise (plain unit-test worlds).
    fn refresh_globals(&mut self, world: &World) {
        if self.global_mirrors.is_empty() {
            return;
        }
        let resource = world.get_resource::<crate::logic::globals::GlobalVars>();
        for i in 0..self.global_mirrors.len() {
            let (local, gid) = self.global_mirrors[i];
            let value = resource
                .and_then(|g| g.global_cell(gid))
                .or_else(|| self.globals.global_cell(gid))
                .map(|c| (c.is_obj, c.obj.clone(), c.num));
            if let Some((is_obj, obj, num)) = value {
                let cell = self.arena.get_mut(local);
                cell.is_obj = is_obj;
                cell.obj = obj;
                cell.num = num;
            }
        }
    }

    /// `LExecutor.runOnce`.
    pub fn run_once(&mut self, world: &mut World) {
        self.refresh_globals(world);
        self.run_once_inner(world);
    }

    fn run_once_inner(&mut self, world: &mut World) {
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
        self.refresh_globals(world);
        for _ in 0..max {
            let counter = self.arena.get(self.counter).num;
            if self.stopped
                || self.yielded
                || counter < 0.0
                || counter >= self.instructions.len() as f64
            {
                break;
            }
            self.run_once_inner(world);
        }
    }

    /// Instruction-budget driver (`LogicBuild.updateTile` inner loop).
    pub fn run_budget(&mut self, world: &mut World, accumulator: &mut f32, edelta: f32, ipt: f32) {
        let max_scale = MAX_INSTRUCTION_SCALE;
        if *accumulator > max_scale * ipt {
            *accumulator = max_scale * ipt;
        }
        self.refresh_globals(world);
        while *accumulator >= 1.0 {
            self.run_once_inner(world);
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
        Statement::Query {
            shape,
            type_,
            team,
            x,
            y,
            w,
            h,
        } => Instruction::Query {
            shape: *shape,
            type_: *type_,
            team: asm.var(team),
            x: asm.var(x),
            y: asm.var(y),
            w: asm.var(w),
            h: asm.var(h),
        },
        Statement::Fetch {
            type_,
            result,
            team,
            index,
            extra,
        } => Instruction::Fetch {
            type_: *type_,
            result: asm.var(result),
            team: asm.var(team),
            index: asm.var(index),
            extra: asm.var(extra),
        },
        Statement::GetBlock {
            layer,
            result,
            x,
            y,
        } => Instruction::GetBlock {
            layer: *layer,
            result: asm.var(result),
            x: asm.var(x),
            y: asm.var(y),
        },
        Statement::SetBlock {
            layer,
            block,
            x,
            y,
            team,
            rotation,
        } => Instruction::SetBlock {
            layer: *layer,
            block: asm.var(block),
            x: asm.var(x),
            y: asm.var(y),
            team: asm.var(team),
            rotation: asm.var(rotation),
        },
        Statement::SpawnUnit {
            type_,
            x,
            y,
            rotation,
            team,
            result,
            effect,
        } => Instruction::SpawnUnit {
            type_: asm.var(type_),
            x: asm.var(x),
            y: asm.var(y),
            rotation: asm.var(rotation),
            team: asm.var(team),
            result: asm.var(result),
            effect: asm.var(effect),
        },
        Statement::SpawnBullet {
            result,
            from,
            index,
            x,
            y,
            rotation,
            team,
            owner,
            damage,
            velocity_scl,
            life_scl,
            aim_x,
            aim_y,
        } => Instruction::SpawnBullet {
            result: asm.var(result),
            from: asm.var(from),
            index: asm.var(index),
            x: asm.var(x),
            y: asm.var(y),
            rotation: asm.var(rotation),
            team: asm.var(team),
            owner: asm.var(owner),
            damage: asm.var(damage),
            velocity_scl: asm.var(velocity_scl),
            life_scl: asm.var(life_scl),
            aim_x: asm.var(aim_x),
            aim_y: asm.var(aim_y),
        },
        Statement::ApplyStatus {
            clear,
            effect,
            unit,
            duration,
        } => Instruction::ApplyStatus {
            clear: *clear,
            effect: asm.var(effect),
            unit: asm.var(unit),
            duration: asm.var(duration),
        },
        Statement::WeatherSense { to, weather } => Instruction::WeatherSense {
            to: asm.var(to),
            weather: asm.var(weather),
        },
        Statement::WeatherSet { weather, state } => Instruction::WeatherSet {
            weather: asm.var(weather),
            state: asm.var(state),
        },
        Statement::SpawnWave { x, y, natural } => Instruction::SpawnWave {
            x: asm.var(x),
            y: asm.var(y),
            natural: asm.var(natural),
        },
        Statement::SetRule {
            rule,
            value,
            p1,
            p2,
            p3,
            p4,
        } => Instruction::SetRule {
            rule: *rule,
            value: asm.var(value),
            p1: asm.var(p1),
            p2: asm.var(p2),
            p3: asm.var(p3),
            p4: asm.var(p4),
        },
        Statement::FlushMessage {
            type_,
            duration,
            out_success,
        } => Instruction::FlushMessage {
            type_: *type_,
            duration: asm.var(duration),
            out_success: asm.var(out_success),
        },
        Statement::Cutscene {
            action,
            p1,
            p2,
            p3,
            p4,
        } => Instruction::Cutscene {
            action: *action,
            p1: asm.var(p1),
            p2: asm.var(p2),
            p3: asm.var(p3),
            p4: asm.var(p4),
        },
        Statement::Effect {
            type_,
            x,
            y,
            sizerot,
            color,
            data,
        } => Instruction::Effect {
            effect: crate::logic::fx::get(type_).copied(),
            x: asm.var(x),
            y: asm.var(y),
            rotation: asm.var(sizerot),
            color: asm.var(color),
            data: asm.var(data),
        },
        Statement::Explosion {
            team,
            x,
            y,
            radius,
            damage,
            air,
            ground,
            pierce,
            effect,
        } => Instruction::Explosion {
            team: asm.var(team),
            x: asm.var(x),
            y: asm.var(y),
            radius: asm.var(radius),
            damage: asm.var(damage),
            air: asm.var(air),
            ground: asm.var(ground),
            pierce: asm.var(pierce),
            effect: asm.var(effect),
        },
        Statement::GetFlag { result, flag } => Instruction::GetFlag {
            result: asm.var(result),
            flag: asm.var(flag),
        },
        Statement::SetFlag { flag, value } => Instruction::SetFlag {
            flag: asm.var(flag),
            value: asm.var(value),
        },
        Statement::SetMarker {
            type_,
            id,
            p1,
            p2,
            p3,
        } => Instruction::SetMarker {
            type_: *type_,
            id: asm.var(id),
            p1: asm.var(p1),
            p2: asm.var(p2),
            p3: asm.var(p3),
        },
        Statement::MakeMarker {
            type_,
            id,
            x,
            y,
            replace,
        } => Instruction::MakeMarker {
            type_: type_.clone(),
            id: asm.var(id),
            x: asm.var(x),
            y: asm.var(y),
            replace: asm.var(replace),
        },
        Statement::PlaySound {
            positional,
            id,
            volume,
            pitch,
            pan,
            x,
            y,
            limit,
        } => Instruction::PlaySound {
            positional: *positional,
            id: asm.var(id),
            volume: asm.var(volume),
            pitch: asm.var(pitch),
            pan: asm.var(pan),
            x: asm.var(x),
            y: asm.var(y),
            limit: asm.var(limit),
        },
        Statement::PlayMusic { name, interrupt } => Instruction::PlayMusic {
            name: asm.var(name),
            interrupt: asm.var(interrupt),
        },
        Statement::LocalePrint { value } => Instruction::LocalePrint {
            name: asm.var(value),
        },
        Statement::Sync { variable } => Instruction::Sync {
            variable: asm.var(variable),
        },
        Statement::ClientData {
            channel,
            value,
            reliable,
        } => Instruction::ClientData {
            channel: asm.var(channel),
            value: asm.var(value),
            reliable: asm.var(reliable),
        },
        // Remaining unregistered/unbuilt statements compile to a no-op so
        // program structure (indices/jumps) is preserved.
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
