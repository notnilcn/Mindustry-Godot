<!-- SPDX-License-Identifier: GPL-3.0-only -->
# G05 — Campaign UI / meta screens (execution plan)

Date: 2026-10-13.

This plan is self-contained. Read `docs/parity-plans/00-environment.md` first
(paths `core/`, `gdext/`, `ui/`, `scenes/`, `J/`, `ARC/`, the `$G $H $I $C $MC`
gmcp shorthands, build/test/golden rules, commit rules). Work the tasks in the
§3 order. Each task is one commit. If the code you find differs from what a task
says (a function is missing, a line moved by more than a search can fix, a value
differs), stop that task, write down what you saw, and continue with the next
independent task. Never improvise a design.

Precondition line: "Start state: `main` contains the G00 integration merge
(`git log main --oneline | grep -m1 'G00: integrate'` prints a line)".

Area: the dialogs a player uses while playing the campaign: research tree,
planet page (sector panel, globe, rename/icon/stats), content info (`?`) with
its stats, core database, hints, schematics browser, launch-loadout cosmetics,
dialog stacking/confirm infrastructure, generated (non-preset) sectors
(PAR-0112).

Port line numbers were read in the integration tree on 2026-10-13; if a line
moved, `grep -n` the quoted symbol.

---

## 1. Inventory

### 1.1 Java file -> port -> status

| Java (`J/…`) | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `ui/dialogs/ResearchDialog.java` (815 l.) | `ui/dialogs/research_dialog.gd` (169 l.) | **partial / wrong shape** | Port = root rail + flat row list (`_rebuild_nodes`, l.91); no graph, no connectors, no pan/zoom, no hover popup, no items display, no Database button, toast instead of `uiUnlock` sound | T1a-T1c |
| `ui/layout/TreeLayout.java`, `BranchTreeLayout.java` | `ui/layout/tree_layout.gd`, `branch_tree_layout.gd` | **stub** | `layout_entries` is a cursor row (`tree_layout.gd:20`); `branch_positions` is depth rows sorted by name (`branch_tree_layout.gd:40`), not the abego walk | T1a (Rust port) |
| `ui/layout/RowTreeLayout.java`, `RadialTreeLayout.java` | `ui/layout/row_tree_layout.gd`, `radial_tree_layout.gd` | stubs, unused by Java campaign UI | ResearchDialog only uses BranchTreeLayout | deferred (unused) |
| `content/TechTree.java` (requirements, `req-*`) | `core/content/tech/mod.rs` `materialize_requirements` (l.590) | **ported & wired** (commit `2fe5800`) | `ResearchView.requirements/finished` filled; root per row fixed | PAR-0160 verify in T1a |
| `ResearchDialog.View.canSpend` | `core/game/tech_tree.rs:116` `can_spend` | **wrong** | port: `all(finished>=req || items>0)`; Java: any spendable item, else `content.locked()` (always true for a locked node) => partial payment allowed when one item is missing | T1a |
| `ResearchDialog.rebuildItems.add` (percent removal) | `gdext/campaign.rs:1071` `research` | **wrong** | port drains sectors greedily in order; Java removes `ceil(percentage*available)` from each non-current sector, current last | T1a |
| `ui/ItemsDisplay.java` | `ui/widgets/items_display.gd` | **partial** | port: a bare HBox of chips; Java: `Tex.button` table, `@globalitems` collapser, rows `amount icon name` in item-id order, shine | T1c |
| `ui/dialogs/BaseDialog.java` + `ARC/arc-core/src/arc/scene/ui/Dialog.java` + `ui/Styles.java:458-469` | `ui/mind_dialog.gd`, `scenes/ui/dialogs/mind_dialog_base.tscn` (Dim 0.55) | **wrong** | every Java BaseDialog uses `defaultDialog` (`stageBackground = black9`, 0.9); only `PlanetDialog` uses `fullDialog` (`black`); show/hide fade 0.1 s (`core/UI.java:132-133`) | T2 |
| Dialog stacking (Arc keeps lower dialogs drawn) | `gdext/ui/ui_host.rs:187` `open_dialog` hides previous unless `keep_previous` | **wrong** (PAR-0149) | `previous.call_deferred("hide_dialog")` | T2 |
| `core/UI.java:574-600` `showConfirm`, `:305-357` `showTextInput`, `:466` `showInfo` | `ui/ui_root.gd:413` `_on_show_confirm` etc. | **wrong** (PAR-0150) | no title, hard-coded "Yes"/"No"/"OK"/"Cancel", 0.55 dim, no accent rule | T2 |
| `ui/dialogs/PlanetDialog.java` `updateSelected` (1184-1354) | `ui/dialogs/planet_dialog.gd:946` `_build_sector_panel` | **partial** | missing: locked objectives list, `@sectors.nolaunchcandidate`, button gate `canSelect`, `vulnerable` rule (needs `near()` enemy base + `sectorInvasion`), threat colour, findLauncher (port `_find_launcher` l.1171 = first based sector) | T3 |
| `PlanetDialog.findLauncher` + `PlanetGenerator.findLaunchCandidate` + `SerpuloPlanetGenerator.findLaunchCandidate/allowNumberedLaunch` | none (GDScript approximation) | **missing** | `planet_dialog.gd:1171` | T3 |
| `PlanetDialog` drag/scroll/zoom (112-147), `lookAt`, newPresets reveal (270-288, 986-1003) | `ui/planet_view.gd` `focus_tile` (l.322) snaps camera on every select; no drag, no zoom, no reveal | **missing** | grep `drag`/`zoom` in `planet_view.gd` finds nothing | T6 |
| `PlanetDialog` rename pencil + icon picker (1201-1281) | none | **missing** | `_build_sector_panel` title row has neither | T7 |
| `PlanetDialog.showStats` + `displayItems` (1026-1147) + `abandonSectorConfirm` | `planet_dialog.gd:1076` `_show_sector_stats` -> `full_text` text dump | **partial** | no icons, no /min unit, no item-id order, no export destination/import sources, no playtime unless >0, no Abandon button | T8 |
| `PlanetDialog.showOverview` | — | Java itself unimplemented (`//TODO`) | J/ui/dialogs/PlanetDialog.java:319 | none |
| `ui/fragments/HintsFragment.java` | `ui/fragments/hints_fragment.gd` | **partial / wrong** | 5 of 27 `shown` predicates; wrong order (`desktopShoot` before `breaking`); 15 s timer auto-**completes** (persists `-hint-done`, Java never does); gate counts session time not total playtime; never `hide()`s when `shown` turns false | T4a, T4b |
| `ui/dialogs/ContentInfoDialog.java` + `world/meta/{Stat,StatCat,StatUnit,Stats,StatValues}.java` + 80 `setStats()` overrides + 19 `Consume*.display` | `ui/dialogs/content_info_dialog.gd` calls `MindCampaign.content_info_json` which **does not exist** (grep: only the GDScript call site) -> bundle-only fallback; `core/world/stats.rs` (`Stats::from_specs`, wrong `StatCat` set, no producer), `core/ui/stat_display.rs` (partial units, wrong `unit.powersec` key) | **missing** | the `?` dialog shows name/description/details only, never a stat row | T5a-T5e |
| `ui/dialogs/DatabaseDialog.java` | `ui/dialogs/database_dialog.gd` + `core/ui/database.rs` | **partial** | tab row is text (PAR-0144); `shown()` does not select the current planet tab (Java 42-50); no banned cross | T11 |
| `ui/dialogs/SchematicsDialog.java` (+ `SchematicImage`, `SchematicInfoDialog`) | `ui/dialogs/schematics_dialog.gd` | **partial** (PAR-0143) | list+detail layout; no tags row; no card grid; clicking a card does not `useSchematic` in game | T10b |
| `SchematicsDialog.SchematicImage` + `game/Schematics.java:200-290 getBuffer` | none | **missing** | grep `SchematicImage` in `ui/`: none | T10a |
| `ui/dialogs/LaunchLoadoutDialog.java` | `ui/dialogs/launch_loadout_dialog.gd` | ported (T1/T2 of session 5); only 200 px `SchematicImage` toggles missing | `LaunchLoadoutDialog.java:173-190` | T10a |
| `ui/dialogs/LoadoutDialog.java` | `ui/dialogs/loadout_dialog.gd` | ported | session-5 summary | — |
| `ui/dialogs/IconSelectDialog.java` | `ui/dialogs/icon_select_dialog.gd` | partial (no `none` button, no content icons, fixed 8 columns) | file l.28-48 | T7 |
| `ui/dialogs/CampaignRulesDialog.java` | `ui/dialogs/campaign_rules_dialog.gd` | ported | — | — |
| `ui/dialogs/CustomRulesDialog.java` | `ui/dialogs/custom_rules_dialog.gd` (PAR-0132) | not reachable in campaign (Java `PausedDialog` has no rules entry) | J/ui/dialogs/PausedDialog.java | out of area |
| `maps/generators/PlanetGenerator.java:177-196` + `type/Sector.java:255-282 makeRect` | `core/maps/planet/mod.rs:52` `generate_sector` ignores `sector` (`let _ = sector;`), fixed seed 1, 128x128 | **wrong** (PAR-0112) | `gdext/sim_host.rs:1389` | T9a |
| `SerpuloPlanetGenerator.java:719 Schematics.placeLaunchLoadout(spawn)` | `core/maps/planet/serpulo.rs:1333` comment "plan-12 hook (no tile effect)" | **missing** | | T9b |
| `SerpuloPlanetGenerator.java:725-751` + `BaseGenerator.java:42-` enemy base | `core/maps/generators/base.rs` `BaseGenerator::generate` logs only | **missing** | | T9c (cores) ; parts deferred |
| `graphics/g3d/{MeshBuilder.buildHex,HexMesh,PlanetMesh}` + `shaders/planet.vert` + `SerpuloPlanetGenerator.getColor/getEmissiveColor` | `ui/planet_view.gd:481 _build_body` = billboard sprite; `client/shaders/planet.gdshader` exists but unused and lacks `a_emissive`; `serpulo.rs:512 get_color` returns black | **missing** (PAR-0142) | | T12a-T12c |
| `HexSkyMesh` clouds | none | missing; invisible at normal zoom (`alpha = 1 - uiAlpha`, uiAlpha=1 while zoom<1.9) | J/graphics/g3d/HexSkyMesh.java:56-59 | deferred |
| `game/Saves.java:343-352 savePreview` (Load dialog previews) | `ui/dialogs/load_dialog.gd:189` placeholder | missing | needs a save-completed hook in G04's save path | deferred (§3.3) |

### 1.2 Already done / verified (do not redo)

* Research requirements + `req-*` progress materialized, `ResearchView.root`
  per row (commit `2fe5800`). PAR-0160 is fixed by it; T1a only re-verifies.
* Configure Loadout validity / Max / capacity / item rows / schematic core
  (commits `44b0f7b`, `fe9381b`). PAR-0025/0026/0027 only keep the 200 px
  `SchematicImage` toggle cosmetic (T10a). PAR-0162 (user schematics loaded via
  `MindCampaign.load_schematics`, `gdext/campaign.rs:297`) and PAR-0163
  (`launch_dialog_json` carries capacity, `gdext/campaign.rs:2352`) are fixed on
  the dialog side by `44b0f7b`; G04 owns any remaining read-model change.
* Remap notice styling + `MindUi.open_dialog` `keep_previous` (commit `22f800d`).
* `PlanetDialog.canSelect` for look mode is already ported in
  `core/ui/campaign.rs:709 annotate_globe` (`SectorView.selectable`), including
  `allowNumberedLaunch` via a local `PlanetGrid` neighbour list. T3 replaces that
  local list with the campaign's `planet.neighbors` (see T3 change 2).
* Sector list (`rebuildExpand`/`rebuildList`) is ported (`planet_dialog.gd:717`).

---

## 2. Tasks

(Filled in below, in execution order.)

### T1a. Research: Rust tree read model, BranchTreeLayout, Java `canSpend`, Java item removal (PAR-0145 data half, PAR-0047, PAR-0160 verify)

**Symptom / acceptance.** Today the research dialog is a list; the data it gets
(`ResearchView`) has no parent/children links, no layout, no objective text
(`format!("{spec:?}")`), and the spend gate refuses partial payments. After:
`MindCampaign.research_tree_json(root_name)` returns the selected tree with
Java-identical node positions, per-node lock/selectable/canSpend state,
requirement rows and the global item pool; `research()` accepts a partial
payment when at least one item is missing (Java behaviour) and removes items
from sectors by percentage.
Acceptance: unit tests below pass; in-engine `research_tree_json("serpulo")`
returns `nodes.size() == 223` (serpulo tree) with `nodes[0].x == 0`.

**Java.**
* `ResearchDialog.java:332-372 treeLayout()`:
  ```
  spacing = 20; node = LayoutNode(root)            // width = height = nodeSize = 60 for every node
  children = node.children
  leftHalf  = children[0 .. ceil(n/2))   rightHalf = children[ceil(n/2) .. n)
  node.children = leftHalf
  BranchTreeLayout{gapBetweenLevels = gapBetweenNodes = 20; rootLocation = top}.layout(node)
  lastY = node.y
  if rightHalf non-empty:
      node.children = rightHalf
      BranchTreeLayout{gap 20; rootLocation = bottom}.layout(node)   // SAME root object, fresh LayoutNodes of the right half
      shift(leftHalf, node.y - lastY)                               // add to y of every node in left subtrees, recursively
  node.children = children
  copyInfo(node)            // TechTreeNode.x/y = LayoutNode.x/y recursively
  bounds over VISIBLE nodes: minx=min(0, x-w/2) maxx=max(0, x+w/2) miny=min(0, y-h/2) maxy=max(0,y+h/2)
  bounds = Rect(minx, miny, maxx-minx, maxy-miny); bounds.y += nodeSize*1.5   // = +90
  ```
  LayoutNodes include invisible nodes (layout uses the whole tree). A leaf
  LayoutNode has `children = []` (non-null, empty), so `calcWidth()` = `width`.
* `ui/layout/TreeLayout.java` `TreeNode`: fields `width,height,x,y,children,parent,
  mode=0,prelim=0,change=0,shift=0,cachedWidth=-1,number=-1,thread=null,ancestor=null`;
  `isLeaf = children==null || children.length==0`;
  `calcWidth(): if children==null return width; if cachedWidth>0 return cachedWidth;
  cWidth = sum(child.calcWidth()); return cachedWidth = max(width, cWidth)`.
* `ui/layout/BranchTreeLayout.java` (transcribe EXACTLY, it is a modified abego
  walker; `alignment = awayFromRoot` default):
  ```
  layout(root): firstWalk(root,null); calcSizeOfLevels(root,0); secondWalk(root, -root.prelim, 0, 0)
  levelChangeInY = (rootLocation==top || bottom)  -> true for both uses
  sign = (rootLocation==bottom||right) ? -1 : 1
  nodeThickness(n) = levelChangeInY ? n.height : n.calcWidth()     // = height
  nodeSize(n)      = levelChangeInY ? n.calcWidth() : n.height     // = calcWidth (SUBTREE width!)
  distance(v,w) = (nodeSize(v)+nodeSize(w))/2 + gapBetweenNodes
  nextLeft(v)  = v.isLeaf ? v.thread : v.children[0]
  nextRight(v) = v.isLeaf ? v.thread : v.children[last]
  getNumber(node,parent): if node.number==-1: number children 1..k; return node.number
  ancestorOf(vIMinus, parentOfV, default) = (vIMinus.ancestor ?: vIMinus).parent == parentOfV ? (vIMinus.ancestor ?: vIMinus) : default
  moveSubtree(wm, wp, parent, shift): subtrees = number(wp)-number(wm)
      wp.change -= shift/subtrees; wp.shift += shift; wm.change += shift/subtrees; wp.prelim += shift; wp.mode += shift
  apportion(v, defaultAncestor, leftSibling, parentOfV):
      if leftSibling==null return defaultAncestor
      vOPlus=v; vIPlus=v; vIMinus=leftSibling; vOMinus=parentOfV.children[0]
      sIPlus=vIPlus.mode; sOPlus=vOPlus.mode; sIMinus=vIMinus.mode; sOMinus=vOMinus.mode
      nr=nextRight(vIMinus); nl=nextLeft(vIPlus)
      while nr!=null && nl!=null:
          vIMinus=nr; vIPlus=nl; vOMinus=nextLeft(vOMinus); vOPlus=nextRight(vOPlus); vOPlus.ancestor=v
          shift=(vIMinus.prelim+sIMinus)-(vIPlus.prelim+sIPlus)+distance(vIMinus,vIPlus)
          if shift>0: moveSubtree(ancestorOf(vIMinus,parentOfV,defaultAncestor), v, parentOfV, shift); sIPlus+=shift; sOPlus+=shift
          sIMinus+=vIMinus.mode; sIPlus+=vIPlus.mode; sOMinus+=vOMinus.mode; sOPlus+=vOPlus.mode
          nr=nextRight(vIMinus); nl=nextLeft(vIPlus)
      if nr!=null && nextRight(vOPlus)==null: vOPlus.thread=nr; vOPlus.mode += sIMinus-sOPlus
      if nl!=null && nextLeft(vOMinus)==null: vOMinus.thread=nl; vOMinus.mode += sIPlus-sOMinus; defaultAncestor=v
      return defaultAncestor
  executeShifts(v): shift=0; change=0; for i=last..0: w=v.children[i]; change+=w.change; w.prelim+=shift; w.mode+=shift; shift+=w.shift+change
  firstWalk(v, leftSibling):
      if v.isLeaf: if leftSibling!=null: v.prelim = leftSibling.prelim + distance(v,leftSibling)
      else: da=v.children[0]; prev=null
            for w in children: firstWalk(w,prev); da=apportion(w,da,prev,v); prev=w
            executeShifts(v); mid=(children[0].prelim+children[last].prelim)/2
            if leftSibling!=null: v.prelim=leftSibling.prelim+distance(v,leftSibling); v.mode=v.prelim-mid
            else v.prelim = mid
  calcSizeOfLevels(n, level): sizeOfLevel[level] = max(old or 0, nodeThickness(n)); recurse children at level+1 (if not leaf)
  secondWalk(v, m, level, levelStart):
      levelSize=sizeOfLevel[level]; x=v.prelim+m
      y = levelStart + levelSize - sign*(nodeThickness(v)/2)          // awayFromRoot
      (levelChangeInY => no swap) v.x=x; v.y=y
      if !leaf: next = levelStart + (levelSize+gapBetweenLevels)*sign; for w: secondWalk(w, m+v.mode, level+1, next)
  ```
  Each `layout()` call uses a NEW `BranchTreeLayout` (fresh `sizeOfLevel`), but the
  root LayoutNode object is SHARED between the two calls (its prelim/mode/
  cachedWidth carry over; harmless but keep it identical: reuse one root struct).
* `ResearchDialog.java:391-410` `checkNodes(node)`:
  ```
  locked = !content.unlockedHost()
  if(!locked && (parent==null || parent.visible)) node.visible = true      // initial visible = true for all
  node.selectable = content.unlockedHost() || all(node.objectives complete)
  for child: child.visible = !locked && child.parent.visible; checkNodes(child)
  ```
  (TechTreeNode default `visible = true`, so the root is always visible.)
* `ResearchDialog.java:551-565 canSpend(node)`:
  `if(!selectable) false; if(requirements.length==0) true; if any i: finished[i]<req[i] && items.has(req[i].item) -> true; return content.locked()`.
  `items.has(item)` = amount > 0 (`type/ItemSeq.java:79`).
* `ResearchDialog.java:212-299 rebuildItems()`: rootPlanets = planets whose
  `techTree == lastNode` or `lastNode.planet == planet` (fallback serpulo); sum
  `max(amount,0)` of `sector.items()` over `hasBase() && !isFrozen()` sectors.
  Removal `add(item, -amount)`:
  ```
  cur = state.isCampaign() ? state.rules.sector : null
  pct = amount / max(total(item) - (cur cached ? cache[cur][item] : 0), 1)        // double
  counter = amount
  for (sector, seq) in cache (skip cur): if counter==0 break-ish; avail=seq[item]; if avail<=0 continue
       toRemove = min(counter, min(ceil(pct*avail), avail)); sector.removeItem(item,toRemove); counter-=toRemove
  if counter>0 && cur!=null && cache[cur]!=null: toRemove=min(cache[cur][item], counter); cur.removeItem; counter-=toRemove
  ```
* Requirement text in the hover popup (`ResearchDialog.java:684-736`):
  progress line when any finished>0: `sum += item.cost*req.amount; used += item.cost*finished.amount`;
  `bundle.format("research.progress", min((int)(used/sum*100), 99))`;
  per unfinished requirement: `reqAmount = req - finished`; text
  `" " + formatAmount(min(items[item], reqAmount)) + " / " + formatAmount(reqAmount)`,
  colour lightGray if `items.has(item)` else scarlet.
* Objective display (`game/Objectives.java:26-131`): Research ->
  `bundle.format("requirement.research", parent unlocked ? emoji+" "+localizedName : "???")`;
  Produce -> `requirement.produce` (`content unlocked ? emoji+name : "???"`);
  SectorComplete -> `requirement.capture` + preset name; OnSector ->
  `requirement.onsector`; OnPlanet -> `requirement.onplanet` + planet name.
  Only incomplete objectives are listed (`if(o.complete()) continue`).
* Root list (`ResearchDialog.java:86-108`): roots shown unless
  `node.requiresUnlock && !node.content.unlockedHost() && node != prefRoot`;
  title label `bundle.get("techtree." + name, name)`; Serpulo root is
  `nodeRoot("serpulo", coreShard)`, Erekir `nodeRoot("erekir", coreBastion, true)`.

**Port today.**
* `core/ui/campaign.rs:196 ResearchView` (index, content, depth, root, tree,
  planet, unlocked, dependencies_met, requirements_met, requirements, finished,
  objectives (Debug strings), children count). Built by `research_views` (l.829)
  for every node of the active planet.
* `core/content/tech/mod.rs:79 TechNode` has `parent`, `children: Vec<TechNodeRef>`,
  `objectives: Vec<ObjectiveSpec>`, `content: Option<ContentRef>`, `requires_unlock`,
  `name`, `tree`; `TechStore { nodes, roots, trees }` (l.149), `TechTree{name, root}` (l.140).
* `core/game/tech_tree.rs:116 can_spend` (wrong, see table), `:169 objectives_complete`,
  `:185 spend` (correct partial spend), `:429 content_unlocked`.
* `core/game/objectives.rs:42 Objective`, `from_spec`, `complete(ctx)`;
  `CampaignObjectiveContext::new(&registry, &campaign)` (l.120).
* `gdext/campaign.rs:1071 research(content)`: builds the item pool (live core
  of the played sector via `host.bind().player_core_items(team)` + every other
  based, non-frozen sector's `info.info.items`), calls `can_spend` then `spend`,
  then deducts greedily (l.~1170-1200) and `check_auto_unlocks`.
* `core/ui/text.rs:259 format_amount_default(i64)` = `UI.formatAmount`.
* Item cost: `core/content/registries/items.rs:43 pub cost: f32`.
* `MindCampaign.bundle: Bundle` (`gdext/campaign.rs:79`) resolves `Core.bundle`.

**Changes.**
1. New file `core/ui/tree_layout.rs` (SPDX header + port header, cite
   `ui/layout/BranchTreeLayout.java`, `TreeLayout.java`, `ResearchDialog.java:332-389`).
   Arena representation (no Rc):
   ```rust
   #[derive(Clone, Debug, Default)]
   pub struct LayoutNode { pub width: f32, pub height: f32, pub x: f32, pub y: f32,
       pub children: Vec<usize>, pub parent: Option<usize>,
       mode: f32, prelim: f32, change: f32, shift: f32, cached_width: f32 /* -1 */,
       number: i32 /* -1 */, thread: Option<usize>, ancestor: Option<usize> }
   pub enum TreeLocation { Top, Bottom }
   pub struct BranchTreeLayout { pub root_location: TreeLocation, pub gap_between_levels: f32,
       pub gap_between_nodes: f32, size_of_level: Vec<f32> }
   impl BranchTreeLayout { pub fn layout(&mut self, nodes: &mut [LayoutNode], root: usize) }
   /// ResearchDialog.treeLayout: returns (x, y) per input node index (Java y-up).
   pub fn research_tree_layout(children: &[Vec<usize>], root: usize, node_size: f32) -> Vec<(f32, f32)>
   ```
   `research_tree_layout` builds one `LayoutNode` per tech node (width = height =
   `node_size` = 60), splits the root's children, runs the two layouts on the
   same arena, applies `shift` to the left-half subtrees, restores children and
   returns positions. `calc_width` must cache like Java (`cached_width > 0`).
   Register `pub mod tree_layout;` in `core/ui/mod.rs`.
2. `core/game/tech_tree.rs:116 can_spend`: rewrite to Java semantics:
   ```rust
   // ResearchDialog.View.canSpend (ResearchDialog.java:551-565)
   if content unlocked -> return false  (keep: the button only spends locked nodes)
   if !selectable(...)  -> false   // selectable = unlocked || objectives_complete
   let reqs = effective_requirements(registry, node_ref);
   if reqs.is_empty() { return true; }
   if reqs.iter().enumerate().any(|(i,r)| finished(i) < r.amount && items.get(r.item) > 0) { return true; }
   true   // content.locked() — a locked node can always be "spent" (spends nothing)
   ```
   Keep the `dependencies_unlocked` check (Java guarantees it via visibility:
   an invisible node cannot be clicked). Update the doc comment. Existing test
   `locked_node_is_rejected` must still pass (parent locked => false).
3. `gdext/campaign.rs` `research()`: extract the pool building into
   `fn research_inventory(&self, registry: &ContentRegistry, planets: &[PlanetId]) -> (ItemModule, Vec<(SectorRef, Vec<(String, i32)>)>, Vec<(String,i32)> /*played*/)`
   (planets = Java `rootPlanets` for the node's tree; vanilla: the node's
   `planet`, fallback serpulo). Replace the greedy deduction loop with the Java
   percentage loop above (cast: `pct = amount as f64 / max(total - cur, 1) as f64`;
   `to_remove = min(counter, min((pct * avail as f64).ceil() as i32, avail))`),
   iterating `sector_entries` in their current order (planet order, sector id
   order) — Java iterates an `ObjectMap` (hash order); id order is the
   deterministic choice, record it in the doc comment. Remove the
   `show_toast("@unlocked")` call in GDScript later (T1c).
4. New `#[func] research_tree_json(&self, root: GString) -> GString` on
   `MindCampaign` (place it after `research`). Root resolution: `root` empty ->
   the tree of the played sector's planet (`session.sector`) else serpulo;
   otherwise the `TechTree` whose `name == root`. Output (serde_json):
   ```json
   {"root":"serpulo","root_title":"Serpulo","root_icon":"block-core-shard-ui",
    "roots":[{"name":"serpulo","title":"Serpulo","icon":"block-core-shard-ui","selected":true}],
    "items":[{"item":"copper","amount":1234,"text":"1234","name":"Copper"}],   // item-id order, amount>0 only
    "bounds":[minx,miny,w,h],
    "nodes":[{"index":0,"parent":-1,"children":[1,2],"content":"core-shard","type":"block",
       "icon":"block-core-shard-ui","name":"Core: Shard","x":0.0,"y":90.0,
       "visible":true,"locked":false,"selectable":true,"can_spend":false,
       "description":"…","inline_description":true,
       "progress":-1, "reqs":[{"item":"copper","name":"Copper","have":"40","need":"1.2k","has":true}],
       "objectives":["Capture Frozen Forest"]}]}
   ```
   * `nodes` = depth-first pre-order from the root (`TechStore::each`), `index`
     is the position in this array, `parent`/`children` are array indices.
   * `x,y` from `research_tree_layout` (Java y-up; GDScript flips).
   * `visible/locked/selectable` from the `checkNodes` transcription; `locked =
     !content_unlocked_with_rules(registry, content, &session.rules)`.
   * `can_spend` from the new `can_spend`.
   * `reqs`: only unfinished requirements; `have = format_amount_default(min(pool, need_left))`,
     `need = format_amount_default(need_left)`, `has = pool > 0`.
   * `progress`: `-1` when no finished amount > 0, else `min((used/sum*100) as i32, 99)` (f32 math as Java).
   * `objectives`: incomplete only, text = `bundle.format("requirement.<kind>", [arg])`
     with arg per the Java rule above; `emoji` = `Iconc` char of the content if the
     port has it (`core/assets/icons.rs`, look for a name->char lookup; if none,
     omit the emoji and the space — record that deviation in the commit).
   * names: `bundle.get("<type>.<name>.name")` with type
     `block|item|unit|liquid|status|sector|planet` (Java `ContentType.name()`);
     description `<type>.<name>.description` (empty when missing) and
     `inline_description` from `UnlockFields.inline_description`.
   * `items`: the `research_inventory` pool, rows in item id order, `text =
     format_amount_default(amount)`.
   * `bounds` per the `treeLayout` rule (visible nodes only, `+90` on y).
5. `core/ui/campaign.rs:829 research_views`: replace `format!("{spec:?}")` with
   the objective kind name only (`"research"`, `"produce"`, …) so the golden is
   stable and non-Debug; this changes `client/rust/mind-core/tests/goldens/ui/campaign.json`
   -> regenerate it per `00-environment.md` §2 and name it in the commit body.
   (If the golden currently has no research rows with objectives, nothing changes; then skip.)

**Tests.**
* `core/ui/tree_layout.rs` tests:
  * `three_leaf_children_split_layout`: children `[[1,2,3],[],[],[]]`, root 0,
    size 60 -> positions `0:(0,90) 1:(-40,170) 2:(40,170) 3:(0,10)`. Derivation:
    left half `[1,2]` top layout: leaf prelims 0 and 0+(60+60)/2+20=80; root
    prelim = mid 40; secondWalk m=-40: root (0, 0+60-30=30), leaves at level start
    80 -> y=80+60-30=110, x=-40/40. Right half `[3]` bottom layout: root prelim 0,
    y = 0+60+30 = 90; leaf level start (60+20)*-1=-80 -> y=-80+60+30=10, x=0.
    Shift left half by 90-30=60 -> 170.
  * `subtree_width_spacing`: children `[[1,2],[3,4],[]... ]` i.e. root 0 with
    children 1 (children 3,4) and 2 (leaf); left half = `[1]`, right half `[2]`.
    Expected: node1 (0,170), node3 (-40,250), node4 (40,250), node0 (0,90),
    node2 (0,10). Derivation: left layout: node1 children prelims 0,80, node1
    prelim 40, root prelim 40 -> m=-40: root y 30, node1 x 0 y 110, node3/4 at m
    = -40 + root.mode(0) + node1.mode(0) -> x = 0-40=-40 and 80-40=40, y = 190;
    shift +60 -> 170/250. Write the assertion with `1e-4` tolerance.
* `core/game/tech_tree.rs` tests:
  * `can_spend_allows_partial_when_an_item_is_missing`: `synthetic_pair` node
    with two requirements (copper 10, lead 10; build it with `push_node`), parent
    unlocked, pool copper 5 / lead 0 -> `can_spend == true`; `spend` leaves
    finished `[5, 0]`, `complete == false`.
  * `can_spend_locked_node_with_empty_pool_is_true`: pool empty -> `true`
    (Java `content.locked()`), and `spend` returns `spent.is_empty()`.
* `gdext` has no unit tests; put the percentage math in a pure fn
  `core/game/tech_tree.rs pub fn distribute_removal(amount: i32, current: Option<i32>, others: &[i32]) -> (Vec<i32>, i32)`
  (returns per-other removals and the current-sector removal) and test
  `distribute_removal_matches_java`: amount 10, others `[30, 10]`, current 20 ->
  total-others = 40, pct = 10/40 = 0.25; sector A: min(10, min(ceil(7.5)=8,30)) = 8,
  counter 2; sector B: min(2, min(ceil(2.5)=3,10)) = 2, counter 0; current 0
  -> `([8,2], 0)`. Second case amount 50, others `[10]`, current 100:
  pct = 50/10 = 5 -> A: min(50, min(50,10)) = 10; current: min(100, 40) = 40 -> `([10], 40)`.
  Use it from `research()`.

**Verify in-engine** (after `--import` boot, menu up):
```bash
$G --eval "return JSON.parse_string($MC.research_tree_json(\"serpulo\"))[\"nodes\"].size()"     # 223 (UNVERIFIED count: compare with campaign_views research rows for serpulo)
$G --eval "return JSON.parse_string($MC.research_tree_json(\"serpulo\")).nodes[0].content"   # "core-shard"
$G --eval "return JSON.parse_string($MC.research_tree_json(\"serpulo\")).nodes[0].x"   # 0
$G --eval "return JSON.parse_string($MC.research_tree_json(\"\")).root"   # "serpulo"
```
(each `--eval` is ONE `return` statement, env §4; never use `var` in an eval.) PAR-0160 check: pick `lancer`:
`reqs` empty and `locked == false` on the fully-unlocked profile.

**Pitfalls.** `campaign_views_json` must not be called while a sector is loaded
(env §4) — `research_tree_json` is cheap but do not embed sector views in it.
Do not hold `self.registry.take()` across `self.host()` calls with a `bind()`
held (existing pattern in `research()` is safe; copy it). `TechStore::each` is
recursive; vanilla depth is small, fine.

**Commit.** `PAR-0145: research tree read model, BranchTreeLayout port, Java canSpend and item removal (T1a)`

### T1b. Research dialog: node graph view, connectors, pan/zoom, click-to-research, root selector (PAR-0145 view half, PAR-0047)

**Symptom / acceptance.** Today: a list of rows. After: the Java graph — 60 px
node buttons at the T1a positions, yellow (`Pal.accent` #ffd37f) / gray
(`Pal.gray` #454545) connector lines, drag to pan, wheel to zoom (0.25..1),
locked nodes show the lock icon, clicking a spendable locked node researches it,
the title is a button with the root icon + `techtree.<root>` opening a tree
selector when more than one root is selectable.
Acceptance: screenshot of the serpulo tree shows the connected graph centered
(root ~110 px below the vertical center, see below); clicking `duo` on a fresh
profile is not needed — use the click path on the imported profile only to
verify no-op on an unlocked node.

**Java.** `ResearchDialog.java`:
* Title (75-108): `titleTable.button(b -> { imageDraw(root icon).padRight(8).size(iconMed=32);
  add().growX(); label(root.localizedName).color(Pal.accent); add().growX(); add().size(32) },
  onClick -> new BaseDialog("@techtree.select") with a Tex.button table of 300x60
  `Styles.flatTogglet` buttons (icon iconMed, marginLeft 12, checked = node == lastNode),
  click -> rebuildTree(node); hide) + addCloseButton()`; the title button is
  visible only when `count(roots shown) > 1`, minWidth 300.
* `cont.stack(titleTable, view, itemDisplay).grow()` — title on top, view fills,
  items display top-left over it. `margin(0).marginBottom(8)`.
* Buttons row: `addCloseButton()` (210x64 `@back`) then
  `buttons.button("@database", Icon.book, ...).size(210, 64)` -> hide research, show database.
* `shown`: switch to the current planet's tree (`ui.planet` page planet, else the
  played sector's planet), `rebuildItems`, `checkNodes`, `treeLayout`, clear hover.
* `hidden(ui.planet::setup)` -> refresh the planet page when research closes.
* `rebuildTree(node)`: switchTree; `panX = 0; panY = -200; scale 1`; clear hover; checkNodes; treeLayout.
* Wheel (157-164): `scale = clamp(scale - amountY/10*scale, 0.25, 1)`, origin = view center.
  Arc `amountY > 0` on wheel-down -> Godot `MOUSE_BUTTON_WHEEL_DOWN` => amountY = +1, WHEEL_UP => -1.
* Drag (193-198): `panX += dx/scale; panY += dy/scale; moved = true; clamp()`
  (Arc dy is y-up: Godot `panY -= relative.y/scale`). `released -> moved = false`;
  a click right after a drag (`moved`) is ignored.
* `clamp()` (539-549): `pad = 60; ox = w/2; oy = h/2; rx = bounds.x + panX + ox; ry = panY + oy + bounds.y;
  rx = clamp(rx, -bounds.w + pad, screenW - pad); ry = clamp(ry, -bounds.h + pad, screenH - pad);
  panX = rx - bounds.x - ox; panY = ry - bounds.y - oy`.
* Node button (457-516): `ImageButton(uiIcon, Styles.nodei)`, image 32 px fit,
  size 60, visible = node.visible, touchable only when visible; position
  `center = (node.x + panX + w/2, node.y + panY + h/2 + (screenH % 2)/2)` (Java y-up);
  up drawable: `!locked ? Tex.buttonOver : (!selectable || !canSpend) ? Tex.buttonRed : Tex.button`;
  image region: `selectable ? uiIcon : Icon.lock`; image colour:
  `!locked ? white : selectable ? Color.gray (#7f7f7f) : Pal.gray (#454545)`.
  Desktop click: `if(moved) return; if(canSpend && locked) spend(node)`.
* Connectors (786-813), drawn under the buttons, for every visible node and
  visible child: `lock = locked(node) || locked(child)`; stroke 4,
  colour `lock ? Pal.gray : Pal.accent`, lock lines under unlocked lines
  (`Draw.z(lock ? 1 : 2)`); if `|dy| == |dx| (±1)` and `dstm <= width*3` -> one
  straight line node->child; else two segments: `(node.x,node.y)->(child.x,node.y)`
  then `(child.x,node.y)->(child.x,child.y)`; all offset by `(panX + w/2, panY + h/2)`.

**Port today.** `ui/dialogs/research_dialog.gd` (rows), `ui/layout/*.gd` stubs,
`MindDialog.full_dialog = true` in `_ready`. Theme styles: `nodei` exists in
`ui/styles_manifest.json`; atlas ninepatches `button`, `button-over`,
`button-red` exist in `assets/sprites/sprites.atlas.json` (36x27, splits
`[12,12,12,12]`); `MindWidgets.icon_texture(region)` (`ui/mind_widgets.gd:265`)
loads an atlas region.

**Changes.**
1. New widget `ui/widgets/research_view.gd` (`class_name MindResearchView extends Control`,
   SPDX header, cite `ResearchDialog.View`). `clip_contents = true`,
   `mouse_filter = STOP`. State: `pan := Vector2(0, -200)`, `scale_factor := 1.0`,
   `moved := false`, `_tree: Dictionary`, `_buttons: Array[Button]`.
   * `set_tree(tree: Dictionary)`: frees old buttons (`queue_free`), creates one
     `Button` per node (`# code-instantiated: one button per tech node`),
     `custom_minimum_size = size = Vector2(60,60)`, child `TextureRect` 32x32
     centered (`STRETCH_KEEP_ASPECT_CENTERED`, `EXPAND_IGNORE_SIZE`),
     `mouse_filter` pass-through on the image. Store `node_index` meta.
   * `_style_for(node) -> StyleBoxTexture` built from the atlas region chosen per the
     rule above (`button-over` / `button-red` / `button`), texture margins 12
     (cache the three boxes in static vars). Set it on `normal`, `hover`,
     `pressed`, `disabled`.
   * Image: `icon` region (`MindWidgets.icon_texture(node.icon)`; fallback
     `sector-<content>` for sectors) when `selectable`, else the `lock` glyph
     texture (use `MindWidgets.glyph("lock", 32, color)` as a child Label instead
     of the TextureRect when unavailable); `modulate` per the colour rule.
   * `_process`: position every button:
     `java_c = Vector2(node.x + pan.x + size.x/2, node.y + pan.y + size.y/2)`;
     Godot center `= Vector2(java_c.x, size.y - java_c.y)`; then apply zoom around
     the view center: `p = center_of_view + (p - center_of_view) * scale_factor`;
     `button.scale = Vector2.ONE * scale_factor`; `button.position = p - Vector2(30,30) * scale_factor`;
     `button.visible = node.visible`. Call `queue_redraw()` when anything changed.
   * `_draw()`: connectors with `draw_line(a, b, color, 4.0 * scale_factor)`;
     draw all `lock` segments first, then the accent ones (z rule); same coordinate
     transform as the buttons; Godot `_draw` paints under child buttons.
   * `_gui_input`: `InputEventMouseMotion` with left button held ->
     `pan.x += relative.x / scale_factor; pan.y -= relative.y / scale_factor; moved = true; _clamp()`;
     `InputEventMouseButton` wheel -> `scale_factor = clamp(scale_factor - amount_y/10.0*scale_factor, 0.25, 1.0)`
     (`amount_y = +1` for WHEEL_DOWN, `-1` for WHEEL_UP); left release -> `moved = false` (deferred one frame so
     the button `pressed` handler still sees `moved`).
   * `_clamp()` = Java clamp with `screenW/H = get_viewport_rect().size`, `bounds` from the tree JSON (Java y-up values; the clamp is done in Java space, so no flip).
   * signals `node_pressed(index: int)`, `node_hovered(index: int)`, `node_exited(index: int)`.
     Button `pressed` -> `if not moved: node_pressed.emit(i)`; `mouse_entered`/`mouse_exited` -> hover signals.
   * `reset_view()`: `pan = Vector2(0,-200); scale_factor = 1`.
2. Rewrite `ui/dialogs/research_dialog.gd`:
   * `_ready`: `title_text = ""` (Java `super("")` + removes the title table),
     `should_pause = true`, `full_dialog = true`, `super._ready()`, build: a
     `MindResearchView` filling the body (set `body.vertical_scroll_mode =
     SCROLL_MODE_DISABLED` and size flags fill — the view must not sit in a
     ScrollContainer that eats drags: put the view directly under `Center/Panel/Layout`
     before `Body` and hide `Body`, `# code-instantiated`), a title `Button`
     (top-center, min width 300, `flatTogglet`-like, icon + accent label) added
     above the view, `add_close_button(210)` and an `@database` button
     (`add_button(_t("@database"), _open_database, "book", 210)`; height 64).
   * `shown()`: `_tree = campaign_json("research_tree_json", [planet])` where
     `planet = str(_context.get("planet",""))` mapped to its tree root name
     (vanilla: planet name == root name; else pass `""`); `_view.set_tree(_tree)`;
     `_view.reset_view()` only when the root changed since the last open (Java
     keeps pan between opens of the same tree); title visible only when
     `_tree.roots.size() > 1`.
   * `_on_node_pressed(i)`: node = `_tree.nodes[i]`; `if node.locked and node.can_spend:`
     `campaign.call("research", node.content)`; then re-fetch the tree and
     `set_tree` (positions change when nodes unlock: Java calls `treeLayout()` after unlock).
   * `_open_database()`: `close_dialog("research")` through `_close_pressed()` then
     `MindUi.open_dialog("database", "")` (call deferred; `MindUi` is bound during callbacks? No — button callbacks are outside `open_dialog`, safe).
   * Root selector: title button opens `full_text`-free inline popup: a
     `PopupPanel` (`# code-instantiated`) with one 300x60 toggle button per root
     from `_tree.roots` (`checked = selected`), click -> `shown()` with that root,
     `reset_view()`. (Java uses a BaseDialog; a modal `MindDialog` would need a new
     manifest entry — the popup is an accepted presentation deviation; record it.)
   * `hidden()`: if `MindUi.is_dialog_shown("planet")`, call the planet dialog's
     public refresh: `get_node("/root/Spine/Ui/UiRoot").dialog("planet").call_deferred("refresh_page")`
     — add `func refresh_page() -> void: _show_planet(_selected_planet, false)` to
     `planet_dialog.gd` (Java `hidden(ui.planet::setup)`).
   * Delete the old `_roots`/`_nodes` row code, `_requirements_text`, the toast.
3. Delete nothing in `ui/layout/` (unused stubs stay; T13 may remove them).

**Tests.** GDScript parse gate (env §2). No Rust change.

**Verify in-engine.**
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("research", JSON.stringify({"planet":"serpulo"}))'; sleep 2
$G --screenshot runs/<date>-s<N>-G05-T1b/research.png        # graph, connectors, centered root
$G --eval 'return [get_node("/root/Spine/Ui/UiRoot").dialog("research")._view._buttons.size(), get_node("/root/Spine/Ui/UiRoot").dialog("research")._view.pan]'   # [223, (0, -200)]
python3 REPO/tools/gmcp.py godot_input '{"action":"mouse_button","params":{"button":"MOUSE_BUTTON_WHEEL_DOWN","position":{"x":576,"y":324},"coords":"viewport","pressed":true}}'
$G --eval 'return get_node("/root/Spine/Ui/UiRoot").dialog("research")._view.scale_factor'   # 0.9
$G --screenshot runs/<date>-s<N>-G05-T1b/research-zoom.png
```
Compare with the Java reference frame of the tech tree if one exists under
`REPO/runs/*ref*` (search `ls REPO/runs | grep -i research`); otherwise record
"no Java frame".

**Pitfalls.** Buttons placed with `position` inside a plain `Control` (not a
container) — do not use a `Container`, it would re-layout them. 223 buttons with
per-frame repositioning is fine; only update when `pan/scale/size` changed.
`_gui_input` drags are swallowed by child buttons: connect the buttons'
`gui_input` to forward motion events to the view (or set button `mouse_filter =
MOUSE_FILTER_PASS`). The research dialog must NOT be inside the dialog's
`ScrollContainer`.

**Commit.** `PAR-0145: research dialog node graph, connectors, pan/zoom and root selector (T1b)`

### T1c. Research dialog: hover info popup, global items display, unlock sound (PAR-0145 rest, PAR-0047)

**Symptom / acceptance.** After: hovering a node shows the Java info table at
the button's top-right (info button, name or `[accent]???`, progress %,
requirement rows coloured lightgray/scarlet, `@complete` objectives with red
cross, or `@completed`, inline description); a top-left `@globalitems`
collapsible list shows the tree's item pool; researching plays `uiUnlock`
(no toast).

**Java.** `ResearchDialog.java:490-501` hover/exit (desktop: hover sets
`hoverNode` when visible; exit clears unless the mouse is over the info table);
`rebuild(shine)` 632-784: info table `background(Tex.button).margin(8)`, placed
`topLeft` at `(button.x + button.width, button.y + button.height)` (Java y-up =>
Godot: top-left corner at the button's top-right corner); row 1: if `selectable`
an `Icon.info` `Styles.flati` button width 50 growY -> `ui.content.show(content)`;
a growX spacer; a `desc` table pad 9: line 1 `selectable ? localizedName : "[accent]???"`;
if locked: if selectable: progress line (when any) lightGray, then one row per
unfinished requirement: icon 24 + padRight 3, `localizedName` lightGray,
`" have / need"` (colour as T1a); else if objectives: `@complete` then
`"> " + display` lightGray + `Icon.cancel` scarlet per incomplete objective;
if unlocked: `@completed`. Row 2: description wrapped lightGray, max width 270
when longer than 20 chars, only if `description != null && inlineDescription && selectable`.
Shine: after a spend the used requirement labels flash `Pal.accent` -> target
colour over 0.75 s (`Interp.fade`).
`ItemsDisplay.java:24-67`: `Tex.button` table, margins 10/12/15/12; a
`Styles.flatTogglet` button `@globalitems` with `Icon.downOpen`/`upOpen`,
minWidth 200, toggling a collapser (0.3 s); inside: for each item in id order
with amount>0: `formatAmount(amount)` label, item icon 24 (pad 4), localized
name lightGray. Shine: used items flash accent -> white 0.75 s.
`ResearchDialog.java:608-625 unlock`: `Sounds.uiUnlock.play()`.

**Port today.** `ui/widgets/items_display.gd` (HBox chips; used by
`MindWidgets.items_display()`; grep its other users before changing:
`grep -rn "items_display\|ItemsDisplay" client/ui` — if other dialogs use it, add a
new class instead of changing it). `MindUi.play_ui_sound(name)`
(`gdext/ui/ui_host.rs:367`). Bundle keys present: `globalitems`, `completed`,
`complete`, `research.progress`, `info.title`.

**Changes.**
1. `ui/widgets/research_items_display.gd` (`class_name MindResearchItemsDisplay extends PanelContainer`):
   `Tex.button` StyleBoxTexture (region `button`, margins 12) with content margins
   left 12 top 15 right 12 bottom 10; a toggle button (`@globalitems`, glyph
   `downOpen`/`upOpen`, min width 200) and a `VBoxContainer` list in a
   `ScrollContainer` (max height = view height - 120). `rebuild(items: Array, shine: Dictionary = {})`
   with rows `amount text (markup)`, 24 px icon `item-<name>-ui`, name lightgray
   (#bfbfbf). Collapsed state persists across rebuilds (Java `collapsed` field).
   Place it as a child of the research view's parent at top-left (`stack` order:
   title, view, items: items drawn on top).
2. `research_dialog.gd`: on `node_hovered(i)` build the popup
   (`PanelContainer` with the `button` StyleBoxTexture, `# code-instantiated`) from
   `_tree.nodes[i]` exactly per the Java layout above; info button ->
   `open_content_info(node.content)`. Keep it while the mouse is over the popup;
   remove on `node_exited` unless `popup.get_global_rect().has_point(mouse)`; also
   remove when the popup itself is exited and the mouse is not over the node.
   Position updates every frame (`_process`) from the button's rect.
3. After a successful `research`: compute `shine` = set of requirement item names
   whose `have` changed, rebuild the popup with those labels tweened
   `modulate` from accent to the target colour over 0.75 s, rebuild the items
   display with the same shine; call `MindUi.play_ui_sound("uiUnlock")` only when the
   node became unlocked (`!node.locked` in the re-fetched tree).
4. Remove the `show_toast(_t("@unlocked"))` line.

**Tests.** Parse gate.

**Verify in-engine.** Hover via the bridge (`mouse_button` is not needed; move
the mouse): `python3 REPO/tools/gmcp.py godot_input '{"action":"mouse_motion","params":{"position":{"x":X,"y":Y},"coords":"viewport"}}'`
(if `mouse_motion` is not a supported action, call `d._on_node_hovered(i)` directly
via eval — check `tools/gmcp.py --help` first). X,Y = center of `d._view._buttons[5]`
(`return d._view._buttons[5].get_global_rect().get_center()`). Screenshot the
popup; expect name + `[accent]Researched` (`@completed`) on the imported profile.
For a locked node use a fresh profile: `rm -rf $U && tools/session-play.sh N` (no
import), start Ground Zero, open research, hover `conveyor`'s child — expect
requirement rows. Then `--import` again before other tasks (env §4.3).

**Pitfalls.** G07 owns the `play_ui_sound` wiring; this task only emits
`MindUi.play_ui_sound("uiUnlock")` at Java's call site (`ResearchDialog.java:623`).
If nothing listens yet, nothing plays — acceptable. Never call `MindUi.open_dialog` from inside
`shown()` (env, PAR-0066): `open_content_info` from a button is fine.

**Commit.** `Fixes PAR-0145: research hover info, global items display and unlock sound (T1c)`

### T2. Dialog infrastructure: 0.9 stage dim, stacked dialogs stay drawn, modal Confirm/TextInput/Info (PAR-0148, PAR-0149, PAR-0150)

**Symptom / acceptance.** Today every dialog dims 0.55 (pause/full_text patch
0.9 locally), opening a dialog hides the one below, and confirms are an inline
"Yes/No" panel. After: every `MindDialog` dims the stage with black 0.9 except
the planet page (opaque black / globe); a dialog opened over another leaves the
lower one drawn (dimmed by the top one's 0.9 backdrop); `show_confirm` renders a
`@confirm`-titled dialog (accent 3 px rule, body 500 px wrapped centered,
`@cancel` (Icon.cancel) + `@ok` (Icon.ok) 200x54 buttons, Enter = OK, Esc = cancel).
Acceptance: open `research` over `planet` -> `MindUi.dialog_stack()` is
`["planet","research"]` and `dialog("planet").visible == true`; the
pause-menu Quit confirm screenshot shows title "Confirm" and buttons
"Cancel"/"OK".

**Java.**
* `ui/Styles.java:458-469`: `defaultDialog.stageBackground = black9` (0,0,0,0.9);
  `fullDialog.stageBackground = black` (only `PlanetDialog`, `PlanetDialog.java:78`).
  `BaseDialog(title)` uses the scene default style = `defaultDialog`
  (`Scene.registerStyles` registers fields named `default*`; `UI.java:128`).
* `ARC/arc-core/src/arc/scene/ui/Dialog.java:288-317`: every shown dialog draws its
  `stageBackground` over the whole stage before itself, alpha `color.a * parentAlpha`;
  lower dialogs remain in the scene and are drawn (dimmed by upper backdrops).
  Modal: hits outside go to the top dialog (`hit()` 320-324).
* `core/UI.java:132-133`: show = `alpha(0) -> fadeIn(0.1s)`, hide = `fadeOut(0.1s)`.
* `core/UI.java:582-604 showConfirm(title, text, hide, confirmed)`:
  `BaseDialog(title)`; `cont.add(text).width(500).wrap().pad(4)` centered;
  `buttons.defaults().size(200,54).pad(2)`; `setFillParent(false)`;
  `@cancel` + `Icon.cancel` -> hide; `@ok` + `Icon.ok` -> hide + run;
  Enter -> hide + run; `closeOnBack` (Esc -> hide). `showConfirm(text, cb)` uses title `@confirm`.
* `core/UI.java:309-357 showTextInput` (desktop): `Dialog(titleText)`;
  `cont.image().width(400).pad(2).height(4).color(Pal.accent)`, row, `cont.add(text)`,
  row, field 400x50 (max length, digits filter when numeric); buttons
  `defaults().size(120,54).pad(4)`: `@cancel` (run closed, hide), `@ok`
  (disabled while empty unless allowEmpty; run confirmed(text), hide); Enter = OK; Esc = cancel.
* `core/UI.java:466-474 showInfo`: `Dialog("")`, text width 400 wrapped centered
  margin 15, `@ok` 110x50 pad 4, Enter/Esc close.

**Port today.**
* `scenes/ui/dialogs/mind_dialog_base.tscn` node `Dim` `color = Color(0,0,0,0.55)`;
  `about_dialog.tscn:21` and `widget_gallery.tscn:21` have their own `Dim` (0.55).
  Local 0.9 overrides: `ui/dialogs/paused_dialog.gd:35`, `full_text_dialog.gd:22`.
  Planet: `planet_dialog.gd:1197` toggles `Dim.visible` with the globe.
* `gdext/ui/ui_host.rs:187 open_dialog` hides the previous top unless
  `keep_previous`; `close_dialog` (l.230) / `close_top_dialog` (l.268) call
  `reveal_top` (l.255) which re-runs `show_dialog` (=> `shown()` rebuild).
* `ui/mind_dialog.gd:115 show_dialog` (`visible = true; move_to_front(); shown()`).
* `ui/ui_root.gd:413 _on_show_confirm`, `:427 _on_text_input_request`,
  `:470 _build_prompt` (ColorRect 0.55 + `defaultDialog` panel 420 wide),
  `_on_show_info`/`_toast_label` (show_info is a toast label, not a dialog).
* Confirm consumers route results by their own pending flags
  (`paused_dialog.gd:306`, `schematics_dialog.gd:221`, `save_dialog.gd:137`,
  `settings_menu_dialog.gd:349`, `mods_dialog.gd:306`) — the signal API stays.

**Changes.**
1. `mind_dialog_base.tscn` `Dim.color = Color(0,0,0,0.9)`; same for the `Dim`
   in `about_dialog.tscn` and `widget_gallery.tscn` if they still say 0.55. Remove
   the now-redundant overrides in `paused_dialog.gd` and `full_text_dialog.gd`
   (leave a one-line comment citing `Styles.java:458`). Planet dialog keeps its
   own handling (globe page: Dim hidden; chooser over menu: Dim visible at 0.9 —
   Java chooser is a BaseDialog => black9).
2. Sounds: in `mind_dialog.gd hide_dialog` emit `MindUi.play_ui_sound("uiBack")`
   (Java `BaseDialog.java:31` `hidden(() -> Sounds.uiBack.play())`); emit only —
   G07 owns the `play_ui_sound` wiring. Plain `Dialog`s (confirm/text-input/info
   prompts) play nothing (they are not BaseDialogs).
   `mind_dialog.gd show_dialog`: after `visible = true`, `modulate.a = 0.0` and
   tween to 1.0 over 0.1 s (`MindWidgets.fade_in(self, 0.1)`, `ui/mind_widgets.gd:247`).
   `hide_dialog`: keep instant hide (a fade-out would need deferred hide; Java
   fades 0.1 s — record as accepted deviation unless trivial with
   `create_tween().tween_property(self,"modulate:a",0,0.1).finished.connect(...)`;
   if you implement it, guard against re-show during the fade).
3. `ui_host.rs open_dialog`: drop the auto-hide of the previous top entirely
   (delete the `keep_previous` branch's hide call; keep parsing `keep_previous`
   for backward compatibility but ignore it; update the doc comment: "Java
   `Dialog.show` never hides the lower dialog (Dialog.java:288-317)").
   `reveal_top`: call `show_dialog` only when `node.call("is_shown")` is false
   (the lower dialog is normally still visible; this keeps legacy callers that
   hid it explicitly working).
4. Lower dialogs must not react to input while covered:
   * Mouse: the top dialog's `Dim` (ColorRect, `mouse_filter` STOP by default)
     already blocks GUI clicks. Verify `Dim.mouse_filter == 0` in the base scene.
   * `_input`-based handlers bypass GUI: `planet_dialog.gd:1295 _input` must
     return early unless the planet dialog is the top of the stack. Add to
     `mind_dialog.gd`:
     ```gdscript
     ## True when this dialog is the top of the MindUi stack (Arc modal focus).
     func is_top() -> bool:
         var ui := get_node_or_null("/root/MindUi")
         if ui == null: return visible
         var stack: PackedStringArray = ui.call("dialog_stack")
         return not stack.is_empty() and stack[stack.size() - 1] == String(name)
     ```
     Verified: `ui_root.gd:251-266 _bind` registers each dialog under its node
     name, which equals the manifest name, so `String(name)` is correct.
     Use `is_top()` in every `_input`/`_unhandled_input` of a dialog: `grep -n "func _input\|func _unhandled" client/ui/dialogs/*.gd`.
   * ESC goes to `close_top_dialog` already (`ui_root.gd:114`).
5. Confirm/text-input/info prompts in `ui_root.gd`: rebuild `_build_prompt` as a
   Java dialog: full-rect `ColorRect` `Color(0,0,0,0.9)` (STOP), centered
   `PanelContainer` with `theme_type_variation = "defaultDialog"` (no fixed 420
   min width), title Label (accent `MindStyles.ACCENT`, centered) + 3 px accent
   `ColorRect` underline (same as `mind_dialog.gd:100 _ensure_title_underline`),
   body `MindWidgets.label` with `custom_minimum_size.x = 500`, autowrap,
   centered. Signature: `_build_prompt(title_text, message, with_field, body_width)`.
   * `_on_show_confirm(text)`: title `_t("@confirm")`, body width 500; buttons via a
     helper `_add_prompt_button(container, label_key, glyph, size, cb)` using
     `MindWidgets.icon_button(glyph, _t(label_key))` with `custom_minimum_size =
     Vector2(200,54)`: `@cancel`/`cancel` -> resolve false; `@ok`/`ok` -> resolve true.
     Enter -> OK, Escape -> cancel: give the root a script-less `gui_input`
     handler or `set_process_unhandled_key_input` via a small inner node; simplest:
     connect `root.gui_input` and also `grab_focus()` on the OK button so Enter
     activates it (`Button` reacts to `ui_accept`), and handle `ui_cancel` in
     `_unhandled_input` of `ui_root.gd` while a prompt is up (track `_active_prompt`).
   * `_on_text_input_request`: title row, 400x4 accent rule, message, `LineEdit`
     400x50; buttons `@cancel`, `@ok` 120x54 (no icons). Keep numeric filter and
     empty gating.
   * `_on_show_text(title, text)`: `Dialog(title)` + text + `@ok` 110x50.
   * Every hard-coded "Yes"/"No"/"OK"/"Cancel" string goes through `_t()`.
6. `scenes`: no new nodes (prompts stay code-instantiated; add the
   `# code-instantiated: transient prompt dialogs` comment if missing).

**Tests.** `cargo test -p mind-headless ui_goldens_match` (manifest unchanged);
parse gate; `cargo clippy` for the ui_host change.

**Verify in-engine.**
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'return get_node("/root/MindUi").open_dialog("research", JSON.stringify({"planet":"serpulo"}))'; sleep 2
$G --eval 'return [get_node("/root/MindUi").dialog_stack(), get_node("/root/Spine/Ui/UiRoot").dialog("planet").visible]'   # [["planet","research"], true]
$G --screenshot runs/<date>-s<N>-G05-T2/stacked.png      # research over a ~10% planet page
$G --eval 'get_node("/root/MindUi").close_top_dialog(); return get_node("/root/MindUi").dialog_stack()'   # ["planet"]
# confirm (pause menu Quit, in a sector — env §4.1):
$G --eval 'get_node("/root/MindUi").show_confirm("test"); return true'
$G --screenshot runs/<date>-s<N>-G05-T2/confirm.png      # title Confirm, Cancel/OK with icons
python3 REPO/tools/gmcp.py godot_input '{"action":"key","params":{"key":"Escape","pressed":true}}'   # prompt closes, confirm_result(false)
```
Java reference: `REPO/runs/*ref*` pause-menu frames (search `ls REPO/runs | grep ref`;
the PAR-0040 run holds a Java confirm frame — `jq -r '.ledger["PAR-0040"].run' REPO/chains.json`).

**Pitfalls.** Keeping lower dialogs visible changes behaviour for dialogs whose
`hidden()` restores stage layers (planet dialog `_restore_stage`) — they no
longer get `hidden()` when a child opens, which is the Java behaviour; verify
the planet -> research -> back path still shows the globe (the research dialog
is opaque-ish 0.9 over it). `campaign_complete`/`restart` dialogs opened over
`paused` also stay stacked now. Re-run the T5 remap-notice check (it used
`keep_previous`) — it must still overlay the planet page.
Shared file: `gdext/ui/ui_host.rs` (also edited by other areas' settings work —
keep the diff local to `open_dialog`/`reveal_top`).

**Commit.** `Fixes PAR-0148, PAR-0149, PAR-0150: 0.9 dialog backdrop, stacked dialogs stay drawn, modal confirm/text-input (T2)`

### T3. Planet page sector panel: launch candidate (findLauncher), "No Launch Sector", locked objectives, vulnerable rule, threat colour, action gate

**Symptom / acceptance.** Today the panel launches from "the first based,
unattacked sector of the planet" (`planet_dialog.gd:1171 _find_launcher`), never
shows `@sectors.nolaunchcandidate`, shows only a disabled `@locked` button for
locked presets (no objectives), labels every based sector next to *any* enemy
base flag as vulnerable (`_sector_state_labels`, uses the sector's own
`has_enemy_base`), and prints the threat without its colour. After: Java rules.
Acceptance (imported profile, planet page on serpulo): selecting a non-preset
sector that is not adjacent to a based sector with a >=4-size core shows
`No Launch Sector` and a disabled button; a locked preset (e.g. one whose
objectives are incomplete — pick any `lock_icon == true` sector from the view)
shows `Complete:` + `> Capture …` lines; `frozenForest` (64) launch opens Configure
Loadout from the sector Java would pick (`launch_from` printed in the view).

**Java.**
* `PlanetDialog.java:1286-1354 updateSelected` (look mode):
  ```
  locked = isLocked(sector) = preset!=null && preset.locked() && !hasBase() && preset.techNode!=null
  if locked: table "@complete" (colspan 2) then for o in preset.techNode.objectives if !o.complete():
             "> " + o.display() (lightGray) + Icon.cancel (scarlet) padLeft 3
  else if !hasBase(): "@sectors.threat" + " [accent]" + displayThreat()      // NOTE: Core.bundle.get("sectors.threat") + " [accent]" + threat text
  if isAttacked(): "@sectors.underattack"
  else if hasBase() && planet.campaignRules.sectorInvasion && near().any(s -> s.hasEnemyBase() && (s.preset==null || !s.preset.requireUnlock)): "@sectors.vulnerable"
  else if !hasBase() && hasEnemyBase(): "@sectors.enemybase"
  resources row (save != null && resources.any())
  if hasBase(): "@stats" button (Icon.info, Styles.cleart, height 40, fillX)
  if (hasBase() && mode==look) || canSelect(sector) || (preset!=null && preset.alwaysUnlocked):
      noCandidate = sector != planet.getStartSector() && !hasBase() && findLauncher(sector)==null
      button(label, locked ? Icon.lock : Icon.play).growX().height(54).minWidth(170).padTop(4).disabled(locked || noCandidate)
      label = isBeingPlayed ? "@sectors.resume" : hasBase ? "@sectors.go" : locked ? "@locked" : noCandidate ? "@sectors.nolaunchcandidate" : "@sectors.launch"
  ```
* `displayThreat()` (`type/Sector.java:80-85`): `"[#" + white.lerp(scarlet, Mathf.round(threat, 0.25)) + "]" + bundle("threat.<bucket>")`;
  `Mathf.round(v, step) = (int)(v/step)*step` (truncation, `ARC/arc-core/src/arc/math/Mathf.java:508`);
  scarlet = `0xff341cff`; `Color.toString` = 8 hex digits of `(int)(255*c)` per channel.
* `PlanetDialog.java:437-444 findLauncher(to)` (look mode):
  ```
  actual = launchSector != null && launchSector.planet == to.planet ? launchSector : null   // launchSector = gameOver ? null : state.getSector()
  candidate = to.planet.generator.findLaunchCandidate(to, actual)
  return candidate == null && actual != null && !actual.isAttacked() ? actual : candidate
  ```
* `SerpuloPlanetGenerator.java:90-112`:
  ```
  allowNumberedLaunch(s) = s.hasBase() && !s.isAttacked() && (s.info.bestCoreType.size >= 4 || s.isBeingPlayed() && defaultTeam has a core of size >= 4)
  findLaunchCandidate(dest, sel):
     if dest.preset == null || !dest.preset.requireUnlock:
        return (sel != null && sel.isNear(dest) && allowNumberedLaunch(sel)) ? sel : dest.near().find(allowNumberedLaunch)
     else return super.findLaunchCandidate(dest, sel)
  ```
* `PlanetGenerator.java:59-97 findLaunchCandidate(dest, sel)` (base):
  ```
  if !dest.allowLaunchLoadout() && dest.preset != null:
      tmp = preset map rules().loadout filtered to items on dest.planet
      if sel != null && sel.planet == dest.planet && sel.hasBase() && sel.items().has(tmp) && !sel.isAttacked(): return sel
      return dest.planet.sectors.min(s -> s.hasBase() && !s.isAttacked() && s.items().has(tmp), s -> s.tile.v.dst(dest.tile.v))
  else:
      launchSector = sel != null && sel.planet == dest.planet && sel.hasBase() && !sel.isAttacked() ? sel : null
      if dest.near().contains(launchSector) return launchSector
      launchFrom = launchSector
      if launchFrom == null || dest.preset == null:
          launchFrom = dest.near().find(s -> s.hasBase() && !s.isAttacked())
          if launchFrom == null && dest.preset != null:
              if launchSector != null return launchSector
              launchFrom = dest.planet.sectors.min(s -> s.hasBase() && !s.isAttacked(), s -> s.tile.v.dst2(dest.tile.v))
      return launchFrom
  ```
  `Seq.min(pred, f)` returns the first minimum in sector-id order (strict `<`).
  `Sector.near()` = the grid tile's neighbour list in `Ptile.tiles` order.
* `allowLaunchLoadout()` = `preset != null && preset.overrideLaunchDefaults ? preset.allowLaunchLoadout : planet.allowLaunchLoadout` (`type/Sector.java:97`).

**Port today.**
* `core/ui/campaign.rs:91 SectorView` (fields listed in the struct; has
  `selectable`, `threat`, `threat_band`, `has_enemy_base`, `attacked`, …);
  `annotate_globe` (l.709) builds its own `near(id) -> Vec<usize>` from a fresh
  `PlanetGrid::create(def.sector_tiles)` (redundant: `Campaign::from_registry`,
  `core/game/universe.rs:~335-350`, already fills `Planet.neighbors: Vec<Vec<u16>>`
  (`core/game/planet.rs:115`) from the same grid whenever `sector_tiles > 0`;
  `EmptyNeighborhood` is only the gridless fallback). `annotate_globe` receives
  `planet: &Planet`, so it can read `planet.neighbors[id]` directly. It also
  has `numbered_launcher` (= `allowNumberedLaunch` without the being-played core clause).
* `core/game/sector.rs`: `Sector { id, preset, info (SectorInfoState with .info: SectorInfo),
  save, being_played, threat, … }`, `has_base()`, `is_attacked(None)`,
  `is_frozen(None)`, `threat_bucket()` (l.683).
* `SectorInfo.items: IndexMap<String,i32>` (`core/io/json/rules.rs:952`),
  `best_core_type: String` (l.954).
* `PlanetDef.allow_launch_to_numbered`, `generator: GeneratorKind`, `start_sector`
  (`core/content/registries/planets.rs`). Planet `campaign_rules.sector_invasion`:
  check the field name with `grep -n "sector_invasion" client/rust/mind-core/src/game/campaign_rules.rs`.
* Preset loadout for the `!allowLaunchLoadout` branch: `gdext/campaign.rs:319 preset_launch_loadout`
  already reads it (gdext only; move the pure part into core if needed, or pass
  the map loadout in from gdext — see change 2).
* `CampaignViews::from_campaign(campaign, registry, schematics, active)` (l.1064),
  called by `gdext/campaign.rs:1339 campaign_views_json` and the headless fixture (l.1156).

**Changes.**
1. `SectorView` new fields (append at the end of the struct, `#[derive(Serialize)]` order matters for the golden):
   ```rust
   /// `PlanetDialog.findLauncher` result (look mode), sector id.
   pub launch_from: Option<u16>,
   /// `PlanetDialog.hasNoCandidate`.
   pub no_candidate: bool,
   /// `updateSelected` action-button gate: (hasBase && look) || canSelect || preset.alwaysUnlocked.
   pub show_action: bool,
   /// `isLocked(sector)` (preset locked, no base, has tech node).
   pub objectives_locked: bool,
   /// Incomplete objectives of the preset's tech node: (kind, argument content name).
   pub objectives: Vec<(String, String)>,
   /// `sectors.vulnerable` rule (sectorInvasion + enemy-base neighbour).
   pub vulnerable: bool,
   /// `displayThreat` colour, 8 hex digits (`ff998dff`).
   pub threat_color: String,
   ```
2. `annotate_globe(views, registry, planet)` gains `launch_sector: Option<u16>` and
   `loadout_of: &dyn Fn(SectorId) -> Vec<(String,i32)>` (preset map loadout,
   items already filtered to the planet; headless passes `&|_| Vec::new()`).
   First replace the local `grid`/`near` closure with
   `let near = |id: u16| planet.neighbors.get(id as usize).cloned().unwrap_or_default();`
   (keep a `PlanetGrid` only for `tile_v` distances, or read `Sector.tile_v` if the
   port stores it: `grep -n "tile_v\|pub v:" client/rust/mind-core/src/game/sector.rs`).
   Compute per view, in this order: `objectives_locked`, `objectives` (use
   `Objective::from_spec(spec).complete(&ctx)` with a `CampaignObjectiveContext`
   — `annotate_globe` has no `Campaign`; add a `ctx: &dyn ObjectiveContext`
   parameter and build it in `from_campaign` via `CampaignObjectiveContext::new(registry, campaign)`),
   `vulnerable`, `threat_color`, `launch_from` (transcribe both
   `findLaunchCandidate`s + `findLauncher`; the Serpulo override applies when
   `def.generator == GeneratorKind::Serpulo`; others use the base version), then
   `no_candidate = id != def.start_sector && !has_base && launch_from.is_none()`,
   `show_action = has_base || selectable || preset.always_unlocked` (check field
   name `grep -n always_unlocked client/rust/mind-core/src/content/registries/sectors*.rs`).
   `numbered_launcher`: add the being-played clause: `|| (s.being_played && launch_core_size >= 4)`
   where `launch_core_size` is a new `annotate_globe` arg `played_core_size: i32`
   (0 when unknown; gdext passes the live best core size via `host.bind().best_core_position()`
   block size — if no cheap seam exists, pass 0 and record the deviation).
   `items().has(tmp)`: every `(item, n)` of `tmp` satisfies `info.items[item] >= n`
   (for the played sector use the live core items if gdext supplies them; else `info.items`).
   Distance: `tile_v` from the same `PlanetGrid` (`grid.tiles[id].v`), Euclidean.
3. `CampaignViews::from_campaign(..., played: Option<(PlanetId,u16)>, played_core_size: i32, loadout_of)`;
   gdext passes `self.session.sector` filtered by `!self.session.game_over`
   (`core/game/play.rs:115,129`) and `preset_launch_loadout(registry, planet, preset_id)`
   wrapped in a closure; the headless fixture passes `None, 0, &|_| Vec::new()`.
   Regenerate `client/rust/mind-core/tests/goldens/ui/campaign.json` (env §2) and
   name it in the commit body.
4. `ui/dialogs/planet_dialog.gd _build_sector_panel`:
   * if `objectives_locked`: a `[lightgray]Complete:` row (`_tm("@complete")`) then
     one row per objective: label `"> " + text` lightgray + `MindWidgets.glyph("cancel", 18, Color(1,0.204,0.11))`
     where `text = MindWidgets.markup_format("@requirement.%s" % kind, [localized(arg)])`
     (`localized` = `sector.<arg>.name` for capture/onsector, `planet.<arg>.name`
     for onplanet, content name lookup `_localized_content`-style for research/produce;
     use `???` for research when the arg's parent is locked is NOT available here —
     Java prints the name when `techNode.parent` unlocked; accept always printing the name and note it).
   * else if `!has_base`: `"%s [color=#%s]%s[/color]" % [_t("@sectors.threat"), threat_color.left(6), _t("@threat.%s" % threat_band)]`
     (Java prints the colour; `[accent]` before it is overridden by the hex colour).
   * state line: `attacked` -> `@sectors.underattack`; elif `vulnerable` ->
     `@sectors.vulnerable`; elif `!has_base and has_enemy_base` -> `@sectors.enemybase`.
   * action button only when `show_action`; label rule above with
     `@sectors.nolaunchcandidate` and `disabled = objectives_locked or no_candidate`;
     icon `lock` when `objectives_locked` else `play`; min width 170, height 54.
   * `_sector_action_pressed` launch branch: use `sector.launch_from` (null ->
     free launch `_launch(planet, id)`; else open Configure Loadout from that
     sector: pass it to `_open_launch_loadout(planet_name, sector_id)` — read that
     function and make it take the launcher id instead of calling `_find_launcher`).
     Delete `_find_launcher`.

**Tests.** `core/ui/campaign.rs` tests (use the fixture campaign the existing
tests use; read `fn fixture` near l.1102):
* `threat_color_matches_java`: threat 0.3 -> `"ffccc6ff"`; 0.5 -> `"ff998dff"`; 0.74 -> `"ff998dff"`; 0.0 -> `"ffffffff"`.
  Derivation (0.3): t = (int)(0.3/0.25)*0.25 = 0.25; g = 1+(52/255-1)*0.25 = 0.80098 -> (int)204.25 = 204 = cc;
  b = 1+(28/255-1)*0.25 = 0.77745 -> 198 = c6. (0.5): g 0.60196 -> 153 = 99, b 0.55490 -> 141 = 8d.
* `serpulo_numbered_launch_needs_size4_neighbour`: in the fixture make sector A
  based with `best_core_type = "core-shard"` (size 3) and B (a neighbour per
  `PlanetGrid::create(3).tiles[A].tiles[0]`) unbased non-preset -> `B.launch_from == None`,
  `B.no_candidate == true`; set A's core to `core-foundation` (size 4) -> `B.launch_from == Some(A)`.
* `preset_launch_prefers_played_sector`: preset destination that requires unlock,
  `launch_sector = Some(A)` based & unattacked, A not adjacent -> `launch_from == Some(A)`
  (base branch: `launchSector != null` returned before the min search).

**Verify in-engine.**
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'return get_node("/root/Spine/Ui/UiRoot").dialog("planet").campaign_section("sectors").filter(func(x): return x.preset == "frozenForest")[0].launch_from'   # 170 (Ground Zero) or the Java pick; UNVERIFIED — compare with the Java client's Configure Loadout "Launching From"
$G --eval 'get_node("/root/Spine/Ui/UiRoot").dialog("planet")._select_view_tile(<id>); return true'   # a non-adjacent numbered sector
$G --screenshot runs/<date>-s<N>-G05-T3/no-candidate.png   # "No Launch Sector", disabled
```
Pick `<id>` by evaluating `return get_node("/root/Spine/Ui/UiRoot").dialog("planet").campaign_section("sectors").filter(func(x): return x.preset == null and x.no_candidate).map(func(x): return x.id)`.

**Pitfalls.** Golden churn: any `SectorView` field change regenerates
`campaign.json` and its sha256 in `parity/golden_manifest.json`. `annotate_globe`
must stay deterministic: iterate `planet.sectors` in id order, neighbours in
`planet.neighbors` order (= `Ptile.tiles` order, as Java `Sector.near()`).
Shared files: `core/ui/campaign.rs` (G04 read model), `planet_dialog.gd`.

**Commit.** `Gameplay G05 T3: planet sector panel launch candidate, locked objectives, vulnerable rule and threat colour`

### T4a. Hints: Java driver semantics (order, no timed completion, total-playtime gate, hide vs complete) + Rust hint inputs

**Symptom / acceptance.** Today `hints_fragment.gd` auto-**completes** any hint
after 15 s (`HINT_DURATION`, l.53/101) and persists `<name>-hint-done`, so a
player who reads slowly loses the hint forever; the gate waits 8 s of *session*
time; `desktopShoot` is ordered before `breaking`; a hint whose condition
disappears stays up. After: Java driver; hints persist only on their completion
predicate or the Skip button.
Acceptance: in Ground Zero (imported profile has `desktopMove-hint-done`; use a
fresh profile, see Verify), the `desktopMove` hint stays up past 30 s without input
and completes on WASD; `hint_finished("zoom")` is false until a wheel event.

**Java.** `ui/fragments/HintsFragment.java`:
* Visibility (45): `settings.getBool("hints", true) && ui.hudfrag.shown()`.
* Update (46-68), every frame:
  ```
  if current != null:
      if current.complete(): complete()          // finish() persists "<name>-hint-done", remove, hide()
      else if !current.show(): hide()            // NOT persisted; hint stays in the list
  else if hints non-empty:
      hint = first in list (ordinal order) with show()
      if hint != null && hint.complete(): hints.remove(hint)        // silently, NOT persisted
      else if hint != null && !renderer.isCutscene() && state.isGame() && saves.getTotalPlaytime() > 8000: display(hint)
      else: if build under mouse is derelict: events.add("derelictmouse")
  ```
* `checkNext` (105-116) after hide(): removes `!valid || finished || (show && complete)`,
  sorts by ordinal, displays the first `show()` one (no playtime check here).
* `display` (118-137): left-anchored `Styles.black5` table, text width 400,
  alpha 0->1 over 1 s (`Interp.smooth`); `@hint.skip` `Styles.nonet` 112x40.
  `hide()` (150-159): fade out 0.6 s + translate y -200 (`Interp.smooth`), then remove.
* Ordinal order (169-304): desktopMove, zoom, breaking, desktopShoot,
  depositItems, desktopPause, unitControl, unitSelectControl, respawn
  (mobile-only => invalid on desktop), launch, schematicSelect, conveyorPathfind,
  boost, blockInfo, derelict, payloadPickup, payloadDrop, waveFire, rebuildSelect,
  guardian, cannotUpgrade, factoryControl, coreUpgrade, serpuloCoreZone,
  presetLaunch, presetDifficulty, coreIncinerate (27).
  `valid()` on desktop = visibility has `visibleDesktop` (all except `respawn`).
* Text (349-355): desktop replaces `"tap"`->`"click"`, `"Tap"`->`"Click"` in
  `hint.<name>`, then `UI.formatIcons`.
* Events (74-102): `BlockBuildEndEvent` !breaking && unit == player.unit ->
  `placedBlocks.add(block)`; breaking -> `events.add("break")`;
  `Trigger.cannotUpgrade` -> "cannotupgrade"; `Trigger.fireCreate` -> "fire";
  `BuildingCommandEvent` on a `PayloadBlockBuild` -> "factorycontrol";
  `BlockBuildBeginEvent` by the player on a ConstructBuild whose `prevBuild`
  contains a derelict building -> "derelictbreak"; `ResetEvent` clears both sets.

**Port today.** `ui/fragments/hints_fragment.gd` (254 l.; constants l.16-53,
`_process` l.81, `_show_next` l.180, `_hint_shown` l.196, `complete_hint` l.237);
`MindUi.hint_finished/hint_finish` (`gdext/ui/ui_host.rs:697,704`);
`MindCampaign.get_total_playtime()` (`gdext/campaign.rs:1782`);
`SimHost.player_unit_moved()`; camera `is_cutscene()`. Sim events:
`core/event.rs` `BlockPlacedEvent {x,y,block,team}`, `BlockBrokenEvent {x,y,block}`;
the player-unit build loop is `core/sim/builder.rs:942 Sim::update_builders`
(only the player unit builds; its `outcome.events` are pushed at l.~973).

**Changes.**
1. `core/sim/mod.rs`: add `pub hint_events: HintEvents` to `Sim` (default) with
   ```rust
   /// HintsFragment `placedBlocks` + `events` (HintsFragment.java:38-102).
   #[derive(Debug, Default, Clone)]
   pub struct HintEvents { pub placed_by_player: std::collections::BTreeSet<BlockId>, pub events: std::collections::BTreeSet<&'static str> }
   ```
   In `update_builders` before pushing `outcome.events`: for
   `SimEvent::BlockPlacedEvent(p)` insert `p.block` into `placed_by_player`; for
   `SimEvent::BlockBrokenEvent(_)` insert `"break"`. Clear both in the sim's reset
   path (find where `ResetEvent` is fired: `grep -n "ResetEvent" client/rust/mind-core/src/sim/*.rs client/rust/mind-gdext/src/sim_host.rs`;
   clear in `MindSimHost::load_sector`/`load_scenario` if no reset hook exists).
   Fire/cannotupgrade/factorycontrol/derelict hooks are T4b.
2. `gdext/sim_host.rs`: `#[func] fn hint_state_json(&self) -> GString` returning
   `{"placed":["router",…],"events":["break",…],"player_dead":bool,
   "player_has_item":bool,"player_can_boost":bool,"player_spawned_by_core":bool,
   "player_payload":-1|0|n,"team_units":n,"enemies":n,"wave":n,
   "conveyors":n,"plans":n,"core_block":"core-shard"|"","core_copper":n,
   "core_capacity":n,"core_has_foundation_cost":bool,"cores":n,
   "fabricators":n,"extinguishers":n,"core_zone_present":bool,"cores_destroyed":n,
   "boss":null}`. Sources (verify each name before use):
   `player_unit()` + `UnitCore.dead`/`spawned_by_core` (`core/entities/comp/unit/comp.rs:20`),
   `ItemsComp.item` (l.392), `UnitPayloads.stack.len()` (l.291; -1 when the unit has no
   `UnitPayloads`), unit def `can_boost` (`core/content/registries/units/mod.rs:607`
   is the spec; find the resolved def field with `grep -n "can_boost" client/rust/mind-core/src/content/registries/units/mod.rs`),
   enemies/wave from the campaign runtime session (`PlaySession.enemies`/`.wave`,
   `core/game/play.rs:111,121`), team unit count from `UnitRuntime.units`
   (count entries of the default team), building counts by block name from the
   grid (iterate `grid` buildings once; count `conveyor`, `tank-fabricator` +
   `ground-factory`, blocks with the `extinguisher` flag (`BlockDef.flags`
   contains `BlockFlag::Extinguisher` — check the variant name), core blocks of the
   default team (`cores`, `core_block` of the first core, its copper and
   `storage_capacity`), `core_zone_present` = any `core-zone` floor/overlay tile
   (`indexer.isBlockPresent`), player plans from `player_build_plans_json`'s source,
   `cores_destroyed` = `GameStats` destroyed counts of the three Serpulo cores
   if the port tracks them (`grep -n "destroyed" client/rust/mind-core/src/game/stats.rs`; else 0 and note),
   `core_has_foundation_cost` = core items >= `core-foundation` requirements.
   `boss` stays `null` until the G01 boss seam lands (dependency: "G01 boss seam
   (see G01 §3.1)"); then fill `{armor, flying}` from it.
   Keep it O(buildings); it is polled at most once per frame by the fragment
   (poll every 10 frames to be safe).
3. Rewrite the driver in `hints_fragment.gd`:
   * `HINTS` in the Java ordinal order above (move `breaking` before `desktopShoot`; drop `respawn`).
   * Delete `HINT_DURATION`, `_elapsed`, the timed completion, and the session
     `_playtime`; the gate is `MindCampaign.get_total_playtime() > 8000`.
   * `_process`: if `_current` set: `if _hint_complete(_current): complete_hint()`
     `elif not _hint_shown(_current, state): _hide_current()` (no persist); else
     find the first not-finished, not-`_dropped` hint with `_hint_shown`; if
     `_hint_complete` -> add to `_dropped` (session-only); elif not cutscene and in
     game and total playtime > 8000 -> `show_hint`.
   * `_hide_current()`: tween `modulate:a` 1->0 and `position.y += 200` over 0.6 s
     (`Tween.TRANS_SINE`, `EASE_IN_OUT` ≈ `Interp.smooth`), then reset `_current`.
     `show_hint` fades the panel in over 1 s.
   * Text: `MindWidgets.markup("@hint.%s")` then `.replace("tap","click").replace("Tap","Click")`.
   * `_hint_complete(name)` for the five existing predicates: desktopMove (moved),
     zoom (wheel), desktopPause (paused), launch (`MindUi.is_dialog_shown("planet")`),
     presetLaunch (`campaign && preset == "frozenForest"`).
   * Visibility: `MindUi.settings_get_flag_default("hints", true) and hud_visible`.

**Tests.** `core/sim/builder.rs` test `hint_events_record_player_placements`
(use `sim_with_player()` from the same file's tests: queue a plan for
`copper-wall` near the core, step until built, assert `sim.hint_events.placed_by_player`
contains the copper-wall id; queue a break, step, assert `"break"` in events).
Parse gate.

**Verify in-engine.** Fresh profile (no import): `rm -rf $U && tools/session-play.sh N --play`,
play Ground Zero via the planet page "Launch" (env §4.1 fresh recipe with 170 is not
possible without import — use `load_sector("serpulo",170)` + `start_sector`).
```bash
$G --eval 'return get_node("/root/Spine/Ui/UiRoot/HudGroup/hints")._current'    # "desktopMove" after 8 s of total playtime
sleep 30; $G --eval 'return get_node("/root/Spine/Ui/UiRoot/HudGroup/hints")._current'   # still "desktopMove"
python3 REPO/tools/gmcp.py godot_input '{"action":"key","params":{"key":"D","pressed":true}}'; sleep 1
$G --eval 'return get_node("/root/MindUi").hint_finished("desktopMove")'   # true
$G --eval "return $H.hint_state_json()"   # JSON with player_dead false, core_block "core-shard"
```
Then restore: `rm -rf $U && tools/session-play.sh N --import`.

**Pitfalls.** The fragment path: check it with
`get_node("/root/Spine/Ui/UiRoot").find_child("hints", true, false)` if the
`HudGroup/hints` path differs. `hint_state_json` must not panic on a menu
world (no player, no runtime): every lookup `Option`-guarded. Do not iterate a
`HashMap` to build the counts (determinism rule; counts are order-free anyway, but
emit sorted `placed`).

**Commit.** `Gameplay G05 T4a: hints driver matches HintsFragment (no timed completion, order, playtime gate) + hint inputs`

### T4b. Hints: the remaining 22 predicates

**Symptom / acceptance.** After: every desktop-valid `DefaultHint` shows and
completes on the Java predicates (except `guardian`, which depends on the G01 boss seam (see G01 §3.1),
and input-key completions that need the G08 seam — both listed below).

**Java** (`HintsFragment.java:169-304`; `isTutorial` = sector is groundZero;
`isSerpulo` = `!rules.hasEnv(Env.scorching)`):

| hint | shown | complete |
|---|---|---|
| breaking | tutorial && team conveyor count > 5 | events has "break" |
| desktopShoot | serpulo && enemies > 0 | `player.shooting` |
| depositItems | !dead && unit has item | !dead && !has item |
| unitControl | serpulo && team units > 2 && !net && !dead | !dead && !spawnedByCore |
| unitSelectControl | serpulo && team units > 3 && !dead && sector != fungalPass && fungalPass has no save | commandMode && selected units > 0 && first selected is CommandAI with targetPos |
| schematicSelect | placedBlocks has router or duct-router | keyRelease(schematicSelect) or keyTap(pick) |
| conveyorPathfind | selected block == titanium-conveyor | keyRelease(diagonalPlacement) |
| boost | !dead && unit type canBoost | keyDown(boost) |
| blockInfo | selected block == graphite-press | content dialog shown |
| derelict | events has "derelictmouse" && !tutorial | events has "derelictbreak" |
| payloadPickup | serpulo && !dead && unit is Payloadc && payloads empty | payloads any |
| payloadDrop | !dead && Payloadc && payloads any | payloads empty |
| waveFire | events has "fire" && `wave` block unlocked | team has an extinguisher-flag building |
| rebuildSelect | team plans >= 10 | `input.isRebuildSelecting()` |
| guardian | boss != null && serpulo && boss.armor >= 4 && salvo unlocked && !boss.flying | boss == null |
| cannotUpgrade | events has "cannotupgrade" | false |
| factoryControl | !(campaign && preset == onset) && team tank-fabricator + ground-factory count > 0 | events has "factorycontrol" |
| coreUpgrade | campaign && planet serpulo && core-foundation unlocked && core is core-shard && core items has foundation requirements | placedBlocks has core-foundation |
| serpuloCoreZone | campaign && planet serpulo && core-zone present && (!attackMode \|\| destroyed shard+foundation+nucleus > 0) | team cores > 1 |
| presetDifficulty | campaign && preset == null && threat >= 0.5 && tarFields not captured | campaign && preset != null |
| coreIncinerate | campaign && core != null && core copper >= storageCapacity - 10 | false |

("team plans" = `state.rules.defaultTeam.data().plans` = the team's rebuild
block plans (broken-block ghosts), NOT the player's build queue.)

**Changes.**
1. `hint_state_json` additions: `selected_block` (from `MindInput.get_input_state_json().block`,
   read in GDScript instead), `unlocked` map for `wave`,`salvo`,`core-foundation`
   (from `MindCampaign` — call `campaign_call` on a content-unlock query; if no
   `#[func]` exists, add `MindCampaign.content_unlocked(name) -> bool`),
   `team_rebuild_plans` (team data block plans — `grep -n "rebuild\|BlockPlan" client/rust/mind-core/src/game/teams.rs`),
   `serpulo` (= `!rules.env has scorching`), `fungal_pass_saved`, `tar_fields_captured`,
   `sector_threat`, `attack_mode`.
2. Event hooks into `Sim.hint_events.events`: `"fire"` where fires are created
   (`grep -n "fn create_fire\|Fires::create" client/rust/mind-core/src`),
   `"cannotupgrade"` where the core-upgrade rejection is decided
   (`grep -rn "cannotUpgrade\|cannot_upgrade" client/rust/mind-core/src`),
   `"factorycontrol"` in the building-command path for payload blocks
   (`grep -rn "BuildingCommand\|command_building" client/rust/mind-core/src/sim`),
   `"derelictbreak"` when the player deconstructs a team-0 (derelict) building.
   `"derelictmouse"`: in GDScript, when no hint is current and
   `SimHost.hover_info_world`/`tile_debug` under the mouse reports team 0 — add the
   event via a new `#[func] SimHost.hint_event(name)` that inserts into the set.
   Any hook whose site cannot be found: leave it out and list it in the commit body.
3. Input completions DEPEND ON G08: G08 adds `MindInput.take_hint_keys() -> PackedStringArray`
   returning `"shoot"`, `"boost_down"`, `"schematic_select_release"`, `"pick_tap"`,
   `"diagonal_release"`, `"rebuild_selecting"` since the last call. If G08 has not
   added it when you reach this task (`grep -n take_hint_keys client/rust/mind-gdext/src/input/mod.rs`),
   implement the predicates' `shown` half and leave `complete = false` for
   desktopShoot/schematicSelect/conveyorPathfind/boost/rebuildSelect (they then
   only finish via Skip) — record that in the commit and §6.
4. Port each `shown`/`complete` row into `_hint_shown`/`_hint_complete`.

**Tests.** Parse gate; Rust test for one event hook you add (e.g.
`fire_creation_records_hint_event` next to the fire tests).

**Verify in-engine.** Ground Zero fresh profile: place 6 conveyors with the
player plan op (`apply_sim_command_json` `"player":0`), wait -> `_current == "breaking"`;
break one -> completes. Seed copper to capacity (`debug_add_core_items("copper", 4000)`)
-> `coreIncinerate` shows. Screenshot each.

**Pitfalls.** `presetDifficulty` reads `threat` of the current sector: use
`MindCampaign.get_sector_state()` (check it has `threat`; add if not).
Shared files: `gdext/input/mod.rs` (G08), `core/sim/*` fire/upgrade sites (G01/G02).

**Commit.** `Gameplay G05 T4b: remaining HintsFragment predicates and hint events`

### T5a. Content info (`?`): Stats model, base stats (Block/Item/Liquid/Status/UnitType base/consumers) and the dialog renderer

**Symptom / acceptance.** Today the `?` dialog (`content_info_dialog.gd`) calls
`MindCampaign.content_info_json`, which does not exist, so it falls back to
bundle name/description/details: no stat rows at all. After:
`MindCampaign.content_info_json(name)` returns Java-ordered categories/stats and
the dialog renders them like `ContentInfoDialog`. Acceptance: `?` on `duo`
shows `General` with Size `1x1`, Health `250`, Build Time `0.292 seconds`,
Build Cost copper 35 (test values below); `copper` shows
Explosiveness 0 %, Flammability 0 %, Radioactivity 0 %, Charge 0 %.

**Java.**
* `world/meta/Stat.java`: ordered list with categories (id = declaration
  index). Transcribe the full list in order (name, category):
  general: health, armor, size, displaySize, buildTime, buildCost, memoryCapacity,
  explosiveness, flammability, radioactivity, charge, heatCapacity, viscosity,
  temperature, flying, speed, buildSpeed, mineSpeed, mineTier, payloadCapacity,
  baseDeflectChance, lightningChance, lightningDamage, abilities, canBoost,
  boostingSpeed ("boostingspeed"), maxUnits, damageMultiplier, healthMultiplier,
  speedMultiplier, reloadMultiplier, buildSpeedMultiplier, reactive, healing, immunities;
  items: itemCapacity, itemsMoved, launchTime, maxConsecutive;
  liquids: liquidCapacity; power: powerCapacity, powerUse, powerDamage, powerRange,
  powerConnections, basePowerGeneration, meltdownTime, warmupTime;
  crafting: tiles, input, output, productionTime, maxEfficiency, drillTier,
  drillSpeed, linkRange, instructions; function: weapons, bullet, speedIncrease,
  repairTime, repairSpeed, range, shootRange, inaccuracy, shots, reload,
  crushDamage, legSplashDamage, targetsAir, targetsGround, damage, status,
  frequency, ammo, ammoCapacity, ammoUse, shieldHealth, cooldownTime,
  regenerationRate, activationTime, moduleTier ("moduletier"), unitType ("unittype"),
  receiveRate; optional: booster, boostEffect, affinities, opposites.
  Label = `bundle("stat." + name.toLowerCase())`; tooltip `stat.<name>.info` when present.
* `StatCat.java`: general, power, liquids, items, crafting, function, optional
  (label `category.<name>`).
* `StatUnit.java`: (name, space, icon): blocks, blocksSquared, worldUnits,
  tilesSecond, powerSecond (icon `[accent]<power glyph>[]`), liquidSecond (`[sky]<liquid>[]`),
  itemsSecond, liquidUnits (`[sky]<liquid>[]`), powerUnits (`[accent]<power>[]`),
  powerEquilibrium, heatUnits (`[red]<waves>[]`), degrees, seconds, minutes,
  shots, perSecond (no space), perMinute (no space), perShot (no space), perLeg,
  perSide, timesSpeed (no space), multiplier (no space), percent (no space),
  shieldHealth, none, items, instant. `localized() = none ? "" : bundle("unit." + name.toLowerCase())`.
* `Stats.java`: `add` appends a value to `map[stat.category][stat]` (ordered
  maps); `remove(stat)`; `replace`; `toMap()` sorts categories by id and stats by
  id (values keep insertion order); `useCategories` (Block sets true); `timePeriod`
  (default -1). Helpers: `add(stat, float, unit)` = `number`; `addPercent(stat, v)` =
  `number((int)(v*100), percent)`; `add(stat, bool)` = `bool`;
  `add(stat, String fmt, args…)` = `string(Strings.format(fmt,args))` (`@` placeholders).
* `StatValues.java:35-145`: `string`; `bool` -> `@yes`/`@no`;
  `fixValue(v) = Strings.autoFixed(v, 3)`; `number(v, unit)` -> cell1
  `(icon? icon+" " : "") + fixValue(v)`, cell2 `(space?" ":"") + unit.localized()`;
  `squared(v, unit)` -> `fixed + "x" + fixed` + unit; `multiplierModifier(v)` ->
  `multStat(v) + "x"` merged; `percentModifier(v)` -> `ammoStat((v-1)*100) + "%"`;
  `items(displayName, stacks)` -> per stack `displayItem(item, amount, displayName)`
  (icon stack with amount overlay; name when displayName);
  `items(timePeriod, stacks)` -> per stack icon + `name\n[lightgray]` +
  `autoFixed(amount/(timePeriod/60), 3)` + `unit.persecond`;
  `items(timePeriod, filter)` -> unlocked, non-hidden items matching, separated by "/";
  `liquid(l, amount, perSecond)` -> icon with `autoFixed(amount,3)` overlay, `/sec`
  when perSecond && amount != 0, then the name; `liquids(filter, amount, perSecond)`
  likewise joined with "/"; `liquids(timePeriod, perSecond, stacks)` amount
  `stack.amount * 60/timePeriod`.
* Base setStats (dump them with the command in "Port today"):
  * `world/Block.java:644-678`: `useCategories = true`; `size "@x@"`; if
    `synthetic()`: `health` (none), `armor` if > 0; if `canBeBuilt() && requirements.length>0`:
    `buildTime = buildTime/60` seconds, `buildCost = items(false, requirements)`;
    if `instantTransfer`: `itemsMoved = unit.instant` text, `maxConsecutive`;
    every consumer `display(stats)` in declaration order; `liquidCapacity` (liquidUnits)
    if hasLiquids; if hasItems: `itemCapacity` (items) when > 0 else
    `bundle("none") + ". " + bundle("unabletoclog")` when `instantTransfer || itemCapacity == 0`.
  * Consumers (`world/consumers/*.java display`): `ConsumeItems` -> `booster ? booster : input`,
    `timePeriod < 0 ? items(items) : items(timePeriod, items)`; `ConsumeLiquid` ->
    `liquid(liquid, amount*60, true)`; `ConsumeLiquids` -> `liquids(1, true, stacks)`;
    `ConsumePower` -> `buffered ? powerCapacity(capacity, none) : usage>0 ? powerUse(usage*60, powerSecond)`;
    `ConsumeCoolant` (a ConsumeLiquidFilter) -> `liquids(filter, amount*60, true)` under `booster` when booster.
  * `type/Item.java setStats`: addPercent explosiveness, flammability, radioactivity, charge.
  * `type/Liquid.java setStats`: addPercent explosiveness, flammability, temperature, heatCapacity, viscosity.
  * `type/StatusEffect.java:98-137` (transcribe all branches: multipliers via
    `addMultModifier` when != 1, damage/healing per second, interval damage +
    frequency, opposites/reactive/affinities lines).
  * `type/UnitType.java:790-853` (this slice: everything except `weapons`
    (T5e) and `abilities` (T5e)): health, armor, speed `speed*60/8` tilesSecond,
    size `squared(hitSize/8, blocks)`, itemCapacity, range
    `autoFixed(maxRange/8, 1)` blocks, crushDamage `*60*5` perSecond,
    legSplashDamage, targetsAir, targetsGround, flying, canBoost (+ boostingSpeed),
    mineSpeed (percent) + mineTier (drillables — render as icons of drillable items),
    buildSpeed (percent), payloadCapacity `squared(sqrt(payloadCapacity/64), blocks)`,
    buildCost `items(firstRequirements)`, immunities `statusEffects(sorted)`.
* `ContentInfoDialog.java:35-135` layout: header icon `iconXLarge` (48) +
  `[accent]` name (+ `\n[gray]name` when setting `console`); patched notice; if
  description: when any stats -> `@category.purpose` accent, description
  `[lightgray]` width 500 padLeft 10, and if `!useCategories && any` ->
  `@category.general`; for each category (sorted) with stats: category label
  accent (only when `useCategories`); each stat row padLeft 10:
  `"[lightgray]" + stat.localized() + ":[] "` then each value cell followed by a
  10 px space; details `[gray]` width 400 padTop 20 (or lock + `unlock.incampaign`
  when locked && hideDetails); credit `content.createdby` gray padTop 40.

**Port today.**
* `ui/dialogs/content_info_dialog.gd` (139 l.): renders `info.stats` rows
  `{category,label,value}` (`_render_stats` l.87) — never populated.
* `core/world/stats.rs`: `StatCat` with the WRONG set (Production/Defense/
  Turrets, no crafting/optional), `Stats::from_specs` used only by its own test;
  `BlockDef.stats: Vec<StatSpec>` (`content/registries/blocks/mod.rs:2120`) is never filled.
* `core/ui/stat_display.rs`: `fix_value`, `auto_fixed` (l.25), `ammo_stat`,
  `mult_stat`, a partial `StatUnit` (keys `unit.powersec` WRONG vs bundle
  `unit.powersecond`).
* Block data: `world/block.rs BlockView` -> `def()` (`BlockDef`: size, health,
  armor, requirements, build_time? check `BlockView::build_time()` l.337),
  `consumers()` (`world/consumers.rs:103`, kinds Items/Liquid/Liquids/Power/Coolant
  + `booster`, `optional`), `kind_data()` (`world/block_kind_data.rs:437`).
  Build a `BlockTable` with `BlockTable::build_default(&registry)` (l.240) — cache it in
  `MindCampaign` (build once per registry change; it is not cheap).
* Bundle: `MindCampaign.bundle` (`gdext/campaign.rs:79`); markup conversion:
  `mind_core::ui::text::render_markup(s, iconc)` (`core/ui/text.rs:293`), used by
  `MindAssets.bundle_markup` (`gdext/assets/atlas.rs:443`).
* Dump all Java `setStats` bodies for reference (run from any dir):
  `cd /home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry && for f in $(grep -rln "public void setStats()" .); do awk '/public void setStats\(\)/{p=1;d=0} p{print FILENAME":"FNR": "$0; n=gsub(/{/,"{"); m=gsub(/}/,"}"); d+=n-m; if(d==0 && /}/){p=0}}' $f; done > /tmp/setstats.txt` (918 lines; 80 classes).

**Changes.**
1. New `core/ui/content_stats.rs` (SPDX + port header citing the Java files):
   ```rust
   pub enum StatCat { General, Power, Liquids, Items, Crafting, Function, Optional }  // Java order; key() -> "general"…
   pub enum Stat { Health, Armor, Size, … }      // full Java list, same order; fn name() -> Java name; fn category()
   pub enum StatUnit { Blocks, BlocksSquared, …, Instant }  // Java list; fn key() -> "unit.<lower>"; fn space(); fn icon() -> Option<&'static str> (markup with glyph name)
   /// One display cell; a stat value is a Vec<Cell> laid out left to right.
   #[derive(Serialize)] #[serde(tag = "k")]
   pub enum Cell { Text { t: String },                         // Mindustry markup, bundle already resolved
                   Icon { region: String, amount: Option<String>, size: u8, content: Option<String> },  // content -> click opens info
                   Space { w: f32 },
                   Row }                                       // line break inside the value
   pub struct Stats { pub use_categories: bool, pub time_period: f32, map: Vec<(StatCat, Vec<(Stat, Vec<Vec<Cell>>)>)> }
   impl Stats { pub fn add(&mut self, stat: Stat, value: Vec<Cell>); pub fn remove(&mut self, stat: Stat);
                pub fn replace(&mut self, stat: Stat, value: Vec<Cell>); pub fn to_sorted(&self) -> Vec<(StatCat, Vec<(Stat, Vec<Vec<Cell>>)>)> }
   pub struct StatCtx<'a> { pub registry: &'a ContentRegistry, pub blocks: &'a BlockTable, pub bundle: &'a dyn Fn(&str) -> String }
   pub mod values { /* number, squared, bool, string, mult_modifier, percent_modifier, items_named, items_per_second, items_filter, liquid, liquids_filter, liquids_per_second — exactly as Java */ }
   pub fn content_stats(ctx: &StatCtx, content: ContentRef) -> Stats
   ```
   `content_stats` dispatches on `ContentType`: Block -> `block_stats` (base
   + family hook `match view.kind_data()` that is empty in this slice), Item,
   Liquid, Status, Unit (base). Each family hook in T5b-T5e appends after the base
   exactly like Java's `super.setStats()` first.
   Move `auto_fixed`/`fix_value`/`ammo_stat`/`mult_stat` usage from
   `stat_display.rs` (re-export; do not duplicate). Do NOT change
   `core/world/stats.rs` (leave a doc line pointing to `content_stats.rs`).
2. `gdext/campaign.rs #[func] content_info_json(&mut self, name: GString) -> GString`:
   resolve the content by name across types (block, item, liquid, unit, status,
   sector, planet), build `Stats`, and emit
   `{"content","type","localized","icon","description","details","credit","use_categories",
     "locked","hide_details","categories":[{"name":"general","label":"General",
     "stats":[{"name":"health","label":"Health","tooltip":"","values":[[cells…]]}]}]}`
   with every `Text` cell passed through `render_markup` so GDScript gets BBCode.
   `Iconc` source: whatever `MindAssets` uses (`grep -n "iconc" client/rust/mind-gdext/src/assets/atlas.rs`);
   if `MindCampaign` cannot reach it, emit raw Mindustry markup and add
   `#[func] MindAssets.markup_text(raw) -> GString` (= `render_markup(raw, &self.iconc)`)
   that the dialog calls per text cell.
3. `content_info_dialog.gd`: render per Java layout; one `HBoxContainer` per stat
   (`padLeft 10`), label `[lightgray]Label:[] `, then cells: Text -> `MindWidgets.label`
   (BBCode), Icon -> `TextureRect` (`size` 24 or 32) with an optional bottom-left
   amount label (outline style; use `MindWidgets.styled_label(amount, "outlineLabel")`
   if that style exists, else plain), click -> `open_content_info(content)`;
   Row -> start a new HBox under the same stat; a 10 px spacer after each value.
   Category labels accent only when `use_categories`. Header icon 48 px.
4. Delete the `_bundle_info` fallback only after the endpoint works (keep it as a
   fallback when `campaign_json` returns `{}` — menu boot without campaign).

**Tests** (`core/ui/content_stats.rs`, using `create_base_content` +
`BlockTable::build_default`; a test bundle closure returning the key):
* `stat_ids`: `Stat::Health as usize == 0`, `Stat::ItemCapacity.category() == StatCat::Items`,
  `Stat::Opposites` is the last entry.
* `copper_item_stats`: four rows in order explosiveness, flammability,
  radioactivity, charge, each value text `"0"` + `"%"` (Java `addPercent((int)(0*100))` -> `number(0, percent)` -> `"0"`, `"%"`, no space).
* `duo_block_stats` (base part only): general `size` = text `"1x1"`, `health` = `"250"`
  (`Blocks.java:3340 health = 250`), `buildTime` = `"0.292"` + `" seconds"`
  (`Block.init` `Block.java:1452-1464`: buildTime = sum(amount * item.cost) * buildCostMultiplier
  = 35 * 0.5 (copper cost, `Items.java`) = 17.5 ticks; /60 = 0.29167; autoFixed(…,3) = "0.292"),
  `buildCost` = one Icon cell `item-copper-ui` amount `"35"`; items category
  `itemCapacity` is absent after T5e (ItemTurret removes it) — in T5a it is present
  with the port's duo `item_capacity` value (assert whatever `BlockDef.item_capacity` holds and note T5e removes it).
* `cryofluid_liquid_stats`: explosiveness `"0"`, flammability `"0"`, temperature `"25"`,
  heatCapacity `"90"`, viscosity `"50"` (`Liquids.java:40-47`: heatCapacity 0.9,
  temperature 0.25; viscosity default 0.5 `Liquid.java:44`), each followed by `"%"`.
* `stat_order_matches_java`: there are 88 `new Stat(` entries in `Stat.java`
  (`grep -c "new Stat(" J/world/meta/Stat.java` = 88); assert `Stat::ALL.len() == 88`.

**Verify in-engine.**
```bash
$G --eval "return JSON.parse_string($MC.content_info_json(\"copper\")).categories[0].stats.size()"   # 4
$G --eval 'return get_node("/root/MindUi").open_dialog("content", JSON.stringify({"content":"duo"}))'; sleep 2
$G --screenshot runs/<date>-s<N>-G05-T5a/duo-info.png
```
Java reference: open the same in the Java client only if no frame exists
(`ls REPO/runs/*ref*/ | grep -i info`).

**Pitfalls.** `Strings.format` uses `@` placeholders (`"@x@"`). `autoFixed`
must match Arc (`ARC/arc-core/src/arc/util/Strings.java autoFixed`); verify the
port's `auto_fixed` against it before relying on it (read both). Units' `uiIcon`
region names: `unit-<name>-ui`; items `item-<name>-ui`; liquids `liquid-<name>-ui`;
status `status-<name>-ui`. Keep the endpoint cheap (no world access).

**Commit.** `Gameplay G05 T5a: Stats model and base content stats for the content info dialog`

### T5b. Content stats: production, crafting and power families

**Java classes** (each `super.setStats()` first; transcribe from
`/tmp/setstats.txt`): `GenericCrafter.java:66` (sets `timePeriod = craftTime`
BEFORE super so consumers render per-second), `AttributeCrafter.java:77`,
`Separator.java:37`, `HeatCrafter.java:36`, `HeatProducer`, `Drill.java:175`
(`drillTier` = drillables, `drillSpeed = 60/drillTime*size*size` itemsSecond,
liquid booster `speedBoosters`), `BurstDrill`, `BeamDrill.java:119`,
`Pump`, `SolidPump.java:58`, `Fracker.java:19`, `WallCrafter.java:76`,
`PowerGenerator.java:80` (`generationType` default `basePowerGeneration`,
`powerProduction*60` powerSecond), `ConsumeGenerator`, `SolarGenerator`,
`ThermalGenerator.java:56`, `NuclearReactor`, `ImpactReactor`,
`VariableReactor`, `HeaterGenerator`, `PowerNode.java:144` (`laserRange`
blocks, `maxNodes`), `BeamNode`, `PowerVoid`, `ItemSource.java:45`.
StatValues needed: `drillables`, `blocks(attr…)`, `speedBoosters`,
`itemBoosters`, `boosters`, `itemEffMultiplier`, `liquidEffMultiplier`,
`displayItemPercent` (`StatValues.java:325-660`).
**Port:** `world/block_kind_data.rs` variants `Crafter(CrafterDef)`,
`AttributeCrafter`, `Drill`, `BurstDrill`, `BeamDrill`, `Pump`, `SolidPump`,
`Fracker`, `WallCrafter`, `Separator`, `ItemSource`, …; power families live in
other modules — find each with `grep -rn "pub struct .*Def\b" client/rust/mind-core/src/world/blocks/power`.
**Changes:** one `fn <family>_stats(ctx, view, def, stats)` per class in a new
`core/ui/content_stats/production.rs` (make `content_stats` a directory module).
**Tests:** `mechanical_drill_stats`: drillSpeed = `60/600*2*2 = 0.4` items/second
(`Blocks.java:2887-2888 drillTime = 600, size = 2`) -> `"0.4"`;
`graphite_press_stats` (`Blocks.java:1045-1050`): productionTime `90/60 = 1.5` seconds, output `graphite`
`1/(90/60) = 0.667` /sec (`autoFixed(0.6667,3) = "0.667"`), input coal 2 -> `"1.333"` /sec.
**Commit.** `Gameplay G05 T5b: production, crafting and power content stats`

### T5c. Content stats: distribution, storage, logic, defense (non-turret)

Java: `Conveyor.java:56` (`itemsMoved = displayedSpeed` itemsSecond),
`StackConveyor`, `Duct`, `DuctRouter`, `DuctBridge`, `DuctJunction` (removes
itemCapacity), `OverflowDuct`, `Junction`, `ItemBridge`, `BufferedItemBridge`,
`MassDriver`, `DirectionalUnloader`, `Unloader`, `CoreBlock`, `LaunchPad`,
`LandingPad`, `LiquidJunction` (removes liquidCapacity), `LogicBlock.java:175`,
`MemoryBlock`, `LogicDisplay`, `Wall` (lightning/deflect), `ShieldWall`,
`MendProjector`, `OverdriveProjector.java:63`, `ForceProjector`,
`RegenProjector.java:91`, `ShockMine`, `ShockwaveTower.java:51`,
`BuildTurret.java:81`. Same structure as T5b, file `content_stats/logistics.rs`.
Test: `conveyor_stats` -> itemsMoved `"5"` + ` items/second` (`Blocks.java:2074 displayedSpeed = 5f`; titanium conveyor 10);
`mender_stats` per `MendProjector.setStats` (read it) with one concrete value.
**Commit.** `Gameplay G05 T5c: logistics, storage, logic and defense content stats`

### T5d. Content stats: unit factories and payload blocks

Java: `UnitFactory`, `Reconstructor.java:92` (`output` = upgrade pairs icon ->
right arrow -> icon, only `unlockedNow` pairs, `Styles.grayPanel` boxes 40 px icons),
`UnitAssembler.java:206`, `UnitAssemblerModule.java:32`, `RepairTurret.java:71`,
`RepairTower.java:36`, `PayloadConveyor`, `PayloadMassDriver`, `Constructor.java:42`.
Port: `BlockDef.unit_plans`, `reconstructor_upgrades`, `assembler_plans`
(`content/registries/blocks/mod.rs:2030-2034`). Cells: add `Cell::Panel { cells: Vec<Cell> }`
if needed for grayPanel boxes (render as a `PanelContainer` with `grayPanel` style).
Test: `additive_reconstructor_lists_unlocked_upgrades` (fully-unlocked registry: dagger->mace first).
**Commit.** `Gameplay G05 T5d: unit factory and payload content stats`

### T5e. Content stats: turrets + ammo, unit weapons and abilities

Java: `BaseTurret.java:93` (shootRange blocks, activationTime), `Turret`,
`ReloadTurret`, `ItemTurret.java:49` (removes itemCapacity; `ammo` =
`StatValues.ammo(ammoTypes, name)`; ammoCapacity `maxAmmo/max(ammoPerShot,1)` shots),
`LiquidTurret`, `PowerTurret`, `LaserTurret.java:26`, `ContinuousTurret.java:29`,
`ContinuousLiquidTurret`, `PayloadAmmoTurret`, `PointDefenseTurret`,
`TractorBeamTurret`; `StatValues.ammo` (read `StatValues.java` from l.700 to the
end: per bullet type damage, splash, pierce, status, frag, lightning, … lines),
`StatValues.weapons(unit, weapons)` (l.655), `abilities` (l.682), `statusText` (l.229).
Port: turret configs in `core/world/blocks/defense/turrets/` (`mod.rs`,
`behavior.rs`); bullets/weapons in `core/weapons/` and the unit registry.
This slice is the largest; split into two commits if it exceeds one sitting
(`T5e-1` turrets+ammo, `T5e-2` unit weapons+abilities).
Test: `duo_ammo_lists_copper_graphite_silicon` (order = `ammoTypes` insertion order
from `Blocks.java duo ammo(...)`), each with its `damage` line value (copper
`BasicBulletType(2.5f, 9)` -> damage `9`).
**Commit.** `Gameplay G05 T5e: turret ammo and unit weapon content stats`

### T6. Planet globe: drag-rotate, wheel zoom, selection does not move the camera, newly-unlocked preset reveal

**Symptom / acceptance.** Today the globe cannot be dragged or zoomed; every
selection snaps the camera onto the sector (`planet_view.gd:322 focus_tile`
sets `camera.position = direction * …`), so sectors on the far side are
reachable only by clicking near the limb repeatedly. After research unlocks a
preset, Java pans to it and announces it; the port does nothing.
After: drag rotates around the planet like Java, the wheel zooms
(`planet.minZoom..2`), selecting a sector does not move the camera, the page
opens looking at the last played sector, and newly unlocked presets are revealed.
Acceptance: drag 300 px right -> the camera's azimuth changes by
`300/9*speed` degrees (speed=1 at the equator); wheel down x3 -> zoom 1.3;
after unlocking `frozenForest`'s tech on a fresh profile the next planet-page open
shows an announce `<lockOpen> Frozen Forest`.

**Java.** `PlanetDialog.java`:
* drag (114-138): ignored while revealing (`showing() && newPresets.peek() != lastSector`), then `newPresets.clear()`;
  ```
  pos = state.camPos; upV = pos.angle(Vec3.Y)      // degrees 0..180
  speed = 1 - |upV - 90| / 90
  pos.rotate(camUp, cx / 9 * speed)                // cx = drag dx in px (Arc, x right)
  amount = cy / 10; amount = clamp(upV + amount, 1, 179) - upV
  pos.rotate(camUp.cpy().rotate(camDir, 90), amount)          // cy = drag dy, Arc y-UP
  ```
  `camUp` = the camera up after `lookAt` (`PlanetRenderer.java:84-87`: `cam.up.set(Vec3.Y)`,
  `cam.lookAt(planet)`, `params.camUp.set(cam.up)`), `camDir` = camera direction.
  `Vec3.rotate(axis, deg)` = right-handed rotation by quaternion
  (`ARC/arc-core/src/arc/math/geom/Vec3.java:437`).
* wheel (140-147): `zoom = clamp(zoom + amountY/10, planet.minZoom, planet.maxZoom)`
  (`minZoom` default 0.5, `maxZoom` 2; serpulo/erekir use defaults; `Planets.java:178`
  sets `minZoom = 0.6` for one planet). `state.zoom = lerpDelta(state.zoom, zoom, 0.4)`.
* camera distance (`PlanetRenderer.java:65`): `camPos.setLength((radius + camRadius) * 4 + (zoom - 1) * (radius + camRadius) * 2)`.
* `show()` (238-291): `lookAt(state.planet.getLastSector())` when any; selecting
  does NOT move the camera (only `launching` does: `lookAt(selected, 0.1)` per frame).
* `uiAlpha = lerpDelta(uiAlpha, zoom < 1.9 ? 1 : 0, 0.1)` (sector icons/labels fade when zoomed out).
* New presets (270-288, 986-1003): on show, for each preset with
  `unlocked() && sector.preset == preset && !alwaysUnlocked && !sector.info.shown && requireUnlock && !sector.hasBase() && preset.planet == state.planet`:
  add its sector, set `info.shown = true`, `saveInfo()`. If any, append
  `planet.getLastSector()`; reverse. Each frame while non-empty: `to = peek()`,
  `presetShow += delta`, `lookAt(to, 0.11)` (slerp), `zoom = 0.75`; at
  `presetShow >= 20` (ticks) and `size > 1` and not yet shown:
  `ui.announce(Iconc.lockOpen + " [accent]" + to.name(), 2)`; at
  `presetShow > 144` (`sectorShowDuration = 60*2.4`) pop, reset. Esc during the
  reveal truncates to the last element (1 = last sector). `lookAt(sector, alpha)`:
  `camPos.slerp(sector.tile.v (rotated by -planet rotation) .setLength(len), alpha)`.

**Port today.** `ui/planet_view.gd` (683 l.): `camera` (Camera3D), `CAM_LENGTH = 4`,
`focus_tile` (l.322), `pick` (l.412), `set_hover`, `_update_atmosphere_camera`.
`planet_dialog.gd:1295 _input` (picking), `_select_view_tile`. Planet data
from `MindCampaign.planet_view` (`gdext/campaign.rs:2675`: `radius`, `cam_radius`,
no `min_zoom`). `PlanetDef.min_zoom` exists (`content/registries/planets.rs:331`), no `max_zoom`.
`SectorInfo.shown` exists (`core/io/json/rules.rs` ~l.1002). Announce:
`MindUi.announce(text, duration)` (`ui_host.rs:351`).

**Changes.**
1. `planet_view.gd`: keep a `cam_pos: Vector3` (planet space) and `zoom := 1.0`,
   `zoom_target := 1.0`. `_process(delta)`: `zoom = lerp(zoom, zoom_target, 1 - pow(1-0.4, delta*60))`
   (`lerpDelta`); camera `position = cam_pos.normalized() * ((r + cr)*4 + (zoom-1)*(r+cr)*2)`,
   `look_at(Vector3.ZERO, Vector3.UP)`, update atmosphere uniforms.
   `drag(relative: Vector2)`: transcribe Java with `cx = relative.x`, `cy = -relative.y`;
   `cam_up = camera.global_transform.basis.y`, `cam_dir = -camera.global_transform.basis.z`;
   Godot `Vector3.rotated(axis.normalized(), deg_to_rad(angle))` matches the
   right-handed rotation. `scroll(amount_y)`: `zoom_target = clamp(zoom_target + amount_y/10, min_zoom, 2.0)`.
   `look_at_tile(tile, alpha := 1.0)`: `cam_pos = cam_pos.slerp(dir * cam_pos.length(), alpha)`
   (alpha 1 = snap). Remove the camera move from `focus_tile` (selection only).
2. `MindCampaign.planet_view`: add `min_zoom` (`def.min_zoom`) and `max_zoom` 2.0
   (add `pub max_zoom: f32` default 2.0 to `PlanetDef` only if you need it elsewhere; else constant).
3. `planet_dialog.gd _input`: left-drag (`InputEventMouseMotion` with
   `button_mask & MOUSE_BUTTON_MASK_LEFT`, not over UI) -> `view.drag(event.relative)`,
   mark `_dragged = true`; a release after a drag must not select (Arc `tap` vs drag);
   wheel -> `view.scroll(+1 / -1)`. Guard with `is_top()` (T2).
4. On page show (`_show_planet` first build per open): `view.look_at_tile(last sector tile)`
   where last sector = `Planet.getLastSector()` (`J/type/Planet.java:312-317`):
   `sectors[min(settings.getInt(planet.name + "-last-sector", startSector), size-1)]`
   (written by `setLastSector` when a sector is played; read it with
   `MindUi.settings_get_i32("serpulo-last-sector")` — if the port never writes this
   key, `grep -rn "last-sector" client/rust` and add the write next to where the
   played sector is set in `start_sector`, citing `Planet.setLastSector`).
5. New presets: Rust `#[func] MindCampaign.take_new_presets(planet: GString) -> PackedInt32Array`
   implementing the Java filter, setting `info.shown = true` and saving the info
   (`sector.save_info(&mut self.settings, planet_name)`, then `self.persist()`),
   returning sector ids in reveal order (the Java reversed list: last-played
   sector LAST after reversal -> i.e. return `[presets in reverse content order…]` exactly as
   Java: build `newPresets` in `content.sectors()` order, push last sector, reverse,
   then the dialog pops from the END (`peek/pop` = last element)). The GDScript
   reveal loop (in `planet_dialog.gd _process`) mirrors the per-frame Java code
   with `presetShow` counted in ticks (`delta * 60`).
   Call it from `shown()` via `call_deferred` (it mutates and persists).

**Tests.** Rust: `take_new_presets_marks_shown` in `gdext`? — gdext has no tests;
put the filter in `core/game/planet.rs` as `pub fn new_presets(planet: &Planet, registry: &ContentRegistry) -> Vec<u16>`
and test it on the fixture campaign (unlock a requireUnlock preset without base ->
returned once, then `info.shown` true -> not returned).

**Verify in-engine.**
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'return get_node("/root/Spine/Planet/PlanetView").cam_pos'
python3 REPO/tools/gmcp.py godot_input '{"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":600,"y":300},"coords":"viewport","pressed":true}}'
# motion: use godot_input "mouse_motion" if supported (check tools/gmcp.py --help); else call view.drag(Vector2(300,0)) via eval
$G --eval 'get_node("/root/Spine/Planet/PlanetView").drag(Vector2(300, 0)); return get_node("/root/Spine/Planet/PlanetView").cam_pos'
$G --screenshot runs/<date>-s<N>-G05-T6/dragged.png
```
Expected: `cam_pos` rotated ~33.3° * speed about the up axis.

**Pitfalls.** `look_at` with a camera exactly on the Y axis is degenerate —
the Java clamp (1..179°) prevents it; keep it. `_input` is global: guard with
`is_top()` and `_over_ui`.

**Commit.** `Gameplay G05 T6: planet globe drag/zoom, no camera snap on select, new preset reveal`

### T7. Planet page: rename pencil and sector icon picker

**Symptom / acceptance.** Today neither exists. After: the selected-sector title
row has a pencil (only when `preset == null || !preset.requireUnlock`) opening
`@sectors.rename` text input (max 32) and an icon button (tooltip
`@sector.changeicon`) opening the icon grid; choices persist in the sector info
(`<planet>-s-<id>-info`) and show in the panel, sector list and globe.
Acceptance: rename a based non-preset sector to "Base A", restart the session
(`tools/session-play.sh N --play`), open the planet page: the panel title and the
sector list show "Base A".

**Java.** `PlanetDialog.java:1199-1283`: title row `[accent]name` padLeft 3;
pencil `Icon.pencilSmall` `Styles.clearNonei` 40x40 padLeft 4 ->
`ui.showTextInput("@sectors.rename", "@name", 32, sector.name(), v -> { sector.setName(v); updateSelected(); rebuildList(); })`;
icon button (drawable = contentIcon uiIcon, else `Icon.icons[info.icon + "Small"]`,
else `Icon.noneSmall`), 40x40, opens a fill-parent `Dialog("")` with a pane of 48 px
`Styles.squareTogglei` buttons: `Icon.none` (checked when both null; sets both
null), then `accessibleIcons` (Vars.java:164-169, 31 names; checked when
`info.icon == key`; sets `icon = key, contentIcon = null`), then for each
`defaultContentIcons` type (item, liquid, block, unit, status): a 3 px accent
rule then every `!isHidden() && unlocked()` content (iconMed 32; sets
`contentIcon = u, icon = null`); `cols = min(20, screenW / 52)`; any choice ->
`saveInfo(); hide(); updateSelected(); rebuildList()`; `@back` 210x64.
`Sector.setName(name)`: `info.name = name; saveInfo()`.

**Port today.** `SectorInfo.name/icon/content_icon` (`core/io/json/rules.rs:994-998`);
`SectorView.named/icon/content_icon` (`core/ui/campaign.rs:91`);
`Sector::save_info(&self, settings, planet_name)` (`core/game/sector.rs:584`);
`Universe::sector_mut(planet, id)` (`core/game/universe.rs:393`);
`MindUi.show_text_input` + `text_input_result` signal; `icon_select_dialog.gd`
(menu-oriented: no none button, no content icons, emits `icon_selected(region)`).

**Changes.**
1. `gdext/campaign.rs`: `#[func] set_sector_name(planet: GString, id: i32, name: GString) -> bool`
   and `#[func] set_sector_icon(planet: GString, id: i32, icon: GString, content_icon: GString) -> bool`
   (empty strings = None). Mutate `self.campaign.sector_mut(..).info.info`, call
   `save_info(&mut self.settings, planet)`, `self.persist()`, and
   `self.push_campaign_to_runtime()` when the sector is the played one.
2. `planet_dialog.gd _build_sector_panel` title row: after the title,
   a growX spacer, the pencil (`MindWidgets.image_button("pencilSmall", "clearNonei")` 40x40 —
   if the region is missing use `glyph_button("pencil")`), then the icon button
   (40x40, `clearNonei`, tooltip `_t("@sector.changeicon")`).
   Pencil -> `MindUi.show_text_input(_t("@sectors.rename"), _t("@name"), 32, name, false, false)`
   and a one-shot `text_input_result` handler (guard with a `_pending_rename` id like
   `schematics_dialog.gd:234`) -> `set_sector_name` -> `refresh_page()`.
3. Icon picker: extend `icon_select_dialog.gd` with a mode set from context
   `{"mode":"sector","icon":…,"content_icon":…}`: first `none` button, the
   accessible icons, then the five content sections from
   `MindCampaign.database_json` entries filtered `unlocked` and not hidden (reuse its
   `content_type`/`name` fields), `squareTogglei` style, 48 px cells, columns
   `min(20, int(viewport_width / 52))`, checked state; emit
   `sector_icon_selected(icon, content_icon)`; the existing menu mode stays unchanged.
   Panel: open with `open_dialog("icon_select", ctx)` (stacked over the planet page after T2).
4. The sector list (`_fill_sector_entries`) and the globe icon (`planet_view.gd _add_icon`)
   already read `icon`/`content_icon`/`named` — verify after the change.

**Tests.** Parse gate. Rust: none beyond clippy (pure setters).

**Verify in-engine.** Rename a based numbered sector via eval of the handler path
(`dialog("planet")._on_rename_result("Base A")` or the real text-input flow),
restart with `--play`, then
`$G --eval 'return get_node("/root/Spine/Ui/UiRoot").dialog("planet").campaign_section("sectors").filter(func(s): return s.id == <id>)[0].name'` -> `"Base A"`; Ground Zero (170) panel shows no pencil.
Restore the profile afterwards (env §4.3).

**Pitfalls.** Ground Zero has `requireUnlock = true` (default,
`J/type/SectorPreset.java:31`; `SectorPresets.java:22-30` does not override it),
so it gets NO pencil — only the icon button. Test rename on a numbered
(non-preset) sector with a base (after T9 a generated sector you launched to), or
on a preset with `requireUnlock = false` if any exists; Ground Zero must show
the icon button only.
`SectorView` needs `require_unlock` (exists).

**Commit.** `Gameplay G05 T7: planet page sector rename and icon picker`

### T8. Sector stats dialog (`PlanetDialog.showStats`) with Abandon

**Symptom / acceptance.** Today `@stats` opens `full_text` with plain lines
(`planet_dialog.gd:1076`). After: a `sector_stats` dialog titled with the sector
name: preset description (420 wrap), `Time: [accent]h:mm:ss`, attempts, wave,
threat, resources icons, lockdown text, production/export/import rows as
`icon amount/min` (3 per row, item-id order, only `>1`), export destination and
import source lines, stored items (4 per row), `@back` + `@sector.abandon`
(when based) -> confirm -> abandon.
Acceptance: Ground Zero stats shows `Time:` with the save playtime and the
Abandon button; screenshot matches the Java layout.

**Java.** `PlanetDialog.java:1026-1147` (transcribed in the inventory table):
```
dialog = BaseDialog(sector.name()); cont.pane(c -> { c.defaults().padBottom(5)
 if preset && preset.description: add(displayDescription).width(420).wrap().left()
 add(bundle("sectors.time") + " [accent]" + sector.save.getPlayTime()).left()       // Strings.formatMillis(meta.timePlayed or totalPlaytime if current)
 if info.attempts > 0: "sectors.attempts [accent]n"
 if info.waves && hasBase(): "sectors.wave [accent]n"
 if isAttacked() || !hasBase(): "sectors.threat [accent]" + displayThreat()
 if save != null && info.resources.any(): "@sectors.resources" row, then icons iconSmall(24) padRight 3, padLeft 10
 if isAttacked() && !isBeingPlayed(): formatIcons(bundle("sector.lockdown")) wrap fillX padBottom 10
 displayItems(production, "@sectors.production")
 displayItems(export, "@sectors.export", + if destination != null && destination.hasBase(): " " + (iconChar? iconChar+" ") + destination.name() padLeft 10)
 if hasBase(): displayItems(imports, "@sectors.import", + for each eachImport source: same line)
 if hasBase() && items.total > 0: "@sectors.stored" then rows icon + formatAmount(max(n,0)) lightGray, 4 per row, padLeft 10
}); addCloseButton(); if hasBase(): buttons.button("@sector.abandon", Icon.cancel, () -> abandonSectorConfirm(sector, dialog::hide))
displayItems: per item in content.items() order: total = (int)(stat.mean * 60); if total > 1: icon padRight 3 + formatAmount(total) + " " + bundle("unit.perminute") lightGray padRight 3; 3 per row; header name, builder lines, then table padLeft 10
```
`Strings.formatMillis(ms)` (`ARC/arc-core/src/arc/util/Strings.java:814`):
`hours` unpadded, `:mm`, `:ss` (2-digit) — e.g. 3_725_000 -> `"1:02:05"`, 65_000 -> `"0:01:05"`.
`abandonSectorConfirm` (1149-1171): confirm `@sector.abandon.confirm`; being
played -> hide planet page and kill the player's cores after 7 ticks; else clear
`info.items`, `hasCore = false`, `production`, `saveInfo()`; update panel/list.

**Port today.** `SectorStatsView` (`core/ui/campaign.rs:176`; rows built by
`sector_stats_view` l.660 in IndexMap order — not item-id order); no playtime;
`Paths::sector_save(planet, id)` (`core/io/fs.rs:452`) + `SaveIo::get_meta(fs, path)`
(`core/io/save/mod.rs:165`, `SaveMeta.time_played`); `MindCampaign.abandon_sector()`
(`gdext/campaign.rs:1929`, abandons the PLAYED sector — check whether it accepts an id;
the off-play branch needs `(planet, id)`). Pause menu already offers Abandon via
`paused_dialog.gd:270` (uses the played sector).

**Changes.**
1. `sector_stats_view`: sort the three rate lists and `stored` by item id
   (`registry.item_id(name)`), keep `> 1` filter; add `playtime_ms: i64` (filled
   in gdext), `destination: Option<String>` (destination sector display name when it
   has a base), `import_sources: Vec<String>` (Java `info.eachImport(planet, …)`: read
   `J/game/SectorInfo.java eachImport` and port it; names via `Sector.name()` rules),
   `lockdown: bool` (attacked && !being_played), `preset_description_key: Option<String>`.
   Regenerate the UI golden.
2. gdext `campaign_views_json`: after building views, for every based sector fill
   `stats.playtime_ms` from the save meta (`Paths::new(user://).sector_save(planet, id)`,
   `get_meta`), or `self.saves.get_total_playtime()` for the played sector. (Only
   for sectors with `has_save`; skip on read error.)
3. New dialog `ui/dialogs/sector_stats_dialog.gd` + `scenes/ui/dialogs/sector_stats_dialog.tscn`
   (instance of `mind_dialog_base.tscn`), manifest entry
   `{"name":"sector_stats","scene":"res://scenes/ui/dialogs/sector_stats_dialog.tscn","pause":false,"menu_openable":false,"args":["planet:Str","sector:Int"]}`
   in `ui/dialogs_manifest.json`, a static instance in `scenes/ui/ui_root.tscn`
   (copy how `full_text` is instanced), and `"sector_stats"` in the
   `EXPECTED_*_DIALOGS` list of `core/ui/manifest.rs`. Render per the Java block above
   (format playtime with a GDScript port of `formatMillis`).
4. Abandon: `#[func] MindCampaign.abandon_sector_at(planet, id) -> bool`: if it is
   the played sector delegate to `abandon_sector()`; else clear `items`,
   `has_core = false`, `production`, `save_info`, `persist`. Dialog: confirm via
   `MindUi.show_confirm(_t("@sector.abandon.confirm"))` + pending flag, then call it,
   close stats, `refresh_page()` the planet dialog.
5. `planet_dialog.gd`: `@stats` opens `sector_stats` with `{"planet","sector"}`; delete `_show_sector_stats`, `_stat_line`.

**Tests.** `cargo test -p mind-core --lib ui::manifest` (expected dialogs),
`ui_goldens_match` after regeneration; Rust test `sector_stats_rows_in_item_order`
(fixture sector with production `{silicon: mean 1.0, copper: mean 1.0}` -> rows
`[copper 60, silicon 60]`; `mean 0.01` -> `(int)0.6 = 0` filtered).

**Verify in-engine.** Planet page -> Ground Zero -> `$G --eval 'get_node("/root/Spine/Ui/UiRoot").dialog("planet")._show_stats_for(170); return true'`
(or press the button via its node) -> screenshot; compare with a Java stats frame if present.

**Pitfalls.** Adding a dialog needs all three registrations (env §3.1) or the
manifest test fails. Sector `.msav` meta read is I/O: only in `campaign_views_json`
(already heavy) — never per frame.

**Commit.** `Gameplay G05 T8: sector stats dialog with item rows, playtime and abandon`

### T9a. PAR-0112 step 1: generated sectors use their own sector rect, seed and size

**Symptom / acceptance.** Today every non-preset sector is the same 128x128 map
(`gdext/sim_host.rs:1376 const SECTOR_SIZE = 128`, `generate_sector(..., 1, ...)`,
`core/maps/planet/mod.rs:52` ignores `sector`). After: terrain depends on the
sector (Java `sector.rect.project` per tile, `rand.setSeed(sec.id + seedOffset + baseSeed)`),
size = `getSectorSize`, `seedOffset = 0`.
Acceptance: test `sectors_differ_by_id` passes; in-engine two numbered sectors
screenshot differently and `get_state` size reports the Java size.

**Java.**
* `type/Sector.java:42-53` constructor: `rect = tile.corners.length == 0 ? SectorRect(1, 0, Y, X, 0) : makeRect()`.
* `Sector.java:255-282 makeRect()`:
  ```
  corners[i] = tile.corners[i].v.cpy().setLength(planet.radius)
  center = sum(corners) / n
  radius = center.dst(corners[0]) * 0.98
  plane.set(corners[0], corners[2], corners[4])
  top   = plane.project(center + (0,1,0)) - center, setLength(radius)
  right = plane.project(center.rotate(Vec3.Y, -4°)) - center, setLength(radius)    // rotate a COPY of center
  rotation = (corners[1] - center).angle(top)    // degrees, unused by project
  ```
* `ARC/extensions/g3d/src/arc/math/geom/Plane.java:53-75`:
  `set(p1,p2,p3): normal = (p1 - p2).crs(p2 - p3).nor(); d = -p1.dot(normal)`;
  `project(v): npd = normal.dot(v) + d; return v - npd*normal`.
* `Vec3.rotate(Vec3.Y, deg)` (quaternion, right-handed): `x' = x cosθ + z sinθ; y' = y; z' = -x sinθ + z cosθ`, θ = deg in radians.
* `SectorRect.project(x, y)` = `center + right*(2x-1) + top*(2y-1)` (already ported: `core/maps/generators/planet.rs` `SectorRect::project`).
* `PlanetGenerator.java:177-180 getSectorSize` = `(int)(rect.radius * getSizeScl())`, `+1` if odd;
  `getSizeScl` 3200 default (Serpulo), Erekir `2000*1.07*6/5`, Tantros 2000 (ported as `size_scl`).
* `PlanetGenerator.java:182-196 generate`: `seed = params.seedOffset + baseSeed`;
  `rand.setSeed(sec.id + params.seedOffset + baseSeed)`; tile position
  `rect.project(x / width, y / height)`. `World.loadSector` passes `WorldParams` with `seedOffset = 0` default.

**Port today.** `core/maps/generators/planet.rs`: `SectorView` trait,
`FlatSectorView { id, radius, threat, tile_v, has_enemy_base, has_base }`
(`rect()` returns the default plane with only `radius`), `PlanetGenerator::prepare/run`
already seed with `sector.id() + seed_offset + base_seed`. Serpulo/Erekir/Tantros/Asteroid
hold `pub sector: FlatSectorView` (`serpulo.rs:382`, `erekir.rs:143`, `tantros.rs:31`, `asteroid.rs:32`).
`generator_for(planet, seed) -> Box<dyn WorldGenerator>` (`maps/planet/mod.rs`);
`WorldGenerator` trait (`maps/generators/mod.rs:33`). `PlanetGrid::create(size)`
(`core/render/g3d/grid.rs:159`), `Ptile.corners`/`v`, `Corner.v`.

**Changes.**
1. `FlatSectorView`: add `pub rect: Option<SectorRect>`; `rect()` returns it when
   set (else the current default). Default `None` keeps every test unchanged.
2. New `core/game/sector_rect.rs` (or in `core/maps/generators/planet.rs`):
   `pub fn make_rect(grid: &PlanetGrid, tile: usize, planet_radius: f32) -> SectorRect`
   transcribing `makeRect` with f32 math in Java order (no FMA tricks; plain ops).
3. `WorldGenerator` trait: add `fn set_sector(&mut self, _view: FlatSectorView) {}`
   (default no-op) and `fn sector_size_scl(&self) -> f32 { 3200.0 }`; implement both
   for the four planet generators (`self.sector = view`; return `self.planet.size_scl`).
4. `generate_sector(planet, sector, seed_offset, content)` -> new signature
   `generate_sector(planet: &str, sector: u16, content: &ContentRegistry, view_extra: SectorExtras) -> Option<GeneratedSector>`:
   build the grid for the planet (`content.planet_by_name(planet)?.sector_tiles`),
   `rect = make_rect(&grid, sector as usize, def.radius)`, `size = get_sector_size(rect.radius)`
   with the generator's size scale, `view = FlatSectorView { id: sector as u32, rect: Some(rect),
   tile_v: grid.tiles[sector].v, threat, has_enemy_base, has_base, radius: rect.radius }`
   (`SectorExtras { threat, has_enemy_base, has_base }` comes from the campaign
   `Sector`: `threat`, `generate_enemy_base`/`has_enemy_base` — find the field with
   `grep -n "enemy_base" client/rust/mind-core/src/game/sector.rs`), `WorldParams { seed_offset: 0, width: size, height: size }`,
   `generator.set_sector(view)`. Return `GeneratedSector { grid, rules, spawn }` (spawn: T9b).
   Keep the old function name for callers by updating them all
   (`grep -rn "generate_sector(" client/rust`).
5. `gdext/sim_host.rs load_sector` (~l.1376-1400): drop `SECTOR_SIZE`; pass the
   campaign sector's extras (read from `MindCampaign` — `load_sector` is on SimHost;
   find how it reaches campaign state: it already loads presets by `planet_name/sector`;
   add a `#[func] set_next_sector_extras(threat, has_enemy_base)` called by
   `MindCampaign.start_sector*` before `load_sector`, or read the runtime campaign
   resource if present — choose the variant that already exists:
   `grep -n "CampaignRuntime" client/rust/mind-gdext/src/sim_host.rs`).

**Tests** (`core/maps/planet/mod.rs` tests):
* `sectors_differ_by_id`: serpulo sectors 1 and 2 -> `grid.tiles` floor vectors differ.
* `sector_size_is_even_and_from_rect`: serpulo sector 1 size `% 2 == 0` and equals
  `get_sector_size(make_rect(..).radius)`. UNVERIFIED absolute value: executor
  confirms against Java with a numbered sector in the imported profile —
  `jq` is not usable on settings.bin; instead read `SectorInfo.last_width` of a
  numbered based sector via `$MC.get_sector_state()` while it is loaded (if the
  imported profile has none, record "no Java reference size").
* `make_rect_center_projects`: `rect.project(0.5, 0.5) == rect.center` (1e-5) and
  `|rect.top| == rect.radius` (1e-4) and `top · plane_normal ≈ 0`.

**Verify in-engine.** Two non-preset selectable sectors (list:
`$G --eval 'return get_node("/root/Spine/Ui/UiRoot").dialog("planet").campaign_section("sectors").filter(func(s): return s.preset == null and s.selectable).map(func(s): return s.id)'`
after opening the planet page), then for each: `load_sector` + `start_sector`
(fresh recipe, env §4.1), screenshot, `return $H.get_state_json().length()` is NOT
allowed on campaign maps — use `return $H.tile_debug(0,0)` and the screenshot.

**Pitfalls.** Preset sectors keep their `.msav` (`load_preset_map_grid`); only
the `None` branch changes. Generation of a ~300x300 sector is slower: fine.
Map-generation goldens: run `cargo test -p mind-core --lib maps::` and
`cargo run -q -p mind-headless -- parity goldens`; regenerate only goldens that
cover generated campaign sectors and name them in the commit.

**Commit.** `PAR-0112: generated sectors use their sector rect, seed and size (T9a)`

### T9b. PAR-0112 step 2: launch loadout core at the generator spawn

**Java.** `SerpuloPlanetGenerator.java:719 Schematics.placeLaunchLoadout(spawn.x, spawn.y)`;
`game/Schematics.java:465-469`: `placeLoadout(universe.getLastLoadout(), x, y, defaultTeam)`
then `tile.build.items.add(universe.getLaunchResources())`.
`Universe.getLastLoadout()` (`game/Universe.java:129-132`): `lastLoadout ?: (sector.planet.generator.defaultLoadout ?: Loadouts.basicShard)`.
Erekir `ErekirPlanetGenerator.generate` also places its loadout: `grep -n placeLaunchLoadout J/maps/planet/ErekirPlanetGenerator.java`.

**Port today.** `serpulo.rs:883 self.spawn = (…)`, `:1333` comment "no tile
effect"; `Sim::place_loadout(schem, x, y, team, check)` (`core/sim/mod.rs:549`);
`MindSimHost.place_launch_loadout(schematic_index, stacks_json)` (`gdext/sim_host.rs:2116`)
replaces the EXISTING core (fails with "no default-team core to replace" on a coreless map).

**Changes.**
1. `WorldGenerator`: `fn player_spawn(&self) -> Option<(i32, i32)> { None }`; Serpulo
   returns `Some(self.spawn)`; Erekir its spawn. `GeneratedSector.spawn: Option<(i32,i32)>`.
2. `MindSimHost`: store `generated_spawn: Option<(i32,i32)>` from `load_sector`;
   `place_launch_loadout`: when no default-team core exists and
   `generated_spawn` is set, place at the spawn (`self.sim.place_loadout(&schem, x, y, team, false)`),
   then add the stacks. The planet dialog's free-launch path (`_launch` without a
   launcher) must also place the default loadout: in `MindCampaign.start_sector`
   for a NewGame on a generated sector, call `place_launch_loadout` with the
   planet's default loadout index (`universe.clearLoadoutInfo` => basicShard /
   `defaultLoadout`) and the launch resources (`get_launch_resource_stacks`).
   Read `start_sector_with_loadout` (`gdext/campaign.rs:615`) to place the call
   where the T2 (session 5) launcher branch already places the loadout.
**Tests.** `generated_sector_reports_spawn`: serpulo sector 1 `spawn.is_some()` and
the spawn tile is not solid.
**Verify.** Free-launch into a numbered sector neighbouring Ground Zero after
giving Ground Zero a foundation core (session-5 T2 recipe in
`audit-campaign-import_summary.md`), then `return $H.best_core_position()` != (-1,-1)
and `return $H.core_items_json()` = launch resources.
**Commit.** `PAR-0112: place the launch loadout core at the generated sector spawn (T9b)`

### T9c. PAR-0112 step 3: enemy cores for enemy-base sectors

**Java.** `SerpuloPlanetGenerator.java:721-751`: enemy spawn rooms; if
`sector.hasEnemyBase()`: `basegen.generate(tiles, enemies.map(room tile), spawn, waveTeam, sector, difficulty)`,
`state.rules.attackMode = sector.info.attack = true`; else `winWave = …`.
`BaseGenerator.java:42-68`: `if bases.cores empty return; Mathf.rand.setSeed(sector.id); nextDouble();`
for each enemy core tile: `clearOverlay(); placeLoadout(bases.cores.getFrac((difficulty + rand.range(0.4))/1.4).schematic, x, y, team, false)`;
fill the core with `itemCapacity` of every item. (`Seq.getFrac(f)` = `items[clamp((int)(f*size), 0, size-1)]`.)
The rest of `BaseGenerator.generate` (random parts passes, walls) is deferred (§3.3).
**Port today.** `core/ai/base_registry.rs` `BaseRegistry { cores, parts }`,
`BaseRegistry::load(...)` (l.86); `core/maps/generators/base.rs BaseGenerator::generate`
logs only; `serpulo.rs` keeps `enemy_spawns` (overlay = spawn) and `generate_rules`
reads `has_enemy_base` (l.1359).
**Changes.** `WorldGenerator`: `fn enemy_core_tiles(&self) -> Vec<(i32,i32)>`
(Serpulo: the enemy rooms when `sector.has_enemy_base()`); in `load_sector` after
the grid is installed: load `BaseRegistry` (cache it), seed `Mathf.rand` port
(`ArcRand::new(sector as u64)` + one `next_double()`), and per tile place the
picked core schematic via `Sim::place_loadout(.., wave_team, false)` and fill
items to capacity; when the enemy spawn overlay sits on a core tile, Java clears
the overlay first. `rules.attack_mode = true` is already written by `generate_rules`
(verify).
**Tests.** `enemy_base_sector_lists_core_tiles` (pick a serpulo sector id whose
`generate_enemy_base` is true in the base campaign).
**Verify.** Launch to such a sector (needs a neighbouring based sector with a
>=4 core — recipe from T9b) and `return $MC.get_hud_state()` shows attack mode / enemy core count.
**Commit.** `PAR-0112: enemy cores for generated enemy-base sectors (T9c)`

### T10a. `SchematicImage` widget + Configure Loadout 200 px toggles (PAR-0025/26/27 cosmetic remainder, PAR-0085 cosmetic)

**Java.** `SchematicsDialog.java` `SchematicImage` (inner class, end of file):
`scaling = 16`, `thickness = 4`, `borderColor = Pal.gray (#454545)`; draw:
background `sprites/schematic-background.png` tiled (`u2 = width/16`, `v2 = height/16`),
then the preview texture (fit), then a 4 px rectangle border, `Pal.accent` when
the parent button is hovered. Preview (`game/Schematics.java:200-290 getBuffer`):
`padding = 2`, `resolution = 32` px per tile; a 1-px-per-tile black silhouette of
every occupied tile (block footprint with offset `-(size-1)/2`) at `(padding/2 + wx + 0.5)`,
stretched over the buffer (linear filter => soft shadow), then each tile's plan
region (`fullIcon`, rotated `rotation*90` when `block.rotate && rotateDraw`) at
`(x + 1.5, y + 1.5)` tiles * 32 px, plus `drawPlanConfigTop` (skip: config overlays,
record). `LaunchLoadoutDialog.java:160-195`: `cols = max((int)(screenW / 230), 1)`; each
allowed loadout (skip schematics with a tile whose block does not support the
planet env or is not on the planet) is a `Styles.togglet` button containing a
`SchematicImage`, button group (single selection), `pad(4)`, `size(200)`,
checked = selected; a new row every `cols` buttons; loadouts come from
`schematics.getLoadouts()` entries whose core size `<= core.size`.
**Port.** `assets/sprites/schematic-background.png` exists; no widget. Block
icon regions: `block-<name>-full` (fullIcon) and `block-<name>-ui` exist in the atlas
(verified for duo).
`SchematicView` (`core/ui/campaign.rs:227`) has no tile list.
**Changes.** (1) `SchematicView.tiles: Vec<SchematicTileView { block: String, x: i32, y: i32, rotation: u8, size: i32, rotate: bool }>`
using `crate::world::block::kind_rotates(def.kind)` (`core/world/block.rs:520`)
for `rotate` (record that `rotateDraw` is assumed true). Regenerate the UI golden.
(2) `ui/widgets/schematic_image.gd` (`class_name MindSchematicImage extends Control`)
drawing in `_draw()`: tiled background (`draw_texture_rect(tex, rect, true)` with a
16 px tile scale), the silhouette (black rects alpha 1 at tile cells, scaled; a
1-px-per-tile `Image` upscaled with linear filtering reproduces the blur: build
`Image.create(w+2, h+2)`, `ImageTexture`, draw it over the full rect with
`texture_filter = LINEAR`), the icons (`draw_set_transform` for rotation), border 4 px.
Cache the composed texture per schematic index in a static dictionary.
(3) `launch_loadout_dialog.gd`: replace name buttons with the image toggles per Java.
**Commit.** `Gameplay G05 T10a: SchematicImage widget and Configure Loadout schematic toggles`

### T10b. Schematics dialog: tags row, card grid, in-game use (PAR-0143)

**Java.** `SchematicsDialog.java:40-210`: title `@schematics`, `shouldPause`,
close button + `@schematic.import` (Icon.download) in an overlay button row
(`makeButtonOverlay`); `shown`: clear search, `setup()`. Layout: search row
(`Icon.zoom` + field, hint `@schematic.search`, right-click clears); tags row
(`@schematic.tags` label, horizontal toggle buttons `Styles.togglet` height
`tagh = 42` per tag in `tags` (settings JSON `schematic-tags`), pencil button
-> `showAllTags`); card pane: `cols = max((int)(screenW / 230), 1)`; for each
schematic in `schematics.all()` whose labels contain ALL selected tags and whose
name (symbols stripped: `ignoreSymbols` regex) contains the search: a `Styles.flati`
button (up = `Tex.pane`) with a 50-px icon row (`Icon.info` -> info dialog,
`Icon.upload` -> export, `Icon.pencil` -> edit, `Icon.trash` -> confirm delete
(`@schematic.delete.confirm`) or `Icon.link` for workshop), then a 200x200 stack:
`SchematicImage` + top `Styles.black3` name label (outline, ellipsis, max 192).
Click: menu -> `showInfo(s)`; in game -> `!rules.schematicsAllowed ?
showInfo("@schematic.disabled") : control.input.useSchematic(s); hide()`.
Enter in the search field uses the first match. Empty: `@none.found` (filtered) or `@none`.
**Port.** `ui/dialogs/schematics_dialog.gd` (list + details; actions
export/edit/icon tag/delete/configure loadout exist as functions l.155-246 —
reuse them). In-game use needs an input seam owned by G08:
`MindInput.use_schematic(index)` — DEPENDENCY on G08, which adds this desktop seam (not present today:
`grep -n "fn use_schematic" client/rust/mind-gdext/src/input/mod.rs`).
**Changes.** Rebuild the dialog per Java with `MindSchematicImage` cards; tags from
`MindUi.settings_get_text("schematic-tags", "[]")` (JSON array); click in game ->
`if $I.has_method("use_schematic"): $I.use_schematic(index); close` else toast
`@schematic.disabled`-style info and record the missing seam (cross-area G08).
`showAllTags` editor: defer (record) unless trivial.
**Commit.** `Fixes PAR-0143: schematics dialog tags row and card grid (T10b)` (use `PAR-0143:` if the use path is still blocked on G08).

### T11. Core Database: icon planet tabs and current-planet default (PAR-0144)

**Java.** `DatabaseDialog.java:37-125`: `shown`: `checkTabList(); sortContents();`
if campaign and the tab list contains the current planet -> `tab = planet`; else if in
game and `rules.planet` is a tab -> that. Tab buttons: `Styles.clearNoneTogglei`
50x50, image `iconMed` 32: sun -> `Icon.eyeSmall`; planet -> `Icon.icons.get(p.icon, Icon.commandRally)`
tinted `p.iconColor`; tooltip `@all` / localized name; 10 per row. Entry images:
locked -> `Icon.lock` `Pal.gray`; in game: patched -> file icon 12 px 50% white;
banned -> `Icon.cancel` scarlet 60%; sorted banned last when in game.
**Port.** `database_dialog.gd` `_build_tabs` (text buttons 90x40), `_tab` keeps
its value between opens; `core/ui/database.rs DatabaseTabView` lacks `icon`.
`PlanetDef.icon` (`content/registries/planets.rs:337`, Java default `"planet"`).
**Changes.** add `icon: String` to `DatabaseTabView` (`"eyeSmall"` for sun, else
`def.icon`); tabs become 50x50 `clearNoneTogglei` buttons with
`MindWidgets.glyph(icon, 32, color)`; tooltip as Java. `shown()`: when
`MindCampaign.get_sector_state().campaign` select the played planet's tab.
Banned cross: only if the rules expose banned blocks (`grep -n banned_blocks client/rust/mind-core/src/game/rules.rs`); else record.
**Tests.** `database_view` unit test: tab 0 icon `"eyeSmall"`, serpulo tab icon = def icon.
Regenerate any database golden if one exists (`grep -rn database client/rust/mind-core/tests/goldens`).
**Commit.** `Fixes PAR-0144: database planet tabs render icons and default to the current planet (T11)`

### T12a. Planet globe: hex mesh body with generator colours and the planet shader (PAR-0142 part 1)

**Java.** `HexMesh(planet, 6)` = `MeshBuilder.buildHex(planet.generator, 6, planet.radius, 0.2)`
(`graphics/g3d/HexMesh.java`, `MeshBuilder.java:84-190`):
```
grid = PlanetGrid.create(divisions); generator.seed = generator.baseSeed
heights[corner] = (1 + mesher.getHeight(corner.v) * intensity) * radius
for tile in grid.tiles (skip if mesher.skip(tile.v)):
   c = tile.corners; nor = normal(c[0].v*h0, c[2].v*h2, c[4].v*h4)   // (b-a) x (c-a), normalized
   color = mesher.getColor(tile.v) (start white); emissive = mesher.getEmissiveColor(tile.v) (start 0,0,0,0) when isEmissive
   indexed (tiles*6 < 65535): one vertex per corner (corner.v * height), triangles (0,1,2),(0,2,3),(0,3,4) and (0,4,5) if 6 corners
```
`SerpuloPlanetGenerator.getColor(p)` (133-138): `block = getBlock(p, true); if salt -> sand; out = block.mapColor, a = 1 - block.albedo`.
`getHeight(p) = max(rawHeight(p), water)`. Albedo values (`Blocks.java`): deep-water,
shallow-water, tainted-water, deep-tainted-water, darksand-tainted-water,
sand-water, darksand-water 0.9; arkycite-floor 0.9; snow 0.7; ice 0.65;
ice-snow 0.6; ice-wall 0.6; default 0 (`Block.java:342`).
Shader `core/assets/shaders/planet.vert` (needs `a_emissive`):
`emissive = a_emissive.a * u_emissive * min(pow(max(0, (1 - norc.r)*1.2), 3), 1.1)`;
`v_col = vec4(mix(a_color.rgb, a_emissive.rgb, emissive), 1) * vec4(mix(norc, vec3(1), emissive), 1)`.
`HexMesh.preRender`: `emissive = generator.isEmissive()`, `lightDir = (sun.position - planet.position)` rotated by planet rotation, normalized; `ambientColor = sun.lightColor`.
**Port.** `planet_view.gd:481 _build_body` (billboard sprite), `client/shaders/planet.gdshader`
(ported but uses `COLOR` twice and has no emissive attribute), `core/render/g3d/mesh_data.rs`
(`MeshData`, `hex_build_plan`), `HexMesher` trait (`core/maps/generators/planet.rs`),
Serpulo `get_color` returns black (`serpulo.rs:512`), `get_block_with(planet, pos, ids)` (l.1416)
without the `visualOnly` branch, `BlockDef.map_color: Option<Rgba>`, no `albedo` field.
**Changes.** (1) `BlockDef.albedo: f32` (default 0) filled from the list above
(where block specs set fields: follow how `map_color` is set). (2) Serpulo/Erekir
`get_color` per Java (Erekir: crystalline-stone -> crystal-floor). (3)
`mesh_data.rs pub fn build_hex(mesher: &dyn HexMesher, divisions: usize, radius: f32, intensity: f32) -> MeshData`
with normals (`MeshData.normals: Vec<[f32;3]>`, `emissive: Vec<u32>`), exactly the
Java loop. (4) `MindCampaign.planet_mesh(planet) -> Dictionary` (vertices,
normals, colors, emissive, indices) built with the planet's generator (set its seed to
base seed). (5) `planet_view.gd`: replace the billboard with a `MeshInstance3D`
`ArrayMesh` (ARRAY_VERTEX, ARRAY_NORMAL, ARRAY_COLOR, ARRAY_CUSTOM0 = emissive RGBA8,
ARRAY_INDEX), material = `planet.gdshader` with `CUSTOM0` as `a_emissive`; fix the
shader to the Java formula; `u_lightdir = LIGHT_NORMAL`, `u_ambientColor` = sun light colour
(`Planets.sun` lightColor — check `PlanetDef` for it), `u_emissive = 1` for serpulo.
Keep depth behaviour so the grid lines behind the globe stay hidden.
**Tests.** `build_hex_counts`: divisions 6 -> `tiles = PlanetGrid::create(6).tiles.len()`;
indexed = `tiles*6 < 65535`; vertex count = sum of corner counts; index count =
`12 * 6-corner tiles + 9 * 5-corner tiles`. `serpulo_color_alpha_from_albedo`:
a deep-water position gives alpha 0.1.
**Verify.** Planet page screenshot vs a Java planet frame (`ls REPO/runs | grep -i planet`).
**Commit.** `PAR-0142: planet globe hex mesh with generator colours and planet shader (T12a)`

### T12b. Planet globe: Serpulo emissive sector glow (PAR-0142 part 2)
**Java.** `SerpuloPlanetGenerator.java:141-183 getEmissiveColor` (enemy-base
sectors in crux colour, captured sectors in sharded colour scaled by
`info.lightCoverage`) and `getBlock(p, true)` `visualOnly` branch (`:211-225`, derelict
megabase region near `basePos` within 0.65). Needs per-sector state passed into the
mesher (`has_enemy_base && !captured`, `has_base`, `tile_v`, `preset.difficulty`,
`light_coverage`): add a `SectorGlow` list argument to `planet_mesh` and rebuild the
mesh when sector states change (Java rebuilds `planet.reloadMesh()` on capture/loss —
check `grep -n reloadMesh J/**/*.java`).
**Commit.** `PAR-0142: Serpulo emissive sector glow on the globe (T12b)`

### T12c. Planet globe: bloom (PAR-0142 part 3)
**Java.** `PlanetRenderer.java:31` `Bloom(w/4, h/4, true, false)` around the planet
render (`bloom.capture()` .. `bloom.render()`), `blending = !drawSkybox`.
Port with a `WorldEnvironment` on the `/root/Spine/Planet` 3D scene: glow enabled,
`glow_hdr_threshold` tuned to match a Java frame; record the tuned values and that
it approximates Arc `Bloom` (intensity/threshold read from `ARC/.../Bloom.java`).
**Commit.** `Fixes PAR-0142: planet globe bloom (T12c)`

### T13. Cosmetic residuals (campaign UI)
* Globe sector icons behind the limb drawn at the rim: cull when
  `tile_v · camera_dir <= 0` (`planet_view.gd _add_icon`).
* Unlocked preset ring pulse `derelict -> white` with `Mathf.absin(Time.time, 10, 1)`
  (`PlanetDialog.java:461-466`); `absin(t, scl, mag) = abs(sin(t/scl)) * mag`, `Time.time` in ticks.
* Sector list panel background `Styles.black8` (`planet_dialog.gd _build_sector_list`).
* `uiAlpha` fade of icons/labels when `zoom >= 1.9` (T6 state).
* Remove the unused `ui/layout/*.gd` stubs if nothing references them
  (`grep -rn "MindTreeLayout\|MindBranchTreeLayout\|MindRowTreeLayout\|MindRadialTreeLayout" client`).
* `icon_select` title key `@icons` does not exist in the bundle: Java `IconSelectDialog` has no title (`Dialog()`); set an empty title.
**Commit.** `Gameplay G05 T13: campaign UI cosmetic residuals`

---

## 3. Execution order, dependencies, shared files

### 3.1 Order (player impact first)
1. **T1a -> T1b -> T1c** research tree (T1b needs T1a's endpoint; T1c needs T1b).
2. **T2** dialog infrastructure (before T7/T8/T10 so new dialogs stack correctly;
   independent of T1).
3. **T3** sector panel / launch candidate (golden regen).
4. **T4a -> T4b** hints (T4b's input completions depend on a G08 seam; see §6).
5. **T5a -> T5b -> T5c -> T5d -> T5e** content stats (each depends on T5a).
6. **T6** globe controls + preset reveal (uses `is_top()` from T2).
7. **T7** rename/icon (uses T2 stacking for `icon_select` over the planet page).
8. **T8** stats dialog (golden regen; manifest + scene + `EXPECTED_*_DIALOGS`).
9. **T9a -> T9b -> T9c** generated sectors (T9b after T9a; T9c after T9b).
10. **T10a -> T10b** schematics (T10b's in-game use depends on G08).
11. **T11** database tabs.
12. **T12a -> T12b -> T12c** globe mesh (lowest gameplay impact).
13. **T13** cosmetics.

Run the UI-golden regeneration only in the task that changes a view struct
(T1a if objectives change, T3, T8, T10a, T11) and re-run
`cargo test -p mind-headless ui_goldens_match` after every later task.

### 3.2 Files shared with other areas (keep diffs minimal, merge carefully)
* `gdext/campaign.rs` (G04 campaign loop, launch): T1a, T3, T5a, T6, T7, T8, T9b, T12a.
* `core/ui/campaign.rs` + `client/rust/mind-core/tests/goldens/ui/campaign.json` +
  `parity/golden_manifest.json` (G04 owns PAR-0162/0163 read model): T1a, T3, T8, T10a.
* `gdext/sim_host.rs` (everyone): T4a, T4b, T9a, T9b, T9c.
* `gdext/ui/ui_host.rs` (G04 settings hub PAR-0164): T2.
* `gdext/input/mod.rs` (G08): T4b, T10b (seams only, see §6).
* `core/sim/builder.rs`, `core/sim/mod.rs` (G01/G02/G08): T4a.
* `core/maps/planet/*.rs`, `core/maps/generators/*.rs` (G04 sector lifecycle): T9a-T9c, T12a.
* `ui/dialogs/planet_dialog.gd`: T1b (refresh_page), T2, T3, T6, T7, T8.
* `core/content/registries/blocks/mod.rs` (G02): T12a (`albedo`).

### 3.3 Deferred (with reasons)
* `BaseGenerator` parts/walls passes (`J/maps/generators/BaseGenerator.java:70-` to end): large
  schematic-placement port on top of T9c; enemy cores are enough to make attack
  sectors playable. File as a new ledger item (§4).
* `HexSkyMesh` clouds: invisible at normal zoom (`alpha = 1 - uiAlpha`,
  `HexSkyMesh.java:56-59`); only visible when zoomed out past 1.9.
* Load-dialog save previews (`Saves.java:343-352`): needs a "save written" hook in
  G04's save path; the minimap texture is available at
  `/root/Spine/World/MinimapProvider.get_texture()` (`gdext/render/minimap.rs:327`) and the
  path helper `Paths::preview_for_save(stem)` (`core/io/fs.rs:459`) exists. Hand to G04.
* `ui/layout/RowTreeLayout`/`RadialTreeLayout`: unused by Java campaign UI.
* Mobile-only paths (research `@research` button, `respawn` hint, mobile schematic flow).
* Research dialog multiplayer (`net.client()`) branches.
* `PlanetDialog.showPlanetLaunch`/`planetLaunch` mode (interplanetary accelerator flow) — G04 owns the accelerator; the dialog mode can follow G04.
* `SchematicsDialog.showAllTags` tag editor and Steam workshop links.
* Dialog hide fade-out (T2 may skip it; Java 0.1 s).

---

## 4. Ledger changes to report (coordinator applies; never edit `chains.json`)

| Item | New status / note |
|---|---|
| PAR-0145 | `fixing` after T1a/T1b, `verified` after T1c with evidence `runs/<date>-s<N>-G05-T1b/research.png` |
| PAR-0047 | unblock: rows/node graph exist after T1b; re-run the research click + hover popup half (r05-02, r05-14) -> `verified` |
| PAR-0160 | `verified` — fixed by `2fe5800` (requirements materialized); T1a re-checks `reqs`/`progress` in `research_tree_json` |
| PAR-0148 | `verified` after T2 (note: Java dims EVERY BaseDialog with black9; only PlanetDialog is opaque `fullDialog` — the seeded note's "fullDialog surfaces carry no dim" is wrong) |
| PAR-0149, PAR-0150 | `verified` after T2 |
| PAR-0142 | `fixing` after T12a, `verified` after T12c |
| PAR-0143 | `fixing` after T10b; `verified` once G08's `use_schematic` seam is wired |
| PAR-0144 | `verified` after T11 |
| PAR-0112 | `fixing` after T9a/T9b, note "enemy cores only (T9c); BaseGenerator parts deferred" |
| PAR-0025/0026/0027 | `verified` after T10a (only the 200 px SchematicImage toggles were left) |
| PAR-0162, PAR-0163 | dialog side fixed by `44b0f7b`; G04 decides the final status |
| new | "Content info (`?`) shows no stats: `content_info_json` missing, no Stats model" -> fixed by T5a-T5e |
| new | "Hints auto-complete after 15 s and persist `-hint-done` (Java never times out); 22 of 27 predicates missing" -> T4a/T4b |
| new | "Research spend refuses partial payment when one item is missing; items removed from sectors greedily instead of by percentage" -> T1a |
| new | "Planet page: launch source is the first based sector, no `No Launch Sector`, no locked objectives, vulnerable rule wrong" -> T3 |
| new | "Planet globe cannot be dragged or zoomed; selection snaps the camera; no new-preset reveal" -> T6 |
| new | "Planet page lacks rename/icon picker and the stats dialog is a text dump" -> T7, T8 |
| new | "Generated sectors: enemy base parts/walls (BaseGenerator passes) missing" -> deferred |

---

## 5. Done criteria

* Every task above committed (or explicitly reported as blocked/deferred with
  the reason) on the session branch, one commit per task, subject as given.
* `cargo test -p mind-core --lib --features msav-import`, `cargo test -p mind-headless ui_goldens_match`,
  `cargo run -q -p mind-headless -- parity goldens`, clippy and the GDScript parse
  gate pass; mind-headless failures only from the G00 known list.
* In-engine on a clean re-import (env §4.3, 282/375): research graph opens over
  the planet page (stacked), node hover popup shows; `?` on duo shows stats;
  Ground Zero resume and a fresh numbered launch show no `panic` in the log;
  hints never complete without their trigger.
* Summary file `docs/parity-plans/G05-campaign-ui_summary.md` written and committed (env §6).

---

## 6. Cross-area findings

1. **G08 (input) — dependencies:** G08 adds `MindInput.use_schematic(index)` (desktop; T10b) and `MindInput.take_hint_keys()` (T4b). Today: no desktop `use_schematic` seam (`MindInput` has only
   `mobile_toggle_schematic`, `gdext/input/mod.rs:1118`; core `use_schematic` exists only in
   `core/input/mobile.rs:743`). T10b needs `MindInput.use_schematic(index)`.
   T4b needs `MindInput.take_hint_keys()` (shoot, boost down, schematicSelect release,
   pick tap, diagonalPlacement release, rebuild-selecting). Research key closing the
   research dialog (`ResearchDialog.java:145-149`, `Binding.research`) and
   `planetMap` key toggling the planet page are G08 key paths.
2. **G01 (boss):** no `state.boss()` / `teams.bosses` in the port; the `guardian`
   hint (T4b) depends on the G01 boss seam (see G01 §3.1).
3. **(no gap) Neighbours:** `Campaign::from_registry` (`core/game/universe.rs:~335-350`)
   fills `Planet.neighbors` from `PlanetGrid` whenever `sector_tiles > 0`;
   `EmptyNeighborhood` is only the gridless fallback. The only cleanup is in G05 T3:
   `annotate_globe`'s redundant local grid is replaced by `planet.neighbors`.
4. **G04 (saves):** save previews for the Load dialog (§3.3).
5. **G07 (audio):** G07 owns `play_ui_sound` wiring; G05 only emits
   `uiBack` (dialog hide, T2) and `uiUnlock` (research unlock, T1c) at Java's call sites.
6. **G02 (blocks):** `BlockDef.stats: Vec<StatSpec>` (`content/registries/blocks/mod.rs:2120`)
   and `core/world/stats.rs` are dead code with a wrong `StatCat` set; T5 replaces
   their purpose with `core/ui/content_stats/`; G02 may delete them.
7. **G04 (generated sectors / generators):** `SerpuloPlanetGenerator.get_color`
   returns black (`serpulo.rs:512`) — fixed by T12a here, but any G04 code using
   `HexMesher` colours should wait for it.
