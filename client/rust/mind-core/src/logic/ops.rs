// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LogicOp` and `ConditionOp` ports.
//!
//! Ported from `core/src/mindustry/logic/LogicOp.java` and `ConditionOp.java`.
//! `strictEqual` is handled by the enclosing instruction (`strictEqual`'s numeric
//! lambda is unused upstream).

use crate::logic::enums::{FieldKind, MlogField, logic_enum};
use crate::logic::value::{LVar, structs_eq};
use crate::math::ArcRand;

logic_enum!(
    /// `LogicOp` (Java enum name is the text-IO token; [`LogicOp::symbol`] is the UI symbol).
    LogicOp {
        Add => "add",
        Sub => "sub",
        Mul => "mul",
        Div => "div",
        Idiv => "idiv",
        Mod => "mod",
        Emod => "emod",
        Pow => "pow",
        Equal => "equal",
        NotEqual => "notEqual",
        Land => "land",
        LessThan => "lessThan",
        LessThanEq => "lessThanEq",
        GreaterThan => "greaterThan",
        GreaterThanEq => "greaterThanEq",
        StrictEqual => "strictEqual",
        Shl => "shl",
        Shr => "shr",
        Ushr => "ushr",
        Or => "or",
        And => "and",
        Xor => "xor",
        Not => "not",
        Max => "max",
        Min => "min",
        Angle => "angle",
        AngleDiff => "angleDiff",
        Len => "len",
        Noise => "noise",
        Abs => "abs",
        Sign => "sign",
        Log => "log",
        Logn => "logn",
        Log10 => "log10",
        Floor => "floor",
        Ceil => "ceil",
        Round => "round",
        Sqrt => "sqrt",
        Rand => "rand",
        Sin => "sin",
        Cos => "cos",
        Tan => "tan",
        Asin => "asin",
        Acos => "acos",
        Atan => "atan"
    }
);

const DEG_RAD: f64 = std::f64::consts::PI / 180.0;
const RAD_DEG: f64 = 180.0 / std::f64::consts::PI;

impl LogicOp {
    /// UI symbol (`LogicOp.symbol` / `toString`).
    pub const fn symbol(self) -> &'static str {
        use LogicOp::*;
        match self {
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Idiv => "//",
            Mod => "%",
            Emod => "%%",
            Pow => "^",
            Equal => "==",
            NotEqual => "not",
            Land => "and",
            LessThan => "<",
            LessThanEq => "<=",
            GreaterThan => ">",
            GreaterThanEq => ">=",
            StrictEqual => "===",
            Shl => "<<",
            Shr => ">>",
            Ushr => ">>>",
            Or => "or",
            And => "b-and",
            Xor => "xor",
            Not => "flip",
            Max => "max",
            Min => "min",
            Angle => "angle",
            AngleDiff => "anglediff",
            Len => "len",
            Noise => "noise",
            Abs => "abs",
            Sign => "sign",
            Log => "log",
            Logn => "logn",
            Log10 => "log10",
            Floor => "floor",
            Ceil => "ceil",
            Round => "round",
            Sqrt => "sqrt",
            Rand => "rand",
            Sin => "sin",
            Cos => "cos",
            Tan => "tan",
            Asin => "asin",
            Acos => "acos",
            Atan => "atan",
        }
    }

    /// `LogicOp.unary`.
    pub const fn is_unary(self) -> bool {
        use LogicOp::*;
        matches!(
            self,
            Not | Abs
                | Sign
                | Log
                | Log10
                | Floor
                | Ceil
                | Round
                | Sqrt
                | Rand
                | Sin
                | Cos
                | Tan
                | Asin
                | Acos
                | Atan
        )
    }

    /// `LogicOp.func` (functional/labeled operators).
    pub const fn is_func(self) -> bool {
        use LogicOp::*;
        matches!(self, Max | Min | Angle | AngleDiff | Len | Noise)
    }

    /// `LogicOp.objFunction2 != null`.
    pub const fn has_obj_function(self) -> bool {
        matches!(self, LogicOp::Equal | LogicOp::NotEqual)
    }

    /// Applies the numeric/object function, returning a new numeric `LVar`.
    pub fn eval(self, a: &LVar, b: &LVar, rng: &mut ArcRand) -> LVar {
        use LogicOp::*;
        let av = a.num();
        let bv = b.num();
        let value = match self {
            Add => av + bv,
            Sub => av - bv,
            Mul => av * bv,
            Div => av / bv,
            Idiv => (av / bv).floor(),
            Mod => av % bv,
            Emod => ((av % bv) + bv) % bv,
            Pow => av.powf(bv),
            Equal | NotEqual => {
                if a.is_obj && b.is_obj {
                    let eq = structs_eq(a.obj.as_ref(), b.obj.as_ref());
                    if (self == Equal) == eq { 1.0 } else { 0.0 }
                } else {
                    let eq = (av - bv).abs() < 0.000001;
                    if (self == Equal) == eq { 1.0 } else { 0.0 }
                }
            }
            Land => {
                if av != 0.0 && bv != 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            LessThan => (av < bv) as i32 as f64,
            LessThanEq => (av <= bv) as i32 as f64,
            GreaterThan => (av > bv) as i32 as f64,
            GreaterThanEq => (av >= bv) as i32 as f64,
            StrictEqual => 0.0,
            Shl => ((av as i64) << ((bv as i64) & 63)) as f64,
            Shr => ((av as i64) >> ((bv as i64) & 63)) as f64,
            Ushr => (((av as i64) as u64) >> ((bv as i64) & 63)) as f64,
            Or => ((av as i64) | (bv as i64)) as f64,
            And => ((av as i64) & (bv as i64)) as f64,
            Xor => ((av as i64) ^ (bv as i64)) as f64,
            Not => (!(av as i64)) as f64,
            Max => av.max(bv),
            Min => av.min(bv),
            Angle => angle(av as f32, bv as f32) as f64,
            AngleDiff => angle_dist(av as f32, bv as f32) as f64,
            Len => (av as f32).hypot(bv as f32) as f64,
            Noise => crate::math::noise::raw2d(0, av, bv),
            Abs => av.abs(),
            Sign => {
                if av > 0.0 {
                    1.0
                } else if av < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            }
            Log => av.ln(),
            Logn => av.ln() / bv.ln(),
            Log10 => av.log10(),
            Floor => av.floor(),
            Ceil => av.ceil(),
            Round => av.round(),
            Sqrt => av.sqrt(),
            Rand => rng.next_double() * av,
            Sin => (av * DEG_RAD).sin(),
            Cos => (av * DEG_RAD).cos(),
            Tan => (av * DEG_RAD).tan(),
            Asin => av.asin() * RAD_DEG,
            Acos => av.acos() * RAD_DEG,
            Atan => av.atan() * RAD_DEG,
        };
        LVar::num_const("___op", value)
    }
}

logic_enum!(
    /// `ConditionOp`.
    ConditionOp {
        Equal => "equal",
        NotEqual => "notEqual",
        LessThan => "lessThan",
        LessThanEq => "lessThanEq",
        GreaterThan => "greaterThan",
        GreaterThanEq => "greaterThanEq",
        StrictEqual => "strictEqual",
        Always => "always"
    }
);

impl ConditionOp {
    /// UI symbol (`ConditionOp.symbol` / `toString`).
    pub const fn symbol(self) -> &'static str {
        use ConditionOp::*;
        match self {
            Equal => "==",
            NotEqual => "not",
            LessThan => "<",
            LessThanEq => "<=",
            GreaterThan => ">",
            GreaterThanEq => ">=",
            StrictEqual => "===",
            Always => "always",
        }
    }

    /// `ConditionOp.test(LVar, LVar)`.
    pub fn test(self, va: &LVar, vb: &LVar) -> bool {
        use ConditionOp::*;
        if self == StrictEqual {
            return va.is_obj == vb.is_obj
                && ((va.is_obj && structs_eq(va.obj.as_ref(), vb.obj.as_ref()))
                    || (!va.is_obj && va.num == vb.num));
        }
        match self {
            Equal => {
                if va.is_obj && vb.is_obj {
                    structs_eq(va.obj.as_ref(), vb.obj.as_ref())
                } else {
                    (va.num() - vb.num()).abs() < 0.000001
                }
            }
            NotEqual => {
                if va.is_obj && vb.is_obj {
                    !structs_eq(va.obj.as_ref(), vb.obj.as_ref())
                } else {
                    (va.num() - vb.num()).abs() >= 0.000001
                }
            }
            LessThan => va.num() < vb.num(),
            LessThanEq => va.num() <= vb.num(),
            GreaterThan => va.num() > vb.num(),
            GreaterThanEq => va.num() >= vb.num(),
            Always => true,
            StrictEqual => false,
        }
    }
}

/// `Angles.angle(x, y)` in degrees `[0, 360)`.
pub fn angle(x: f32, y: f32) -> f32 {
    let mut a = y.atan2(x) * 180.0 / std::f32::consts::PI;
    if a < 0.0 {
        a += 360.0;
    }
    a
}

/// `Angles.angleDist(a, b)`.
pub fn angle_dist(a: f32, b: f32) -> f32 {
    let d = (b - a).abs() % 360.0;
    if d > 180.0 { 360.0 - d } else { d }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(v: f64) -> LVar {
        LVar::num_const("___", v)
    }

    #[test]
    fn arithmetic_and_bitwise() {
        let mut rng = ArcRand::new(1);
        assert_eq!(LogicOp::Add.eval(&n(2.0), &n(3.0), &mut rng).num(), 5.0);
        assert_eq!(LogicOp::Idiv.eval(&n(7.0), &n(2.0), &mut rng).num(), 3.0);
        assert_eq!(LogicOp::Emod.eval(&n(-1.0), &n(3.0), &mut rng).num(), 2.0);
        assert_eq!(LogicOp::Shl.eval(&n(1.0), &n(4.0), &mut rng).num(), 16.0);
        assert_eq!(
            LogicOp::Ushr.eval(&n(-1.0), &n(1.0), &mut rng).num(),
            (i64::MAX as f64)
        );
        assert_eq!(LogicOp::Not.eval(&n(0.0), &n(0.0), &mut rng).num(), -1.0);
        assert_eq!(LogicOp::Sign.eval(&n(0.0), &n(0.0), &mut rng).num(), 0.0);
    }

    #[test]
    fn object_equality_uses_structs_eq() {
        let mut rng = ArcRand::new(1);
        let a = LVar::null_obj("a");
        let b = LVar::null_obj("b");
        assert_eq!(LogicOp::Equal.eval(&a, &b, &mut rng).num(), 1.0);
        assert!(ConditionOp::Equal.test(&a, &b));
        assert!(!ConditionOp::NotEqual.test(&a, &b));
    }

    #[test]
    fn strict_equal_requires_same_type() {
        let a = n(1.0);
        let mut null_obj = LVar::null_obj("x");
        null_obj.set_num(1.0);
        assert!(ConditionOp::StrictEqual.test(&a, &a));
        assert!(!ConditionOp::StrictEqual.test(&a, &LVar::null_obj("y")));
    }

    #[test]
    fn condition_numeric_tolerance() {
        assert!(ConditionOp::Equal.test(&n(1.0), &n(1.0 + 1e-9)));
        assert!(!ConditionOp::Equal.test(&n(1.0), &n(1.001)));
    }
}
