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
import sys
from pathlib import Path

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
    return s.lower()


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
            return -self.unary()
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
                return float(raw)
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
    if op == "+":
        return a + b
    if op == "-":
        return a - b
    if op == "*":
        return a * b
    if op == "/":
        if b == 0:
            raise EvalError("division by zero")
        return a / b
    if op == "%":
        return a % b
    if op == "|":
        return int(a) | int(b)
    if op == "&":
        return int(a) & int(b)
    if op == "^":
        return int(a) ^ int(b)
    raise EvalError(f"unhandled operator {op}")


def resolve_ident(text, env):
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
            return num(args[0]) + (num(args[1]) - num(args[0])) * num(args[2])
        if base == "mod":
            return num(args[0]) % num(args[1])
        if base == "zero":
            return abs(num(args[0])) <= 0.001
        if base == "sign":
            return 1 if num(args[0]) >= 0 else -1
        if base == "sqr":
            return num(args[0]) ** 2
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


def eval_expr(text, env):
    text = text.strip()
    if "->" in text:
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
    if decl_type == "BulletType" and rhs.strip().startswith("new") and unit is not None:
        cls, raws, init_text, _ = parse_construction(rhs.strip(), env)
        node = parse_bullet(cls, raws, init_text, dict(env), unit)
        unit.pre_bullets.append((name, node))
        env[name] = ("pre", len(unit.pre_bullets) - 1)
    else:
        env[name] = eval_expr(rhs, env)
    return True


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


def parse_bullet(cls, raws, init_text, env, unit):
    args = eval_args(raws, env)
    node = BulletNode(cls, args)
    child_env = env
    seed_bullet_ctor_fields(node, child_env)
    if init_text:
        for statement in split_top_level(init_text):
            statement = statement.strip()
            if not statement:
                continue
            if re.match(r"for\s*\(", statement):
                expand_for(statement, child_env, lambda body, e: parse_bullet_statement(node, body, e, unit))
                continue
            parse_bullet_statement(node, statement, child_env, unit)
    return node


def parse_bullet_statement(node, statement, env, unit):
    statement = statement.strip()
    if not statement:
        return
    if declare_locals(statement, env, unit):
        return
    if re.match(r"for\s*\(", statement):
        expand_for(statement, env, lambda body, e: parse_bullet_statement(node, body, e, unit))
        return
    m = re.match(r"spawnBullets\.add\((.*)\)\s*$", statement, re.S)
    if m:
        cls, raws, init_text, _ = parse_construction(m.group(1), env)
        node.spawn_bullets.append(parse_bullet(cls, raws, init_text, dict(env), unit))
        return
    assignment = split_assignment(statement)
    if assignment is None:
        return
    targets, rhs = assignment
    first = targets[0]
    if first in NESTED_BULLET_FIELDS and rhs.startswith("new"):
        cls, raws, init_text, _ = parse_construction(rhs, env)
        node.nested[first] = parse_bullet(cls, raws, init_text, dict(env), unit)
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
        return
    value = eval_expr(rhs, env)
    for target in targets:
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
    if re.match(r"for\s*\(", statement):
        expand_for(statement, env, lambda body, e: parse_weapon_statement(node, body, e, unit))
        return
    m = re.match(r"parts\.add(?:All)?\((.*)\)\s*$", statement, re.S)
    if m:
        for piece in split_top_level(m.group(1), ","):
            piece = piece.strip()
            if piece.startswith("new"):
                cls, raws, init_text, _ = parse_construction(piece, env)
                node.parts.append(parse_part(cls, raws, init_text, dict(env)))
        return
    assignment = split_assignment(statement)
    if assignment is None:
        return
    targets, rhs = assignment
    first = targets[0]
    if first == "bullet":
        if rhs.startswith("new"):
            cls, raws, init_text, _ = parse_construction(rhs, env)
            node.bullet = parse_bullet(cls, raws, init_text, dict(env), unit)
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
        node.fields[target] = value
        env[target] = value


def parse_part(cls, raws, init_text, env):
    args = eval_args(raws, env)
    node = PartNode(cls, args)
    if init_text:
        for statement in split_top_level(init_text):
            statement = statement.strip()
            if not statement:
                continue
            if re.match(r"for\s*\(", statement):
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
        if re.match(r"for\s*\(", statement):
            expand_for(statement, env, lambda body2, e: parse_unit_body(unit, body2, e))
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
                    unit.parts.append(parse_part(cls, raws, init_text, dict(env)))
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
    text = re.sub(r"//[^\n]*", "", raw)

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

    load_start = text.index("public static void load(){")
    body = text[load_start + len("public static void load(){"):]
    region_marks = [(m.start(), m.group(1)) for m in re.finditer(r"//region\s+([\w -]+)", raw)]

    units = []
    env = {"tilesize": 8.0}
    statements = split_top_level(body)
    char_pos = 0
    region = "standard"
    for statement in statements:
        stmt_pos = text.find(statement, char_pos)
        char_pos = stmt_pos + max(len(statement), 1)
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
