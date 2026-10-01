#!/usr/bin/env python3
"""Generate flat Rust block metadata from Mindustry's Blocks.java.

One-off conversion used for plan 02 M3/M4; the data is committed and this
script is re-run only when upstream content changes.

Usage: gen_blocks.py [upstream_mindustry_src] [--out-dir DIR] [--ledger PATH]
  upstream_mindustry_src defaults to /mnt/c/Users/Clinton/g/code_examples/Mindustry/core/src/mindustry
  --out-dir defaults to client/rust/mind-core/src/content/registries/blocks
  --ledger defaults to parity/ledgers/blocks.md
"""
import hashlib
import json
import re
import struct
import sys
from pathlib import Path

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith("--")
            else "/mnt/c/Users/Clinton/g/code_examples/Mindustry/core/src/mindustry")
OUT = Path("client/rust/mind-core/src/content/registries/blocks")
LEDGER = Path("parity/ledgers/blocks.md")
args = sys.argv[2:] if len(sys.argv) > 1 and not sys.argv[1].startswith("--") else sys.argv[1:]
i = 0
while i < len(args):
    if args[i] == "--out-dir":
        OUT = Path(args[i + 1]); i += 2
    elif args[i] == "--ledger":
        LEDGER = Path(args[i + 1]); i += 2
    else:
        i += 1

WAVES = {
    "environment": "B1", "ore": "B1",
    "crafting": "B2", "defense": "B2",
    "distribution": "B3", "liquid": "B3", "power": "B3",
    "production": "B4", "storage": "B4",
    "turrets": "B5", "units": "B5", "units - erekir": "B5", "payloads": "B5",
    "sandbox": "B6", "legacy": "B6", "campaign": "B6", "logic": "B6",
}
PORTED_REGIONS = (
    "environment", "ore", "crafting", "defense",
    "distribution", "liquid", "power",
    "production", "storage",
    "turrets", "units", "units - erekir", "payloads",
    "sandbox", "legacy", "campaign", "logic",
)


def field_map(path):
    text = Path(path).read_text(encoding="utf-8")
    text = re.sub(r"//[^\n]*", "", text)
    pat = re.compile(r"(\w+)\s*=\s*new\s+[\w.]+\(\s*\"([^\"]+)\"")
    return {m.group(1): m.group(2) for m in pat.finditer(text)}


items = field_map(ROOT / "content/Items.java")
liquids = field_map(ROOT / "content/Liquids.java")
statuses = field_map(ROOT / "content/StatusEffects.java")
units = field_map(ROOT / "content/UnitTypes.java")
planets = field_map(ROOT / "content/Planets.java")

# Item colors (for `OreBlock.mapColor` metadata).
item_colors = {}
_color_text = re.sub(r"//[^\n]*", "", (ROOT / "content/Items.java").read_text(encoding="utf-8"))
for m in re.finditer(r"(\w+)\s*=\s*new\s+Item\(\s*\"[^\"]+\"\s*,\s*Color\.valueOf\(\"([0-9a-fA-F]{6,8})\"\)", _color_text):
    item_colors[m.group(1)] = m.group(2)


def kebab(name):
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1-\2", name)
    s = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1-\2", s)
    return s.lower()


def content_name(qual, ident):
    if qual == "Items":
        return items.get(ident, kebab(ident))
    if qual == "Liquids":
        return liquids.get(ident, kebab(ident))
    if qual == "StatusEffects":
        return statuses.get(ident, kebab(ident))
    if qual == "UnitTypes":
        return units.get(ident, kebab(ident))
    if qual == "Planets":
        return planets.get(ident, kebab(ident))
    return None


# ---------------------------------------------------------------- class graph

CLASS_RE = re.compile(r"public (?:abstract\s+)?class (\w+)(?:<[^>]*>)?\s+extends\s+([\w.]+)")
CLASS_DECL = re.compile(r"public (?:abstract\s+)?class (\w+)(?:<[^>]*>)?")

class_files = {}
class_super = {}
class_bodies = {}
for path in (ROOT / "world/blocks").rglob("*.java"):
    text = path.read_text(encoding="utf-8")
    text_nc = re.sub(r"//[^\n]*", "", text)
    for m in CLASS_DECL.finditer(text_nc):
        cname = m.group(1)
        if cname not in class_files:
            class_files[cname] = path
    for m in CLASS_RE.finditer(text_nc):
        class_super.setdefault(m.group(1), m.group(2).split(".")[-1])
    class_bodies[path.name] = text_nc

# Block.java root lives outside world/blocks.
_block_java = (ROOT / "world/Block.java").read_text(encoding="utf-8")
_block_nc = re.sub(r"//[^\n]*", "", _block_java)
class_files["Block"] = ROOT / "world/Block.java"
class_super["Block"] = None
class_bodies["Block.java"] = _block_nc

# Curated field names extracted from class constructors/initializers.
CURATED_ENV = ("envRequired", "envEnabled", "envDisabled")
CURATED_FLAGS = ("flags",)

ENV_FLAGS = ["terrestrial", "spores", "groundOil", "groundWater", "oxygen", "scorching", "underwater", "space"]


def class_body_text(cname):
    path = class_files.get(cname)
    if path is None:
        return ""
    for body_name, text in class_bodies.items():
        if body_name == path.name:
            return text
    return path.read_text(encoding="utf-8")


def extract_class_defaults(cname, field):
    """Walks the superclass chain collecting assignments to `field` in
    constructors that take at most a single `String name` parameter, plus field
    initializers. Covers the metadata half of `world/blocks/**` constructors."""
    value = None
    chain = []
    cur = cname
    while cur and cur not in chain:
        chain.append(cur)
        cur = class_super.get(cur)
    for cls in reversed(chain):
        text = class_body_text(cls)
        # field initializer: `type field = expr;`
        for m in re.finditer(rf"(?:public|protected|private)\s+(?:static\s+)?(?:final\s+)?([\w<>\[\], .@]+)\s+{field}\s*=\s*([^;]+);", text):
            declared_type = m.group(1)
            if field in INT_FIELDS and "int" not in declared_type:
                continue
            value = m.group(2).strip()
        # constructor assignments (no-arg or single `String name` parameter)
        for m in re.finditer(rf"public\s+{cls}\s*\(\s*(?:String\s+\w+)?\s*\)\s*\{{", text):
            body = balanced(text, m.end() - 1)
            for statement in body.split(";"):
                mm = re.match(rf"\s*(?:this\.)?{field}\s*=\s*([^;]+)$", statement.strip())
                if mm:
                    value = mm.group(1).strip()
    return value


CLASS_DEFAULTS_CACHE = {}
NUMERIC_CONSTANTS_CACHE = {}


def class_numeric_constants(cname):
    """Simple numeric field initializers along the chain (e.g. reactor heating)."""
    cached = NUMERIC_CONSTANTS_CACHE.get(cname)
    if cached is not None:
        return cached
    constants = {}
    chain = []
    cur = cname
    while cur and cur not in chain:
        chain.append(cur)
        cur = class_super.get(cur)
    for cls in reversed(chain):
        text = class_body_text(cls)
        for m in re.finditer(
            r"(?:public|protected|private)\s+(?:static\s+)?(?:final\s+)?(?:float|int|double)\s+(\w+)\s*=\s*([-+]?\d+(?:\.\d+)?[fFdD]?)\s*;",
            text,
        ):
            try:
                constants[m.group(1)] = float(m.group(2).rstrip("fFdD"))
            except ValueError:
                continue
    NUMERIC_CONSTANTS_CACHE[cname] = constants
    return constants


def class_defaults_for(cname):
    """Evaluates the curated class defaults along the superclass chain."""
    cached = CLASS_DEFAULTS_CACHE.get(cname)
    if cached is not None:
        return cached
    defaults = {}
    for camel in CURATED_FIELDS:
        expr = extract_class_defaults(cname, camel)
        if not expr:
            continue
        try:
            value = eval_text(expr, {"__blocks__": {}})
        except EvalError:
            continue
        if isinstance(value, tuple) and value and value[0] == "unknown":
            continue
        if isinstance(value, tuple) and value:
            if value[0] in ("stacklist", "flaglist"):
                value = value[1]
        snake = CAMEL_TO_SNAKE.get(camel, camel)
        defaults[snake] = value
    CLASS_DEFAULTS_CACHE[cname] = defaults
    return defaults


def balanced(text, open_index):
    """Returns the text inside the braces starting at `open_index` (`{`)."""
    depth = 0
    i = open_index
    while i < len(text):
        c = text[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return text[open_index + 1:i]
        i += 1
    return text[open_index + 1:]


# ------------------------------------------------------------- expression eval

class EvalError(Exception):
    pass


TOKEN_RE = re.compile(r"""
    (?P<ws>\s+)
  | (?P<num>\d+\.\d+(?:[eE][+-]?\d+)?[fFdD]?|\d+[fFdD]|\d+)
  | (?P<str>"(?:[^"\\]|\\.)*")
  | (?P<ident>[A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)*)
  | (?P<op>\|\||&&|==|!=|<=|>=|\|=|&=|\||&|\^|\+|-|\*|/|%|<|>|\?|:|\(|\)|,|\.)
""", re.X)


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
        return self.primary()

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
            value = self.ternary()
            self.expect(")")
            return value
        if text == "new":
            tkind, tname = self.next()
            self.expect("(")
            call_args = self.call_args(")")
            return make_new(tname, call_args, self.env)
        if kind == "ident":
            if self.peek()[1] == "(":
                self.next()
                call_args = self.call_args(")")
                return call_function(text, call_args, self.env)
            return resolve_ident(text, self.env)
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
    if op == "|" and isinstance(left, dict) and isinstance(right, dict):
        if left.get("any") or right.get("any"):
            return {"any": True, "flags": []}
        return {"any": False, "flags": sorted(set(left.get("flags", [])) | set(right.get("flags", [])))}
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
        result = a / b
        return result if isinstance(a, float) or isinstance(b, float) else result
    if op == "%":
        return a % b
    if op == "&" and isinstance(left, dict) and isinstance(right, dict):
        return left
    if op == "^":
        return a ^ b
    raise EvalError(f"unhandled operator {op}")


def resolve_ident(text, env):
    if text == "true":
        return True
    if text == "false":
        return False
    if text == "this":
        return ("this",)
    if text in env:
        return env[text]
    if "." in text:
        qual, ident = text.rsplit(".", 1)
        if qual in ("Items", "Liquids", "StatusEffects", "UnitTypes", "Planets"):
            name = content_name(qual, ident)
            if name is not None:
                return ("content", qual, name)
        if qual == "Blocks":
            return ("block", env.get("__blocks__", {}).get(ident, kebab(ident)))
        if qual == "BlockGroup":
            return {"__group__": ident}
        if qual == "Category":
            return {"__category__": ident}
        if qual == "BuildVisibility":
            return {"__visibility__": ident}
        if qual == "BlockFlag":
            return {"__flag__": ident}
        if qual == "TargetPriority":
            return {"__priority__": ident}
        if qual == "Env":
            if ident == "any":
                return {"any": True, "flags": []}
            if ident == "none":
                return {"any": False, "flags": []}
            return {"any": False, "flags": [ident]}
        if qual == "Mathf" and ident == "pi":
            return 3.1415927
        if ident == "requirements":
            return BLOCK_REQS.get(qual, ("stacklist", []))
        if ident == "health":
            return BLOCK_VALUES.get(qual, {}).get("health", 0)
    raise EvalError(f"unknown identifier {text!r}")


def make_new(tname, call_args, env):
    tname = tname.split(".")[-1]
    if tname == "ItemStack":
        item = call_args[0]
        amount = call_args[1] if len(call_args) > 1 else 1
        return ("stacklist", [stack_pair(item, amount)])
    if tname == "LiquidStack":
        return ("liquidstacklist", [(call_args[0], num(call_args[1]))])
    if tname == "Color":
        if len(call_args) >= 3:
            r, g, b = (num(v) for v in call_args[:3])
            return ("rgba", (r, g, b, 1.0))
    raise EvalError(f"unhandled new {tname}")


def stack_pair(item, amount):
    if not (isinstance(item, tuple) and item and item[0] == "content"):
        raise EvalError(f"invalid item ref {item!r}")
    return (item[2], int(num(amount)))


def expand_stack_list(value):
    if isinstance(value, tuple) and value[0] == "stacklist":
        return value[1]
    raise EvalError(f"not a stack list: {value!r}")


def call_function(name, args, env):
    base = name.split(".")[-1]
    if base == "with":
        pairs = []
        index = 0
        while index + 1 < len(args) or index < len(args):
            if index >= len(args):
                break
            pairs.append(stack_pair(args[index], args[index + 1]))
            index += 2
        return ("stacklist", pairs)
    if base == "mult" and name.startswith("ItemStack"):
        stacks = expand_stack_list(args[0])
        factor = num(args[1])
        return ("stacklist", [(item, int(round(amount * factor))) for item, amount in stacks])
    if base == "of" and name.startswith("EnumSet"):
        return ("flaglist", [resolve_flag(a) for a in args])
    if name.startswith("Mathf"):
        if base == "round" and len(args) == 2:
            value, step = num(args[0]), num(args[1])
            return int((value / step) + 0.5) * int(step) if step == int(step) else ((value / step) + 0.5).__floor__() * step
        if base == "pow":
            return float(num(args[0])) ** float(num(args[1]))
        if base == "max":
            return max(num(a) for a in args)
        if base == "min":
            return min(num(a) for a in args)
        if base == "abs":
            return abs(num(args[0]))
        if base == "clamp":
            return min(max(num(args[0]), num(args[1])), num(args[2]))
        if base == "lerp":
            return num(args[0]) + (num(args[1]) - num(args[0])) * num(args[2])
    if base == "valueOf" and name.startswith("Color"):
        hex_text = args[0][1]
        return ("rgbahex", hex_text)
    raise EvalError(f"unhandled call {name}")


def resolve_flag(value):
    if isinstance(value, dict) and "__flag__" in value:
        return value["__flag__"]
    raise EvalError(f"not a flag: {value!r}")


def eval_text(expr, env):
    return Parser(tokenize(expr), env).parse()


# ------------------------------------------------------------- Blocks.java parse

blocks_text = (ROOT / "content/Blocks.java").read_text(encoding="utf-8")
load_start = blocks_text.index("void load(){")
load_body = balanced(blocks_text, load_start + len("void load()") )
# strip line comments but keep region markers
clean_lines = []
for line in load_body.split("\n"):
    stripped = line.strip()
    if stripped.startswith("//region"):
        clean_lines.append(line)
        continue
    clean_lines.append(re.sub(r"//[^\n]*", "", line))
load_body = "\n".join(clean_lines)

# Split into regions.
region_text = {}
current = None
buffer = []
for line in load_body.split("\n"):
    m = re.match(r"\s*//region\s+(.+?)\s*$", line)
    if m:
        if current is not None:
            region_text[current] = "\n".join(buffer)
        current = m.group(1)
        buffer = []
        continue
    if current is not None:
        buffer.append(line)
if current is not None:
    region_text[current] = "\n".join(buffer)

REGION_ORDER = ["environment", "ore", "crafting", "defense", "distribution", "liquid", "power",
                "production", "storage", "turrets", "units", "units - erekir", "payloads",
                "sandbox", "legacy", "campaign", "logic"]


def split_statements(text):
    """Splits on top-level semicolons, keeping braces/parens/strings intact."""
    statements = []
    depth = 0
    current = []
    i = 0
    in_str = False
    while i < len(text):
        c = text[i]
        if in_str:
            current.append(c)
            if c == "\\":
                if i + 1 < len(text):
                    current.append(text[i + 1])
                    i += 2
                    continue
            elif c == '"':
                in_str = False
            i += 1
            continue
        if c == '"':
            in_str = True
            current.append(c)
        elif c == "(" or c == "{":
            depth += 1
            current.append(c)
        elif c == ")" or c == "}":
            depth -= 1
            current.append(c)
        elif c == ";" and depth == 0:
            statements.append("".join(current))
            current = []
        else:
            current.append(c)
        i += 1
    tail = "".join(current).strip()
    if tail:
        statements.append(tail)
    return [s.strip() for s in statements if s.strip()]


ASSIGN_RE = re.compile(r"^\s*(?:this\.)?([A-Za-z_$][\w$]*)\s*=(?!=)\s*(.*)$", re.S)


CURATED_FIELDS = {
    "size", "health", "scaledHealth", "armor", "requirements", "researchCost",
    "researchCostMultiplier", "researchCostMultipliers", "buildCostMultiplier", "buildTime",
    "group", "priority", "unitCapModifier", "flags", "itemCapacity", "liquidCapacity",
    "hasItems", "hasLiquids", "hasPower", "outputsPower", "consumesPower", "conductivePower",
    "outputsLiquid", "buildVisibility", "envRequired", "envEnabled", "envDisabled",
    "solid", "floating", "update", "destructible", "saveData", "saveConfig", "configurable",
    "inEditor", "placeablePlayer", "placeableLiquid", "placeableOn", "insulated", "absorbLasers",
    "allowCorePlacement", "playerUnmineable", "wallOre", "itemDrop", "oreDefault", "oreThreshold",
    "oreScale", "fogRadius", "region", "generateIcons", "mapColor", "hasColor", "squareSprite",
}

CAMEL_TO_SNAKE = {
    "scaledHealth": "scaled_health", "researchCost": "research_cost",
    "researchCostMultiplier": "research_cost_multiplier",
    "researchCostMultipliers": "research_cost_multipliers",
    "buildCostMultiplier": "build_cost_multiplier", "buildTime": "build_time",
    "unitCapModifier": "unit_cap_modifier", "itemCapacity": "item_capacity",
    "liquidCapacity": "liquid_capacity", "hasItems": "has_items", "hasLiquids": "has_liquids",
    "hasPower": "has_power", "outputsPower": "outputs_power", "consumesPower": "consumes_power",
    "conductivePower": "conductive_power", "outputsLiquid": "outputs_liquid",
    "buildVisibility": "build_visibility", "envRequired": "env_required",
    "envEnabled": "env_enabled", "envDisabled": "env_disabled", "saveData": "save_data",
    "saveConfig": "save_config", "inEditor": "in_editor", "placeablePlayer": "placeable_player",
    "placeableLiquid": "placeable_liquid", "placeableOn": "placeable_on",
    "absorbLasers": "absorb_lasers", "allowCorePlacement": "allow_core_placement",
    "playerUnmineable": "player_unmineable", "wallOre": "wall_ore", "itemDrop": "item_drop",
    "oreDefault": "ore_default", "oreThreshold": "ore_threshold", "oreScale": "ore_scale",
    "fogRadius": "fog_radius", "generateIcons": "generate_icons", "mapColor": "map_color",
    "hasColor": "has_color", "squareSprite": "square_sprite",
}


class RawBlock:
    def __init__(self, ident, cls, name, region, body):
        self.ident = ident
        self.cls = cls
        self.name = name
        self.region = region
        self.body_text = body
        self.overrides = {}
        self.consumes = []
        self.notes = []
        self.extra_env = {}

    def set(self, field, value):
        field = CAMEL_TO_SNAKE.get(field, field)
        if isinstance(value, tuple) and value:
            if value[0] == "stacklist":
                value = value[1]
            elif value[0] == "flaglist":
                value = value[1]
        self.overrides[field] = value


def parse_constructor_args(arg_text):
    """Returns (explicit_name, kwargs) for a `new Class(...)` argument list."""
    text = arg_text.strip()
    if text.startswith('"'):
        end = text.index('"', 1)
        return text[1:end], text[end + 1:].lstrip(", ").strip()
    return None, text


def parse_body(block, body, env):
    global LOCALS
    for statement in split_statements(body):
        try:
            parse_statement(block, statement, env)
        except EvalError as error:
            block.notes.append(f"unparsed `{statement.strip()[:80]}`: {error}")


def split_call_chain(s):
    """Splits `callee(args).mod1(...).mod2(...)`; returns (callee, args, modifiers)."""
    m = re.match(r"^(?:this\.)?([A-Za-z_$][\w$]*)\s*\(", s)
    if not m:
        return None
    callee = m.group(1)
    open_index = s.index("(", m.start())
    close = find_matching(s[open_index:], "(", ")") + open_index
    args = s[open_index + 1:close]
    rest = s[close + 1:].strip()
    modifiers = []
    while rest.startswith("."):
        end = rest.find("(")
        if end < 0:
            break
        mod_close = find_matching(rest[end:], "(", ")") + end
        modifiers.append((rest[1:end], rest[end + 1:mod_close]))
        rest = rest[mod_close + 1:].strip()
    return callee, args, modifiers


def parse_statement(block, statement, env):
    s = statement.strip()
    if not s:
        return
    # Control flow: recurse into braces.
    control = re.match(r"^(for|if|else if|while)\b", s)
    if control:
        brace = s.find("{")
        if brace >= 0:
            inner = balanced(s, brace)
            # `for(int i = 0; i < 5; i++){...}` with no metadata: recurse anyway.
            for sub in split_statements(inner):
                parse_statement(block, sub, env)
        return
    if s.startswith("var ") or re.match(r"^[A-Za-z_][\w<>\[\], .]*\s+[A-Za-z_$][\w$]*\s*=", s):
        m = re.match(r"^(?:var|[A-Za-z_][\w<>\[\], .]*)\s+([A-Za-z_$][\w$]*)\s*=\s*(.*)$", s, re.S)
        if m:
            try:
                env[m.group(1)] = eval_text(m.group(2).strip().rstrip(";"), env)
            except EvalError:
                pass
        return
    # OR-assignment (`envEnabled |= Env.space`).
    or_assign = re.match(r"^\s*(?:this\.)?([A-Za-z_$][\w$]*)\s*\|=\s*(.*)$", s, re.S)
    if or_assign:
        field = CAMEL_TO_SNAKE.get(or_assign.group(1), or_assign.group(1))
        value = try_eval(or_assign.group(2), env, block)
        block.extra_env[field] = merge_env(block.extra_env.get(field), value)
        return
    # Method call with optional `.optional()/.boost()/.update()` chain.
    call_chain = split_call_chain(s)
    if call_chain:
        callee, args_text, modifiers = call_chain
        pending = {}
        for method, mod_args in modifiers:
            if method == "optional":
                pending_args = split_args(mod_args)
                pending["optional"] = truthy(try_eval(pending_args[0], env, block)) if pending_args else True
            elif method == "boost":
                pending["optional"] = True
            elif method == "ignore":
                pending["ignore"] = True
        PENDING_CONSUME[id(block)] = pending
        handle_call(block, callee, args_text, env)
        return
    mutate = re.match(r"^([A-Za-z_$][\w$]*(\.[A-Za-z_$][\w$]*)?)\s*\.\s*([A-Za-z_$][\w$]*)\s*\((.*)\)\s*$", s, re.S)
    if mutate:
        handle_mutation(block, mutate.group(1), mutate.group(3), mutate.group(4), env)
        return
    assign = ASSIGN_RE.match(s)
    if assign:
        if assign.group(1) not in CURATED_FIELDS:
            return
        handle_assignment(block, assign.group(1), assign.group(2), env)
        return


def set_pending_from_chain(block, chain_text, env):
    """Records `.optional(...)`/`.boost()`/`.update(...)` modifiers for the next consumer."""
    if not chain_text:
        return
    pending = {}
    for m in re.finditer(r"\.([A-Za-z_$][\w$]*)\(([^()]*)\)", chain_text):
        method, args_text = m.group(1), m.group(2)
        if method == "optional":
            args = split_args(args_text)
            pending["optional"] = truthy(try_eval(args[0], env, block)) if args else True
        elif method == "boost":
            pending["optional"] = True
        elif method == "ignore":
            pending["ignore"] = True
    PENDING_CONSUME[id(block)] = pending


def chain_assign(block, lvalue_text, rhs_text, env):
    """Handles `a = b = expr` chains."""
    parts = [lvalue_text]
    value_text = rhs_text
    while True:
        m = ASSIGN_RE.match(value_text)
        if m:
            parts.append(m.group(1))
            value_text = m.group(2)
        else:
            break
    value = try_eval(value_text, env, block)
    for part in parts:
        block.set(part, value)


def handle_assignment(block, lvalue, rhs, env):
    chain_assign(block, lvalue, rhs, env)


def merge_env(left, right):
    if left is None:
        return right
    if not (isinstance(left, dict) and isinstance(right, dict)):
        return right
    if left.get("any") or right.get("any"):
        return {"any": True, "flags": []}
    return {"any": False, "flags": sorted(set(left.get("flags", [])) | set(right.get("flags", [])))}


def handle_call(block, name, args_text, env):
    if name == "requirements":
        args = split_args(args_text)
        category = try_eval(args[0], env, block)
        visibility = None
        rest = args[1:]
        if rest and "BuildVisibility" in rest[0]:
            visibility = try_eval(rest[0], env, block)
            rest = rest[1:]
        stacks = []
        if rest:
            value = try_eval(rest[0], env, block)
            if isinstance(value, tuple) and value[0] == "stacklist":
                stacks = value[1]
            elif isinstance(value, tuple) and value and value[0] == "unknown":
                block.notes.append(f"unparsed requirements `{rest[0][:60]}`")
            else:
                stacks = []
        block.set("requirements", stacks)
        block.set("category", category)
        if visibility is not None:
            block.set("build_visibility", visibility)
        return
    if name in ("consumeItem", "consumeItems"):
        args = split_args(args_text)
        if name == "consumeItem" and len(args) == 1:
            value = try_eval(args[0], env, block)
            stacks = [stack_pair(value, 1)]
        elif name == "consumeItem":
            value = try_eval(args[0], env, block)
            stacks = [stack_pair(value, int(num(try_eval(args[1], env, block))))]
        else:
            stacks = expand_stack_list(try_eval(args[0], env, block))
        apply_consume_modifiers(block, {"kind": "items", "stacks": stacks})
        return
    if name == "consumeLiquid":
        args = split_args(args_text)
        liquid = try_eval(args[0], env, block)
        amount = num(try_eval(args[1], env, block))
        apply_consume_modifiers(block, {"kind": "liquid", "liquid": liquid[2], "amount": float(amount)})
        return
    if name == "consumeLiquids":
        args = split_args(args_text)
        stacks = try_eval(args[0], env, block)
        if isinstance(stacks, tuple) and stacks[0] == "liquidstacklist":
            pairs = [(s[0][2], float(s[1])) for s in stacks[1]]
        else:
            pairs = []
        apply_consume_modifiers(block, {"kind": "liquids", "stacks": pairs})
        return
    if name == "consumePower":
        args = split_args(args_text)
        usage = float(num(try_eval(args[0], env, block)))
        apply_consume_modifiers(block, {"kind": "power", "usage": usage, "buffered": 0.0})
        return
    if name == "consumePowerBuffered":
        args = split_args(args_text)
        capacity = float(num(try_eval(args[0], env, block)))
        apply_consume_modifiers(block, {"kind": "power", "usage": 0.0, "buffered": capacity})
        return
    if name == "consumeCoolant":
        args = split_args(args_text)
        amount = 1.0
        allow_liquid, allow_gas = True, False
        if args:
            amount = float(num(try_eval(args[0], env, block)))
        if len(args) >= 3:
            allow_liquid = truthy(try_eval(args[1], env, block))
            allow_gas = truthy(try_eval(args[2], env, block))
        apply_consume_modifiers(block, {"kind": "coolant", "amount": amount,
                                        "allow_liquid": allow_liquid, "allow_gas": allow_gas})
        return


PENDING_CONSUME = {}


def apply_consume_modifiers(block, consume):
    """Applies pending `.optional()`/`.boost()`/`.update()` modifiers."""
    pending = PENDING_CONSUME.pop(id(block), None)
    consume["optional"] = bool(pending and pending.get("optional"))
    consume["ignore"] = bool(pending and pending.get("ignore"))
    block.consumes.append(consume)


def handle_mutation(block, target, method, args_text, env):
    if target == "researchCostMultipliers" and method == "put":
        args = split_args(args_text)
        item = try_eval(args[0], env, block)
        multiplier = float(num(try_eval(args[1], env, block)))
        block.overrides.setdefault("research_cost_multipliers", []).append((item[2], multiplier))
        return
    # `itemConsumer = consumeItem(...).boost()` arrives as an assignment; handled
    # in try_eval via `call_chain`.
    if method in ("optional", "boost", "update", "ignore"):
        PENDING_CONSUME[id(block)] = pending = PENDING_CONSUME.get(id(block), {})
        if method == "optional":
            args = split_args(args_text)
            pending["optional"] = truthy(try_eval(args[0], env, block)) if args else True
            pending["ignore"] = False
        elif method == "boost":
            pending["optional"] = True
        elif method == "ignore":
            pending["ignore"] = True
        elif method == "update":
            pass


def split_args(text):
    args = []
    depth = 0
    current = []
    in_str = False
    i = 0
    while i < len(text):
        c = text[i]
        if in_str:
            current.append(c)
            if c == "\\" and i + 1 < len(text):
                current.append(text[i + 1])
                i += 2
                continue
            if c == '"':
                in_str = False
        elif c == '"':
            in_str = True
            current.append(c)
        elif c in "({[":
            depth += 1
            current.append(c)
        elif c in ")}]":
            depth -= 1
            current.append(c)
        elif c == "," and depth == 0:
            args.append("".join(current).strip())
            current = []
        else:
            current.append(c)
        i += 1
    tail = "".join(current).strip()
    if tail or args:
        args.append(tail)
    return args


def try_eval(text, env, block):
    text = text.strip()
    if text.endswith(";"):
        text = text[:-1]
    # Strip consume-call chains (`consumeItem(Items.x).boost()` used as a value).
    if text.startswith("consume"):
        call_chain = split_call_chain(text)
        if call_chain:
            callee, args_text, modifiers = call_chain
            pending = {}
            for method, mod_args in modifiers:
                if method == "optional":
                    pending_args = split_args(mod_args)
                    pending["optional"] = truthy(try_eval(pending_args[0], env, block)) if pending_args else True
                elif method == "boost":
                    pending["optional"] = True
                elif method == "ignore":
                    pending["ignore"] = True
            PENDING_CONSUME[id(block)] = pending
            handle_call(block, callee, args_text, env)
            return ("consume",)
    if text.startswith("//"):
        return None
    if block is not None:
        for name, value in class_numeric_constants(block.cls).items():
            env.setdefault(name, value)
    try:
        return eval_text(text, env)
    except EvalError as error:
        if block is not None:
            block.notes.append(f"unparsed `{text}`: {error}")
        return ("unknown",)


LOCALS = {}


def block_env():
    env = {"__blocks__": {}}
    return env


def parse_blocks():
    """Parses all regions in order, returning the ordered RawBlock list."""
    parsed = []
    env = block_env()
    for region in REGION_ORDER:
        text = region_text.get(region)
        if text is None:
            continue
        for statement in split_statements(text):
            parse_top_statement(parsed, statement, region, env)
    return parsed


def parse_top_statement(parsed, statement, region, env):
    s = statement.strip()
    if not s:
        return
    construct_loop = re.match(r"^for\s*\(.*\)\s*\{", s, re.S)
    if construct_loop:
        brace = s.find("{")
        inner = balanced(s, brace)
        if "new ConstructBlock" in inner:
            for size in range(1, 17):
                raw = RawBlock(f"build{size}", "ConstructBlock", f"build{size}", region, f"size = {size};")
                raw.set("size", size)
                raw.set("health", 10)
                finalize(raw, env)
                parsed.append(raw)
            return
        for sub in split_statements(inner):
            parse_top_statement(parsed, sub, region, env)
        return
    if match := re.match(r"^new\s+([\w.]+)\s*\((.*)$", s, re.S):
        # Anonymous/free construction (e.g. ConstructBlock loop) — nothing to register.
        return
    m = re.match(r"^(?:this\.)?([A-Za-z_$][\w$]*)\s*=\s*new\s+([\w.]+)\s*\((.*)$", s, re.S)
    if not m:
        assign = ASSIGN_RE.match(s)
        if assign:
            handle_local(assign.group(1), assign.group(2), env)
        return
    ident = m.group(1)
    cls = m.group(2).split(".")[-1]
    rest = m.group(3)
    # Find the argument list terminator, then optional `{{ body }}`.
    close = find_matching(rest, "(", ")")
    arg_text = rest[1:close]
    after = rest[close + 1:]
    body = ""
    if "{{" in after:
        start = after.index("{{") + 2
        body = balanced(after, after.index("{{") + 1)
    name, extra_args = parse_constructor_args(arg_text)
    if cls == "OreBlock":
        if name is None:
            item_arg = extra_args if extra_args else arg_text
            value = try_eval(item_arg, env, None) if item_arg else None
            if isinstance(value, tuple) and value[0] == "content":
                name = "ore-" + value[2]
            else:
                name = "ore-unknown"
        # itemDrop is set from the constructor; metadata extraction below.
        item_value = None
        args_list = split_args(arg_text)
        if len(args_list) >= 2:
            item_value = try_eval(args_list[1], env, None)
        elif args_list:
            item_value = try_eval(args_list[0], env, None)
        body = f"itemDrop = {args_list[-1]}; " + body
    if name is None:
        name = kebab(ident)
    raw = RawBlock(ident, cls, name, region, body)
    if cls == "OreBlock":
        # `wallOre` variant names use the explicit name; itemDrop resolved in body.
        pass
    finalize(raw, env)
    parsed.append(raw)


def find_matching(text, open_char, close_char):
    depth = 0
    in_str = False
    for i, c in enumerate(text):
        if in_str:
            if c == '"' and text[i - 1] != "\\":
                in_str = False
            continue
        if c == '"':
            in_str = True
        elif c == open_char:
            depth += 1
        elif c == close_char:
            depth -= 1
            if depth == 0:
                return i
    return len(text) - 1


def handle_local(name, rhs, env):
    if rhs.strip().startswith("consume"):
        return
    try:
        env[name] = eval_text(rhs, env)
    except EvalError:
        pass


def finalize(raw, env):
    parse_body(raw, raw.body if hasattr(raw, "body") else "", env)


# RawBlock stores the body separately for finalize.
_raw_parse_top = parse_top_statement


def parse_top_statement(parsed, statement, region, env):  # noqa: F811
    s = statement.strip()
    if not s:
        return
    construct_loop = re.match(r"^for\s*\(.*\)\s*\{", s, re.S)
    if construct_loop:
        brace = s.find("{")
        inner = balanced(s, brace)
        if "new ConstructBlock" in inner:
            for size in range(1, 17):
                raw = RawBlock(f"build{size}", "ConstructBlock", f"build{size}", region, f"size = {size};")
                raw.set("health", 10)
                raw.set("update", True)
                raw.set("inEditor", False)
                raw.set("generateIcons", False)
                parse_body(raw, raw.body_text, env)
                parsed.append(raw)
            return
        for sub in split_statements(inner):
            parse_top_statement(parsed, sub, region, env)
        return
    m = re.match(r"^(?:this\.)?([A-Za-z_$][\w$]*)\s*=\s*new\s+([\w.]+)\s*(\(.*)$", s, re.S)
    if not m:
        local = re.match(r"^(?:var|[A-Za-z_][\w<>\[\], .]*)\s+([A-Za-z_$][\w$]*)\s*=\s*(.*)$", s, re.S)
        if local:
            try:
                env[local.group(1)] = eval_text(local.group(2), env)
            except EvalError:
                pass
        return
    ident = m.group(1)
    cls = m.group(2).split(".")[-1]
    rest = m.group(3)
    close = find_matching(rest, "(", ")")
    arg_text = rest[1:close]
    after = rest[close + 1:]
    body = ""
    if "{{" in after:
        body = balanced(after, after.index("{{") + 1)
    args_list = split_args(arg_text)
    name = None
    if arg_text.strip().startswith('"'):
        end = arg_text.index('"', 1)
        name = arg_text[1:end]
    if cls == "OreBlock":
        item_value = None
        try:
            if len(args_list) >= 2:
                item_value = eval_text(args_list[1], env)
            elif args_list:
                item_value = eval_text(args_list[0], env)
        except EvalError:
            item_value = None
        if isinstance(item_value, tuple) and item_value[0] == "content":
            if name is None:
                name = "ore-" + item_value[2]
            body = f"itemDrop = {args_list[-1]};" + body
    if name is None:
        name = kebab(ident)
    raw = RawBlock(ident, cls, name, region, body)
    env.setdefault("__blocks__", {})[ident] = name
    if cls == "OreBlock":
        item_ident = None
        if len(args_list) >= 2:
            item_ident = args_list[1]
        elif args_list:
            item_ident = args_list[0]
        if item_ident and item_ident.startswith("Items."):
            hex_color = item_colors.get(item_ident.split(".", 1)[1])
            if hex_color:
                raw.set("mapColor", ("rgbahex", hex_color))
    parse_body(raw, body, env)
    if cls == "OreBlock" and name and not name.startswith("ore-"):
        pass
    BLOCK_REQS[ident] = ("stacklist", raw.overrides.get("requirements", []))
    BLOCK_VALUES[ident] = {"health": raw.overrides.get("health", 0), "name": name}
    parsed.append(raw)


# Ident-indexed values referenced by other blocks during parsing
# (`ItemStack.mult(copperWall.requirements, 4)`, `x.health`).
BLOCK_REQS = {}
BLOCK_VALUES = {}


# ------------------------------------------------------------------ emit Rust

RUST_FIELD_ORDER = [
    "size", "health", "scaled_health", "armor", "category", "requirements", "research_cost",
    "research_cost_multiplier", "research_cost_multipliers", "build_cost_multiplier", "build_time",
    "group", "priority", "unit_cap_modifier", "flags", "consumes", "item_capacity",
    "liquid_capacity", "has_items", "has_liquids", "has_power", "outputs_power", "consumes_power",
    "conductive_power", "outputs_liquid", "build_visibility", "env_required", "env_enabled",
    "env_disabled", "solid", "floating", "update", "destructible", "save_data", "save_config",
    "configurable", "in_editor", "placeable_player", "placeable_liquid", "placeable_on",
    "insulated", "absorb_lasers", "allow_core_placement", "player_unmineable", "wall_ore",
    "item_drop", "ore_default", "ore_threshold", "ore_scale", "fog_radius", "region",
    "generate_icons", "map_color", "has_color", "square_sprite",
]

BOOL_FIELDS = {
    "has_items", "has_liquids", "has_power", "outputs_power", "consumes_power", "conductive_power",
    "outputs_liquid", "solid", "floating", "update", "destructible", "save_data", "save_config",
    "configurable", "in_editor", "placeable_player", "placeable_liquid", "placeable_on",
    "insulated", "absorb_lasers", "allow_core_placement", "player_unmineable", "wall_ore",
    "ore_default", "generate_icons", "has_color", "square_sprite",
}
INT_FIELDS = {"size", "health", "item_capacity", "unit_cap_modifier", "fog_radius"}
F32_FIELDS = {"scaled_health", "armor", "research_cost_multiplier", "build_cost_multiplier",
              "build_time", "priority", "liquid_capacity", "ore_threshold", "ore_scale"}

PRIORITY_NAMES = {
    "wall": "TARGET_PRIORITY_WALL", "under": "TARGET_PRIORITY_UNDER",
    "transport": "TARGET_PRIORITY_TRANSPORT", "base": "TARGET_PRIORITY_BASE",
    "turret": "TARGET_PRIORITY_TURRET", "core": "TARGET_PRIORITY_CORE",
}

CAMEL_GROUP = {
    "none": "None", "walls": "Walls", "projectors": "Projectors", "turrets": "Turrets",
    "transportation": "Transportation", "power": "Power", "liquids": "Liquids",
    "drills": "Drills", "units": "Units", "logic": "Logic", "payloads": "Payloads",
    "heat": "Heat",
}
CAMEL_VISIBILITY = {
    "hidden": "Hidden", "shown": "Shown", "debugOnly": "DebugOnly", "editorOnly": "EditorOnly",
    "coreZoneOnly": "CoreZoneOnly", "worldProcessorOnly": "WorldProcessorOnly",
    "sandboxOnly": "SandboxOnly", "campaignOnly": "CampaignOnly",
    "legacyLaunchPadOnly": "LegacyLaunchPadOnly", "notLegacyLaunchPadOnly": "NotLegacyLaunchPadOnly",
    "lightingOnly": "LightingOnly", "fogOnly": "FogOnly",
}
CAMEL_CATEGORY = {
    "turret": "Turret", "production": "Production", "distribution": "Distribution",
    "liquid": "Liquid", "power": "Power", "defense": "Defense", "crafting": "Crafting",
    "units": "Units", "effect": "Effect", "logic": "Logic",
}
CAMEL_FLAG = {
    "core": "Core", "storage": "Storage", "generator": "Generator", "turret": "Turret",
    "factory": "Factory", "repair": "Repair", "battery": "Battery", "reactor": "Reactor",
    "extinguisher": "Extinguisher", "drill": "Drill", "shield": "Shield", "launchPad": "LaunchPad",
    "unitCargoUnloadPoint": "UnitCargoUnloadPoint", "unitAssembler": "UnitAssembler",
    "hasFogRadius": "HasFogRadius", "steamVent": "SteamVent", "blockRepair": "BlockRepair",
    "synced": "Synced",
}
CAMEL_ENV = {
    "terrestrial": "Terrestrial", "spores": "Spores", "groundOil": "GroundOil",
    "groundWater": "GroundWater", "oxygen": "Oxygen", "scorching": "Scorching",
    "underwater": "Underwater", "space": "Space",
}


def f32_literal(value):
    """Shortest decimal that round-trips to the same IEEE-754 binary32."""
    packed = struct.pack("<f", value)
    unpacked = struct.unpack("<f", packed)[0]
    for precision in range(1, 10):
        text = f"{unpacked:.{precision}g}"
        if struct.pack("<f", float(text)) == packed:
            return text
    return repr(unpacked)


def fmt_num(value, f32=False):
    if isinstance(value, bool):
        return "1" if value else "0"
    if isinstance(value, int):
        return f"{value}.0f32" if f32 else str(value)
    if isinstance(value, float):
        if f32:
            text = f32_literal(value)
            if "." not in text and "e" not in text:
                text += ".0"
            return f"{text}f32"
        text = repr(value)
        if text.endswith(".0") or "e" in text:
            text = f"{value:.6f}".rstrip("0").rstrip(".")
        return text
    raise EvalError(f"cannot format {value!r}")


def rust_env(mask):
    if mask is None or not isinstance(mask, dict):
        return None
    if mask.get("any"):
        return "EnvMask::any()"
    flags = mask.get("flags", [])
    if not flags:
        return "EnvMask::of(vec![])"
    inner = ", ".join(f"EnvFlag::{CAMEL_ENV.get(f, f.capitalize())}" for f in flags)
    return f"EnvMask::of(vec![{inner}])"


def rust_consume(consume):
    kind = consume["kind"]
    if kind == "items":
        stacks = ", ".join(f'stack("{item}", {amount})' for item, amount in consume["stacks"])
        expr = f"consume_items(vec![{stacks}])"
    elif kind == "liquid":
        expr = f'consume_liquid("{consume["liquid"]}", {fmt_num(consume["amount"], f32=True)})'
    elif kind == "liquids":
        stacks = ", ".join(f'liquid_stack("{liquid}", {fmt_num(amount, f32=True)})'
                           for liquid, amount in consume["stacks"])
        expr = f"consume_liquids(vec![{stacks}])"
    elif kind == "power":
        if consume.get("buffered", 0.0) > 0:
            expr = f'consume_power_buffered({fmt_num(consume["buffered"], f32=True)})'
        else:
            expr = f'consume_power({fmt_num(consume["usage"], f32=True)})'
    elif kind == "coolant":
        expr = (f'consume_coolant({fmt_num(consume["amount"], f32=True)}, '
                f'{"true" if consume["allow_liquid"] else "false"}, '
                f'{"true" if consume["allow_gas"] else "false"})')
    else:
        raise EvalError(f"unknown consume {consume!r}")
    if consume.get("optional"):
        expr = f"consume_optional({expr})"
    return expr


BASE_DEFAULTS = {
    "size": 1, "health": -1, "scaled_health": -1.0, "armor": 0.0,
    "item_capacity": 10, "liquid_capacity": -1.0,
    "has_items": False, "has_liquids": False, "has_power": False,
    "outputs_power": False, "consumes_power": True, "conductive_power": False,
    "outputs_liquid": False, "build_visibility": {"__visibility__": "hidden"},
    "group": {"__group__": "none"}, "priority": {"__priority__": "base"},
    "unit_cap_modifier": 0, "category": {"__category__": "distribution"},
    "solid": False, "floating": False, "update": False, "destructible": False,
    "save_data": False, "save_config": False, "configurable": False,
    "in_editor": True, "placeable_player": True, "placeable_liquid": False,
    "placeable_on": True, "insulated": False, "absorb_lasers": False,
    "allow_core_placement": False, "player_unmineable": False, "wall_ore": False,
    "ore_default": False, "ore_threshold": 0.828, "ore_scale": 24.0,
    "fog_radius": -1, "generate_icons": True, "has_color": False,
    "square_sprite": True, "build_cost_multiplier": 1.0, "research_cost_multiplier": 1.0,
    "build_time": -1.0, "flags": [],
    "env_required": {"any": False, "flags": []},
    "env_enabled": {"any": False, "flags": ["terrestrial"]},
    "env_disabled": {"any": False, "flags": []},
}


def effective_fields(block):
    """Class defaults (source-derived) overridden by the block body."""
    class_defaults = class_defaults_for(block.cls)
    merged = dict(class_defaults)
    merged.update(block.overrides)
    for field in ("env_required", "env_enabled", "env_disabled"):
        if field in block.extra_env:
            merged[field] = merge_env(class_defaults.get(field), block.extra_env[field])
    return merged


def render_fields(merged):
    lines = []
    for field in RUST_FIELD_ORDER:
        if field not in merged:
            continue
        value = merged[field]
        if value is None or (isinstance(value, tuple) and value and value[0] == "unknown"):
            continue
        if field in BASE_DEFAULTS and value == BASE_DEFAULTS[field]:
            continue
        if field == "requirements":
            if not isinstance(value, list) or not value:
                continue
            stacks = ", ".join(f'stack("{item}", {amount})' for item, amount in value)
            lines.append(f"        requirements: vec![{stacks}],")
        elif field == "research_cost":
            if not isinstance(value, list) or not value:
                continue
            stacks = ", ".join(f'stack("{item}", {amount})' for item, amount in value)
            lines.append(f"        research_cost: Some(vec![{stacks}]),")
        elif field == "research_cost_multipliers":
            if not isinstance(value, list) or not value:
                continue
            inner = ", ".join(f'("{item}", {fmt_num(mult, f32=True)})' for item, mult in value)
            lines.append(f"        research_cost_multipliers: vec![{inner}],")
        elif field == "category":
            name = value.get("__category__") if isinstance(value, dict) else None
            if name:
                lines.append(f"        category: Some(Category::{CAMEL_CATEGORY.get(name, name.capitalize())}),")
        elif field == "build_visibility":
            name = value.get("__visibility__") if isinstance(value, dict) else None
            if name:
                lines.append(f"        build_visibility: Some(BuildVisibility::{CAMEL_VISIBILITY.get(name, name)}),")
        elif field == "group":
            name = value.get("__group__") if isinstance(value, dict) else None
            if name:
                lines.append(f"        group: Some(BlockGroup::{CAMEL_GROUP.get(name, name.capitalize())}),")
        elif field == "priority":
            name = value.get("__priority__") if isinstance(value, dict) else None
            if name:
                lines.append(f"        priority: Some({PRIORITY_NAMES.get(name, 'TARGET_PRIORITY_BASE')}),")
        elif field == "item_drop":
            item = value[2] if isinstance(value, tuple) and value[0] == "content" else None
            if item:
                lines.append(f'        item_drop: Some("{item}"),')
        elif field == "flags":
            if not isinstance(value, list) or not value:
                continue
            flags = [CAMEL_FLAG.get(f, f) for f in value]
            inner = ", ".join(f"BlockFlag::{f}" for f in flags)
            lines.append(f"        flags: vec![{inner}],")
        elif field == "map_color":
            if isinstance(value, tuple) and value[0] == "rgbahex":
                lines.append(f'        map_color: Some(rgba_hex("{value[1]}")),')
        elif field == "env_required":
            mask = rust_env(value)
            if mask:
                lines.append(f"        env_required: Some({mask}),")
        elif field == "env_enabled":
            mask = rust_env(value)
            if mask:
                lines.append(f"        env_enabled: Some({mask}),")
        elif field == "env_disabled":
            mask = rust_env(value)
            if mask:
                lines.append(f"        env_disabled: Some({mask}),")
        elif field == "region":
            if isinstance(value, tuple) and value[0] == "string":
                lines.append(f'        region: Some("{value[1]}"),')
        elif field in BOOL_FIELDS:
            lines.append(f"        {field}: Some({'true' if truthy(value) else 'false'}),")
        elif field in INT_FIELDS:
            lines.append(f"        {field}: Some({fmt_num(int(num(value)))}),")
        elif field in F32_FIELDS:
            lines.append(f"        {field}: Some({fmt_num(float(num(value)), f32=True)}),")
    return lines


def emit_consumes(block):
    if not block.consumes:
        return []
    exprs = ", ".join(rust_consume(c) for c in block.consumes)
    return [f"        consumes: vec![{exprs}],"]


def emit_block(block):
    merged = effective_fields(block)
    lines = render_fields(merged)
    lines.extend(emit_consumes(block))
    kind = block.cls
    if not lines:
        return f'    sink.push(spec("{block.name}", BlockKind::{kind}))?;'
    out = ["    sink.push(BlockSpec {"]
    out.extend(lines)
    out.append(f'        ..spec("{block.name}", BlockKind::{kind})')
    out.append("    })?;")
    return "\n".join(out)


def classify(value):
    """Renders a parsed override value for the ledger fingerprint."""
    if isinstance(value, dict):
        return {k: v for k, v in value.items()}
    if isinstance(value, tuple):
        return {"tuple": list(value)}
    return value


def main():
    parsed = parse_blocks()
    by_region = {}
    for block in parsed:
        by_region.setdefault(block.region, []).append(block)

    problems = [b for b in parsed if b.notes]
    if problems:
        print(f"WARN: {len(problems)} blocks with unparsed statements", file=sys.stderr)
        for b in problems[:40]:
            print(f"  {b.name}: {b.notes[:2]}", file=sys.stderr)
    unknown_classes = sorted({b.cls for b in parsed if b.cls not in class_files})
    if unknown_classes:
        print(f"WARN: classes with no source file found: {unknown_classes}", file=sys.stderr)

    OUT.mkdir(parents=True, exist_ok=True)
    for region in PORTED_REGIONS:
        blocks = by_region.get(region, [])
        body = "\n\n".join(emit_block(b) for b in blocks)
        text = f"""// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Blocks.java (region `{region}`).
//
//! Generated block metadata (`Blocks.java` region `{region}`).
//! Regenerate with `parity/tools/gen_blocks.py` after upstream content changes.

#![allow(unused_imports)]

use super::{{spec, stack, liquid_stack, BlockFlag, BlockKind, BlockSink, BlockSpec, BuildVisibility}};
use super::{{consume_coolant, consume_item, consume_items, consume_liquid, consume_liquids, consume_optional, consume_power, consume_power_buffered, rgba_hex}};
use super::{{BlockGroup, EnvFlag, EnvMask, TARGET_PRIORITY_BASE, TARGET_PRIORITY_CORE, TARGET_PRIORITY_TRANSPORT, TARGET_PRIORITY_TURRET, TARGET_PRIORITY_UNDER, TARGET_PRIORITY_WALL}};
use crate::content::{{Category, ContentError}};

/// Loads the `{region}` region in upstream order.
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {{
{body}
    let _ = sink;
    Ok(())
}}
"""
        (OUT / f"{region.replace(' - ', '_').replace(' ', '_')}.rs").write_text(text, encoding="utf-8", newline="\n")
        print(f"wrote {region}: {len(blocks)} blocks")

    # Ledger.
    ledger_lines = [
        f"# Ledger — blocks (upstream `content/Blocks.java`, {len(parsed)} entries)",
        "",
        "> Source-derived fingerprint (`golden_sha` = sha256/12 of the canonical metadata spec).",
        "> The JVM golden (`parity/golden_content.json`) is pending (NUD-10); this ledger is the",
        "> M3/M4 mechanical audit per plan 02 §6.2/§9.",
        "",
        "| pos | name | kind | ported | golden_sha | wave | notes |",
        "|-----|------|------|--------|-----------|------|-------|",
    ]
    unported = 0
    for pos, block in enumerate(parsed):
        wave = WAVES.get(block.region, "?")
        ported = block.region in PORTED_REGIONS
        if not ported:
            unported += 1
        fingerprint = hashlib.sha256(
            json.dumps({"name": block.name, "kind": block.cls, "region": block.region,
                        "overrides": {k: classify(v) for k, v in block.overrides.items()},
                        "consumes": block.consumes},
                       sort_keys=True, default=str).encode()
        ).hexdigest()[:12]
        notes = "; ".join(block.notes[:1])
        ledger_lines.append(
            f"| {pos} | {block.name} | {block.cls} | [{'x' if ported else ' '}] | {fingerprint} | {wave} | {notes} |"
        )
    ledger_lines.extend([
        "",
        f"- Unported: {unported}",
        f"- Parsed: {len(parsed)}",
    ])
    LEDGER.parent.mkdir(parents=True, exist_ok=True)
    LEDGER.write_text("\n".join(ledger_lines) + "\n", encoding="utf-8", newline="\n")

    for pos, block in enumerate(parsed):
        if block.name == "stone-wall":
            print(f"stone-wall id = {pos}")
        if block.name == "build2":
            print(f"build2 id = {pos}")
    print(f"total parsed = {len(parsed)}; ported regions = {sum(len(by_region.get(r, [])) for r in PORTED_REGIONS)}")


if __name__ == "__main__":
    main()
