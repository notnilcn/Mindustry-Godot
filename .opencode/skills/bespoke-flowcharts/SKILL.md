---
name: bespoke-flowcharts
description: Hand-design Obsidian .canvas flowcharts for Khvor' Gates docs, laid out as a left-to-right timeline across swim-lane groups (Server Table, Relay, Player Client 1, Player Client 2, …), with a white main path and coloured ancillary forks, 480×480 cards on a 240 grid, 60-unit group padding, advanced-canvas bezier edges, and a companion TDD-style .md doc whose headings the canvas cards embed; the doc is written by parallel subagents from the generated TDD docs and checked against the code by verifier subagents. Use when asked to write, redo, tidy or review a bespoke (not plugin-generated) flowchart canvas, an end-to-end sequence chart and its companion doc, or the group spacing of an existing canvas.
---

# Bespoke flowcharts

A bespoke flowchart is a `.canvas` file written for one question ("what happens,
in order, when two players kill a Gorgon?"), as opposed to the generated
`flowcharts/**` canvases that the vault plugins regenerate from code. It lives
beside the doc it explains (for example `docs/HUMAN/_TDD/<topic>/`) and is linked
from that doc's `## Flowcharts` list.

A timeline chart comes as a **pair**: `<Name>.canvas` and a companion
`<Name>.md` in the same folder. The canvas cards are **note cards**: `file`
nodes that embed one heading of the companion doc (`"subpath": "#<heading>"`),
so the card and the doc can never disagree and every claim has room for links.
Only phase/sub-chart embeds (`file` nodes pointing at other canvases) and,
rarely, a short caption stay as text or other files.

These charts are produced **after** the generated TDD docs (`docs/HUMAN/_TDD/00–22`
and the topic docs beside them) exist, so the doc is written from those docs and
then checked against the code (see [Multi-agent workflow](#multi-agent-workflow)).

Before you start, read:

1. `.opencode/skills/json-canvas/SKILL.md` — the JSON Canvas 1.0 format (node and
   edge fields, `\n` pitfall, z-order = array order).
2. `.opencode/skills/obsidian-cli/SKILL.md` — the `obsidian` CLI you use to open,
   screenshot and debug the result in the running Obsidian.
3. `docs/HUMAN/_TDD/AGENTS.md` if the chart belongs to a TDD doc (link style,
   current-code-only rule, proprietary `server/` rule).

The vault root is the repo root; every `file` node path is vault-relative with
forward slashes. Where the generic json-canvas skill disagrees with this one
(16-hex ids, 50–100 px spacing, 20–50 px group padding), **this skill wins**:
readable ids such as `s-spawn` are fine, and the spacing rules are below.

## House rules

These are the user's rules. `scripts/canvas_tool.py check --profile timeline`
enforces every rule a machine can see; the rest are on you.

| Rule | Value |
| --- | --- |
| Card size (text and file nodes) | exactly `480 × 480` |
| Card positions | multiples of `240` (per timeline section, see below), so every gap between cards is a multiple of 240 (groups excepted) |
| Group padding | a group is **exactly** its children's bounding box grown by `60` on every side (nested groups count as children) |
| Timeline direction | time runs **left → right** |
| Primary groups ("lanes") | stacked top to bottom, colour `#c0c0c0` (192,192,192), labelled with the actor, e.g. `Server Table`, `Relay`, `Player Client 1`, `Player Client 2` |
| Timeline sections | groups that span **all** lanes vertically and one stretch of time horizontally (e.g. `Spawning and observing`); colour `#c0c0c0`; pad everything they wrap (cards, fork groups, and vertically the lane stack) by exactly `240`; neighbouring sections are exactly `720` apart |
| Main path | the "everything goes right" story; its cards and the edges between them are `#ffffff` |
| Forks | ancillary branches off the main path; any colour except white and `#c0c0c0`; one colour per fork family |
| Edges inside one lane | `fromSide: "right"` → `toSide: "left"` |
| Edges between lanes | `fromSide: "bottom"` → `toSide: "top"` (also when the target lane is above) |
| Every edge | `"styleAttributes": {"bezierCurvature": "5"}`, `toEnd: "arrow"` |

`bezierCurvature` is the advanced-canvas edge style attribute (see
`BUILTIN_EDGE_STYLE_ATTRIBUTES` in `.obsidian/plugins/advanced-canvas/main.js`):
values `"1.5"`, `"2"`, `"3"`, `"5"`; it scales the bezier control points and only
applies to edges without a `pathfindingMethod`. Keep the canvas `metadata`
block (`{"version":"1.0-1.0","frontmatter":{}}`) that advanced-canvas writes.

## The tool

`scripts/canvas_tool.py` (Python 3, stdlib only):

```bash
python .claude/skills/bespoke-flowcharts/scripts/canvas_tool.py build spec.json out.canvas
python .claude/skills/bespoke-flowcharts/scripts/canvas_tool.py check out.canvas --profile timeline
python .claude/skills/bespoke-flowcharts/scripts/canvas_tool.py fit-groups some.canvas --write
python .claude/skills/bespoke-flowcharts/scripts/canvas_tool.py extents out.canvas [--tile 4800]
python .claude/skills/bespoke-flowcharts/scripts/canvas_tool.py skeleton spec.json out.md "<Doc title>"
python .claude/skills/bespoke-flowcharts/scripts/canvas_tool.py doc-check out.canvas out.md
```

- `skeleton` writes the companion doc's skeleton from the spec: the TDD
  hand-written spine, one `## N. <section label>` per timeline section, one
  `### <heading>` per main-path or loose card, one `### <fork>` per fork (its
  cards as `####`), each followed by a `> brief: …` placeholder holding the
  card's `text`. Writers replace every placeholder.
- `skeleton-canvas` and `notes` do the same for a canvas that was not built
  from a spec (see "A companion doc for an existing canvas").
- `doc-check` checks the pair: every note card's heading exists exactly once,
  uses only `A-Za-z0-9 _,.()+-`, every card heading inside a numbered chapter is
  on the canvas, and no `> brief:` placeholder is left.

- `extents` prints the canvas bounds, each lane's y band and one ready-to-run
  `obsidian eval … zoomToBbox(…)` command per horizontal tile (see Verify).
- `build` also warns about edge labels that are too long for their gap.
- `build` writes only `nodes`, `edges` and `metadata`; Obsidian re-adds keys
  such as `_seq` and may re-save the file in its own multi-line format when it
  is open. Compare canvases as parsed JSON, not as text.

- `build` turns a compact spec into a canvas: it converts grid units to pixels,
  stacks the lanes, fits every group to 60 padding, picks each edge's sides from
  the lanes, colours main-path edges white and adds `bezierCurvature`. It then
  runs `check` and prints any problem; exit 1 if there is one.
- `check` validates any canvas (`--profile basic` for a non-timeline chart:
  ids, dangling edges, sizes, grid, overlaps, group padding, groups cutting
  through cards; `--profile timeline` adds lanes, edge sides and curvature).
- `fit-groups` re-fits the groups of an existing canvas (hand-written or
  agent-written) to 60 padding, innermost first, keeping membership from the
  original geometry. Dry run without `--write`. Use it to fix "the groups are too
  loose / overlap" complaints; it changes only group `x/y/width/height`.

Always **write the spec, not the canvas**. Keep the spec in your scratchpad (or
next to the canvas as `<name>.spec.json` only if the user wants it kept) and
regenerate after every change. Hand edits to the `.canvas` are lost on rebuild.

### Spec format

```json
{
  "doc": "docs/HUMAN/_TDD/<topic>/<Name>.md",
  "lane_gap": 2,
  "align_lanes": true,
  "lanes": [
    {"id": "L-server", "label": "Server Table"},
    {"id": "L-relay",  "label": "Relay"},
    {"id": "L-p1",     "label": "Player Client 1"},
    {"id": "L-p2",     "label": "Player Client 2"}
  ],
  "sections": [
    {"id": "S-spawn",  "label": "Spawning and observing", "until_gx": 3},
    {"id": "S-engage", "label": "Engagement",             "until_gx": 9}
  ],
  "groups": [
    {"id": "G-dc", "label": "Fork · P2 disconnects", "fork": "Fork - P2 disconnects",
     "color": "1", "lane": "L-server"},
    {"id": "G-dc-in", "label": "presence pass", "color": "1", "parent": "G-dc"}
  ],
  "nodes": [
    {"id": "s-spawn", "lane": "L-server", "gx": 0, "gy": 0, "main": true,
     "heading": "Server spawns the Gorgon",
     "text": "**admin_spawn_enemy**\n`enemy` + `enemy_health` inserted"},
    {"id": "p1-see", "lane": "L-p1", "gx": 3, "gy": 0, "main": true, "text": "..."},
    {"id": "s-dc", "lane": "L-server", "gx": 6, "gy": 3, "group": "G-dc-in",
     "color": "1", "text": "..."},
    {"id": "s-ph1", "lane": "L-server", "gx": 9, "gy": 0, "main": true,
     "file": "docs/HUMAN/_TDD/Enemies and bullets/Gorgon Phase 1 - Tentacles and Shotguns.canvas"}
  ],
  "edges": [
    {"from": "s-spawn", "to": "p1-see"},
    {"from": "s-dc", "to": "p1-see", "label": "optional label", "color": "1"}
  ]
}
```

- `gx`, `gy` are **grid units of 240**; a card is 2 × 2 units. `gy` counts from
  the top of the card's own lane (every lane starts at `gy 0`); `build` stacks the
  lanes `lane_gap` units apart (default 2 — enough for two nesting levels of
  padding plus labels; raise it if `check` reports lane overlap).
- Leave at least 1 free unit between cards (`gx` steps of 3 for neighbours) and
  1 more on each side of a group boundary (cards of two sibling groups ≥ 2 units
  apart, a card and a sibling group ≥ 2 units apart), or the 60-unit padding
  collides.
- `main: true` makes a card white and every edge between two main cards white.
  Other edges take the fork card's colour (the source, or the target when the
  source is on the main path) unless `color` is given.
- A node in a group names the **innermost** group; a group names its `lane` (top
  level) or its `parent`. Groups are drawn below cards (z-order). A group lives in
  **one** lane, so a fork whose cards span lanes (server + client) is one group
  per lane with the same colour and a `(server)` / `(P1)` suffix on the label.
- `align_lanes: true` widens every lane to the same left/right edge so the bands
  read as rows. That is the one allowed exception to exact padding (horizontal
  only, lanes only); `check --profile timeline` accepts it.
- `heading` makes the card a note card: a `file` node on `doc` with
  `subpath "#<heading>"`. Its `text` is then only the **brief** that `skeleton`
  copies into the doc for the writer; `build` ignores it. Headings are unique,
  short, plain (`A-Za-z0-9 _,.()+-`: no backticks, colons, `#`, `|`, `·`, `→`)
  and name the beat with its actor (`Server validates the taken hit`,
  `P2 keeps a phantom arrow`).
- Two cards may share one `heading` when they show the same beat in two lanes
  (`Both clients receive the tracks` on P1's and P2's card, `Phase 3 at the
  players`). `skeleton` writes the section once, at the first card; write it
  for both lanes and start such headings with `Both clients`.
- `fork` on a group is the doc heading for that fork (`Fork - <what happens>`).
  Groups of the same fork in different lanes share one `fork` value so the doc
  has one `###` for it.
- Text cards are Markdown; use `\n` in the JSON string, never `\\n`.
- `sections` (optional, in time order): a card belongs to the first section
  whose `until_gx` ≥ its `gx`, unless the card sets `"section": "<id>"` (use
  that for a fork tail that runs past the next section's first column). `build`
  then **slides each section sideways** so the section boxes are exactly 720
  apart, so the `gx` values only order cards inside a section; the gap you leave
  between sections in the spec doesn't matter. A fork group must sit in one
  section (`build` errors with "straddles sections" otherwise: move its cards or
  set `section` on them).
- **Grid offset caused by sections.** A section's edge is 240 out from its
  outermost item, and that item is often a fork group (60 outside its cards),
  so the 720 gap can only be exact if a whole section slides by 60/120/180.
  Cards stay on the 240 grid *within* a section (that's what `check` tests),
  but the card spacing *across* a section boundary is 1200 plus 0–240. If the
  user prefers global grid alignment over the exact 720 gap, that's a one-line
  change in `build` (round the shift up to a multiple of 240).

## Designing a timeline

1. **Research first, lay out second.** List every event in order, with the actor
   that performs it (which lane), what it writes/sends, and what the next actor
   observes. Read the source docs and verify names in code; a chart is not
   allowed to invent a reducer, table or constant.
2. **Sections are chapters.** Split the timeline into 5–8 sections a reader
   would name ("Spawning and observing", "The boss fires and a player is hit",
   "A player hits the boss", "Phases 1 to 9", "Death and cleanup"). Each
   section holds the main-path beats of that chapter **and** the forks that
   branch from them.
3. **One column per beat.** A beat is one actor doing one thing. When work moves
   to another lane (a reducer writes a row → a client sees it), the receiving card
   is in a **later** column, so cross-lane edges always move right. Two lanes
   reacting to the same event at the same time (both clients see the boss) share a
   column.
4. **Every input is an edge.** Draw an edge into a card from *every* card whose
   output it uses, not only from the previous beat: the phase schedule (a phase
   canvas embed) feeds each client's firing cards *and* the server card that
   re-derives a claimed bullet; the `ShotRec` card feeds the server's dealt-hit
   check; the track card feeds aim and the contact bodies. A reader following
   only the edges must be able to answer "where does this card's data come
   from?". A long same-lane edge is fine; a missing one is a bug.
   Granularity: one card per function-sized step (receive the row, run the
   schedule, aim, expand, step, build bodies, decide, hold, report, check the
   batch, validate, apply), not one card per subsystem.
5. **Main path on the top row of each lane** (`gy 0`), white, readable alone from
   left to right: start event → … → end event. It must cover the major systems a
   reader asks about, not only the happy-path skeleton.
6. **Forks below the main path** (`gy 3`, `gy 6`, …) in the lane of the actor that
   branches, wrapped in a coloured group labelled `Fork · <what happens>`. A fork
   starts with an edge from the main-path card where it can happen, and either
   rejoins the main path (edge back to a later main card) or ends in a terminal
   card that says how it ends. Keep each fork short (2–6 cards); a family of
   related forks shares a colour.
   A context card that is not a branch (e.g. "before t0, the relay routed both
   players in") is allowed: fork-coloured, in its own group, no incoming edge.
7. **Detail vs. summary.** Show a mechanism in detail once (the first time it
   happens) and summarise repeats; embed other canvases as 480×480 `file` cards
   instead of copying them, captioned by the edge label into them or by one
   caption card for the whole run.
8. **Card text.** A bold title line naming the thing (`**report_player_hits**`),
   then 3–7 short lines: what happens, the exact identifier (`table`,
   `file.rs#Symbol`, constant = value), and why. 480×480 holds about 10 short
   lines at the default text size; split rather than overflow.
9. **Edge labels** only where the edge carries something specific (a reducer
   call, a view, a tick lead such as `+60 t`). A label in a 240 gap fits about
   12 characters; longer labels cover the cards (`build` warns).
10. **Routing with `bezierCurvature: "5"`.** The curves bulge far out of the
   straight line, so plan for them:
   - A cross-lane edge leaves a card's **bottom** and drops through everything
     below it in that lane. Don't park a fork card directly under a main card
     that sends a cross-lane edge; shift the fork a column or two sideways.
   - An edge into a lane **above** still leaves the bottom and enters the top, so
     it loops back on itself. That is the rule, not a bug; keep the two cards in
     nearby columns so the loop stays small.
   - A same-lane edge to a fork card one row down and one column right makes a
     tight S-loop; put a fork's first card at least 3 units right of its origin.
   - Look for edges running through cards they don't connect to in the
     screenshots (`check` cannot see curves) and move cards by whole grid units.
11. **Sibling spacing.** Cards in two sibling groups (or a card and a sibling
   group) need ≥ 2 free units between them, or the 60 padding collides; `check`
   reports this as an overlap or padding error.

## Companion doc

The doc is a hand-written TDD topic doc: follow `docs/HUMAN/_TDD/AGENTS.md`
(spine, link style, current code only, server code linked never pasted, the
verification checklist). `skeleton` produces its exact shape:

```text
# <Name>
<opening block: scope, primary sources, companion chart>
## Assumed knowledge / ## The 30-second version / ## Find it fast / ## Flowcharts
## 1. <section label>            one per timeline section, in time order
<1–3 sentence chapter intro>
### <main-path card heading>    one per main card, in chart order (column, lane)
### Fork - <what happens>       one per fork: when it happens, how it ends
#### <fork card heading>        one per fork card
...
## Source map / ## Known gaps / stubs / ## Where to go next
```

A card section is what a 480×480 card shows, so it must read on its own:

- **Roughly 60–120 words, soft**: a dense causal paragraph ("X does Y *because*
  Z"), then at most a 2–5 item list (fields, refusal codes, numbers). Shorter is
  fine, and a card may run past 120 when it genuinely needs to (the `Enemies and
  bullets` docs often do) — never cut real behaviour to hit the range. No tables,
  no fenced code, no images, no headings of its own.
- Every table, view, reducer, function, constant and `Kg*` class is a wikilink
  (`[[server/modules/region/src/enemy/reducers.rs#report_player_hits|report_player_hits]]`)
  the first time it appears in **that section** (each card is read alone).
- Numbers carry their unit and source constant (`EVENT_LEAD_TICKS` (12 ticks)).
- A card for the second player that mirrors the first says what is the same in
  one sentence and spends the rest on what differs.
- A fork's `###` intro says which main-path beat it branches from and which beat
  it rejoins (or how it ends), linking the doc headings (`[[#P1 reports the hit]]`).
- Never edit a card heading in the doc without changing the spec's `heading`
  (and rebuilding); `doc-check` catches drift.
- Link the pair both ways: the doc's `## Flowcharts` lists the canvas, and the
  source topic docs' `## Flowcharts` may list it too.

### A companion doc for an existing canvas

Some charts were not built from a spec (hand- or agent-made canvases, such as
the `Gorgon Phase N` charts). Give them a doc without rebuilding them:

1. Write a **card map** `map.json`:
   `{"doc": "<vault path>.md", "loose_chapter": "The phase timeline",
   "chapters": {"<top group id>": "Server tables"},
   "group_headings": {"<group label>": "<doc heading>"},
   "cards": {"<text node id>": "<heading>"}}`. Leave tiny caption cards (a
   one-word label under a file embed) out of `cards`; they stay text. When many
   canvases share a layout, generate the maps with a short script keyed on each
   card's first line and group label, so shared cards get the same heading in
   every doc. Avoid a card heading that differs from its group heading only by
   case.
2. `skeleton-canvas <canvas> <map.json> <out.md> "<title>"` builds the skeleton
   from the canvas geometry: loose cards first as chapter 1, then each top-level
   group left to right as a chapter, each inner group (top to bottom) as `###`,
   its cards as `####`. Briefs are the cards' current text.
3. Run the same write → assemble → verify steps as below.
4. `notes <canvas> <map.json> --write` turns the mapped text cards into note
   cards in place (position, size, colour and edges unchanged), then
   `doc-check` the pair.

Card maps used so far live in `examples/` (`gorgon-phase-maps/phase-N.json`).

When several docs share systems (one doc per boss phase), keep the shared-system
cards **short and phase-flavoured** (what the step means for *this* doc's
values, 40–90 words) and link one fuller explanation, such as the matching
section of an end-to-end doc, instead of repeating it in every doc. Give each
writer 2–3 docs and each verifier 3–4; one agent per doc is not needed.

## Multi-agent workflow

The orchestrator (you) owns the spec, the canvas, the spine of the doc and the
final assembly. Subagents write and verify the chapters. Only the orchestrator
writes the real `.canvas` and `.md`; subagents write part files in the
orchestrator's scratchpad.

1. **Outline (orchestrator).** Read the generated TDD docs for the topic (the
   numbered docs and any topic folder beside them) and the existing charts
   they link. Design the timeline (sections, main beats, forks), write the spec
   with a `heading` and a `text` brief per card, `build` it, and run `skeleton`
   into the scratchpad. Split the chapters into 3–5 writer jobs of similar card
   counts (keep a chapter whole; a job may hold non-adjacent chapters that share
   a subsystem).
2. **Write (parallel subagents, one per job).** Brief each writer with: the
   skill path (this file, "Companion doc"), `docs/HUMAN/_TDD/AGENTS.md`, its
   chapters' skeleton text copied from the skeleton file, the list of TDD docs
   to work from, the part-file path (`<scratch>/ch<N>.md`, one per chapter,
   starting at its `## N.` heading), and the rule "headings verbatim, replace
   every `> brief:`". Writers work **from the TDD docs**; they grep the code only
   to find the exact file and symbol for a link, not to re-derive behaviour.
   They report claims the docs did not support instead of inventing them.
3. **Assemble (orchestrator).** Write the spine (opening block, Assumed knowledge,
   30-second version, Find it fast, Flowcharts, Source map, Known gaps / stubs,
   Where to go next) from the chapters, concatenate into `<Name>.md`, and run
   `doc-check`.
4. **Verify against the code (parallel verifier subagents, 2–3).** Each owns a
   disjoint set of part files (one also owns the spine) and acts as a code
   reviewer: every link resolves and its symbol exists in that file; every
   behaviour, guard order, constant value, tick lead, refusal code and client
   reader trace matches the code **today**; nothing from `server/` is pasted.
   Verifiers fix the part files in place, keep headings verbatim, and treat the
   60–120 word range as a soft guide — never cut real content to hit it. They
   report each change (claim → what the code says → file:line). They do not read
   git history. Where the code and a TDD doc disagree, the code wins; they list
   those so the orchestrator can tell the user which TDD doc is stale.
5. **Finish (orchestrator).** Re-assemble, `doc-check`, `check --profile
   timeline`, open both files in Obsidian and screenshot a few tiles zoomed so
   the note cards are readable. Report the verifiers' corrections and the stale
   TDD docs.

## Verify

1. `check --profile timeline` prints `0 problem(s)`.
2. Look at it in Obsidian (must be running; vault `g`). Use **PowerShell** — under
   Git Bash `obsidian dev:screenshot` exits 127:

   ```powershell
   obsidian open path="docs/HUMAN/_TDD/<topic>/<Name>.canvas"
   obsidian command id=advanced-canvas:zoom-to-fit
   obsidian dev:screenshot path=build/<name>.png
   obsidian dev:errors
   ```

   Then read `build/<name>.png`. Zoom-to-fit stops at Obsidian's minimum zoom,
   so a wide chart (> ~10 000 px) shows only its middle. Run `extents` and paste
   its per-tile commands, one screenshot each:

   ```powershell
   obsidian eval code="app.workspace.activeLeaf.view.canvas.zoomToBbox({minX:-180,minY:-120,maxX:4740,maxY:8580}); 'ok'"
   Start-Sleep -Milliseconds 800
   obsidian dev:screenshot path=build/<name>-tile0.png
   ```

   Obsidian keeps an already-open canvas in memory, so after a rebuild close
   its tabs first (`obsidian eval code="app.workspace.getLeavesOfType('canvas').filter(l=>l.view.file?.path.includes('<Name>')).forEach(l=>l.detach()); 'ok'"`)
   and reopen it. Files written in bulk from outside Obsidian can be missed by
   its file watcher: if a note card says `"…#Heading" could not be found` or shows
   the doc from its first line, run `obsidian reload`, wait for the metadata
   cache (`app.metadataCache.getFileCache(file).headings`), then reopen the
   canvas. Zoomed out, note cards show only a big `<doc> › <heading>`
   label; that is Obsidian's low-zoom placeholder, not a broken embed.

   Check the shape in the tiles (lanes stacked, columns aligned, forks below,
   no edge through an unrelated card), then zoom to a few single cards
   (a bbox ~1500 wide) to check that the text fits.
3. Every `file` card path exists (`obsidian file path="…"` or `ls`).
4. Report the node/edge counts, the fork list and anything you could not verify.

## Tidying an existing canvas

For "fix the group spacing" requests: `fit-groups <file> --write`, then
`check --profile basic` (or `timeline` if it is a lane chart). If `check` then
reports a group partially overlapping a card, the groups were holding cards that
belong elsewhere: move those cards by a multiple of 240 rather than widening the
group. Confirm with a semantic diff that only group geometry changed.

## Worked example

`examples/gorgon-duo.spec.json` is the spec behind
`docs/HUMAN/_TDD/Enemies and bullets/Gorgon Duo Fight - End to End.canvas`
(4 lanes, 7 timeline sections, 137 cards, 172 edges, 28 fork/context groups,
10 phase embeds; mirrored client beats share a heading). It is generated by
`examples/gorgon-duo.gen.py <out.spec.json>`: edit the generator (cards,
groups, edges in time order) and rebuild rather than hand-editing the spec.
