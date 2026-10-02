// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LExecutor` — the mlog virtual machine (M0 core).
//!
//! Ported from `core/src/mindustry/logic/LExecutor.java`. This milestone lands
//! the executor core (`load`, `runOnce`, the instruction budget) and the
//! arithmetic/control/IO instruction subset; the world/unit instructions land in
//! later milestones behind the same [`Instruction`] enum.

use bevy_ecs::entity::Entity;

use crate::logic::assembler::Assembler;
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
}

impl Instruction {
    /// Executes one instruction against `exec`.
    pub fn run(&self, exec: &mut Executor) {
        match self {
            Instruction::Noop | Instruction::End => {}
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
                    // yield exactly once
                    exec.yielded = true;
                    exec.set_num(exec.counter, exec.arena.get(exec.counter).num - 1.0);
                } else if *cur_time >= wait_for {
                    // done; fall through (counter was already advanced)
                } else {
                    exec.yielded = true;
                    exec.set_num(exec.counter, exec.arena.get(exec.counter).num - 1.0);
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
        }
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
    /// Text print buffer (cap 400).
    pub text_buffer: String,
    /// Global logic RNG stream.
    pub rng: ArcRand,
    /// Owning build (for `@this`/ipt).
    pub build: Option<Entity>,
    /// Links.
    pub links: Vec<Entity>,
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
            text_buffer: String::new(),
            rng: ArcRand::new(0),
            build: None,
            links: Vec::new(),
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
        let ipt_value = 0.0;
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

    /// Reads the numeric program counter.
    pub fn counter_value(&self) -> f64 {
        self.arena.get(self.counter).num
    }

    /// `LExecutor.runOnce`.
    pub fn run_once(&mut self) {
        let len = self.instructions.len() as f64;
        let mut counter = self.arena.get(self.counter).num;
        if counter >= len || counter < 0.0 {
            counter = 0.0;
        }
        if counter < len {
            self.arena.get_mut(self.counter).is_obj = false;
            let index = counter as usize;
            self.arena.get_mut(self.counter).num = counter + 1.0;
            let instruction = self.instructions[index].clone();
            instruction.run(self);
        }
    }

    /// Runs up to `max` instructions (bounded).
    pub fn run(&mut self, max: usize) {
        for _ in 0..max {
            let counter = self.arena.get(self.counter).num;
            if self.stopped
                || self.yielded
                || counter < 0.0
                || counter >= self.instructions.len() as f64
            {
                break;
            }
            self.run_once();
        }
    }

    /// Instruction-budget driver (`LogicBuild.updateTile` inner loop).
    pub fn run_budget(&mut self, accumulator: &mut f32, edelta: f32, ipt: f32) {
        let max_scale = MAX_INSTRUCTION_SCALE;
        if *accumulator > max_scale * ipt {
            *accumulator = max_scale * ipt;
        }
        while *accumulator >= 1.0 {
            self.run_once();
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

    #[test]
    fn set_op_select_jump_run() {
        let mut e = exec("set result 5\nop add result result 2\n");
        e.run(10);
        assert_eq!(var_num(&e, "result"), 7.0);
    }

    #[test]
    fn jump_skips() {
        let mut e = exec("set x 0\njump skip always\nset x 1\nskip:\nset x 2\n");
        // label `skip` is statement index 3
        e.run(20);
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
        e.run_budget(&mut acc, 0.0, 2.0);
        // <= 5 * 2 = 10 instructions executed
        assert!(var_num(&e, "n") <= 10.0);
        assert!(acc <= 5.0 * 2.0 + 0.0);
    }

    #[test]
    fn pack_unpack_color_roundtrip() {
        let mut e = exec("packcolor c 1 0 0 1\nunpackcolor r g b a c\n");
        e.run(10);
        assert!((var_num(&e, "r") - 1.0).abs() < 1e-9);
        assert!((var_num(&e, "g") - 0.0).abs() < 1e-9);
        assert!((var_num(&e, "a") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn print_and_format() {
        let mut e = exec("print 5\nprint \"x\"\n");
        e.run(10);
        assert_eq!(e.text_buffer, "5x");
    }
}
