# SPDX-License-Identifier: GPL-3.0-only
"""Build, check and tidy bespoke Obsidian flowchart canvases.

    python canvas_tool.py build <spec.json> <out.canvas>
    python canvas_tool.py check <file.canvas> [--profile basic|timeline]
    python canvas_tool.py fit-groups <file.canvas> [--write]
    python canvas_tool.py extents <file.canvas> [--tile 4800]
    python canvas_tool.py skeleton <spec.json> <out.md> "<Doc title>"
    python canvas_tool.py doc-check <file.canvas> <doc.md>
    python canvas_tool.py skeleton-canvas <file.canvas> <map.json> <out.md> "<Doc title>"
    python canvas_tool.py notes <file.canvas> <map.json> [--write]

The rules are the ones in ../SKILL.md: 480x480 nodes on a 240 grid, groups
exactly 60 outside what they wrap, lanes coloured #c0c0c0 stacked top to
bottom, edges right->left inside a lane and bottom->top between lanes, every
edge with styleAttributes.bezierCurvature "5". No dependencies beyond the
standard library.
"""

import json
import re
import sys

GRID = 240
NODE = 480
PAD = 60
LANE_COLOR = "#c0c0c0"
MAIN_COLOR = "#ffffff"
CURVATURE = "5"
SECTION_PAD = 240   # timeline section groups: padding around everything they wrap
SECTION_GAP = 720   # timeline section groups: gap between neighbours


# --------------------------------------------------------------------------
# geometry


def rect(n):
    return (n["x"], n["y"], n["x"] + n["width"], n["y"] + n["height"])


def contains(outer, inner):
    return (outer[0] <= inner[0] and outer[1] <= inner[1]
            and outer[2] >= inner[2] and outer[3] >= inner[3])


def overlaps(a, b):
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


def area(r):
    return (r[2] - r[0]) * (r[3] - r[1])


def union(rects):
    return (min(r[0] for r in rects), min(r[1] for r in rects),
            max(r[2] for r in rects), max(r[3] for r in rects))


def crosses(a, b):
    """a and b overlap and neither contains the other."""
    return overlaps(a, b) and not contains(a, b) and not contains(b, a)


def timeline_sections(nodes):
    """Timeline section groups: #c0c0c0 groups that cross a lane (another
    #c0c0c0 group) and are the taller of the pair. Lanes are the rest."""
    grey = [n for n in nodes if n["type"] == "group" and n.get("color") == LANE_COLOR]
    out = set()
    for a in grey:
        for b in grey:
            if a is not b and crosses(rect(a), rect(b)) and a["height"] > b["height"]:
                out.add(a["id"])
    return out


def parents(nodes):
    """Map node id -> id of the smallest group that fully contains it.
    Timeline sections are not parents (they cross the lanes)."""
    skip = timeline_sections(nodes)
    groups = [n for n in nodes if n["type"] == "group" and n["id"] not in skip]
    out = {}
    for n in nodes:
        if n["id"] in skip:
            out[n["id"]] = None
            continue
        r = rect(n)
        best = None
        for g in groups:
            if g is n:
                continue
            gr = rect(g)
            if contains(gr, r) and gr != r:
                if best is None or area(gr) < area(rect(best)):
                    best = g
        out[n["id"]] = best["id"] if best else None
    return out


def fit_groups(nodes):
    """Resize every group to its children's bounding box plus PAD, innermost
    first. Membership is taken from the geometry before any resize."""
    par = parents(nodes)
    by_id = {n["id"]: n for n in nodes}
    kids = {}
    for nid, pid in par.items():
        if pid:
            kids.setdefault(pid, []).append(nid)

    def depth(gid):
        d = 0
        while par.get(gid):
            gid = par[gid]
            d += 1
        return d

    groups = sorted((n for n in nodes if n["type"] == "group"),
                    key=lambda g: -depth(g["id"]))
    changed = []
    for g in groups:
        ch = kids.get(g["id"])
        if not ch:
            continue
        u = union([rect(by_id[c]) for c in ch])
        new = (u[0] - PAD, u[1] - PAD, u[2] + PAD, u[3] + PAD)
        if new != rect(g):
            changed.append((g["id"], rect(g), new))
            g["x"], g["y"] = new[0], new[1]
            g["width"], g["height"] = new[2] - new[0], new[3] - new[1]
    return changed


# --------------------------------------------------------------------------
# output in Obsidian's own layout (tab indent, one object per line)


def dump(canvas):
    def line(o):
        return json.dumps(o, ensure_ascii=False, separators=(",", ":"))

    out = ["{", '\t"nodes":[']
    out.append(",\n".join("\t\t" + line(n) for n in canvas["nodes"]))
    out.append("\t],")
    out.append('\t"edges":[')
    out.append(",\n".join("\t\t" + line(e) for e in canvas["edges"]))
    out.append("\t],")
    rest = {k: v for k, v in canvas.items() if k not in ("nodes", "edges")}
    rest.setdefault("metadata", {"version": "1.0-1.0", "frontmatter": {}})
    body = json.dumps(rest, ensure_ascii=False, indent="\t")[1:-1].strip("\n")
    out.append(body)
    out.append("}")
    return "\n".join(out) + "\n"


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def save(path, canvas):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(dump(canvas))


# --------------------------------------------------------------------------
# build


def build(spec):
    """Spec -> canvas. Positions in the spec are grid units (1 = 240)."""
    lanes = spec["lanes"]
    lane_gap = spec.get("lane_gap", 2)
    lane_ids = [l["id"] for l in lanes]
    errors = []

    spec_nodes = spec["nodes"]
    for n in spec_nodes:
        if n.get("lane") not in lane_ids:
            errors.append(f"node {n['id']}: unknown lane {n.get('lane')!r}")
    if errors:
        raise SystemExit("\n".join(errors))

    # Stack lane bands: each lane's rows start at gy 0; a band is as tall as
    # its deepest node, and bands are lane_gap grid units apart.
    top = spec.get("origin_gy", 0)
    lane_top = {}
    for l in lanes:
        rows = [n["gy"] + n.get("gh", 2) for n in spec_nodes if n["lane"] == l["id"]]
        lane_top[l["id"]] = top
        top += (max(rows) if rows else 2) + lane_gap

    nodes = []
    for n in spec_nodes:
        out = {"id": n["id"]}
        if "heading" in n:
            # A note card: the companion doc's section under this heading.
            if not spec.get("doc"):
                raise SystemExit(f"node {n['id']}: 'heading' needs a top-level 'doc'")
            out["type"] = "file"
            out["file"] = spec["doc"]
            out["subpath"] = "#" + n["heading"]
        elif "file" in n:
            out["type"] = "file"
            out["file"] = n["file"]
            if "subpath" in n:
                out["subpath"] = n["subpath"]
        else:
            out["type"] = "text"
            out["text"] = n["text"]
        out["x"] = n["gx"] * GRID
        out["y"] = (lane_top[n["lane"]] + n["gy"]) * GRID
        out["width"] = n.get("gw", 2) * GRID
        out["height"] = n.get("gh", 2) * GRID
        out["color"] = MAIN_COLOR if n.get("main") else n.get("color", MAIN_COLOR)
        nodes.append(out)
    by_id = {n["id"]: n for n in nodes}
    meta = {n["id"]: n for n in spec_nodes}

    # Sub-groups (forks, sections) and lanes, fitted to their members.
    sub = spec.get("groups", [])
    member = {}
    for n in spec_nodes:
        g = n.get("group")
        if g:
            member.setdefault(g, []).append(n["id"])
    for g in sub:
        if g.get("parent"):
            member.setdefault(g["parent"], []).append(g["id"])

    group_nodes = {}

    def group_rect(gid):
        if gid in group_nodes:
            return rect(group_nodes[gid])
        rs = []
        for m in member.get(gid, []):
            rs.append(rect(by_id[m]) if m in by_id else group_rect(m))
        if not rs:
            raise SystemExit(f"group {gid} has no members")
        u = union(rs)
        g = next(x for x in sub if x["id"] == gid)
        group_nodes[gid] = {
            "id": gid, "type": "group", "label": g["label"],
            "x": u[0] - PAD, "y": u[1] - PAD,
            "width": u[2] - u[0] + 2 * PAD, "height": u[3] - u[1] + 2 * PAD,
            "color": g.get("color", "4"),
        }
        return rect(group_nodes[gid])

    for g in sub:
        group_rect(g["id"])

    # Timeline sections: shift each section's cards and groups sideways so the
    # section boxes (contents + SECTION_PAD) sit exactly SECTION_GAP apart.
    sections = spec.get("sections", [])
    sec_of = {}
    if sections:
        for n in spec_nodes:
            s = n.get("section") or next(
                (s["id"] for s in sections if n["gx"] <= s["until_gx"]), None)
            if s not in {x["id"] for x in sections}:
                errors.append(f"node {n['id']}: no section (gx {n['gx']})")
            sec_of[n["id"]] = s

        def leaves(gid):
            for m in member.get(gid, []):
                yield from ([m] if m in by_id else leaves(m))

        for g in sub:
            ss = {sec_of[m] for m in leaves(g["id"])}
            if len(ss) != 1:
                errors.append(f"group {g['id']} straddles sections {sorted(map(str, ss))}")
            sec_of[g["id"]] = next(iter(ss))
        if errors:
            raise SystemExit("\n".join(errors))
        prev_right = None
        for s in sections:
            items = [by_id[i] for i, k in sec_of.items() if k == s["id"] and i in by_id]
            items += [group_nodes[i] for i, k in sec_of.items() if k == s["id"] and i in group_nodes]
            if not items:
                raise SystemExit(f"section {s['id']} is empty")
            lo = min(n["x"] for n in items)
            delta = 0 if prev_right is None else prev_right + SECTION_GAP + SECTION_PAD - lo
            for n in items:
                n["x"] += delta
            s["_x0"] = lo + delta - SECTION_PAD
            s["_x1"] = max(n["x"] + n["width"] for n in items) + SECTION_PAD
            prev_right = s["_x1"]

    lane_nodes = []
    for l in lanes:
        rs = [rect(by_id[n["id"]]) for n in spec_nodes
              if n["lane"] == l["id"] and not n.get("group")]
        rs += [rect(group_nodes[g["id"]]) for g in sub
               if not g.get("parent") and g.get("lane") == l["id"]]
        if not rs:
            continue
        u = union(rs)
        lane_nodes.append({
            "id": l["id"], "type": "group", "label": l["label"],
            "x": u[0] - PAD, "y": u[1] - PAD,
            "width": u[2] - u[0] + 2 * PAD, "height": u[3] - u[1] + 2 * PAD,
            "color": LANE_COLOR,
        })

    # Optional: give every lane the same left/right edge so the bands line
    # up. Only valid when that does not change what a lane wraps.
    if spec.get("align_lanes"):
        x0 = min(l["x"] for l in lane_nodes)
        x1 = max(l["x"] + l["width"] for l in lane_nodes)
        for l in lane_nodes:
            l["x"], l["width"] = x0, x1 - x0

    # Groups first (bottom of the z-order), outermost first.
    def gdepth(gid):
        d, cur = 0, next(x for x in sub if x["id"] == gid)
        while cur.get("parent"):
            d += 1
            cur = next(x for x in sub if x["id"] == cur["parent"])
        return d

    section_nodes = []
    if sections and lane_nodes:
        y0 = min(l["y"] for l in lane_nodes) - SECTION_PAD
        y1 = max(l["y"] + l["height"] for l in lane_nodes) + SECTION_PAD
        for s in sections:
            g = {"id": s["id"], "type": "group", "x": s["_x0"], "y": y0,
                 "width": s["_x1"] - s["_x0"], "height": y1 - y0, "color": LANE_COLOR}
            if s.get("label"):
                g["label"] = s["label"]
            section_nodes.append(g)

    ordered_sub = sorted(sub, key=lambda g: gdepth(g["id"]))
    all_nodes = (section_nodes + lane_nodes
                 + [group_nodes[g["id"]] for g in ordered_sub] + nodes)

    edges = []
    seen = set()
    for e in spec["edges"]:
        a, b = e["from"], e["to"]
        for end in (a, b):
            if end not in by_id:
                errors.append(f"edge {a}->{b}: unknown node {end}")
        if a not in by_id or b not in by_id:
            continue
        same = meta[a]["lane"] == meta[b]["lane"]
        eid = e.get("id") or f"e-{a}-{b}"
        if eid in seen:
            errors.append(f"duplicate edge id {eid}")
        seen.add(eid)
        out = {
            "id": eid,
            "fromNode": a, "fromSide": "right" if same else "bottom",
            "toNode": b, "toSide": "left" if same else "top",
            "toEnd": "arrow",
        }
        both_main = meta[a].get("main") and meta[b].get("main")
        fork_end = b if meta[a].get("main") else a
        color = e.get("color") or (MAIN_COLOR if both_main else by_id[fork_end]["color"])
        out["color"] = color
        if "label" in e:
            out["label"] = e["label"]
        out["styleAttributes"] = {"bezierCurvature": CURVATURE}
        edges.append(out)
    if errors:
        raise SystemExit("\n".join(errors))

    return {"nodes": all_nodes, "edges": edges,
            "metadata": {"version": "1.0-1.0", "frontmatter": {}}}


# --------------------------------------------------------------------------
# check


def check(canvas, profile):
    problems = []
    nodes = canvas.get("nodes", [])
    edges = canvas.get("edges", [])
    by_id = {}
    for n in nodes + edges:
        if n["id"] in by_id:
            problems.append(f"duplicate id {n['id']}")
        by_id[n["id"]] = n

    cards = [n for n in nodes if n["type"] != "group"]
    secs = timeline_sections(nodes)
    sections = sorted((n for n in nodes if n["id"] in secs), key=lambda s: s["x"])
    groups = [n for n in nodes if n["type"] == "group" and n["id"] not in secs]

    # Cards sit on the 240 grid. With timeline sections the x grid is per
    # section (the 720 gap and 240 padding can offset a section by 60/120),
    # anchored at the section's left-most card.
    def section_of(r):
        return next((s for s in sections if contains(rect(s), r)), None)

    x_anchor = {}
    for n in cards:
        s = section_of(rect(n))
        if s is not None:
            x_anchor.setdefault(s["id"], n["x"])
            x_anchor[s["id"]] = min(x_anchor[s["id"]], n["x"])
    for n in cards:
        if (n["width"], n["height"]) != (NODE, NODE):
            problems.append(f"{n['id']}: size {n['width']}x{n['height']}, want {NODE}x{NODE}")
        s = section_of(rect(n))
        x0 = x_anchor[s["id"]] if s is not None else 0
        if (n["x"] - x0) % GRID or n["y"] % GRID:
            problems.append(f"{n['id']}: position ({n['x']},{n['y']}) is off the {GRID} grid")
        if n["type"] == "text" and "\\n" in n.get("text", ""):
            problems.append(f"{n['id']}: literal backslash-n in text")

    for i, a in enumerate(cards):
        for b in cards[i + 1:]:
            if overlaps(rect(a), rect(b)):
                problems.append(f"{a['id']} overlaps {b['id']}")

    par = parents(nodes)
    kids = {}
    for nid, pid in par.items():
        if pid:
            kids.setdefault(pid, []).append(nid)
    for g in groups:
        ch = kids.get(g["id"])
        if not ch:
            problems.append(f"group {g['id']} ({g.get('label')}) wraps nothing")
            continue
        u = union([rect(by_id[c]) for c in ch])
        want = (u[0] - PAD, u[1] - PAD, u[2] + PAD, u[3] + PAD)
        if rect(g) != want and not (profile == "timeline" and g.get("color") == LANE_COLOR
                                    and want[1] == g["y"] and want[3] == g["y"] + g["height"]
                                    and g["x"] <= want[0] and g["x"] + g["width"] >= want[2]):
            problems.append(f"group {g['id']} ({g.get('label')}): {rect(g)}, padding-{PAD} fit is {want}")
        # a group must not cut through anything it does not wrap
        for n in nodes:
            if n is g or par.get(n["id"]) == g["id"] or n["id"] in secs:
                continue
            nr, gr = rect(n), rect(g)
            if overlaps(nr, gr) and not contains(gr, nr) and not contains(nr, gr):
                problems.append(f"group {g['id']} partially overlaps {n['id']}")

    for e in edges:
        for end in ("fromNode", "toNode"):
            if e.get(end) not in by_id:
                problems.append(f"edge {e['id']}: {end} {e.get(end)} does not exist")

    if profile == "timeline":
        lanes = [g for g in groups if g.get("color") == LANE_COLOR]
        # Sections: wrap every card and fork group they hold (horizontally) and
        # the whole lane stack (vertically) by SECTION_PAD; neighbours are
        # SECTION_GAP apart; nothing is cut by a section edge.
        inner = [n for n in nodes if n["type"] != "group"
                 or (n["id"] not in secs and n not in lanes)]
        if sections and lanes:
            ly0 = min(l["y"] for l in lanes) - SECTION_PAD
            ly1 = max(l["y"] + l["height"] for l in lanes) + SECTION_PAD
        for s in sections:
            sr = rect(s)
            held = [n for n in inner if contains(sr, rect(n))]
            for n in inner:
                if crosses(sr, rect(n)):
                    problems.append(f"section {s['id']} ({s.get('label')}) cuts through {n['id']}")
            if not held:
                problems.append(f"section {s['id']} ({s.get('label')}) wraps nothing")
                continue
            u = union([rect(n) for n in held])
            want = (u[0] - SECTION_PAD, ly0, u[2] + SECTION_PAD, ly1)
            if sr != want:
                problems.append(f"section {s['id']} ({s.get('label')}): {sr}, padding-{SECTION_PAD} fit is {want}")
        for a, b in zip(sections, sections[1:]):
            gap = b["x"] - (a["x"] + a["width"])
            if gap != SECTION_GAP:
                problems.append(f"sections {a['id']} -> {b['id']}: gap {gap}, want {SECTION_GAP}")
        for n in cards:
            if sections and section_of(rect(n)) is None:
                problems.append(f"{n['id']}: not inside any timeline section")
        if len(lanes) < 2:
            problems.append(f"timeline: found {len(lanes)} lane group(s) coloured {LANE_COLOR}")
        ys = sorted(lanes, key=lambda g: g["y"])
        for a, b in zip(ys, ys[1:]):
            if a["y"] + a["height"] > b["y"]:
                problems.append(f"lanes {a['id']} and {b['id']} overlap vertically")

        def lane_of(nid):
            while nid:
                n = by_id[nid]
                if n["type"] == "group" and n.get("color") == LANE_COLOR:
                    return nid
                nid = par.get(nid)
            return None

        for n in cards:
            if not lane_of(n["id"]):
                problems.append(f"{n['id']}: not inside any lane")
        for e in edges:
            if e.get("fromNode") not in by_id or e.get("toNode") not in by_id:
                continue
            same = lane_of(e["fromNode"]) == lane_of(e["toNode"])
            want = ("right", "left") if same else ("bottom", "top")
            got = (e.get("fromSide"), e.get("toSide"))
            if got != want:
                problems.append(f"edge {e['id']}: sides {got}, want {want}")
            if (e.get("styleAttributes") or {}).get("bezierCurvature") != CURVATURE:
                problems.append(f"edge {e['id']}: styleAttributes.bezierCurvature is not \"{CURVATURE}\"")
    return problems


LABEL_CHARS_PER_GRID = 6  # roughly what fits per 240 of free gap at default size


def label_warnings(canvas):
    """Edge labels too long for the horizontal gap they sit in (same-lane
    edges only; Obsidian clips nothing, but the label covers both cards)."""
    by_id = {n["id"]: n for n in canvas["nodes"]}
    out = []
    for e in canvas["edges"]:
        lab = e.get("label")
        if not lab or e.get("fromSide") != "right":
            continue
        a, b = by_id[e["fromNode"]], by_id[e["toNode"]]
        gap = b["x"] - (a["x"] + a["width"])
        if gap > 0 and len(lab) > LABEL_CHARS_PER_GRID * gap // GRID * 2:
            out.append(f"warning: edge {e['id']} label {lab!r} ({len(lab)} chars) is long for a {gap} gap")
    return out


def extents(canvas, tile):
    """Print the canvas extent, every lane's band, and one PowerShell
    zoomToBbox command per horizontal tile (zoom-to-fit stops at Obsidian's
    minimum zoom, so a wide chart has to be screenshotted in tiles)."""
    nodes = canvas["nodes"]
    u = union([rect(n) for n in nodes])
    print(f"extent x {u[0]}..{u[2]}  y {u[1]}..{u[3]}  ({u[2] - u[0]} x {u[3] - u[1]})")
    secs = timeline_sections(nodes)
    for g in sorted((n for n in nodes if n["type"] == "group" and n.get("color") == LANE_COLOR
                     and n["id"] not in secs),
                    key=lambda g: g["y"]):
        print(f"lane {g['id']:<12} {g.get('label', '')!r:<20} y {g['y']}..{g['y'] + g['height']}")
    x, i = u[0], 0
    while x < u[2]:
        x1 = min(x + tile, u[2])
        print(f"tile {i}: obsidian eval code=\"app.workspace.activeLeaf.view.canvas.zoomToBbox("
              f"{{minX:{x - 60},minY:{u[1] - 60},maxX:{x1 + 60},maxY:{u[3] + 60}}}); 'ok'\"")
        x, i = x1, i + 1


# --------------------------------------------------------------------------


# --------------------------------------------------------------------------
# companion doc


# Characters a card heading may use, so `#Heading` resolves the same way in a
# canvas subpath, a wikilink and Obsidian's heading search.
HEADING_OK = re.compile(r"^[A-Za-z0-9 _,.()+-]+$")
SPINE_TOP = ["Assumed knowledge", "The 30-second version", "Find it fast", "Flowcharts"]
SPINE_END = ["Source map", "Known gaps / stubs", "Where to go next"]
BRIEF = "> brief:"


def skeleton(spec, title):
    """Spec -> companion doc skeleton: the TDD hand-written spine, one
    `## N. <section>` per timeline section, one `###` per main-path card and
    one `### <fork>` (with a `####` per card) per fork. Each card heading is
    followed by a `> brief:` line (the spec's card text) for the writer to
    replace."""
    nodes = [n for n in spec["nodes"] if "heading" in n]
    lanes = [l["id"] for l in spec["lanes"]]
    groups = {g["id"]: g for g in spec.get("groups", [])}
    sections = spec.get("sections") or [{"id": None, "label": "Timeline", "until_gx": 10 ** 9}]

    def sec(n):
        return n.get("section") or next(
            (s["id"] for s in sections if n["gx"] <= s["until_gx"]), sections[-1]["id"])

    def top_group(gid):
        while gid and groups[gid].get("parent"):
            gid = groups[gid]["parent"]
        return gid

    def fork_name(gid):
        g = groups[top_group(gid)]
        return g.get("fork") or g["label"].replace("·", "-")

    def brief(n):
        return " ".join((n.get("text") or "").replace("**", "").split("\n"))

    order = sorted(nodes, key=lambda n: (n["gx"], lanes.index(n["lane"]), n["gy"]))
    # Mirrored cards (P1 and P2 doing the same thing) may share one heading:
    # the doc gets the section once, at the first card.
    first = {}
    for n in order:
        first.setdefault(n["heading"], n["id"])
    order = [n for n in order if first[n["heading"]] == n["id"]]
    out = [f"# {title}", "", "> brief: scope, primary sources, companion chart", ""]
    for h in SPINE_TOP:
        out += [f"## {h}", "", f"{BRIEF} {h}", ""]
    for i, s in enumerate(sections, 1):
        out += [f"## {i}. {s['label']}", "", f"{BRIEF} one or two sentences: what this chapter covers", ""]
        mine = [n for n in order if sec(n) == s["id"]]
        for n in mine:
            if not n.get("group"):
                out += [f"### {n['heading']}", "", f"{BRIEF} {brief(n)}", ""]
        seen = []
        for n in mine:
            if n.get("group") and fork_name(n["group"]) not in seen:
                seen.append(fork_name(n["group"]))
        for f in seen:
            out += [f"### {f}", "", f"{BRIEF} when this fork happens and how it ends", ""]
            for n in mine:
                if n.get("group") and fork_name(n["group"]) == f:
                    out += [f"#### {n['heading']}", "", f"{BRIEF} {brief(n)}", ""]
    for h in SPINE_END:
        out += [f"## {h}", "", f"{BRIEF} {h}", ""]
    return "\n".join(out)


def card_map_order(canvas, cmap):
    """For an existing (non-spec) canvas: the doc outline implied by its
    geometry. Returns [(chapter_label, [(group_label|None, [card ids])])].
    Chapters are the loose cards first, then the top-level groups left to
    right; inside a chapter, cards that sit directly in it come first, then
    each inner group top to bottom. Only cards named in cmap["cards"] count."""
    nodes = canvas["nodes"]
    by_id = {n["id"]: n for n in nodes}
    par = parents(nodes)
    cards = [n for n in nodes if n["id"] in cmap["cards"]]

    def chain(nid):
        out = []
        while par.get(nid):
            nid = par[nid]
            out.append(nid)
        return out[::-1]  # outermost first

    def pos(n):
        return (n["y"], n["x"])

    loose = sorted((c for c in cards if not chain(c["id"])), key=pos)
    chapters = []
    if loose:
        chapters.append((cmap.get("loose_chapter", "Overview"), [(None, [c["id"] for c in loose])]))
    tops = sorted({chain(c["id"])[0] for c in cards if chain(c["id"])},
                  key=lambda g: (by_id[g]["x"], by_id[g]["y"]))
    for t in tops:
        mine = [c for c in cards if chain(c["id"])[:1] == [t]]
        direct = sorted((c for c in mine if len(chain(c["id"])) == 1), key=pos)
        inner = sorted({chain(c["id"])[1] for c in mine if len(chain(c["id"])) > 1},
                       key=lambda g: pos(by_id[g]))
        parts = [(None, [c["id"] for c in direct])] if direct else []
        for g in inner:
            ids = sorted((c for c in mine if chain(c["id"])[1:2] == [g]), key=pos)
            glabel = by_id[g].get("label", g)
            parts.append((cmap.get("group_headings", {}).get(glabel, glabel), [c["id"] for c in ids]))
        label = cmap.get("chapters", {}).get(t) or by_id[t].get("label", t)
        chapters.append((label, parts))
    return chapters


def skeleton_canvas(canvas, cmap, title):
    """Existing canvas + card map -> companion doc skeleton (same spine as
    `skeleton`): `## N.` per chapter, `###` per inner group, a card heading
    per mapped card (`###` directly in a chapter, `####` in a group)."""
    by_id = {n["id"]: n for n in canvas["nodes"]}
    out = [f"# {title}", "", f"{BRIEF} scope, primary sources, companion chart", ""]
    for h in SPINE_TOP:
        out += [f"## {h}", "", f"{BRIEF} {h}", ""]
    for i, (label, parts) in enumerate(card_map_order(canvas, cmap), 1):
        out += [f"## {i}. {label}", "", f"{BRIEF} one or two sentences: what this chapter covers", ""]
        for glabel, ids in parts:
            if glabel is not None:
                out += [f"### {glabel}", "", f"{BRIEF} what this group of cards shows", ""]
            lvl = "####" if glabel is not None else "###"
            for cid in ids:
                text = " ".join((by_id[cid].get("text") or "").replace("**", "").split("\n"))
                out += [f"{lvl} {cmap['cards'][cid]}", "", f"{BRIEF} {text}", ""]
    for h in SPINE_END:
        out += [f"## {h}", "", f"{BRIEF} {h}", ""]
    return "\n".join(out)


def to_notes(canvas, cmap):
    """Turn every text card named in cmap["cards"] into a note card on
    cmap["doc"]; position, size and colour are kept, edges are untouched."""
    changed = 0
    for n in canvas["nodes"]:
        h = cmap["cards"].get(n["id"])
        if h is None or n["type"] != "text":
            continue
        n.pop("text")
        n["type"] = "file"
        n["file"] = cmap["doc"]
        n["subpath"] = "#" + h
        changed += 1
    return changed


def md_headings(text):
    hs, fence = [], False
    for line in text.splitlines():
        if line.startswith("```"):
            fence = not fence
        m = None if fence else re.match(r"^(#{1,6}) (.+?)\s*$", line)
        if m:
            hs.append((len(m.group(1)), m.group(2)))
    return hs


def doc_check(canvas, doc_path, md):
    """Every note card's subpath names exactly one heading of the doc, card
    headings use HEADING_OK characters, no `> brief:` placeholder is left, and
    no card heading in the doc is missing from the canvas."""
    problems = []
    hs = md_headings(md)
    texts = [h for _, h in hs]
    used = set()
    for n in canvas["nodes"]:
        if n["type"] != "file" or n.get("file") != doc_path:
            continue
        sub = (n.get("subpath") or "").lstrip("#")
        used.add(sub)
        if not sub:
            problems.append(f"{n['id']}: note card without a heading subpath")
        elif texts.count(sub) != 1:
            problems.append(f"{n['id']}: heading {sub!r} occurs {texts.count(sub)} times in the doc")
        if sub and not HEADING_OK.match(sub):
            problems.append(f"{n['id']}: heading {sub!r} has characters outside {HEADING_OK.pattern}")
    in_chapter = False
    for i, (level, h) in enumerate(hs):
        is_fork = level == 3 and i + 1 < len(hs) and hs[i + 1][0] == 4
        if level == 2:
            in_chapter = bool(re.match(r"^\d+\. ", h))
        elif in_chapter and h not in used and not is_fork and level >= 3:
            problems.append(f"doc heading {h!r} (level {level}) is not on the canvas")
    for i, line in enumerate(md.splitlines(), 1):
        if line.startswith(BRIEF):
            problems.append(f"doc line {i}: placeholder left: {line[:60]}")
    return problems


def main(argv):
    sys.stdout.reconfigure(encoding="utf-8")
    if len(argv) < 2:
        raise SystemExit(__doc__)
    cmd = argv[1]
    if cmd == "build" and len(argv) == 4:
        canvas = build(load(argv[2]))
        save(argv[3], canvas)
        probs = check(canvas, "timeline")
        print(f"wrote {argv[3]}: {len(canvas['nodes'])} nodes, {len(canvas['edges'])} edges")
        for p in probs + label_warnings(canvas):
            print("  " + p)
        return 1 if probs else 0
    if cmd == "check" and len(argv) >= 3:
        profile = "basic"
        if "--profile" in argv:
            profile = argv[argv.index("--profile") + 1]
        probs = check(load(argv[2]), profile)
        for p in probs:
            print(p)
        print(f"{len(probs)} problem(s)")
        return 1 if probs else 0
    if cmd == "skeleton" and len(argv) == 5:
        with open(argv[3], "w", encoding="utf-8", newline="\n") as f:
            f.write(skeleton(load(argv[2]), argv[4]))
        print(f"wrote {argv[3]}")
        return 0
    if cmd == "skeleton-canvas" and len(argv) == 6:
        with open(argv[4], "w", encoding="utf-8", newline="\n") as f:
            f.write(skeleton_canvas(load(argv[2]), load(argv[3]), argv[5]))
        print(f"wrote {argv[4]}")
        return 0
    if cmd == "notes" and len(argv) >= 4:
        canvas, cmap = load(argv[2]), load(argv[3])
        bad = [h for h in cmap["cards"].values() if not HEADING_OK.match(h)]
        dup = {h for h in cmap["cards"].values() if list(cmap["cards"].values()).count(h) > 1}
        if bad or dup:
            raise SystemExit(f"bad headings: {bad}  duplicates: {sorted(dup)}")
        n = to_notes(canvas, cmap)
        print(f"{n} text card(s) -> note cards on {cmap['doc']}")
        if "--write" in argv:
            save(argv[2], canvas)
            print("written")
        return 0
    if cmd == "doc-check" and len(argv) == 4:
        canvas = load(argv[2])
        doc = argv[3].replace("\\", "/")
        with open(doc, encoding="utf-8") as f:
            md = f.read()
        # The canvas names the doc by vault path; accept the doc at any path
        # (e.g. a scratch copy) as long as one referenced .md matches it.
        notes = sorted({n.get("file") for n in canvas["nodes"]
                        if n["type"] == "file" and str(n.get("file", "")).endswith(".md")})
        vault_path = next((p for p in notes if doc.endswith(p)), None) or next(
            (p for p in notes if p.rsplit("/", 1)[-1] == doc.rsplit("/", 1)[-1]), None) or (
            notes[0] if len(notes) == 1 else doc)
        probs = doc_check(canvas, vault_path, md)
        for p in probs:
            print(p)
        print(f"{len(probs)} problem(s)")
        return 1 if probs else 0
    if cmd == "extents" and len(argv) >= 3:
        extents(load(argv[2]), int(argv[argv.index("--tile") + 1]) if "--tile" in argv else 4800)
        return 0
    if cmd == "fit-groups" and len(argv) >= 3:
        canvas = load(argv[2])
        changed = fit_groups(canvas["nodes"])
        for gid, old, new in changed:
            print(f"{gid}: {old} -> {new}")
        if "--write" in argv and changed:
            save(argv[2], canvas)
            print("written")
        return 0
    raise SystemExit(__doc__)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
