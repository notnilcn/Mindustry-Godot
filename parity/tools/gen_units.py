#!/usr/bin/env python3
"""Generate flat Rust unit metadata from Mindustry's UnitTypes.java.

One-off conversion used for plan 02 M5; the data is committed and this
script is re-run only when upstream content changes.

Usage: gen_units.py [upstream_mindustry_src] [--out-dir DIR] [--ledger PATH]
  upstream_mindustry_src defaults to /mnt/c/Users/Clinton/g/code_examples/Mindustry/core/src/mindustry
  --out-dir defaults to client/rust/mind-core/src/content/registries/units
  --ledger defaults to parity/ledgers/units.md
Also regenerates client/rust/mind-core/src/content/registries/sound_meta.rs
from the upstream `core/assets/sounds` tree.
"""
import hashlib
import json
import re
import struct
import sys
from pathlib import Path


def f32(value):
    """Rounds a Python float to f32 precision (Java `float` semantics)."""
    return struct.unpack("f", struct.pack("f", value))[0]

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith("--")
            else "/mnt/c/Users/Clinton/g/code_examples/Mindustry/core/src/mindustry")
OUT = Path("client/rust/mind-core/src/content/registries/units")
SOUND_OUT = Path("client/rust/mind-core/src/content/registries/sound_meta.rs")
LEDGER = Path("parity/ledgers/units.md")
FX_META = Path("client/rust/mind-core/src/content/registries/fx_meta.rs")
args = sys.argv[2:] if len(sys.argv) > 1 and not sys.argv[1].startswith("--") else sys.argv[1:]
i = 0
while i < len(args):
    if args[i] == "--out-dir":
        OUT = Path(args[i + 1]); i += 2
    elif args[i] == "--ledger":
        LEDGER = Path(args[i + 1]); i += 2
    else:
        i += 1

# ---------------------------------------------------------------- helpers

def kebab(name):
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1-\2", name)
    s = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1-\2", s)
    return s.lower()


def snake(name):
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name)
    return s.lower().replace("-", "_")


def screaming(name):
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name)
    s = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1_\2", s)
    return s.upper()


def pascal(name):
    return name[0].upper() + name[1:]


def field_map(path):
    text = Path(path).read_text(encoding="utf-8")
    text = re.sub(r"//[^\n]*", "", text)
    pat = re.compile(r"(\w+)\s*=\s*new\s+[\w.]+\(\s*\"([^\"]+)\"")
    return {m.group(1): m.group(2) for m in pat.finditer(text)}


statuses = field_map(ROOT / "content/StatusEffects.java")
liquids = field_map(ROOT / "content/Liquids.java")
items = field_map(ROOT / "content/Items.java")

# fx_meta constants (name -> const, lifetime).
fx_name_to_const = {}
fx_lifetimes = {}
if FX_META.exists():
    fx_text = FX_META.read_text(encoding="utf-8")
    for m in re.finditer(r"/// `Fx\.(\w+)`\.\s*\n\s*pub const (\w+): EffectId", fx_text):
        fx_name_to_const[m.group(1)] = m.group(2)
    for m in re.finditer(r"name: \"(\w+)\",\s*\n\s*lifetime: ([\d.eE+-]+),", fx_text):
        fx_lifetimes[m.group(1)] = float(m.group(2))

# Sound names from the assets tree (see `audio/AGENTS.md`: one `Sounds` field
# per `.ogg`, flattened base name).
sound_names = set()
sound_dir = ROOT.parent.parent / "assets/sounds"
sound_assets = {}
if sound_dir.is_dir():
    for path in sorted(sound_dir.rglob("*.ogg")):
        sound_names.add(path.stem)
        sound_assets[path.stem] = path.relative_to(sound_dir).with_suffix('').as_posix()
SOUND_ORDER = ["none", "unset"] + sorted(sound_names)
sound_ids = {name: idx for idx, name in enumerate(SOUND_ORDER)}

# Pal table: evaluate Pal.java assignments in order.
def clamp01(v):
    return min(1.0, max(0.0, v))


def rgba_mul(color, factor):
    return (clamp01(color[0] * factor), clamp01(color[1] * factor), clamp01(color[2] * factor), color[3])


def rgba_set_a(color, a):
    return (color[0], color[1], color[2], a)


PAL = {}
pal_text = (ROOT / "graphics/Pal.java").read_text(encoding="utf-8")
pal_text = re.sub(r"//[^\n]*", "", pal_text)
pal_assign = re.compile(
    r"(\w+)\s*=\s*(?:Color\.valueOf\(\"([0-9a-fA-F]{6,8})\"\)(?:\.a\(([\d.]+)f?\))?|new Color\(([^)]+)\)|Pal\.(\w+)\.cpy\(\)((?:\.\w+\([^)]*\))+)|Pal\.(\w+))")
for m in pal_assign.finditer(pal_text):
    name = m.group(1)
    if m.group(2):
        hexv = m.group(2)
        r = int(hexv[0:2], 16) / 255.0
        g = int(hexv[2:4], 16) / 255.0
        b = int(hexv[4:6], 16) / 255.0
        a = int(hexv[6:8], 16) / 255.0 if len(hexv) == 8 else 1.0
        if m.group(3):
            a = float(m.group(3))
        PAL[name] = (r, g, b, a)
    elif m.group(4):
        parts = [float(x.strip().rstrip("f")) for x in m.group(4).split(",")]
        while len(parts) < 4:
            parts.append(1.0)
        PAL[name] = tuple(parts[:4])
    elif m.group(5):
        color = PAL.get(m.group(5))
        if color is None:
            continue
        for op, arg in re.findall(r"\.(\w+)\(([^)]*)\)", m.group(6)):
            if op == "a":
                color = rgba_set_a(color, float(arg.rstrip("f")))
            elif op == "mul":
                color = rgba_mul(color, float(arg.rstrip("f")))
        PAL[name] = color
    elif m.group(7):
        if m.group(7) in PAL:
            PAL[name] = PAL[m.group(7)]

COLOR_NAMES = {
    "white": (1.0, 1.0, 1.0, 1.0),
    "red": (1.0, 0.0, 0.0, 1.0),
    "black": (0.0, 0.0, 0.0, 1.0),
    "clear": (0.0, 0.0, 0.0, 0.0),
    "gray": (0.5, 0.5, 0.5, 1.0),
    "slate": (0.4, 0.43, 0.47, 1.0),
    "orange": (1.0, 0.5, 0.0, 1.0),
    "yellow": (1.0, 1.0, 0.0, 1.0),
    "green": (0.0, 1.0, 0.0, 1.0),
    "blue": (0.0, 0.0, 1.0, 1.0),
}

# ---------------------------------------------------------------- expression eval

class EvalError(Exception):
    pass


TOKEN_RE = re.compile(r"""
    (?P<ws>\s+)
  | (?P<num>\d+\.\d+(?:[eE][+-]?\d+)?[fFdD]?|\.\d+[fFdD]?|\d+[fFdD]?|\d+)
  | (?P<str>"(?:[^"\\]|\\.)*")
  | (?P<ident>[A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)*)
  | (?P<op>->|\|\||&&|==|!=|<=|>=|::|\|=|&=|\||&|\^|\+|-|\*|/|%|<|>|\?|:|\(|\)|,|\.|\[|\]|\{|\})
""", re.X)

TYPE_KEYWORDS = {"float", "int", "double", "long", "short", "byte", "char", "boolean"}


def tokenize(text):
    tokens = []
    pos = 0
    while pos < len(text):
        m = TOKEN_RE.match(text, pos)
        if not m:
            raise EvalError(f"bad token at {text[pos:pos+20]!r}")
        pos = m.end()
        kind = m.lastgroup
        if kind == "ws":
            continue
        tokens.append((kind, m.group()))
    return tokens


class Parser:
    def __init__(self, tokens, env):
        self.tokens = tokens
        self.pos = 0
        self.env = env

    def peek(self, offset=0):
        index = self.pos + offset
        if index < len(self.tokens):
            return self.tokens[index]
        return (None, None)

    def next(self):
        token = self.peek()
        self.pos += 1
        return token

    def expect(self, value):
        kind, text = self.next()
        if text != value:
            raise EvalError(f"expected {value!r}, got {text!r}")

    def parse(self):
        value = self.ternary()
        if self.pos != len(self.tokens):
            raise EvalError(f"trailing tokens: {self.tokens[self.pos:]!r}")
        return value

    def ternary(self):
        cond = self.binary(0)
        if self.peek()[1] == "?":
            self.next()
            if_true = self.ternary()
            self.expect(":")
            if_false = self.ternary()
            return if_true if truthy(cond) else if_false
        return cond

    BINARY = [
        ["||"], ["&&"], ["|"], ["^"], ["&"], ["==", "!="], ["<", ">", "<=", ">="],
        ["+", "-"], ["*", "/", "%"],
    ]

    def binary(self, level):
        if level >= len(self.BINARY):
            return self.unary()
        left = self.binary(level + 1)
        while self.peek()[1] in self.BINARY[level]:
            op = self.next()[1]
            right = self.binary(level + 1)
            left = apply_binary(op, left, right)
        return left

    def unary(self):
        kind, text = self.peek()
        if text == "-":
            self.next()
            value = self.unary()
            return f32(-value) if isinstance(value, float) else -value
        if text == "!":
            self.next()
            return not truthy(self.unary())
        if text == "+":
            self.next()
            return self.unary()
        return self.postfix()

    def postfix(self):
        value = self.primary()
        # `.method(args)` / `.field` chains
        while self.peek()[1] == ".":
            self.next()
            kind, method = self.next()
            if self.peek()[1] == "(":
                self.next()
                args = self.call_args(")")
                value = apply_method(value, method, args, self.env)
            else:
                value = apply_field(value, method, self.env)
        return value

    def primary(self):
        kind, text = self.next()
        if kind == "num":
            raw = text.rstrip("fFdD")
            if "." in raw or "e" in raw.lower():
                return f32(float(raw))
            if text[-1:] in "fF":
                return f32(float(raw))
            return int(raw)
        if kind == "str":
            return ("string", json.loads(text))
        if text == "(":
            if self.peek()[1] in TYPE_KEYWORDS and self.peek(1)[1] == ")":
                self.next()
                self.next()
                return num(self.unary())
            value = self.ternary()
            self.expect(")")
            return value
        if text == "new":
            tkind, tname = self.next()
            if self.peek()[1] == "[":
                while self.peek()[1] == "[":
                    self.next()
                    if self.peek()[1] != "]":
                        self.ternary()
                    self.expect("]")
                if self.peek()[1] == "{":
                    self.next()
                    values = []
                    if self.peek()[1] != "}":
                        while True:
                            values.append(self.ternary())
                            if self.peek()[1] == ",":
                                self.next()
                                if self.peek()[1] == "}":
                                    break
                                continue
                            break
                    self.expect("}")
                    return ("array", tname, values)
                raise EvalError("unsupported new T[] form")
            self.expect("(")
            call_args = self.call_args(")")
            return make_new(tname, call_args, self.env)
        if kind == "ident":
            if self.peek()[1] == "::":
                self.next()
                _, member = self.next()
                return ("methodref", text, member)
            if self.peek()[1] == "(":
                self.next()
                call_args = self.call_args(")")
                try:
                    return call_function(text, call_args, self.env)
                except EvalError:
                    segments = text.split(".")
                    if len(segments) < 2:
                        raise
                    prefix = ".".join(segments[:-1])
                    value = resolve_ident(prefix, self.env)
                    return apply_method(value, segments[-1], call_args, self.env)
            segments = text.split(".")
            value = None
            cut = len(segments)
            while cut > 0:
                prefix = ".".join(segments[:cut])
                try:
                    value = resolve_ident(prefix, self.env)
                    break
                except EvalError:
                    cut -= 1
            if value is None:
                raise EvalError(f"unknown identifier {text!r}")
            for method in segments[cut:]:
                if self.peek()[1] == "(":
                    self.next()
                    args = self.call_args(")")
                    value = apply_method(value, method, args, self.env)
                else:
                    value = apply_field(value, method, self.env)
            return value
        raise EvalError(f"unexpected token {text!r}")

    def call_args(self, closer):
        args = []
        if self.peek()[1] == closer:
            self.next()
            return args
        while True:
            args.append(self.ternary())
            kind, text = self.peek()
            if text == ",":
                self.next()
                continue
            if text == closer:
                self.next()
                return args
            raise EvalError(f"expected , or {closer!r}, got {text!r}")


def truthy(value):
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return value != 0
    return bool(value)


def num(value):
    if isinstance(value, bool):
        return 1 if value else 0
    if isinstance(value, (int, float)):
        return value
    raise EvalError(f"not numeric: {value!r}")


def apply_binary(op, left, right):
    if op == "||":
        return truthy(left) or truthy(right)
    if op == "&&":
        return truthy(left) and truthy(right)
    if op == "==":
        return left == right
    if op == "!=":
        return left != right
    if op == "<":
        return num(left) < num(right)
    if op == ">":
        return num(left) > num(right)
    if op == "<=":
        return num(left) <= num(right)
    if op == ">=":
        return num(left) >= num(right)
    if op == "|" and isinstance(left, dict) and isinstance(right, dict):
        if left.get("any") or right.get("any"):
            return {"any": True, "flags": []}
        return {"any": False, "flags": sorted(set(left.get("flags", [])) | set(right.get("flags", [])))}
    a, b = num(left), num(right)
    floaty = isinstance(a, float) or isinstance(b, float)
    result = None
    if op == "+":
        result = a + b
    elif op == "-":
        result = a - b
    elif op == "*":
        result = a * b
    elif op == "/":
        if b == 0:
            raise EvalError("division by zero")
        result = a / b
    elif op == "%":
        result = a % b
    if result is not None:
        if floaty:
            return f32(result)
        return result
    if op == "|":
        return int(a) | int(b)
    if op == "&":
        return int(a) & int(b)
    if op == "^":
        return int(a) ^ int(b)
    raise EvalError(f"unhandled operator {op}")


UNIT_FIELD_NAMES = set()


def resolve_ident(text, env):
    if text in UNIT_FIELD_NAMES:
        return ("unitref", kebab(text))
    if text == "true":
        return True
    if text == "false":
        return False
    if text == "null":
        return ("null",)
    if text == "this":
        return ("this",)
    if text in env:
        return env[text]
    if "." in text:
        qual, ident = text.rsplit(".", 1)
        if qual == "StatusEffects":
            return ("status", statuses.get(ident, kebab(ident)))
        if qual == "Liquids":
            return ("liquid", liquids.get(ident, kebab(ident)))
        if qual == "Items":
            return ("item", items.get(ident, kebab(ident)))
        if qual == "UnitTypes":
            return ("unitref", kebab(ident))
        if qual == "Fx":
            return ("effect", ident)
        if qual == "Sounds":
            return ("sound", ident)
        if qual == "Pal":
            if ident in PAL:
                return ("rgba", PAL[ident])
            raise EvalError(f"unknown Pal.{ident}")
        if qual == "Color":
            if ident in COLOR_NAMES:
                return ("rgba", COLOR_NAMES[ident])
            raise EvalError(f"unknown Color.{ident}")
        if qual == "Interp":
            return ("interp", ident)
        if qual == "Env":
            if ident == "any":
                return {"any": True, "flags": []}
            if ident == "none":
                return {"any": False, "flags": []}
            return {"any": False, "flags": [ident]}
        if qual == "BlockFlag":
            return ("blockflag", ident)
        if qual == "Mathf":
            if ident == "PI":
                return 3.1415927
            if ident == "signs":
                return ("floatlist", [-1.0, 1.0])
            if ident == "zeroOne":
                return ("floatlist", [0.0, 1.0])
        if qual == "Blending":
            return ("blending", ident)
        if qual == "Layer":
            layer_values = {
                "debris": 20.0, "turretHeat": 50.1, "groundUnit": 60.0, "legUnit": 75.0,
                "flyingUnitLow": 90.0, "bullet": 100.0, "effect": 110.0, "flyingUnit": 115.0,
                "scorch": 10.0,
            }
            if ident in layer_values:
                return layer_values[ident]
            raise EvalError(f"unknown Layer.{ident}")
        if qual == "PartProgress":
            return ("progress", ident)
        if qual == "UnitCommand":
            return ("commandref", ident)
        if qual == "UnitStance":
            return ("stanceref", ident)
        if text == "Float.POSITIVE_INFINITY":
            return float("inf")
        if text == "Float.NEGATIVE_INFINITY":
            return float("-inf")
        if text == "Float.MAX_VALUE":
            return 3.4028235e38
    if text == "tilesize":
        return 8.0
    if text == "tilePayload":
        return 64.0
    raise EvalError(f"unknown identifier {text!r}")


def make_new(tname, call_args, env):
    tname = tname.split(".")[-1]
    if tname == "Color":
        values = [num(a) for a in call_args]
        while len(values) < 4:
            values.append(1.0)
        return ("rgba", tuple(values[:4]))
    if tname == "Rect":
        return ("rect", [num(a) for a in call_args])
    if tname == "UnitEngine":
        return ("engine", [num(a) for a in call_args])
    if tname == "ShootPattern":
        return ("pattern", "ShootPattern", call_args, {})
    if tname == "ShootAlternate":
        return ("pattern", "ShootAlternate", call_args, {})
    if tname == "ShootSpread":
        return ("pattern", "ShootSpread", call_args, {})
    if tname == "ShootHelix":
        return ("pattern", "ShootHelix", call_args, {})
    return ("constructed", tname, call_args)


def call_function(name, args, env):
    base = name.split(".")[-1]
    qual = name.rsplit(".", 1)[0] if "." in name else ""
    if name == "Color.valueOf":
        arg = args[0]
        if isinstance(arg, tuple) and arg[0] == "string":
            hexv = arg[1]
            r = int(hexv[0:2], 16) / 255.0
            g = int(hexv[2:4], 16) / 255.0
            b = int(hexv[4:6], 16) / 255.0
            a = int(hexv[6:8], 16) / 255.0 if len(hexv) == 8 else 1.0
            return ("rgba", (r, g, b, a))
    if qual == "ObjectSet" and base == "with":
        return ("array", "ObjectSet", args)
    if qual == "Mathf" or base in ("round", "pow", "max", "min", "abs", "clamp", "lerp", "mod", "zero", "sign"):
        if base == "round" and len(args) == 2:
            value, step = num(args[0]), num(args[1])
            return int((value / step) + 0.5) * step
        if base == "round" and len(args) == 1:
            return int(num(args[0]) + 0.5)
        if base == "pow":
            return float(num(args[0])) ** float(num(args[1]))
        if base == "max":
            return max(num(a) for a in args)
        if base == "min":
            return min(num(a) for a in args)
        if base == "abs":
            return abs(num(args[0]))
        if base == "clamp" and len(args) == 3:
            return min(max(num(args[0]), num(args[1])), num(args[2]))
        if base == "clamp" and len(args) == 1:
            return min(max(num(args[0]), 0.0), 1.0)
        if base == "lerp":
            return f32(num(args[0]) + (num(args[1]) - num(args[0])) * num(args[2]))
        if base == "mod":
            return num(args[0]) % num(args[1])
        if base == "zero":
            return abs(num(args[0])) <= 0.001
        if base == "sign":
            return 1 if num(args[0]) >= 0 else -1
        if base == "sqr":
            return num(args[0]) ** 2
        if base == "slope":
            # `Mathf.slope(fin) = 1f - |fin - 0.5| * 2f`.
            return 1.0 - abs(num(args[0]) - 0.5) * 2.0
    raise EvalError(f"unhandled call {name}")


def apply_method(value, method, args, env):
    """Postfix `.method(args)` chains (colors, PartProgress, effects)."""
    if isinstance(value, tuple) and value[0] == "rgba":
        color = value[1]
        if method == "cpy":
            return ("rgba", color)
        if method == "a":
            return ("rgba", rgba_set_a(color, num(args[0])))
        if method == "mul":
            if len(args) == 1:
                return ("rgba", rgba_mul(color, num(args[0])))
            values = list(color)
            for i in range(min(4, len(args))):
                values[i] = clamp01(values[i] * num(args[i]))
            return ("rgba", tuple(values))
        raise EvalError(f"unhandled color method .{method}")
    if isinstance(value, tuple) and value[0] == "effect":
        if method == "lifetime":
            return fx_lifetimes.get(value[1], 0.0)
        raise EvalError(f"unhandled effect method .{method}")
    if isinstance(value, tuple) and value[0] in ("progress", "pexpr"):
        tree = ("progress", value[1]) if value[0] == "progress" else value
        if method in ("delay", "add", "mul", "shorten", "mod", "loop"):
            return ("pexpr", method, tree, num(args[0]))
        if method == "blend":
            other = args[0]
            other_tree = ("progress", other[1]) if isinstance(other, tuple) and other[0] == "progress" else other
            return ("pexpr", method, tree, other_tree, num(args[1]))
        if method in ("min", "max"):
            other = args[0]
            other_tree = ("progress", other[1]) if isinstance(other, tuple) and other[0] == "progress" else other
            return ("pexpr", method, tree, other_tree)
        if method in ("inv", "slope", "clamp"):
            return ("pexpr", method, tree)
        if method == "curve":
            if len(args) == 2:
                return ("pexpr", "curve_range", tree, num(args[0]), num(args[1]))
            arg = args[0]
            if isinstance(arg, tuple) and arg[0] == "interp":
                return ("pexpr", "curve_interp", tree, arg[1])
            raise EvalError("bad curve args")
        if method == "sustain":
            return ("pexpr", method, tree, num(args[0]), num(args[1]), num(args[2]))
        if method == "compress":
            return ("pexpr", method, tree, num(args[0]), num(args[1]))
        if method == "sin":
            if len(args) == 3:
                return ("pexpr", method, tree, num(args[0]), num(args[1]), num(args[2]))
            return ("pexpr", method, tree, 0.0, num(args[0]), num(args[1]))
        if method == "absin":
            return ("pexpr", method, tree, num(args[0]), num(args[1]))
        if method == "apply":
            raise EvalError("PartProgress.apply unsupported")
        raise EvalError(f"unhandled progress method .{method}")
    if isinstance(value, dict) and "flags" in value:
        if method in ("and", "with"):
            other = args[0]
            if isinstance(other, dict):
                return {"any": value.get("any") or other.get("any"),
                        "flags": sorted(set(value.get("flags", [])) | set(other.get("flags", [])))}
        raise EvalError(f"unhandled env method .{method}")
    raise EvalError(f"unhandled method .{method} on {value!r}")


def apply_field(value, field, env):
    if isinstance(value, tuple) and value[0] == "effect":
        if field == "lifetime":
            return fx_lifetimes.get(value[1], 0.0)
    if isinstance(value, tuple) and value[0] == "bulletref":
        node = value[1]
        if isinstance(node, BulletNode):
            if field in node.fields:
                return node.fields[field]
            # constructor-mapped fields
            mapped = {}
            seed_bullet_ctor_fields(node, mapped)
            if field in mapped:
                return mapped[field]
            raise EvalError(f"unknown bullet field .{field}")
    if isinstance(value, tuple) and value[0] == "shootref":
        fields = value[1]
        return fields.get(field, 0.0)
    raise EvalError(f"unhandled field .{field} on {value!r}")


ABSIN_LAMBDA_RE = re.compile(
    r"^\w+\s*->\s*Mathf\.absin\(Time\.time\s*\+\s*(.+?)\s*,\s*(.+?)\s*,\s*(.+?)\s*\)$", re.S)


def eval_expr(text, env):
    text = text.strip()
    if "->" in text:
        # `p -> Mathf.absin(Time.time + offset, scl, mag)` (anthicus blades).
        m = ABSIN_LAMBDA_RE.match(text)
        if m:
            offset = num(eval_expr(m.group(1), env))
            scl = num(eval_expr(m.group(2), env))
            mag = num(eval_expr(m.group(3), env))
            return ("absin_time", offset, scl, mag)
        return ("lambda", text)
    return Parser(tokenize(text), env).parse()


# ---------------------------------------------------------------- source model

class BulletNode:
    def __init__(self, cls, args):
        self.cls = cls
        self.args = args
        self.fields = {}
        self.nested = {}
        self.spawn_bullets = []
        self.spawn_unit = None
        self.effect_fields = {}


class WeaponNode:
    def __init__(self, cls, name):
        self.cls = cls
        self.name = name
        self.fields = {}
        self.bullet = None
        self.shoot = None
        self.parts = []


class AbilityNode:
    def __init__(self, cls, args):
        self.cls = cls
        self.args = args
        self.fields = {}


class PartNode:
    def __init__(self, cls, args):
        self.cls = cls
        self.args = args
        self.fields = {}
        self.children = []
        self.moves = []


class EffectNode:
    def __init__(self, cls, args, children):
        self.cls = cls
        self.args = args
        self.fields = {}
        self.children = children


class UnitNode:
    def __init__(self, field, cls, name, region, line):
        self.field = field
        self.cls = cls
        self.name = name
        self.region = region
        self.line = line
        self.fields = {}
        self.pre_bullets = []
        self.weapons = []
        self.abilities = []
        self.parts = []
        self.engines_mirror = []
        self.immunities = []
        self.target_flags = None
        self.tread_rects = []
        self.entity_group = None
        self.inline = False


# ---------------------------------------------------------------- initializer parsing

BULLET_CLASSES = {
    "BulletType", "BasicBulletType", "ArtilleryBulletType", "MissileBulletType",
    "LaserBoltBulletType", "SapBulletType", "LightningBulletType", "LaserBulletType",
    "FlakBulletType", "ExplosionBulletType", "RailBulletType",
    "ContinuousLaserBulletType", "ShrapnelBulletType", "LiquidBulletType",
    "EmpBulletType", "BombBulletType", "FireBulletType", "SpaceLiquidBulletType",
}
UNIT_CLASSES = {"UnitType", "ErekirUnitType", "TankUnitType", "MissileUnitType", "NeoplasmUnitType"}
EFFECT_CLASSES = {"Effect", "MultiEffect", "ExplosionEffect", "WaveEffect", "WrapEffect"}
NESTED_BULLET_FIELDS = {"fragBullet", "intervalBullet", "lightningType"}
EFFECT_FIELDS = {
    "shootEffect", "hitEffect", "despawnEffect", "smokeEffect", "trailEffect",
    "chargeEffect", "endEffect", "lineEffect", "pierceEffect", "pointEffect",
    "hitPowerEffect", "chainEffect", "applyEffect", "ejectEffect",
    "shootOnDeathEffect", "beamEffect", "healEffect",
}

NEW_RE = re.compile(r"new\s+(\w+)\s*\(")
ASSIGN_RE = re.compile(r"(?<![!<>=+|\-*/%&^])=(?!=)")


def split_top_level(text, sep=";"):
    out = []
    depth = 0
    start = 0
    i = 0
    while i < len(text):
        c = text[i]
        if c in "({[":
            depth += 1
        elif c in ")}]":
            depth -= 1
        elif c == sep and depth == 0:
            out.append(text[start:i])
            start = i + 1
        i += 1
    tail = text[start:]
    if tail.strip():
        out.append(tail)
    return out


def find_matching(text, open_index, open_char="{", close_char="}"):
    depth = 0
    i = open_index
    while i < len(text):
        if text[i] == open_char:
            depth += 1
        elif text[i] == close_char:
            depth -= 1
            if depth == 0:
                return i
        i += 1
    raise EvalError("unbalanced")


def parse_construction(text, env, pos=0):
    """Parses `new X(args)` optionally followed by `{{ init }}`.
    Returns (cls, raw_arg_texts, init_text, end_pos)."""
    m = NEW_RE.match(text, pos)
    if not m:
        raise EvalError(f"expected `new` at {text[pos:pos+30]!r}")
    cls = m.group(1)
    open_paren = text.index("(", m.end() - 1)
    close_paren = find_matching(text, open_paren, "(", ")")
    arg_text = text[open_paren + 1:close_paren]
    raws = [piece.strip() for piece in split_top_level(arg_text, ",") if piece.strip()]
    end = close_paren + 1
    init_text = None
    rest = text[end:].lstrip()
    if rest.startswith("{{"):
        offset = len(text) - len(rest)
        close_brace = find_matching(text, offset + 1)
        init_text = text[offset + 2:close_brace]
        end = close_brace + 1
    return cls, raws, init_text, end


def eval_args(raws, env):
    return [eval_expr(raw, env) for raw in raws]


def split_assignment(statement):
    """Splits `a = b = value` into (['a', 'b'], 'value') at brace depth 0;
    returns None when there is no top-level assignment."""
    depth = 0
    splits = []
    i = 0
    while i < len(statement):
        c = statement[i]
        if c in "({[":
            depth += 1
        elif c in ")}]":
            depth -= 1
        elif c == "=" and depth == 0:
            prev = statement[i - 1] if i > 0 else ""
            nxt = statement[i + 1] if i + 1 < len(statement) else ""
            if prev not in "!<>=+|-*/%&^" and nxt != "=":
                splits.append(i)
        i += 1
    if not splits:
        return None
    targets = []
    start = 0
    for index in splits:
        target = statement[start:index].strip()
        if target.startswith("this."):
            target = target[5:]
        targets.append(target.strip())
        start = index + 1
    return targets, statement[start:].strip()


LOCAL_DECL_RE = re.compile(r"(int|float|boolean|String|BulletType)\s+(\w+)\s*=\s*(.*)$", re.S)


def declare_locals(statement, env, unit=None, bullet_owner=None):
    m = LOCAL_DECL_RE.match(statement)
    if not m:
        return False
    decl_type, name, rhs = m.group(1), m.group(2), m.group(3)
    if "," in rhs and not rhs.strip().startswith("new"):
        # `float xo = 1f, yo = 2f;` — comma-separated declarations.
        pieces = split_top_level(rhs, ",")
        if all(idx == 0 or re.match(r"^\w+\s*=", p.strip()) for idx, p in enumerate(pieces)):
            declare_one(decl_type, name, pieces[0], env, unit)
            for piece in pieces[1:]:
                sub = re.match(r"(\w+)\s*=\s*(.*)$", piece.strip(), re.S)
                if sub is None:
                    raise EvalError(f"bad declaration {piece!r}")
                declare_one(decl_type, sub.group(1), sub.group(2), env, unit)
            return True
    declare_one(decl_type, name, rhs, env, unit)
    return True


def declare_one(decl_type, name, rhs, env, unit):
    inc = re.match(r"^(\w+)\s*\+\+$", rhs.strip())
    if inc:
        # Java post-increment (`int fi = i++;`): value first, then bump.
        value = eval_expr(inc.group(1), env)
        env[inc.group(1)] = num(env.get(inc.group(1), 0)) + 1
        env[name] = value
        return True
    if decl_type == "BulletType" and rhs.strip().startswith("new") and unit is not None:
        cls, raws, init_text, _ = parse_construction(rhs.strip(), env)
        node = parse_bullet(cls, raws, init_text, dict(env), unit)
        unit.pre_bullets.append((name, node))
        env[name] = ("pre", len(unit.pre_bullets) - 1)
    else:
        env[name] = eval_expr(rhs, env)
    return True



def each_statement(body, fn):
    for statement in split_top_level(body):
        statement = statement.strip()
        if statement:
            fn(statement)


def parse_effect(cls, raws, init_text, env):
    children = []
    args = []
    if cls == "MultiEffect":
        for raw in raws:
            if raw.startswith("new"):
                ccls, craws, cinit, _ = parse_construction(raw, env)
                children.append(parse_effect(ccls, craws, cinit, dict(env)))
            else:
                children.append(eval_expr(raw, env))
    elif cls == "WrapEffect":
        for raw in raws:
            if raw.startswith("new"):
                ccls, craws, cinit, _ = parse_construction(raw, env)
                children.append(parse_effect(ccls, craws, cinit, dict(env)))
            else:
                args.append(eval_expr(raw, env))
    else:
        args = eval_args(raws, env)
    node = EffectNode(cls, args, children)
    if cls == "WrapEffect" and len(args) > 1:
        node.fields["color"] = args[1]
    if init_text:
        for statement in split_top_level(init_text):
            statement = statement.strip()
            if not statement:
                continue
            assignment = split_assignment(statement)
            if assignment is None:
                continue
            targets, rhs = assignment
            value = eval_expr(rhs, env)
            for target in targets:
                node.fields[target] = value
                env[target] = value
    return node


def seed_bullet_ctor_fields(node, env):
    """Puts constructor args into env under their field names (speed/damage/...)."""
    args = node.args
    try:
        if node.cls in ("BasicBulletType", "ArtilleryBulletType", "MissileBulletType"):
            if len(args) > 0:
                env["speed"] = num(args[0])
            if len(args) > 1:
                env["damage"] = num(args[1])
        elif node.cls in ("LaserBoltBulletType", "FlakBulletType", "BulletType"):
            if len(args) > 0:
                env["speed"] = num(args[0])
            if len(args) > 1:
                env["damage"] = num(args[1])
        elif node.cls in ("LaserBulletType", "ContinuousLaserBulletType"):
            if len(args) > 0:
                env["damage"] = num(args[0])
        elif node.cls == "ExplosionBulletType":
            if len(args) > 0:
                env["splashDamage"] = num(args[0])
            if len(args) > 1:
                env["splashDamageRadius"] = num(args[1])
        elif node.cls == "BombBulletType":
            if len(args) > 0:
                env["splashDamage"] = num(args[0])
            if len(args) > 1:
                env["splashDamageRadius"] = num(args[1])
    except EvalError:
        pass


def parse_bullet(cls, raws, init_text, env, unit, weapon=None):
    args = eval_args(raws, env)
    node = BulletNode(cls, args)
    child_env = env
    seed_bullet_ctor_fields(node, child_env)
    if init_text:
        for statement in split_top_level(init_text):
            statement = statement.strip()
            if not statement:
                continue
            loop = extract_leading_loop(statement)
            if loop is not None:
                loop_text, remainder = loop
                expand_for(loop_text, child_env, lambda body, e: each_statement(body, lambda st: parse_bullet_statement(node, st, e, unit, weapon)))
                if remainder.strip():
                    parse_bullet_statement(node, remainder, child_env, unit, weapon)
                continue
            parse_bullet_statement(node, statement, child_env, unit, weapon)
    return node


def parse_bullet_statement(node, statement, env, unit, weapon=None):
    statement = statement.strip()
    if not statement:
        return
    if declare_locals(statement, env, unit):
        return
    loop = extract_leading_loop(statement)
    if loop is not None:
        loop_text, remainder = loop
        expand_for(loop_text, env, lambda body, e: each_statement(body, lambda st: parse_bullet_statement(node, st, e, unit, weapon)))
        if remainder.strip():
            parse_bullet_statement(node, remainder, env, unit)
        return
    m = re.match(r"spawnBullets\.add\((.*)\)\s*$", statement, re.S)
    if m:
        cls, raws, init_text, _ = parse_construction(m.group(1), env)
        node.spawn_bullets.append(parse_bullet(cls, raws, init_text, dict(env), unit, weapon))
        return
    assignment = split_assignment(statement)
    if assignment is None:
        return
    targets, rhs = assignment
    first = targets[0]
    if first in NESTED_BULLET_FIELDS and rhs.startswith("new"):
        cls, raws, init_text, _ = parse_construction(rhs, env)
        node.nested[first] = parse_bullet(cls, raws, init_text, dict(env), unit, weapon)
        return
    if first == "shoot" and rhs.startswith("new") and weapon is not None:
        # Enclosing-scope assignment: the bullet initializer mutates the
        # weapon's `shoot` pattern (obviate).
        cls, raws2, init_text2, _ = parse_construction(rhs, env)
        args = eval_args(raws2, env)
        fields = {}
        if init_text2:
            for st in split_top_level(init_text2):
                sub = split_assignment(st)
                if sub:
                    t2, r2 = sub
                    fields[t2[0]] = eval_expr(r2, env)
        weapon.shoot = ("pattern", cls, args, fields)
        env["shoot"] = ("shootref", fields)
        return
    if first == "spawnUnit" and rhs.startswith("new"):
        cls, raws, init_text, _ = parse_construction(rhs, env)
        name_args = eval_args(raws[:1], env)
        name = name_args[0][1] if name_args and isinstance(name_args[0], tuple) else "?"
        missile = UnitNode("<spawnUnit>", cls, name, unit.region, 0)
        missile.inline = True
        if init_text:
            parse_unit_body(missile, init_text, dict(env))
        node.spawn_unit = missile
        return
    if first in EFFECT_FIELDS and rhs.startswith("new"):
        cls, raws, init_text, _ = parse_construction(rhs, env)
        node.effect_fields[first] = parse_effect(cls, raws, init_text, dict(env))
        node.fields.pop(first, None)
        return
    value = eval_expr(rhs, env)
    for target in targets:
        node.effect_fields.pop(target, None)
        # Enclosing-scope routing (`entities/AGENTS.md` anonymous-class
        # semantics): fields the bullet class does not own resolve outward.
        if target in ("shootCone", "ignoreRotation", "ejectEffect") and weapon is not None:
            weapon.fields[target] = value
            continue
        if target == "shake" and node.cls != "ContinuousLaserBulletType" and weapon is not None:
            weapon.fields[target] = value
            continue
        if target == "clipSize":
            unit.fields["clipSize"] = value
            continue
        node.fields[target] = value
        env[target] = value


def parse_weapon(cls, raws, init_text, env, unit):
    args = eval_args(raws, env)
    name = args[0][1] if args and isinstance(args[0], tuple) else ""
    node = WeaponNode(cls, name)
    # `shoot`/`bullet` self-references resolve against the (default) fields.
    env["shoot"] = ("shootref", {})
    if init_text:
        for statement in split_top_level(init_text):
            statement = statement.strip()
            if not statement:
                continue
            parse_weapon_statement(node, statement, env, unit)
    return node


def parse_weapon_statement(node, statement, env, unit):
    statement = statement.strip()
    if not statement:
        return
    if declare_locals(statement, env, unit):
        return
    loop = extract_leading_loop(statement)
    if loop is not None:
        loop_text, remainder = loop
        expand_for(loop_text, env, lambda body, e: each_statement(body, lambda st: parse_weapon_statement(node, st, e, unit)))
        if remainder.strip():
            parse_weapon_statement(node, remainder, env, unit)
        return
    m = re.match(r"immunities\.add(?:All)?\((.*)\)\s*$", statement, re.S)
    if m:
        # Enclosing-scope mutation: weapon initializers may append to the
        # unit's immunities (navanax plasma mounts).
        for piece in split_top_level(m.group(1), ","):
            unit.immunities.append(eval_expr(piece, env))
        return
    m = re.match(r"parts\.add(?:All)?\((.*)\)\s*$", statement, re.S)
    if m:
        for piece in split_top_level(m.group(1), ","):
            piece = piece.strip()
            if piece.startswith("new"):
                cls, raws, init_text, _ = parse_construction(piece, env)
                node.parts.append(parse_part(cls, raws, init_text, dict(env), unit, node))
        return
    assignment = split_assignment(statement)
    if assignment is None:
        return
    targets, rhs = assignment
    first = targets[0]
    if first == "bullet":
        if rhs.startswith("new"):
            cls, raws, init_text, _ = parse_construction(rhs, env)
            node.bullet = parse_bullet(cls, raws, init_text, dict(env), unit, node)
        else:
            ref = eval_expr(rhs, env)
            node.bullet = ref
        env["bullet"] = ("bulletref", node.bullet)
        return
    if first == "shoot" and rhs.startswith("new"):
        cls, raws, init_text, _ = parse_construction(rhs, env)
        args = eval_args(raws, env)
        fields = {}
        if init_text:
            for st in split_top_level(init_text):
                sub = split_assignment(st)
                if sub:
                    t2, r2 = sub
                    fields[t2[0]] = eval_expr(r2, env)
        node.shoot = ("pattern", cls, args, fields)
        env["shoot"] = ("shootref", fields)
        return
    if first.startswith("shoot."):
        field = first.split(".", 1)[1]
        if node.shoot is None:
            node.shoot = ("pattern", "ShootPattern", [], {})
        node.shoot[3][field] = eval_expr(rhs, env)
        env["shoot"] = ("shootref", node.shoot[3])
        return
    if first in EFFECT_FIELDS and rhs.startswith("new"):
        cls, raws, init_text, _ = parse_construction(rhs, env)
        node.fields[first] = parse_effect(cls, raws, init_text, dict(env))
        return
    value = eval_expr(rhs, env)
    for target in targets:
        # Enclosing-scope: unit fields assigned inside a weapon initializer.
        if target in ("aimDst", "alwaysShootWhenMoving", "targetUnderBlocks"):
            unit.fields[target] = value
            env[target] = value
            continue
        node.fields[target] = value
        env[target] = value


def parse_part(cls, raws, init_text, env, unit=None, weapon=None):
    args = eval_args(raws, env)
    node = PartNode(cls, args)
    if init_text:
        for statement in split_top_level(init_text):
            statement = statement.strip()
            if not statement:
                continue
            if re.match(r"for\s*\(", statement):
                continue
            if declare_locals(statement, env):
                continue
            m2 = re.match(r"moves\.add\((.*)\)\s*$", statement, re.S)
            if m2:
                cls2, raws2, init_text2, _ = parse_construction(m2.group(1), env)
                args2 = eval_args(raws2, env)
                # PartMove(progress, x, y, rot) or (progress, x, y, gx, gy, rot)
                progress = args2[0]
                nums = [num(a) for a in args2[1:]]
                if len(nums) == 3:
                    node.moves.append(("partmove", progress, nums[0], nums[1], 0.0, 0.0, nums[2]))
                elif len(nums) == 5:
                    node.moves.append(("partmove", progress, nums[0], nums[1], nums[2], nums[3], nums[4]))
                else:
                    raise EvalError(f"bad PartMove args {args2!r}")
                continue
            assignment = split_assignment(statement)
            if assignment is None:
                continue
            targets, rhs = assignment
            value = eval_expr(rhs, env)
            for target in targets:
                # Enclosing-scope: weapon fields assigned inside a part body.
                if target in ("cooldownTime",) and weapon is not None:
                    weapon.fields[target] = value
                    continue
                node.fields[target] = value
                env[target] = value
    return node


def parse_ability(cls, raws, init_text, env):
    args = eval_args(raws, env)
    node = AbilityNode(cls, args)
    if init_text:
        for statement in split_top_level(init_text):
            statement = statement.strip()
            if not statement:
                continue
            assignment = split_assignment(statement)
            if assignment is None:
                continue
            targets, rhs = assignment
            value = eval_expr(rhs, env)
            for target in targets:
                node.fields[target] = value
    return node


def extract_leading_loop(statement):
    """Splits a leading `for(...){...}` off a statement; returns
    (loop_text, remainder) or None. Loops end with `}`, not `;`, so the
    statement splitter keeps trailing statements glued on."""
    m = re.match(r"\s*(for\s*\()", statement)
    if not m:
        return None
    open_paren = statement.index("(", m.start(1))
    close_paren = find_matching(statement, open_paren, "(", ")")
    open_brace = statement.index("{", close_paren)
    close_brace = find_matching(statement, open_brace)
    return statement[:close_brace + 1], statement[close_brace + 1:]


FOR_KEYWORD_RE = re.compile(r"for\s*\(")

def expand_for(statement, env, body_fn):
    m = re.match(r"for\s*\(\s*(?:float|int)\s+(\w+)\s*:\s*(.*?)\)\s*\{(.*)\}\s*$", statement, re.S)
    if m:
        var, domain_text, body = m.group(1), m.group(2), m.group(3)
        domain = eval_expr(domain_text, env)
        values = None
        if isinstance(domain, tuple) and domain[0] == "floatlist":
            values = domain[1]
        elif isinstance(domain, tuple) and domain[0] == "array":
            values = domain[2]
        if values is None:
            raise EvalError(f"cannot unroll for-each over {domain_text!r}")
        for value in values:
            child = dict(env)
            child[var] = value
            body_fn(body, child)
        return
    m = re.match(
        r"for\s*\(\s*int\s+(\w+)\s*=\s*(-?\w+)\s*;\s*\w+\s*(<|<=)\s*([\w.]+)\s*;\s*\w+\+\+\s*\)\s*\{(.*)\}\s*$",
        statement, re.S)
    if m:
        var, start_text, op, end_text, body = m.group(1), m.group(2), m.group(3), m.group(4), m.group(5)
        start = int(num(eval_expr(start_text, env)))
        end = int(num(eval_expr(end_text, env)))
        rng = range(start, end + 1) if op == "<=" else range(start, end)
        for value in rng:
            child = dict(env)
            child[var] = value
            body_fn(body, child)
        return
    raise EvalError(f"cannot unroll loop {statement[:60]!r}")


def parse_unit_body(unit, body, env):
    for statement in split_top_level(body):
        statement = statement.strip()
        if not statement:
            continue
        loop = extract_leading_loop(statement)
        if loop is not None:
            loop_text, remainder = loop
            expand_for(loop_text, env, lambda body2, e: parse_unit_body(unit, body2, e))
            if remainder.strip():
                parse_unit_body(unit, remainder, env)
            continue
        if declare_locals(statement, env, unit):
            continue
        m = re.match(r"weapons\.add\((.*)\)\s*$", statement, re.S)
        if m:
            for piece in split_top_level(m.group(1), ","):
                piece = piece.strip()
                if piece.startswith("new"):
                    cls, raws, init_text, _ = parse_construction(piece, env)
                    unit.weapons.append(parse_weapon(cls, raws, init_text, dict(env), unit))
            continue
        m = re.match(r"abilities\.add\((.*)\)\s*$", statement, re.S)
        if m:
            for piece in split_top_level(m.group(1), ","):
                piece = piece.strip()
                if piece.startswith("new"):
                    cls, raws, init_text, _ = parse_construction(piece, env)
                    unit.abilities.append(parse_ability(cls, raws, init_text, dict(env)))
            continue
        m = re.match(r"parts\.add(?:All)?\((.*)\)\s*$", statement, re.S)
        if m:
            for piece in split_top_level(m.group(1), ","):
                piece = piece.strip()
                if piece.startswith("new"):
                    cls, raws, init_text, _ = parse_construction(piece, env)
                    unit.parts.append(parse_part(cls, raws, init_text, dict(env), unit))
            continue
        m = re.match(r"immunities\.add(?:All)?\((.*)\)\s*$", statement, re.S)
        if m:
            for piece in split_top_level(m.group(1), ","):
                unit.immunities.append(eval_expr(piece, env))
            continue
        m = re.match(r"setEngines(?:Mirror)?\((.*)\)\s*$", statement, re.S)
        if m:
            for piece in split_top_level(m.group(1), ","):
                unit.engines_mirror.append(eval_expr(piece, env))
            continue
        assignment = split_assignment(statement)
        if assignment is None:
            continue
        targets, rhs = assignment
        first = targets[0]
        if first == "immunities":
            value = eval_expr(rhs, env)
            if isinstance(value, tuple) and value[0] == "array":
                unit.immunities.extend(value[2])
            else:
                unit.immunities.append(value)
            continue
        if first == "targetFlags":
            unit.target_flags = eval_expr(rhs, env)
            continue
        if first == "treadRects":
            unit.tread_rects = eval_expr(rhs, env)
            continue
        value = eval_expr(rhs, env)
        for target in targets:
            unit.fields[target] = value
            env[target] = value


# ---------------------------------------------------------------- UnitTypes.java parse

def parse_unit_types():
    raw = (ROOT / "content/UnitTypes.java").read_text(encoding="utf-8")
    # Comments are replaced with spaces so positions stay valid for region marks.
    text = re.sub(r"//[^\n]*", lambda m: " " * len(m.group(0)), raw)

    field_entities = {}
    entity_groups = []
    decl_re = re.compile(
        r"@EntityDef\((?:value\s*=\s*)?\{([^}]*)\}(?:\s*,\s*legacy\s*=\s*(true))?\)\s*UnitType\s+([^;]+);")
    for m in decl_re.finditer(text):
        components = [c.strip().split(".")[0] for c in m.group(1).split(",")]
        legacy = bool(m.group(2))
        fields = [f.strip() for f in m.group(3).split(",")]
        entity_groups.append((components, legacy))
        index = len(entity_groups) - 1
        for field in fields:
            field_entities[field] = index
            UNIT_FIELD_NAMES.add(field)

    load_start = text.index("public static void load(){")
    body = text[load_start + len("public static void load(){"):]
    region_marks = [(m.start(), m.group(1).strip()) for m in re.finditer(r"//region\s+([^\n]+)", raw)]

    units = []
    env = {"tilesize": 8.0}
    statements = split_top_level(body)
    char_pos = 0
    region = "standard"
    for statement in statements:
        stripped_for_pos = statement.strip()
        if stripped_for_pos:
            stmt_pos = text.find(stripped_for_pos, char_pos)
        else:
            stmt_pos = char_pos
        char_pos = stmt_pos + max(len(stripped_for_pos), 1)
        for mark_pos, mark in region_marks:
            if mark_pos < stmt_pos:
                region = mark
        stripped = statement.strip()
        if not stripped:
            continue
        m = re.match(r"(\w+)\s*=\s*new\s+(\w+)\s*\(", stripped)
        if not m or m.group(2) not in UNIT_CLASSES:
            continue
        field, cls = m.group(1), m.group(2)
        _, raws, init_text, _ = parse_construction(stripped[len(field) + 2:].strip(), env)
        args = eval_args(raws[:1], env)
        name = args[0][1] if args and isinstance(args[0], tuple) else field
        unit = UnitNode(field, cls, name, region, text[:stmt_pos].count("\n") + 1)
        unit.entity_group = field_entities.get(field)
        if init_text:
            parse_unit_body(unit, init_text, dict(env))
        units.append(unit)
    return units, entity_groups

# ---------------------------------------------------------------- emission

def f32_literal(value):
    """Shortest decimal that round-trips to the same IEEE-754 binary32."""
    packed = struct.pack("<f", value)
    unpacked = struct.unpack("<f", packed)[0]
    for precision in range(1, 10):
        text = f"{unpacked:.{precision}g}"
        if struct.pack("<f", float(text)) == packed:
            return text
    return repr(unpacked)


def rust_f32(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if value == float("inf"):
        return "f32::INFINITY"
    if value == float("-inf"):
        return "f32::NEG_INFINITY"
    v = f32(float(value))
    if v == int(v) and abs(v) < 1e15:
        return f"{int(v)}.0"
    return f32_literal(v)


def rust_i32(value):
    return str(int(num(value)))


def rust_bool(value):
    return "true" if truthy(value) else "false"


def rust_rgba(value):
    r, g, b, a = value
    if (r, g, b, a) == (1.0, 1.0, 1.0, 1.0):
        return "Rgba::WHITE"
    return f"Rgba::new({rust_f32(r)}, {rust_f32(g)}, {rust_f32(b)}, {rust_f32(a)})"


def rust_effect_const(name):
    if name == "none":
        return "EffectId::NONE"
    const = fx_name_to_const.get(name)
    if const is None:
        raise EvalError(f"unknown Fx.{name}")
    return f"EffectId::{const}"


def rust_sound_const(name):
    if name in ("none", "unset"):
        return f"SoundId::{name.upper()}"
    if name not in sound_ids:
        raise EvalError(f"unknown Sounds.{name}")
    return f"SoundId::{screaming(name)}"


def rust_env(value):
    if isinstance(value, (int, float)):
        if value == 0:
            return "EnvMask::none()"
        raise EvalError(f"nonzero raw env mask {value}")
    if value.get("any"):
        return "EnvMask::any()"
    flags = value.get("flags", [])
    if not flags:
        return "EnvMask::none()"
    mapped = ", ".join(f"EnvFlag::{pascal(f)}" for f in flags)
    return f"EnvMask::of(vec![{mapped}])"


CONTROLLER_MAP = {
    "new AssemblerAI()": "ControllerKind::Assembler",
    "new CargoAI()": "ControllerKind::Cargo",
    "new NoAI()": "ControllerKind::No",
    "new MissileAI()": "ControllerKind::Missile",
}


def rust_controller(value):
    text = value[1].strip()
    text = text[text.index("->") + 2:].strip()
    if text.startswith("u.team.isAI()"):
        return "ControllerKind::BuilderOrCommand"
    m = re.match(r"new BuilderAI\(true,\s*([\w.]+)\)", text)
    if m:
        rng = m.group(1)
        value_f = 500.0 if rng == "coreFleeRange" else num(eval_expr(rng, {}))
        return f"ControllerKind::Builder {{ core_flee_range: {rust_f32(value_f)} }}"
    if text in CONTROLLER_MAP:
        return CONTROLLER_MAP[text]
    raise EvalError(f"unknown controller lambda {text!r}")


AI_MAP = {
    "DefenderAI": "AiControllerKind::Defender",
    "FlyingFollowAI": "AiControllerKind::FlyingFollow",
    "HugAI": "AiControllerKind::Hug",
    "SuicideAI": "AiControllerKind::Suicide",
}

COMMAND_NAMES = {
    "moveCommand": "move", "repairCommand": "repair", "rebuildCommand": "rebuild",
    "assistCommand": "assist", "mineCommand": "mine", "enterPayloadCommand": "enterPayload",
    "loadUnitsCommand": "loadUnits", "loadBlocksCommand": "loadBlocks",
    "unloadPayloadCommand": "unloadPayload", "loopPayloadCommand": "loopPayload",
}


def rust_effect(value):
    """Emits an EffectRef expression (named or inline)."""
    if isinstance(value, EffectNode):
        return rust_effect_node(value)
    if isinstance(value, tuple) and value[0] == "effect":
        return f"EffectRef::Named({rust_effect_const(value[1])})"
    raise EvalError(f"bad effect value {value!r}")


def rust_effect_node(node):
    if node.cls == "Effect":
        args = [num(a) for a in node.args if isinstance(a, (int, float))]
        lifetime = args[0] if args else 0.0
        if len(args) > 1:
            return f"EffectRef::Inline(Box::new(EffectSpec::plain_clip({rust_f32(lifetime)}, {rust_f32(args[1])})))"
        return f"EffectRef::Inline(Box::new(EffectSpec::plain({rust_f32(lifetime)})))"
    if node.cls == "MultiEffect":
        children = ", ".join(rust_effect(c) for c in node.children)
        lifetime = effect_node_lifetime(node)
        return f"EffectRef::Inline(Box::new(EffectSpec::multi({rust_f32(lifetime)}, vec![{children}])))"
    if node.cls == "WrapEffect":
        child = rust_effect(node.children[0]) if node.children else "EffectRef::Named(EffectId::NONE)"
        color = node.fields.get("color")
        color_text = rust_rgba(color[1]) if isinstance(color, tuple) and color[0] == "rgba" else "Rgba::WHITE"
        lifetime = effect_node_lifetime(node)
        return f"EffectRef::Inline(Box::new(EffectSpec::wrap({child}, {color_text}, {rust_f32(lifetime)})))"
    ctor = "EffectSpec::explosion()" if node.cls == "ExplosionEffect" else "EffectSpec::wave()"
    lines = [f"let mut effect = {ctor};"]
    for field, value in node.fields.items():
        lines.append(f"effect.{EFFECT_FIELD_MAP[field]} = {rust_effect_field(field, value)};")
    body = "\n            ".join(lines)
    return ("{ let mut effect = " + ctor + "; "
            + "; ".join(f"effect.{EFFECT_FIELD_MAP[f]} = {rust_effect_field(f, v)}" for f, v in node.fields.items())
            + "; EffectRef::Inline(Box::new(effect)) }")


def effect_node_lifetime(node):
    if node.cls == "MultiEffect":
        lifetimes = [effect_value_lifetime(c) for c in node.children]
        return max(lifetimes) if lifetimes else 0.0
    if "lifetime" in node.fields:
        return num(node.fields["lifetime"])
    if node.cls == "Effect" and node.args:
        numeric = [a for a in node.args if isinstance(a, (int, float))]
        if numeric:
            return num(numeric[0])
    if node.cls == "WrapEffect" and node.children:
        return effect_value_lifetime(node.children[0])
    return 0.0


def effect_value_lifetime(value):
    if isinstance(value, EffectNode):
        return effect_node_lifetime(value)
    if isinstance(value, tuple) and value[0] == "effect":
        return fx_lifetimes.get(value[1], 0.0)
    return 0.0


EFFECT_FIELD_MAP = {
    "waveStroke": "wave_stroke", "waveColor": "wave_color", "waveLife": "wave_life",
    "waveRad": "wave_rad", "waveRadBase": "wave_rad_base", "sparkColor": "spark_color",
    "sparkRad": "spark_rad", "sparkLen": "spark_len", "sparkStroke": "spark_stroke",
    "smokeColor": "smoke_color", "smokes": "smokes", "smokeSize": "smoke_size",
    "smokeSizeBase": "smoke_size_base", "sparks": "sparks", "colorFrom": "wave_color", "colorTo": "color_to",
    "sizeFrom": "size_from", "sizeTo": "size_to", "strokeFrom": "stroke_from",
    "strokeTo": "stroke_to", "lifetime": "lifetime",
}


def rust_effect_field(field, value):
    if field in ("waveColor", "sparkColor", "smokeColor", "colorTo", "colorFrom"):
        if isinstance(value, tuple) and value[0] == "rgba":
            return f"Some({rust_rgba(value[1])})"
        raise EvalError(f"bad color for {field}: {value!r}")
    if field in ("sparks", "smokes"):
        return rust_i32(value)
    if field == "lifetime":
        return rust_f32(num(value))
    return rust_f32(num(value))


def rust_progress(value):
    """Emits a PartProgressSpec expression."""
    if isinstance(value, tuple) and value[0] == "absin_time":
        return (f"PartProgressSpec::AbsinTime {{ offset: {rust_f32(value[1])}, "
                f"scl: {rust_f32(value[2])}, mag: {rust_f32(value[3])} }}")
    if isinstance(value, tuple) and value[0] == "lambda":
        raise EvalError(f"unhandled progress lambda {value[1]!r}")
    if isinstance(value, tuple) and value[0] == "progress":
        name = value[1]
        return f"PartProgressSpec::{pascal(name)}"
    if isinstance(value, tuple) and value[0] == "pexpr":
        method = value[1]
        if method in ("delay", "add", "mul", "shorten", "mod", "loop"):
            ctor = {"delay": "Delay", "add": "Add", "mul": "Mul", "shorten": "Shorten", "mod": "Mod", "loop": "Loop"}[method]
            return f"PartProgressSpec::{ctor}(Box::new({rust_progress(value[2])}), {rust_f32(value[3])})"
        if method == "blend":
            return f"PartProgressSpec::Blend(Box::new({rust_progress(value[2])}), Box::new({rust_progress(value[3])}), {rust_f32(value[4])})"
        if method in ("min", "max"):
            ctor = "Min" if method == "min" else "Min"
            return f"PartProgressSpec::{ctor}(Box::new({rust_progress(value[2])}), Box::new({rust_progress(value[3])}))"
        if method in ("inv", "slope", "clamp"):
            ctor = {"inv": "Inv", "slope": "Slope", "clamp": "Clamp"}[method]
            return f"PartProgressSpec::{ctor}(Box::new({rust_progress(value[2])}))"
        if method == "curve_range":
            return f"PartProgressSpec::CurveRange(Box::new({rust_progress(value[2])}), {rust_f32(value[3])}, {rust_f32(value[4])})"
        if method == "curve_interp":
            interp = {"linear": "Linear", "one": "One", "slope": "Slope", "pow2In": "Pow2In", "pow5In": "Pow5In"}[value[3]]
            return f"PartProgressSpec::CurveInterp(Box::new({rust_progress(value[2])}), InterpKind::{interp})"
        if method == "sustain":
            return f"PartProgressSpec::Sustain(Box::new({rust_progress(value[2])}), {rust_f32(value[3])}, {rust_f32(value[4])}, {rust_f32(value[5])})"
        if method == "compress":
            return f"PartProgressSpec::Compress(Box::new({rust_progress(value[2])}), {rust_f32(value[3])}, {rust_f32(value[4])})"
        if method == "sin":
            return f"PartProgressSpec::Sin(Box::new({rust_progress(value[2])}), {rust_f32(value[3])}, {rust_f32(value[4])}, {rust_f32(value[5])})"
        if method == "absin":
            return f"PartProgressSpec::Absin(Box::new({rust_progress(value[2])}), {rust_f32(value[3])}, {rust_f32(value[4])})"
    raise EvalError(f"bad progress value {value!r}")


def rust_interp(value):
    return {"linear": "Linear", "one": "One", "slope": "Slope", "pow2In": "Pow2In", "pow5In": "Pow5In"}[value]


# --- unit field map: java name -> (rust name, kind)
UNIT_FIELDS = {}
for _f in ("speed boostMultiplier floorMultiplier rotateSpeed baseRotateSpeed drag accel hitSize "
           "deathShake stepShake rippleScale riseSpeed descentSpeed fallSpeed missileAccelTime health armor "
           "range maxRange mineRange buildRange circleTargetRadius crashDamageMultiplier wreckHealthMultiplier "
           "drownTimeMultiplier strafePenalty researchCostMultiplier knockbackMultiplier groundLayer flyingLayer "
           "payloadCapacity buildSpeed aimDst buildBeamOffset targetPriority shadowElevation shadowElevationScl "
           "engineOffset engineSize engineLayer itemOffsetY lightRadius lightOpacity softShadowScl fogRadius "
           "waveTrailX waveTrailY trailScl deathSoundVolume wreckSoundVolume loopSoundVolume stepSoundVolume "
           "stepSoundPitch stepSoundPitchRange moveSoundVolume moveSoundPitchMin moveSoundPitchMax tankMoveVolume "
           "mineSpeed mineSoundVolume legLength legSpeed legForwardScl legBaseOffset legMoveSpace legExtension "
           "legPairOffset legLengthScl legStraightLength legMaxLength legMinLength legSplashDamage legSplashRange "
           "baseLegStraightness legStraightness mechLandShake mechSideSway mechFrontSway mechStride segmentMag "
           "segmentScl segmentPhase segmentRotSpeed segmentMaxRot segmentSpacing segmentRotationRange "
           "crawlSlowdown crushDamage crawlSlowdownFrac lifetime homingDelay clipSize").split():
    UNIT_FIELDS[_f] = (snake(_f), "f32")
for _f in ("itemCapacity mineTier legCount legGroupSize treadFrames treadPullOffset segments segmentUnits "
           "trailLength outlineRadius").split():
    UNIT_FIELDS[_f] = (snake(_f), "i32")
for _f in ("isEnemy flying wobble targetAir targetGround faceTarget circleTarget autoDropBombs "
           "targetBuildingsMobile canBoost boostWhenBuilding boostWhenMining logicControllable playerControllable "
           "controlSelectGlobal allowedInPayloads hittable killable targetable vulnerableWithPayloads pickupUnits "
           "physics canDrown useUnitCap coreUnitDock createWreck createScorch lowAltitude rotateToBuilding "
           "allowLegStep legPhysicsLayer hovering omniMovement rotateMoveFirst healFlash singleTarget "
           "forceMultiTarget hidden internal internalGenerateSprites bounded autoFindTarget targetUnderBlocks "
           "alwaysShootWhenMoving hoverable alwaysCreateOutline generateFullIcon squareShape drawBuildBeam "
           "drawMineBeam drawCell drawItems drawShields drawBody drawSoftShadow drawMinimap allowChangeCommands "
           "outlines mineWalls mineFloor mineHardnessScaling legBaseUnder lockLegBase legContinuousMove "
           "flipBackLegs flipLegSide emitWalkSound emitWalkEffect mechStepParticles crushFragile "
           "segmentLayerOrder useEngineElevation alwaysUnlocked hideDetails").split():
    UNIT_FIELDS[_f] = (snake(_f), "bool")
for _f in ("outlineColor healColor lightColor shieldColor engineColor engineColorInner trailColor mechLegColor").split():
    UNIT_FIELDS[_f] = (snake(_f), "rgba")
for _f in ("deathSound wreckSound loopSound stepSound tankMoveSound moveSound mineSound").split():
    UNIT_FIELDS[_f] = (snake(_f), "sound")
for _f in ("fallEffect fallEngineEffect deathExplosionEffect").split():
    UNIT_FIELDS[_f] = (snake(_f), "effectref")
for _f in ("envRequired envEnabled envDisabled").split():
    UNIT_FIELDS[_f] = (snake(_f), "env")
UNIT_FIELDS["aiController"] = ("ai_controller", "ai")
UNIT_FIELDS["controller"] = ("controller", "controller")
UNIT_FIELDS["defaultCommand"] = ("default_command", "command")

# --- weapon field map
WEAPON_FIELDS = {}
for _f in ("reload inaccuracy shake recoil recoilTime recoilPow cooldownTime shootX shootY x y xRand yRand "
           "shadow velocityRnd extraVelocity lifeRnd extraLife shootCone rotationLimit minWarmup "
           "shootWarmupSpeed smoothReloadSpeed soundPitchMin soundPitchMax minShootVelocity maxShootVelocity "
           "layerOffset activeSoundVolume shootSoundVolume targetInterval targetSwitchInterval rotateSpeed "
           "baseRotation aimChangeSpeed shootStatusDuration repairSpeed fractionRepairSpeed beamWidth "
           "pulseRadius pulseStroke widthSinMag widthSinScl damageTargetWeight").split():
    WEAPON_FIELDS[_f] = (snake(_f), "f32")
for _f in "recoils".split():
    WEAPON_FIELDS[_f] = (snake(_f), "i32")
for _f in ("display mirror alternate rotate showStatSprite top continuous alwaysContinuous controllable "
           "aiControllable alwaysShooting autoTarget predictTarget useAttackRange linearWarmup ignoreRotation "
           "noAttack parentizeEffects shootOnDeath targetBuildings targetUnits healBeamMount").split():
    WEAPON_FIELDS[_f] = (snake(_f), "bool")
for _f in ("heatColor laserColor beamColor healColor").split():
    WEAPON_FIELDS[_f] = (snake(_f), "rgba")
for _f in ("shootSound activeSound initialShootSound chargeSound").split():
    WEAPON_FIELDS[_f] = (snake(_f), "sound")
for _f in ("ejectEffect shootOnDeathEffect beamEffect").split():
    WEAPON_FIELDS[_f] = (snake(_f), "effectref")
WEAPON_FIELDS["shootStatus"] = ("shoot_status", "status")

# --- bullet field map
BULLET_FIELDS = {}
for _f in ("lifetime speed damage hitSize drawSize drag accel pierceDamageFactor optimalLifeFract "
           "reloadMultiplier ammoMultiplier recoil splashDamage splashDamageRadius knockback "
           "buildingDamageMultiplier shieldDamageMultiplier statusDuration statusChance armorMultiplier "
           "blockArmorMultiplier inaccuracy hitSoundVolume hitSoundPitch hitSoundPitchRange hitShake "
           "despawnShake trailWidth trailChance trailInterval trailParam trailSpread fragRandomSpread "
           "fragSpread fragAngle fragVelocityMin fragVelocityMax fragLifeMin fragLifeMax fragOffsetMin "
           "fragOffsetMax bulletInterval intervalRandomSpread intervalSpread intervalAngle intervalDelay "
           "lightningDamage lightningCone lightningAngle spawnBulletRandomSpread puddleRange puddleAmount "
           "suppressionRange suppressionDuration suppressionEffectChance width height shrinkX shrinkY spin "
           "rotationOffset length damageInterval sideLength sideWidth sideAngle lightningSpacing lightningDelay "
           "lightningAngleRand serrationLenScl serrationWidth serrationSpacing serrationSpaceOffset "
           "serrationFadeOffset sapStrength lengthRand empRadius timeIncrease timeDuration powerDamageScl "
           "powerSclDecrease unitDamageScl pointEffectSpace maxRange rangeOverride homingPower homingRange "
           "homingDelay followAimSpeed weaveScale weaveMag hitColor lightRadius lightOpacity incendSpread "
           "incendChance lifesteal healAmount healPercent trailMult trailSize explodeRange explodeDelay "
           "flakDelay flakInterval layer shake").split():
    BULLET_FIELDS[_f] = (snake(_f), "f32")
for _f in ("pierceCap fragBullets pierceFragCap intervalBullets lightning lightningLength lightningLengthRand "
           "spawnBulletRandomSpread2 puddles incendAmount trailLength serrations").split():
    BULLET_FIELDS[_f] = (snake(_f), "i32")
for _f in ("keepVelocity scaleKeepVelocity scaleLife pierce pierceBuilding removeAfterPierce laserAbsorb "
           "laserBullet displayAmmoMultiplier killShooter instantDisappear splashDamagePierce "
           "scaledSplashDamage impact pierceArmor hittable reflectable absorbable collides collidesAir "
           "collidesGround collidesTiles collidesTeam collideFloor collideTerrain despawnHit trailRotation "
           "fragOnHit fragOnDespawn fragOnAbsorb delayFrags underwater showStats weaveRandom makeFire "
           "setDefaults largeHit hitUnits").split():
    BULLET_FIELDS[_f] = (snake(_f), "bool")
for _f in ("trailColor lightningColor suppressColor backColor frontColor mixColorFrom mixColorTo fromColor "
           "toColor beamColor lightColor hitColor").split():
    BULLET_FIELDS[_f] = (snake(_f), "rgba")
BULLET_FIELDS["color"] = ("beam_color", "rgba")
for _f in ("hitSound despawnSound shootSound").split():
    BULLET_FIELDS[_f] = (snake(_f), "sound")
for _f in ("hitEffect despawnEffect shootEffect smokeEffect chargeEffect trailEffect hitPowerEffect "
           "chainEffect applyEffect pierceEffect pointEffect lineEffect endEffect").split():
    BULLET_FIELDS[_f] = (snake(_f), "effectref")
BULLET_FIELDS["status"] = ("status", "status")
BULLET_FIELDS["puddleLiquid"] = ("puddle_liquid", "liquid")
BULLET_FIELDS["colors"] = ("colors", "rgba_vec")
BULLET_FIELDS["shrinkInterp"] = ("shrink_interp", "interp")
BULLET_FIELDS["trailInterp"] = ("trail_interp", "interp")
BULLET_FIELDS["sprite"] = ("sprite", "string")
BULLET_FIELDS["backSprite"] = ("back_sprite", "string")
BULLET_FIELDS["liquid"] = ("liquid", "liquid")


def rust_value(value, kind):
    if kind == "f32":
        return rust_f32(value)
    if kind == "i32":
        return rust_i32(value)
    if kind == "bool":
        return rust_bool(value)
    if kind == "rgba":
        return rust_rgba(value[1])
    if kind == "sound":
        return rust_sound_const(value[1])
    if kind == "effectref":
        return rust_effect(value)
    if kind == "env":
        return rust_env(value)
    if kind == "status":
        return f"Some(\"{value[1]}\")"
    if kind == "liquid":
        return f"Some(\"{value[1]}\")"
    if kind == "rgba_vec":
        items = ", ".join(rust_rgba(v[1]) for v in value[2])
        return f"Some(vec![{items}])"
    if kind == "interp":
        return f"Some(InterpKind::{rust_interp(value[1])})"
    if kind == "string":
        return f"Some(Some(String::from(\"{value[1]}\")))"
    if kind == "controller":
        return f"Some({rust_controller(value)})"
    if kind == "ai":
        return f"Some({AI_MAP[value[1]]})"
    if kind == "command":
        return f"Some(\"{COMMAND_NAMES[value[1]]}\")"
    raise EvalError(f"unknown kind {kind}")



def emit_literal_fields(node_fields, field_map, indent, skip=()):
    """Struct-literal fields (`name: value,`) for BulletSpec construction."""
    lines = []
    for java, value in node_fields.items():
        if java in skip:
            continue
        if java not in field_map:
            raise EvalError(f"unmapped field {java}")
        rust_name, kind = field_map[java]
        if kind in ("f32", "i32", "bool", "rgba", "sound", "env", "effectref"):
            lines.append(f"{indent}{rust_name}: Some({rust_value(value, kind)}),")
        else:
            lines.append(f"{indent}{rust_name}: {rust_value(value, kind)},")
    return lines

def emit_spec_fields(node_fields, field_map, indent, skip=(), receiver="unit"):
    lines = []
    for java, value in node_fields.items():
        if java in skip:
            continue
        if java not in field_map:
            raise EvalError(f"unmapped field {java}")
        rust_name, kind = field_map[java]
        if kind in ("f32", "i32", "bool", "rgba", "sound", "env", "effectref"):
            lines.append(f"{indent}{receiver}.{rust_name} = Some({rust_value(value, kind)});")
        else:
            lines.append(f"{indent}{receiver}.{rust_name} = {rust_value(value, kind)};")
    return lines


BULLET_KIND_MAP = {
    "BulletType": "BulletKind::Plain",
    "BasicBulletType": "BulletKind::Basic",
    "ArtilleryBulletType": "BulletKind::Artillery",
    "MissileBulletType": "BulletKind::Missile",
    "LaserBoltBulletType": "BulletKind::LaserBolt",
    "SapBulletType": "BulletKind::Sap",
    "LightningBulletType": "BulletKind::Lightning",
    "LaserBulletType": "BulletKind::Laser",
    "FlakBulletType": "BulletKind::Flak",
    "ExplosionBulletType": "BulletKind::Explosion",
    "RailBulletType": "BulletKind::Rail",
    "ContinuousLaserBulletType": "BulletKind::ContinuousLaser",
    "ShrapnelBulletType": "BulletKind::Shrapnel",
    "LiquidBulletType": "BulletKind::Liquid",
    "EmpBulletType": "BulletKind::Emp",
    "BombBulletType": "BulletKind::Bomb",
    "FireBulletType": "BulletKind::Fire",
    "SpaceLiquidBulletType": "BulletKind::SpaceLiquid",
}


def bullet_ctor_fields(node):
    """Constructor args mapped to BulletSpec fields."""
    args = node.args
    out = {}
    if node.cls in ("BasicBulletType", "ArtilleryBulletType", "MissileBulletType"):
        if args:
            out["speed"] = args[0]
            out["damage"] = args[1]
        if len(args) > 2:
            out["sprite"] = args[2]
    elif node.cls in ("LaserBoltBulletType", "FlakBulletType", "BulletType"):
        if args:
            out["speed"] = args[0]
            out["damage"] = args[1]
    elif node.cls in ("LaserBulletType", "ContinuousLaserBulletType"):
        if args:
            out["damage"] = args[0]
    elif node.cls == "ExplosionBulletType":
        out["splash_damage"] = args[0]
        out["splash_damage_radius"] = args[1]
        out["range_override"] = max(-1.0, num(args[1]) * 2.0 / 3.0)
    elif node.cls == "BombBulletType":
        out["splash_damage"] = args[0]
        out["splash_damage_radius"] = args[1]
        if len(args) > 2:
            out["sprite"] = args[2]
    elif node.cls == "LiquidBulletType":
        if args:
            out["liquid"] = args[0]
    return out


def emit_bullet(node, indent):
    """Emits a BulletSpec expression for a BulletNode."""
    lines = ["BulletSpec {"]
    inner = indent + "    "
    lines.append(f"{inner}kind: {BULLET_KIND_MAP[node.cls]},")
    ctor = bullet_ctor_fields(node)
    for java, value in ctor.items():
        if java == "sprite":
            lines.append(f"{inner}sprite: Some(Some(String::from(\"{value[1]}\"))),")
        elif java == "liquid":
            lines.append(f"{inner}liquid: Some(\"{value[1]}\"),")
        elif java in ("splash_damage", "splash_damage_radius", "range_override"):
            lines.append(f"{inner}{java}: Some({rust_f32(value)}),")
        else:
            lines.append(f"{inner}{java}: Some({rust_f32(value)}),")
    lines.extend(emit_literal_fields(node.fields, BULLET_FIELDS, inner))
    for field, effect in node.effect_fields.items():
        rust_name, _ = BULLET_FIELDS[field]
        lines.append(f"{inner}{rust_name}: Some({rust_effect(effect)}),")
    for nested_name, nested in node.nested.items():
        rust_field = {"fragBullet": "frag_bullet", "intervalBullet": "interval_bullet", "lightningType": "lightning_type"}[nested_name]
        nested_text = emit_bullet(nested, inner)
        lines.append(f"{inner}{rust_field}: Some(Box::new({nested_text})),")
    if node.spawn_bullets:
        spawned = ",\n".join(emit_bullet(s, inner + "    ") for s in node.spawn_bullets)
        lines.append(f"{inner}spawn_bullets: vec![\n{inner}    {spawned}\n{inner}],")
    if node.spawn_unit is not None:
        unit_text = emit_unit_inline(node.spawn_unit, inner)
        lines.append(f"{inner}spawn_unit: Some(Box::new({unit_text})),")
    lines.append(f"{indent}..BulletSpec::default()")
    lines.append(f"{indent}}}")
    return "\n".join(lines)


WEAPON_KIND_MAP = {
    "Weapon": "WeaponKind::Weapon",
    "BuildWeapon": "WeaponKind::BuildWeapon",
    "MineWeapon": "WeaponKind::MineWeapon",
    "PointDefenseWeapon": "WeaponKind::PointDefenseWeapon",
    "PointDefenseBulletWeapon": "WeaponKind::PointDefenseBulletWeapon",
    "RepairBeamWeapon": "WeaponKind::RepairBeamWeapon",
}


def emit_shoot(shoot, indent):
    _, cls, args, fields = shoot
    lines = []
    if cls == "ShootPattern":
        shots = num(fields.get("shots", 1))
        delay = num(fields.get("shotDelay", 0.0))
        first = num(fields.get("firstShotDelay", 0.0))
        base = f"ShootPatternSpec::plain({rust_i32(shots)}, {rust_f32(delay)}, {rust_f32(first)})"
        assigned = {k: v for k, v in fields.items() if k not in ("shots", "shotDelay", "firstShotDelay")}
    elif cls == "ShootAlternate":
        spread = num(args[0]) if args else 5.0
        shots = num(fields.get("shots", 1))
        delay = num(fields.get("shotDelay", 0.0))
        barrels = num(fields.get("barrels", 2))
        base = f"ShootPatternSpec::alternate({rust_i32(shots)}, {rust_f32(delay)}, {rust_f32(spread)}, {rust_i32(barrels)})"
        assigned = {k: v for k, v in fields.items() if k not in ("shots", "shotDelay", "barrels")}
    elif cls == "ShootSpread":
        shots = num(args[0]) if args else num(fields.get("shots", 1))
        spread = num(args[1]) if len(args) > 1 else 5.0
        base = f"ShootPatternSpec::spread({rust_i32(shots)}, {rust_f32(spread)})"
        assigned = dict(fields)
        assigned.pop("shots", None)
    elif cls == "ShootHelix":
        scl = num(fields.get("scl", 2.0))
        mag = num(fields.get("mag", 1.5))
        base = f"ShootPatternSpec::helix({rust_f32(scl)}, {rust_f32(mag)})"
        assigned = {k: v for k, v in fields.items() if k not in ("scl", "mag")}
    else:
        raise EvalError(f"unhandled shoot pattern {cls}")
    if not assigned:
        return base
    lines.append(f"{{ let mut shoot = {base};")
    for field, value in assigned.items():
        rust_name = snake(field)
        if field == "firstShotDelay":
            lines.append(f"shoot.first_shot_delay = {rust_f32(value)};")
        elif field == "shotDelay":
            lines.append(f"shoot.shot_delay = {rust_f32(value)};")
        elif field == "shots":
            lines.append(f"shoot.shots = {rust_i32(value)};")
        else:
            lines.append(f"shoot.{rust_name} = {rust_f32(value)};")
    lines.append("shoot }")
    return " ".join(lines)


def emit_weapon(node, indent):
    lines = ["{"]
    inner = indent + "    "
    lines.append(f"{inner}let mut weapon = WeaponSpec {{")
    lines.append(f"{inner}    name: \"{node.name}\",")
    lines.append(f"{inner}    kind: {WEAPON_KIND_MAP[node.cls]},")
    if node.shoot is not None:
        lines.append(f"{inner}    shoot: Some({emit_shoot(node.shoot, inner)}),")
    if node.bullet is not None:
        if isinstance(node.bullet, BulletNode):
            lines.append(f"{inner}    bullet: BulletRef::Inline(Box::new({emit_bullet(node.bullet, inner + '    ')})),")
        elif isinstance(node.bullet, tuple) and node.bullet[0] == "pre":
            lines.append(f"{inner}    bullet: BulletRef::Pre({node.bullet[1]}),")
        else:
            raise EvalError(f"bad bullet ref {node.bullet!r}")
    elif node.cls in ("BuildWeapon", "MineWeapon"):
        # `BuildWeapon`/`MineWeapon` instance initializer: `bullet = new BulletType()`.
        lines.append(f"{inner}    bullet: BulletRef::Inline(Box::new(BulletSpec {{")
        lines.append(f"{inner}        kind: BulletKind::Plain,")
        lines.append(f"{inner}        ..BulletSpec::default()")
        lines.append(f"{inner}    }})),")
    lines.append(f"{inner}    ..WeaponSpec::default()")
    lines.append(f"{inner}}};")
    lines.extend(emit_spec_fields(node.fields, WEAPON_FIELDS, inner, receiver="weapon"))
    for part in node.parts:
        lines.append(f"{inner}weapon.parts.push({emit_part(part, inner)});")
    lines.append(f"{indent}weapon")
    lines.append(f"{indent}}}")
    return "\n".join(lines)


ABILITY_CTOR = {
    "ShieldRegenFieldAbility": ("shield_regen_field", ["amount", "max", "reload", "range"]),
    "RepairFieldAbility": ("repair_field", ["amount", "reload", "range"]),
    "ForceFieldAbility": ("force_field", ["range", "regen", "max", "cooldown"]),
    "StatusFieldAbility": ("status_field", ["effect", "duration", "reload", "range"]),
    "EnergyFieldAbility": ("energy_field", ["amount", "reload", "range"]),
    "SuppressionFieldAbility": ("suppression_field", []),
    "ShieldArcAbility": ("shield_arc", []),
    "MoveEffectAbility": ("move_effect", ["x", "y", "color", "effect", "interval"]),
    "SpawnDeathAbility": ("spawn_death", ["unit", "amount", "spread"]),
    "RegenAbility": ("regen", ["percent_amount"]),
    "LiquidExplodeAbility": ("liquid_explode", ["liquid"]),
    "LiquidRegenAbility": ("liquid_regen", ["liquid", "slurp_effect"]),
}

ABILITY_FIELD_MAP = {
    "amount": ("amount", "f32"), "max": ("max", "f32"), "reload": ("reload", "f32"),
    "range": ("range", "f32"), "healPercent": ("heal_percent", "f32"),
    "sameTypeHealMult": ("same_type_heal_mult", "f32"), "maxTargets": ("max_targets", "i32"),
    "smartHeal": ("smart_heal", "bool"), "smartDowntime": ("smart_downtime", "f32"),
    "sides": ("sides", "i32"), "rotation": ("rotation", "f32"), "regen": ("regen", "f32"),
    "cooldown": ("cooldown", "f32"), "breakSound": ("break_sound", "sound"),
    "effect": ("effect", "status_opt"), "status": ("status", "status_opt"),
    "duration": ("duration", "f32"), "statusDuration": ("duration", "f32"),
    "orbRadius": ("orb_radius", "f32"), "particleSize": ("particle_size", "f32"),
    "particles": ("particles", "i32"), "color": ("color", "rgba_opt"),
    "effectColor": ("effect_color", "rgba_opt"), "particleColor": ("particle_color", "rgba_opt"),
    "active": ("active", "bool"), "x": ("x", "f32"), "y": ("y", "f32"),
    "angle": ("angle", "f32"), "width": ("width", "f32"),
    "radius": ("range", "f32"),
    "chanceDeflect": ("chance_deflect", "f32"), "whenShooting": ("when_shooting", "bool"),
    "minVelocity": ("min_velocity", "f32"), "interval": ("interval", "f32"),
    "teamColor": ("team_color", "bool"), "randAmount": ("rand_amount", "i32"),
    "spread": ("spread", "f32"), "percentAmount": ("percent_amount", "f32"),
    "region": ("region", "string_plain"),
}


def emit_ability(node, indent):
    ctor_name, arg_fields = ABILITY_CTOR[node.cls]
    args = list(node.args)
    if node.cls == "ForceFieldAbility":
        # (radius, regen, max, cooldown[, sides, rotation])
        radius, regen, maxv, cooldown = (num(args[0]), num(args[1]), num(args[2]), num(args[3]))
        sides = f"Some({rust_i32(args[4])})" if len(args) > 4 else "None"
        rotation = f"Some({rust_f32(args[5])})" if len(args) > 5 else "None"
        base = (f"AbilitySpec::force_field({rust_f32(radius)}, {rust_f32(regen)}, {rust_f32(maxv)}, "
                f"{rust_f32(cooldown)}, {sides}, {rotation})")
    elif node.cls == "ShieldRegenFieldAbility":
        if args:
            base = f"AbilitySpec::shield_regen_field({rust_f32(args[0])}, {rust_f32(args[1])}, {rust_f32(args[2])}, {rust_f32(args[3])})"
        else:
            base = "AbilitySpec::for_kind(AbilityKind::ShieldRegenField)"
    elif node.cls == "RepairFieldAbility":
        if args:
            base = f"AbilitySpec::repair_field({rust_f32(args[0])}, {rust_f32(args[1])}, {rust_f32(args[2])})"
        else:
            base = "AbilitySpec::for_kind(AbilityKind::RepairField)"
    elif node.cls == "StatusFieldAbility":
        if args:
            base = f"AbilitySpec::status_field(\"{args[0][1]}\", {rust_f32(args[1])}, {rust_f32(args[2])}, {rust_f32(args[3])})"
        else:
            base = "AbilitySpec::for_kind(AbilityKind::StatusField)"
    elif node.cls == "EnergyFieldAbility":
        if args:
            base = f"AbilitySpec::energy_field({rust_f32(args[0])}, {rust_f32(args[1])}, {rust_f32(args[2])})"
        else:
            base = "AbilitySpec::for_kind(AbilityKind::EnergyField)"
    elif node.cls == "SuppressionFieldAbility":
        base = "AbilitySpec::suppression_field()"
    elif node.cls == "ShieldArcAbility":
        base = "AbilitySpec::shield_arc()"
    elif node.cls == "MoveEffectAbility":
        color = rust_rgba(args[2][1]) if isinstance(args[2], tuple) and args[2][0] == "rgba" else "Rgba::WHITE"
        base = f"AbilitySpec::move_effect({rust_f32(args[0])}, {rust_f32(args[1])}, {color}, {rust_effect_const(args[3][1])}, {rust_f32(args[4])})"
    elif node.cls == "SpawnDeathAbility":
        base = f"AbilitySpec::spawn_death(\"{args[0][1]}\", {rust_i32(args[1])}, {rust_f32(args[2])})"
    elif node.cls == "RegenAbility":
        base = "AbilitySpec::regen(0.0)"
    elif node.cls == "LiquidExplodeAbility":
        base = "AbilitySpec::liquid_explode(\"water\")"
    elif node.cls == "LiquidRegenAbility":
        base = "AbilitySpec::liquid_regen(\"water\", EffectId::HEAL)"
    else:
        raise EvalError(f"unhandled ability {node.cls}")
    if not node.fields:
        return base
    lines = [f"{{ let mut ability = {base};"]
    for java, value in node.fields.items():
        if java not in ABILITY_FIELD_MAP:
            raise EvalError(f"unmapped ability field {java} on {node.cls}")
        rust_name, kind = ABILITY_FIELD_MAP[java]
        if kind == "f32":
            lines.append(f"ability.{rust_name} = {rust_f32(value)};")
        elif kind == "i32":
            lines.append(f"ability.{rust_name} = {rust_i32(value)};")
        elif kind == "bool":
            lines.append(f"ability.{rust_name} = {rust_bool(value)};")
        elif kind == "sound":
            lines.append(f"ability.{rust_name} = {rust_sound_const(value[1])};")
        elif kind == "status_opt":
            lines.append(f"ability.{rust_name} = Some(\"{value[1]}\");")
        elif kind == "rgba_opt":
            lines.append(f"ability.{rust_name} = Some({rust_rgba(value[1])});")
        elif kind == "string_plain":
            lines.append(f"ability.{rust_name} = String::from(\"{value[1]}\");")
    lines.append("ability }")
    return " ".join(lines)


PART_FIELD_MAP = {
    "x": ("x", "f32"), "y": ("y", "f32"), "rotation": ("rotation", "f32"),
    "moveX": ("move_x", "f32"), "moveY": ("move_y", "f32"), "moveRot": ("move_rot", "f32"),
    "growX": ("grow_x", "f32"), "growY": ("grow_y", "f32"),
    "progress": ("progress", "progress"), "heatProgress": ("heat_progress", "progress"),
    "growProgress": ("grow_progress", "progress"),
    "mirror": ("mirror", "bool"), "under": ("under", "bool"), "outline": ("outline", "bool"),
    "color": ("color", "rgba_opt"), "colorTo": ("color_to", "rgba_opt"),
    "heatColor": ("heat_color", "rgba_opt"), "blending": ("blending", "blending"),
    "layer": ("layer", "f32"), "layerOffset": ("layer_offset", "f32"),
    "heatLayerOffset": ("heat_layer_offset", "f32"),
    "radius": ("radius", "f32"), "radiusTo": ("radius_to", "f32"),
    "stroke": ("stroke", "f32"), "sides": ("sides", "i32"),
    "circle": ("circle", "bool"), "hollow": ("hollow", "bool"),
    "phase": ("phase", "f32"), "circles": ("circles", "i32"),
    "minStroke": ("min_stroke", "f32"), "followRotation": ("follow_rotation", "bool"),
    "children": ("children", "children"),
}

PART_CTOR = {
    "RegionPart": "DrawPartSpec::region",
    "ShapePart": "DrawPartSpec::shape",
    "HoverPart": "DrawPartSpec::hover",
    "FlarePart": "DrawPartSpec::flare",
}


def emit_part(node, indent):
    if node.cls == "RegionPart":
        suffix = node.args[0][1] if node.args and isinstance(node.args[0], tuple) else ""
        base = f"DrawPartSpec::region(\"{suffix}\")"
    else:
        base = f"{PART_CTOR[node.cls]}()"
    if not node.fields and not node.moves:
        return base
    lines = [f"{{ let mut part = {base};"]
    for move in node.moves:
        _, progress, x, y, gx, gy, rot = move
        lines.append(
            f"part.moves.push(PartMoveSpec {{ progress: {rust_progress(progress)}, x: {rust_f32(x)}, "
            f"y: {rust_f32(y)}, gx: {rust_f32(gx)}, gy: {rust_f32(gy)}, rot: {rust_f32(rot)} }});")
    for java, value in node.fields.items():
        if java not in PART_FIELD_MAP:
            raise EvalError(f"unmapped part field {java} on {node.cls}")
        rust_name, kind = PART_FIELD_MAP[java]
        if kind == "f32":
            lines.append(f"part.{rust_name} = {rust_f32(value)};")
        elif kind == "i32":
            lines.append(f"part.{rust_name} = {rust_i32(value)};")
        elif kind == "bool":
            lines.append(f"part.{rust_name} = {rust_bool(value)};")
        elif kind == "progress":
            lines.append(f"part.{rust_name} = {rust_progress(value)};")
        elif kind == "rgba_opt":
            lines.append(f"part.{rust_name} = Some({rust_rgba(value[1])});")
        elif kind == "blending":
            lines.append(f"part.blending = BlendingKind::{pascal(value[1])};")
        elif kind == "children":
            lines.append(f"/* children unhandled */")
    lines.append("part }")
    return " ".join(lines)


UNIT_KIND_MAP = {
    "UnitType": "UnitKind::UnitType",
    "ErekirUnitType": "UnitKind::ErekirUnitType",
    "TankUnitType": "UnitKind::TankUnitType",
    "MissileUnitType": "UnitKind::MissileUnitType",
    "NeoplasmUnitType": "UnitKind::NeoplasmUnitType",
}


def emit_target_flags(value, indent):
    if not (isinstance(value, tuple) and value[0] == "array"):
        raise EvalError(f"bad targetFlags {value!r}")
    items = []
    for entry in value[2]:
        if isinstance(entry, tuple) and entry[0] == "blockflag":
            items.append(f"Some(BlockFlag::{pascal(entry[1])})")
        elif isinstance(entry, tuple) and entry[0] == "null":
            items.append("None")
        else:
            raise EvalError(f"bad targetFlags entry {entry!r}")
    return f"vec![{', '.join(items)}]"


def emit_tread_rects(value, indent):
    if not (isinstance(value, tuple) and value[0] == "array"):
        raise EvalError(f"bad treadRects {value!r}")
    rects = []
    for entry in value[2]:
        if isinstance(entry, tuple) and entry[0] == "rect":
            x, y, w, h = entry[1]
            rects.append(
                f"TreadRect {{ x: {rust_f32(x)}, y: {rust_f32(y)}, width: {rust_f32(w)}, height: {rust_f32(h)} }}")
    return f"vec![{', '.join(rects)}]"


def emit_immunity(value):
    if isinstance(value, tuple) and value[0] == "status":
        return f"\"{value[1]}\""
    raise EvalError(f"bad immunity {value!r}")


def emit_engine(value):
    if isinstance(value, tuple) and value[0] == "engine":
        x, y, r, rot = value[1]
        return f"EngineSpec::new({rust_f32(x)}, {rust_f32(y)}, {rust_f32(r)}, {rust_f32(rot)})"
    raise EvalError(f"bad engine {value!r}")


def emit_unit_inline(unit, indent):
    """Emits a UnitSpec expression (used for inline missile spawn units)."""
    lines = ["{", f"{indent}    let mut unit = spec(\"{unit.name}\", {UNIT_KIND_MAP[unit.cls]}, {entity_const(unit.entity_group)});"]
    inner = indent + "    "
    lines.extend(emit_spec_fields(unit.fields, UNIT_FIELDS, inner, skip=("__entity_group__",)))
    for name, bullet in unit.pre_bullets:
        lines.append(f"{inner}unit.pre_bullets.push({emit_bullet(bullet, inner)});")
    for weapon in unit.weapons:
        lines.append(f"{inner}unit.weapons.push({emit_weapon(weapon, inner)});")
    for ability in unit.abilities:
        lines.append(f"{inner}unit.abilities.push({emit_ability(ability, inner)});")
    for part in unit.parts:
        lines.append(f"{inner}unit.parts.push({emit_part(part, inner)});")
    for engine in unit.engines_mirror:
        lines.append(f"{inner}unit.engines_mirror.push({emit_engine(engine)});")
    for immunity in unit.immunities:
        lines.append(f"{inner}unit.immunities.push({emit_immunity(immunity)});")
    if unit.target_flags is not None:
        lines.append(f"{inner}unit.target_flags = {emit_target_flags(unit.target_flags, inner)};")
    if unit.tread_rects:
        lines.append(f"{inner}unit.tread_rects = {emit_tread_rects(unit.tread_rects, inner)};")
    lines.append(f"{inner}unit")
    lines.append(f"{indent}}}")
    return "\n".join(lines)


ENTITY_GROUP_CONSTS = {
    ("Mechc", False): "entity::MECH",
    ("Mechc", True): "entity::MECH_LEGACY",
    ("Legsc", False): "entity::LEGS",
    ("Legsc", True): "entity::LEGS_LEGACY",
    ("ElevationMovec", False): "entity::HOVER",
    ("Unitc", False): "entity::AIR",
    ("Payloadc", False): "entity::AIR_PAYLOAD",
    ("WaterMovec", False): "entity::NAVAL",
    ("BlockUnitc", False): "entity::BLOCK",
    ("BuildingTetherc", False): "entity::TETHER",
    ("TargetDummyc", False): "entity::DUMMY",
    ("Tankc", False): "entity::TANK",
    ("TimedKillc", False): "entity::MISSILE",
    ("Crawlc", False): "entity::CRAWL",
}

ENTITY_GROUP_LEGACY_CONSTS = {}


def entity_const(index):
    if index is None:
        # Inline missiles resolve their constructor through the
        # `MissileUnitType` preset (`constructor = TimedKillUnit::create`).
        return "entity::MISSILE"
    components, legacy = ENTITY_GROUPS[index]
    key = tuple(sorted(c for c in components if c != "Unitc"))[0] if len(components) > 1 else "Unitc"
    if legacy:
        field = LEGACY_GROUP_FIELDS[index]
        return {
            "MechUnitLegacyNova": "entity::MECH_LEGACY",
            "LegsUnitLegacySpiroct": "entity::LEGS_LEGACY",
            "UnitEntityLegacyMono": "entity::AIR_LEGACY_MONO",
            "UnitEntityLegacyPoly": "entity::AIR_LEGACY_POLY",
            "PayloadUnitLegacyQuad": "entity::AIR_PAYLOAD_LEGACY_QUAD",
            "PayloadUnitLegacyOct": "entity::AIR_PAYLOAD_LEGACY_OCT",
            "UnitEntityLegacyAlpha": "entity::AIR_LEGACY_ALPHA",
        }[field]
    return ENTITY_GROUP_CONSTS[(key, False)]


def emit_unit(unit):
    lines = []
    fn_name = snake(unit.name)
    lines.append(f"fn {fn_name}(sink: &mut dyn UnitSink) -> Result<(), ContentError> {{")
    lines.append(f"    sink.push_unit({{")
    lines.append(f"        let mut unit = spec(\"{unit.name}\", {UNIT_KIND_MAP[unit.cls]}, {entity_const(unit.entity_group)});")
    inner = "        "
    lines.extend(emit_spec_fields(unit.fields, UNIT_FIELDS, inner, skip=("__entity_group__",)))
    for name, bullet in unit.pre_bullets:
        lines.append(f"{inner}unit.pre_bullets.push({emit_bullet(bullet, inner)});")
    for weapon in unit.weapons:
        lines.append(f"{inner}unit.weapons.push({emit_weapon(weapon, inner)});")
    for ability in unit.abilities:
        lines.append(f"{inner}unit.abilities.push({emit_ability(ability, inner)});")
    for part in unit.parts:
        lines.append(f"{inner}unit.parts.push({emit_part(part, inner)});")
    for engine in unit.engines_mirror:
        lines.append(f"{inner}unit.engines_mirror.push({emit_engine(engine)});")
    for immunity in unit.immunities:
        lines.append(f"{inner}unit.immunities.push({emit_immunity(immunity)});")
    if unit.target_flags is not None:
        lines.append(f"{inner}unit.target_flags = {emit_target_flags(unit.target_flags, inner)};")
    if unit.tread_rects:
        lines.append(f"{inner}unit.tread_rects = {emit_tread_rects(unit.tread_rects, inner)};")
    lines.append(f"{inner}unit")
    lines.append("    })")
    lines.append("}")
    return "\n".join(lines)


WAVE_HEADER = """// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/UnitTypes.java (wave `{wave}`).
//
//! Generated unit metadata (`UnitTypes.java`, wave `{wave}`).
//! Regenerate with `parity/tools/gen_units.py` after upstream content changes.

#![allow(unused_imports)]

use super::ability::{AbilityKind, AbilitySpec};
use super::parts::{BlendingKind, DrawPartSpec, InterpKind, PartMoveSpec, PartProgressSpec};
use super::weapon::{BulletRef, ShootPatternSpec, WeaponKind, WeaponSpec};
use super::{
    entity, spec, AiControllerKind, ControllerKind, EngineSpec, TreadRect, UnitKind, UnitSink,
};
use crate::content::color::Rgba;
use crate::content::registries::blocks::{BlockFlag, EnvMask};
use crate::content::registries::bullets::{BulletKind, BulletSpec};
use crate::content::registries::fx_meta::{EffectId, EffectRef, EffectSpec};
use crate::content::registries::planets::EnvFlag;
use crate::content::registries::sound_meta::SoundId;
use crate::content::ContentError;
"""

WAVE_REGIONS = {
    "standard": ("ground attack", "ground support", "ground legs", "air attack",
                 "air support", "naval attack", "naval support", "core"),
    "erekir": ("erekir - tank", "erekir - mech", "erekir - flying",
               "erekir - neoplasm", "erekir - core"),
    "special": ("internal + special",),
}


def emit_wave(name, units):
    lines = [WAVE_HEADER.replace("{wave}", name)]
    lines.append("")
    lines.append("/// Loads the `{wave}` wave in upstream order.".replace("{wave}", name))
    lines.append("pub fn load(sink: &mut dyn UnitSink) -> Result<(), ContentError> {")
    for unit in units:
        lines.append(f"    {snake(unit.name)}(sink)?;")
    lines.append("    Ok(())")
    lines.append("}")
    lines.append("")
    for unit in units:
        lines.append(emit_unit(unit))
        lines.append("")
    return "\n".join(lines)


def emit_sound_meta():
    lines = []
    lines.append("// SPDX-License-Identifier: GPL-3.0-only")
    lines.append("//")
    lines.append("// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.")
    lines.append("// Source: generated `mindustry.gen.Sounds` field names (one per file in")
    lines.append("//         `core/assets/sounds/`, plus the `none`/`unset` dummies).")
    lines.append("")
    lines.append("//! Seed sound-name table (names only).")
    lines.append("//!")
    lines.append("//! Plan 18 owns `Sounds` registration and playback; this table is a")
    lines.append("//! pre-registration seed with identical names so content metadata can reference")
    lines.append("//! sounds by a stable id (same reconciliation posture as `fx_meta`, plan 02 R6).")
    lines.append("//! Sounds are not a `ContentType`, so [`SoundId`] indexes this table rather")
    lines.append("//! than a content vector. Regenerate with `parity/tools/gen_units.py`.")
    lines.append("")
    lines.append("/// Number of seed sounds (`none` + `unset` + one per `core/assets/sounds/*.ogg`).")
    lines.append(f"pub const SOUND_COUNT: usize = {len(SOUND_ORDER)};")
    lines.append("")
    lines.append("/// Minimal sound metadata: the generated `Sounds` field name.")
    lines.append("#[derive(Debug, Clone, Copy, PartialEq)]")
    lines.append("pub struct SoundMeta {")
    lines.append("    /// Upstream `Sounds` field name (camelCase file name).")
    lines.append("    pub name: &'static str,")
    lines.append("    /// Asset path relative to `core/assets/sounds/` (without extension).")
    lines.append("    pub asset: &'static str,")
    lines.append("}")
    lines.append("")
    lines.append("/// Stable index into [`SOUNDS`].")
    lines.append("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]")
    lines.append("pub struct SoundId(pub u16);")
    lines.append("")
    lines.append("impl SoundId {")
    lines.append("    /// Raw index.")
    lines.append("    pub const fn raw(self) -> u16 {")
    lines.append("        self.0")
    lines.append("    }")
    lines.append("")
    lines.append("    /// Metadata for this sound.")
    lines.append("    pub fn meta(self) -> &'static SoundMeta {")
    lines.append("        &SOUNDS[(self.0 as usize).min(SOUNDS.len() - 1)]")
    lines.append("    }")
    lines.append("")
    lines.append("    /// `Sounds.none`.")
    lines.append("    pub const NONE: SoundId = SoundId(0);")
    lines.append("")
    lines.append("    /// `Sounds.unset`.")
    lines.append("    pub const UNSET: SoundId = SoundId(1);")
    for name in sorted(sound_names):
        lines.append("")
        lines.append(f"    /// `Sounds.{name}`.")
        lines.append(f"    pub const {screaming(name)}: SoundId = SoundId({sound_ids[name]});")
    lines.append("")
    lines.append("    /// Looks up a sound by its upstream field name.")
    lines.append("    pub fn by_name(name: &str) -> Option<SoundId> {")
    lines.append("        SOUNDS")
    lines.append("            .iter()")
    lines.append("            .position(|meta| meta.name == name)")
    lines.append("            .map(|index| SoundId(index as u16))")
    lines.append("    }")
    lines.append("}")
    lines.append("")
    lines.append("/// Seed sound table (`none`, `unset`, then `core/assets/sounds/**/*.ogg` in")
    lines.append("/// sorted name order). Regenerate with `parity/tools/gen_units.py`.")
    lines.append("pub static SOUNDS: &[SoundMeta] = &[")
    for name in SOUND_ORDER:
        asset = sound_assets.get(name, "")
        lines.append("    SoundMeta {")
        lines.append(f"        name: \"{name}\",")
        lines.append(f"        asset: \"{asset}\",")
        lines.append("    },")
    lines.append("];")
    lines.append("")
    lines.append("#[cfg(test)]")
    lines.append("mod tests {")
    lines.append("    use super::*;")
    lines.append("")
    lines.append("    #[test]")
    lines.append("    fn table_consistent() {")
    lines.append("        assert_eq!(SOUNDS.len(), SOUND_COUNT);")
    lines.append("        assert_eq!(SOUNDS[SoundId::NONE.raw() as usize].name, \"none\");")
    lines.append("        assert_eq!(SOUNDS[SoundId::UNSET.raw() as usize].name, \"unset\");")
    lines.append("    }")
    lines.append("}")
    lines.append("")
    return "\n".join(lines)


def unit_fingerprint(unit):
    payload = repr((
        unit.name, unit.cls, unit.entity_group, sorted(unit.fields.items()),
        len(unit.weapons), len(unit.abilities), len(unit.parts),
        [w.name for w in unit.weapons],
    ))
    return hashlib.sha256(payload.encode()).hexdigest()[:12]


def main():
    global ENTITY_GROUPS, LEGACY_GROUP_FIELDS
    units, groups = parse_unit_types()
    ENTITY_GROUPS = groups
    # Legacy group first-field names (for class names like `MechUnitLegacyNova`).
    LEGACY_GROUP_FIELDS = {}
    raw = (ROOT / "content/UnitTypes.java").read_text(encoding="utf-8")
    text = re.sub(r"//[^\n]*", "", raw)
    decl_re = re.compile(
        r"@EntityDef\((?:value\s*=\s*)?\{([^}]*)\}(?:\s*,\s*legacy\s*=\s*(true))?\)\s*UnitType\s+([^;]+);")
    index = 0
    for m in decl_re.finditer(text):
        components = [c.strip().split(".")[0] for c in m.group(1).split(",")]
        legacy = bool(m.group(2))
        fields = [f.strip() for f in m.group(3).split(",")]
        if legacy:
            base = {
                frozenset(["Unitc", "Mechc"]): "MechUnit",
                frozenset(["Unitc", "Legsc"]): "LegsUnit",
                frozenset(["Unitc"]): "UnitEntity",
                frozenset(["Unitc", "Payloadc"]): "PayloadUnit",
            }[frozenset(components)]
            LEGACY_GROUP_FIELDS[index] = f"{base}Legacy{fields[0][0].upper() + fields[0][1:]}"
        index += 1

    counts = {}
    for wave, regions in WAVE_REGIONS.items():
        wave_units = [u for u in units if u.region in regions]
        counts[wave] = len(wave_units)
        OUT.mkdir(parents=True, exist_ok=True)
        (OUT / f"{wave}.rs").write_text(emit_wave(wave, wave_units) + "\n", encoding="utf-8", newline="\n")
    total_units = len(units) + sum(
        1 for u in units for w in u.weapons
        if isinstance(w.bullet, BulletNode) and w.bullet.spawn_unit is not None
    ) + sum(
        1 for u in units for w in u.weapons
        if isinstance(w.bullet, BulletNode)
        for n in w.bullet.nested.values() if n.spawn_unit is not None
    )
    SOUND_OUT.write_text(emit_sound_meta(), encoding="utf-8", newline="\n")

    # Ledger
    ledger = [f"# Ledger — units (upstream `content/UnitTypes.java`, {total_units} entries)",
              "",
              "> Source-derived fingerprint (`golden_sha` = sha256/12 of the parsed metadata tree).",
              "> The JVM golden (`parity/golden_content.json`) is pending (NUD-10); this ledger is the",
              "> M5 mechanical audit per plan 02 §6.2/§9.",
              "",
              "| pos | name | kind | ported | golden_sha | wave | notes |",
              "|-----|------|------|--------|-----------|------|-------|"]
    pos = 0
    for unit in units:
        wave = next(w for w, regions in WAVE_REGIONS.items() if unit.region in regions)
        ledger.append(f"| {pos} | {unit.name} | {unit.cls} | [x] | {unit_fingerprint(unit)} | {wave} |  |")
        pos += 1
        for w in unit.weapons:
            if isinstance(w.bullet, BulletNode):
                if w.bullet.spawn_unit is not None:
                    ledger.append(
                        f"| {pos} | {w.bullet.spawn_unit.name} | {w.bullet.spawn_unit.cls} | [x] | {unit_fingerprint(w.bullet.spawn_unit)} | {wave} | inline `spawnUnit` |")
                    pos += 1
                for nested in w.bullet.nested.values():
                    if nested.spawn_unit is not None:
                        ledger.append(
                            f"| {pos} | {nested.spawn_unit.name} | {nested.spawn_unit.cls} | [x] | {unit_fingerprint(nested.spawn_unit)} | {wave} | inline `spawnUnit` (nested) |")
                        pos += 1
    ledger += ["", f"- Unported: 0",
               f"- Waves: standard {counts['standard']}, erekir {counts['erekir']}, special {counts['special']}",
               "- The `missile` static field is codegen-only (never constructed) and is not a content record.",
               ""]
    LEDGER.parent.mkdir(parents=True, exist_ok=True)
    LEDGER.write_text("\n".join(ledger), encoding="utf-8", newline="\n")
    print(f"units: {total_units} (standard {counts['standard']}, erekir {counts['erekir']}, special {counts['special']})")
    print(f"ledger: {LEDGER}")
    print(f"sounds: {len(SOUND_ORDER)} -> {SOUND_OUT}")


if __name__ == "__main__":
    main()
