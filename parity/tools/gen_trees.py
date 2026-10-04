#!/usr/bin/env python3
"""Generate flat Rust tech-tree data from Mindustry's SerpuloTechTree/ErekirTechTree.

One-off conversion used for plan 02 M2; node data is committed and re-run only
when upstream content changes.

Usage: gen_trees.py [upstream_mindustry_src] [output_dir]
  upstream_mindustry_src defaults to ../Mindustry/core/src/mindustry
  output_dir defaults to the current directory
"""
import re
import sys
from pathlib import Path

ROOT = Path(
    sys.argv[1]
    if len(sys.argv) > 1
    else "../Mindustry/core/src/mindustry"
)
OUT = Path(sys.argv[2] if len(sys.argv) > 2 else ".")


def field_map(path):
    text = Path(path).read_text(encoding="utf-8")
    text = re.sub(r"//[^\n]*", "", text)
    pat = re.compile(r"(\w+)\s*=\s*new\s+[\w.]+\(\s*\"([^\"]+)\"")
    return {m.group(1): m.group(2) for m in pat.finditer(text)}


items = field_map(ROOT / "content/Items.java")
liquids = field_map(ROOT / "content/Liquids.java")
sectors = field_map(ROOT / "content/SectorPresets.java")
blocks = field_map(ROOT / "content/Blocks.java")
units = field_map(ROOT / "content/UnitTypes.java")

print(f"maps: items={len(items)} liquids={len(liquids)} sectors={len(sectors)} blocks={len(blocks)} units={len(units)}", file=sys.stderr)


def kebab(name):
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1-\2", name)
    s = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1-\2", s)
    return s.lower()


missing = []


def content_name(qual, ident):
    if qual == "Items":
        return items.get(ident, kebab(ident))
    if qual == "Liquids":
        return liquids.get(ident, kebab(ident))
    if qual == "SectorPresets" or qual == "sectors":
        return sectors.get(ident, ident)
    if qual == "Blocks":
        return blocks.get(ident, kebab(ident))
    if qual == "UnitTypes":
        return units.get(ident, kebab(ident))
    if qual == "Planets":
        return ident
    # unqualified: try blocks, units, then sector presets (campaign sectors are nodes)
    if ident in blocks:
        return blocks[ident]
    if ident in units:
        return units[ident]
    if ident in sectors:
        return sectors[ident]
    missing.append(ident)
    return kebab(ident)


def split_args(s):
    """Split top-level comma args."""
    args, depth, cur, in_str = [], 0, [], False
    for c in s:
        if in_str:
            cur.append(c)
            if c == '"':
                in_str = False
            continue
        if c == '"':
            in_str = True
            cur.append(c)
        elif c in "([{":
            depth += 1
            cur.append(c)
        elif c in ")]}":
            depth -= 1
            cur.append(c)
        elif c == "," and depth == 0:
            args.append("".join(cur).strip())
            cur = []
        else:
            cur.append(c)
    if "".join(cur).strip():
        args.append("".join(cur).strip())
    return args


def balanced(text, start):
    """start points at '('; returns (inner, end_index_after_paren)."""
    depth = 0
    i = start
    in_str = False
    while i < len(text):
        c = text[i]
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
        else:
            if c == '"':
                in_str = True
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    return text[start + 1 : i], i + 1
        i += 1
    raise ValueError("unbalanced")


def parse_objectives(expr):
    """Returns list of rust objective expressions."""
    expr = expr.strip()
    if not expr:
        return []
    if expr == "erekirSector":
        return ["OnPlanet(\"erekir\")"]
    m = re.fullmatch(r"Seq\.with\((.*)\)", expr, re.S)
    if not m:
        raise ValueError(f"unknown objectives expr: {expr[:80]}")
    out = []
    for arg in split_args(m.group(1)):
        out.append(parse_objective(arg))
    return out


def parse_objective(arg):
    m = re.fullmatch(r"new (\w+)\((?:(\w+)\.)?(\w+)\)", arg.strip())
    assert m, f"objective {arg!r}"
    kind, qual, ident = m.groups()
    if kind in ("SectorComplete", "OnSector"):
        name = sectors.get(ident, ident)
    elif kind == "OnPlanet":
        name = ident
    else:
        name = content_name(qual, ident)
    if kind == "SectorComplete":
        return f'SectorComplete("{name}")'
    if kind == "OnSector":
        return f'OnSector("{name}")'
    if kind == "OnPlanet":
        return f'OnPlanet("{name}")'
    if kind == "Research":
        return f'Research("{name}")'
    raise ValueError(f"objective kind {kind}")


def parse_content_token(tok):
    m = re.fullmatch(r"(\w+)\.(\w+)", tok)
    if m:
        return content_name(m.group(1), m.group(2))
    return content_name(None, tok)


def strip_comments(text):
    text = re.sub(r"//[^\n]*", "", text)
    return text


def emit_tree(java_path, tree_name, rust_name, cost_multipliers=None):
    text = strip_comments(Path(java_path).read_text(encoding="utf-8"))
    idx = text.index("nodeRoot(")
    inner, _ = balanced(text, text.index("(", idx))
    args = split_args(inner)
    # args: "serpulo", coreShard [, true], () -> { ... }
    root_name_str = args[0].strip().strip('"')
    root_content = parse_content_token(args[1].strip())
    requires = "true" if any(a.strip() == "true" for a in args[2:-1]) else "false"
    lambda_src = args[-1]
    body = lambda_src[lambda_src.index("{") + 1 : lambda_src.rindex("}")]

    lines = []
    lines.append(f'        t.root("{rust_name}", "{root_content}", {requires});')
    if cost_multipliers:
        entries = ", ".join(f'("{name}", {value})' for name, value in cost_multipliers)
        lines.append(f"        t.set_cost_multipliers(&[{entries}]);")

    def walk(src, depth):
        i = 0
        while i < len(src):
            m = re.search(r"\b(nodeProduce|node)\s*\(", src[i:])
            if not m:
                break
            kind = m.group(1)
            call_start = i + m.end() - 1
            inner, end = balanced(src, call_start)
            args = split_args(inner)
            assert args, f"empty {kind} call at {src[call_start:call_start+60]!r}"
            content = parse_content_token(args[0].strip())
            objectives = []
            requirements = []
            for a in args[1:]:
                a = a.strip()
                if "-> {" in a:
                    pass
                elif a.startswith("Seq.with"):
                    objectives.extend(parse_objectives(a))
                elif a == "erekirSector":
                    objectives.append('OnPlanet("erekir")')
                elif a.startswith("ItemStack.with"):
                    inner_req = a[len("ItemStack.with(") : -1]
                    parts = split_args(inner_req)
                    assert len(parts) % 2 == 0, f"bad requirements {a!r}"
                    for i in range(0, len(parts), 2):
                        qual, ident = parts[i].strip().split(".")
                        requirements.append((content_name(qual, ident), parts[i + 1].strip()))
                elif a in ("true", "false"):
                    pass
                else:
                    raise ValueError(f"unexpected arg {a[:60]!r} in {kind}")
            lambda_src2 = next((a for a in args[1:] if "-> {" in a), None)
            if kind == "nodeProduce":
                objectives = [f'Produce("{content}")'] + objectives
            obj_rs = "&[" + ", ".join(f"obj::{o}" for o in objectives) + "]"
            req_rs = "&[" + ", ".join(f'("{n}", {v})' for n, v in requirements) + "]"
            if requirements and objectives:
                lines.append(f'        t.at_full({depth}, "{content}", {req_rs}, {obj_rs});')
            elif requirements:
                lines.append(f'        t.at_req({depth}, "{content}", {req_rs});')
            elif objectives:
                lines.append(f'        t.at_obj({depth}, "{content}", {obj_rs});')
            else:
                lines.append(f'        t.at({depth}, "{content}");')
            # children
            if lambda_src2 is not None:
                body2 = lambda_src2[lambda_src2.index("{") + 1 : lambda_src2.rindex("}")]
                walk(body2, depth + 1)
            i = end

    walk(body, 1)
    return root_name_str, lines


def write_rust(path, tree_name, lines, names):
    out = []
    out.append("// SPDX-License-Identifier: GPL-3.0-only")
    out.append("//")
    out.append("// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.")
    out.append(f"// Source: core/src/mindustry/content/{tree_name}TechTree.java")
    out.append("//")
    out.append("//! Vanilla tech-tree node data (flat pre-order; depth tracks Java nesting).")
    out.append("//!")
    out.append("//! Generated once from the upstream source with a mechanical converter; node")
    out.append("//! order, parents, objectives and produce nodes match `TechTree.node` nesting.")
    out.append("//! Names for content whose registries land in M3/M5 stay unresolved until then")
    out.append("//! (`TechTreeBuildReport::missing`), the same data resolving completely afterwards.")
    out.append("")
    out.append("use super::{NodeObjective as obj, TechTreeBuilder};")
    out.append("")
    out.append("/// Loads the tree into `t`.")
    out.append("pub fn load(t: &mut TechTreeBuilder<'_>) {")
    out.extend(lines)
    out.append("}")
    out.append("")
    Path(path).write_text("\n".join(out) + "\n", encoding="utf-8", newline="\n")


def main():
    serp_name, serp_lines = emit_tree(ROOT / "content/SerpuloTechTree.java", "Serpulo", "Serpulo", None)
    write_rust(OUT / "serpulo.rs", "Serpulo", serp_lines, None)

    # Erekir cost multipliers: all items 0.9, then the upstream overrides.
    cost = [(name, 0.9) for name in items.values()]
    cost += [
        ("oxide", 0.5),
        ("surge-alloy", 0.7),
        ("carbide", 0.3),
        ("phase-fabric", 0.2),
    ]
    erekir_name, erekir_lines = emit_tree(
        ROOT / "content/ErekirTechTree.java", "Erekir", "Erekir", cost
    )
    write_rust(OUT / "ekir.rs", "Erekir", erekir_lines, None)

    print(f"serpulo nodes: {len(serp_lines)} erekir lines: {len(erekir_lines)}", file=sys.stderr)
    print(f"unresolved idents: {sorted(set(missing))}", file=sys.stderr)


if __name__ == "__main__":
    main()
