<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H05 — Generated sectors, campaign UI and meta screens (execution plan)

Date: 2026-10-14. Area owner: H05 (one session, see `docs/H-environment.md` §1).

**Read `docs/H-environment.md` fully first** (paths `REPO WT core/ gdext/ ui/
scenes/ J/ ARC/`, the `$G $H $I $C $MC` gmcp shorthands, build/test/golden
rules, commit rules, RAM rules). Then `docs/H-INDEX.md` §3 if it exists. Then
this file. Work the tasks in the §3 order; one task = one commit.

Start state: `git log main --oneline | grep -m1 'H00'` must print a line (H00
landed: Serpulo `rules.env` = 233 and the player unit spawns at the core). If
it prints nothing, stop and report.

Drift rule: port line numbers below were read on `main` @ `ffbe84dd`. If a
line moved, `grep -n` the quoted symbol. If a function does not exist at all,
or the code already does what the task asks, stop that task, write down what
you saw, and continue with the next independent task. Never improvise a
design that this plan does not describe. Java line numbers were checked on
2026-10-14 and are authoritative.

Area: (a) **planet-generated (numbered, non-preset) campaign sectors**
(PAR-0112, gameplay-critical: today they have no core, no enemy base, no
ruins, a fixed 128x128 map); (b) the UI the player touches in and between
sectors: research tree, content info (`?`) stats, pause menu rule editing,
hints, schematics browser, planet page (globe controls, rename/icon, stats,
notifications), core database, settings fan-out, globe mesh.

Commit subjects: `Fixes PAR-XXXX: ...`, `PAR-XXXX: ...` (partial) or
`Gameplay H05 T<n>: ...` exactly as each task gives.

---

## 1. Inventory

### 1.1 Java -> port -> status (verified 2026-10-14 on `main` @ `ffbe84dd`)

| Java (`J/…`) | Port | Status | Task |
|---|---|---|---|
| `type/Planet.java:476-490 getData` (marks `attackSectors` from `assets/planets/serpulo.json` as `generateEnemyBase`) | `core/content/registries/planets.rs` bakes only `preset_remap` (l.697); `attackSectors` never read (grep `attack_sectors` = nothing) | **missing**: the 45 Serpulo attack cells are plain survival cells | T1 |
| `type/Planet.java:466-473` (`generator.generateSector` then `updateBaseCoverage()` at init) | `core/game/universe.rs Campaign::from_registry` never calls `update_base_coverage_from_neighbors` (only `game/play.rs:704` after a capture) | **wrong**: numbered-sector threat is 0 until the first capture | T1 |
| `type/Sector.java:42-53,255-282 makeRect`, `PlanetGenerator.java:173-196 getSectorSize/generate` | `core/maps/planet/mod.rs:52 generate_sector` (`let _ = sector;`, seed 1), `gdext/sim_host.rs:1820 const SECTOR_SIZE: i32 = 128` | **wrong** (every numbered sector is the same 128x128 map) | T2 |
| `core/World.java:505-529` generated-sector polygon darkness | `core/world/darkness.rs` has only border + static wall terms | **missing** (generated sectors are square, not hexagonal; build gate ignores the hex edge) | T3 |
| `SerpuloPlanetGenerator.java:719`, `ErekirPlanetGenerator.java:428`, `TantrosPlanetGenerator.java:81`, `AsteroidGenerator.java:138` `Schematics.placeLaunchLoadout` (`game/Schematics.java:465-469`) | `core/maps/planet/serpulo.rs:1333` comment "plan-12 hook (no tile effect)"; `MindSimHost.place_launch_loadout` (`gdext/sim_host.rs:3228`) only REPLACES an existing core | **missing** (no player core -> instant loss) | T4 |
| `SerpuloPlanetGenerator.java:721-751` + `maps/generators/BaseGenerator.java` (cores, part passes, walls, path clearing, `postGenerate`) | `core/maps/generators/base.rs BaseGenerator::generate` logs only | **missing** | T5 |
| `SerpuloPlanetGenerator.java:633-711` derelict ruins | `serpulo.rs:1313` "Ruins are skipped" | **missing** | T6 |
| `ui/fragments/HudFragment.java:261-275` (`sector.capture` / `sector.lost` / `sector.attacked` toasts for OTHER sectors) | `gdext/campaign.rs:548` drops `TurnReport.events`; `gdext/ui/hud.rs:415-437` handles only the played sector | **missing** | T7 |
| `ui/dialogs/ResearchDialog.java` (BaseDialog = `defaultDialog`, black9 dim; title button; `@techtree.select` BaseDialog; `ItemsDisplay`; node styles) | `ui/dialogs/research_dialog.gd` sets `full_dialog = true` (l.27), PopupPanel root selector (l.62), oversized title icon, empty item list in evidence | **partial / G05 claims partly false** | T8 |
| Arc `Strings.autoFixed` (`ARC/arc-core/src/arc/util/Strings.java:761-811`) | `core/ui/stat_display.rs:25 auto_fixed` rounds half-up | **wrong** (Arc truncates: 0.29167 -> "0.291") | T9 |
| `ui/dialogs/ContentInfoDialog.java` + `world/meta/{Stat,StatCat,StatUnit,Stats,StatValues}.java` + 80 `setStats()` | `content_info_dialog.gd:33` calls `MindCampaign.content_info_json` which does not exist; `core/world/stats.rs` dead code | **missing** (`?` dialog has no stat rows) | T10a-T10e |
| `ui/dialogs/PausedDialog.java:20-37` rules button + `ui/dialogs/CustomRulesDialog.java` (607 l.) | `ui/dialogs/paused_dialog.gd` has no rules button; `custom_rules_dialog.gd` (53 l.) renders only a tab rail; `MindCampaign.set_rules_json` (`gdext/campaign.rs:1119`) writes `self.session.rules`, which the next `pump_runtime` overwrites from the live runtime (`campaign.rs:565`) | **missing / wrong** (PAR-0132) | T11a, T11b |
| `ui/fragments/HintsFragment.java` remainder | `ui/fragments/hints_fragment.gd` | **partial**: `unitSelectControl` predicate uses `or` where Java uses `&&` (l.147); `cores_destroyed` hard-coded 0 (`gdext/sim_host.rs:5284`); `cannotupgrade`/`factorycontrol` never fired; completion of `unitSelectControl` lacks the CommandAI `targetPos` readback | T12 |
| `ui/dialogs/SchematicsDialog.java:120-215` card | `schematics_dialog.gd:141-167`: no 50 px per-card action row (info/upload/pencil/trash), name badge at the BOTTOM (Java: top), `togglet` style (Java `flati` + `Tex.pane`), import button in the search row (Java: bottom button row), extra detail panel, menu click selects instead of `showInfo` | **partial (G05 T10b claim partly false)** | T13 |
| `schematic-background` sprite | `REPO/assets/sprites/schematic-background.png` exists, not in the atlas (`MindWidgets.icon_texture("schematic-background")` misses) | **missing** | T13 |
| `PlanetDialog.java:112-147` drag/zoom, `lookAt`, new-preset reveal (270-288, 986-1003), `Planet.setLastSector` (`type/Planet.java:319-321`) | `ui/planet_view.gd:322 focus_tile` snaps the camera; no drag/zoom; `<planet>-last-sector` never written | **missing** | T14 |
| `PlanetDialog.java:1199-1283` rename pencil + icon picker | none | **missing** | T15 |
| `PlanetDialog.java:1026-1171 showStats/displayItems/abandonSectorConfirm` | `planet_dialog.gd _show_sector_stats` text dump | **partial** | T16 |
| `ui/dialogs/DatabaseDialog.java:37-125` icon tabs | `database_dialog.gd:71 _build_tabs` text buttons; `core/ui/database.rs:65 DatabaseTabView` has no `icon` | **partial** (PAR-0144) | T17 |
| Settings fan-out | `MindUi.settings_put_*` (`gdext/ui/ui_host.rs:730-754`) never reaches `MindCampaign.settings` (`gdext/campaign.rs:189`) | **wrong** (PAR-0164) | T18 |
| `core/UI.java:132-133` dialog hide fade 0.1 s | `ui/mind_dialog.gd:119-121` (hide instant) | **missing** | T19 |
| `graphics/g3d/HexMesh` + `shaders/planet.vert` + Serpulo `getColor/getEmissiveColor` + `PlanetRenderer` bloom | `planet_view.gd:481 _build_body` billboard; `serpulo.rs:512 get_color` returns black | **missing** (PAR-0142) | T20a-T20c |
| Cosmetic residuals (limb culling, preset pulse, black8 list, uiAlpha fade, empty `icon_select` title) | various | **missing** | T21 |
| `ui/dialogs/HostDialog.java:16-86` | `ui/dialogs/host_dialog.gd` (mode buttons with raw `@mode.*`, no colour swatch, no `?`) | **wrong** (PAR-0151, low priority) | T22 |

### 1.2 Done — do not redo (spot-checked 2026-10-14)

* Research read model + Java `canSpend` (`core/game/tech_tree.rs:122`, matches
  `ResearchDialog.java:551-565`) and percentage removal `distribute_removal`
  (`tech_tree.rs:177`, matches `ResearchDialog.java:256-288`), wired in
  `gdext/campaign.rs:1187,1227`. Node graph/connectors/pan/zoom exist.
  `uiUnlock` is emitted (`research_dialog.gd:220`).
* Dialog stacking: `MindUi.open_dialog` never hides the lower dialog
  (`gdext/ui/ui_host.rs:192-221`); base Dim is `Color(0,0,0,0.9)`
  (`scenes/ui/dialogs/mind_dialog_base.tscn:21`); show fade-in 0.1 s.
* Modal confirm/text-input (PAR-0150), planet sector panel launch
  candidate/locked objectives/threat colour (PAR-0226), Configure Loadout
  200 px schematic toggles (`ui/widgets/schematic_image.gd`).
* Hints driver: no timed completion, total-playtime gate, Skip persistence,
  `take_hint_keys()` input completions, `fire`/`derelictmouse` events.
* `MindInput.use_schematic(index)` exists (`gdext/input/mod.rs:1105`) and the
  schematics dialog calls it in game.
* Pause menu `@objective`, `@abandon`, `@save.quit`, host entry
  (`ui/dialogs/paused_dialog.gd`).
* Game-settings rows (`core/ui/settings.rs:184-194`) match
  `SettingsMenuDialog.java` game section (their gameplay effects are H08's).

False / partly false G05 claims found (each is a task here): research dialog
uses `fullDialog` instead of the black9 BaseDialog (T8); research root
selector is a PopupPanel instead of the `@techtree.select` dialog (T8);
schematics "card grid" lacks the per-card action row and the top name badge
(T13); `unitSelectControl` predicate inverted `&&`->`or` (T12); the G05 stat
acceptance values `0.292` / `0.667` assumed rounding — Arc truncates (T9).

---

## 2. Tasks

Conventions used below:
* `ids` = `self.ids` inside `SerpuloPlanetGenerator` (`core/maps/planet/serpulo.rs`).
* "Arc `Rand`" in Java = the port's `SimRng` with `RngStream::MapGen`
  inside generators (`self.planet.base.rand`, helper `rand_range_int` at
  `serpulo.rs:741`), and `crate::math::rand_arc::ArcRand` for `Mathf.rand`
  / `new Rand(seed)` (methods `new`, `set_seed`, `next_double`, `chance`,
  `range_int`, `range_float`, `random_float`, `random_range_int`,
  `random_inclusive`; read `core/math/rand_arc.rs` once).
* `Seq.getFrac(f)` (Arc `Seq.java`): `items[clamp((int)(f * size), 0, size - 1)]`,
  `null` when empty. Add one helper `fn get_frac<T>(items: &[T], f: f32) -> Option<&T>`
  in `core/maps/generators/base.rs` and reuse it.

---

### T1. Serpulo attack sectors and boot-time threat (PAR-0112 prerequisite)

> **In-engine verification (2026-10-14, session 3, round 3):** PARTLY. Main (`ffbe84dd`, worktree with a local H00-T1 `env_bit` patch, reverted). Planet view (menu, `campaign_section("sectors")`, 272 rows): **every** `threat` is 0.0 (194 `preset == null` rows included; Java floor 0.3), so the boot-coverage half (Change 3) is CONFIRMED; visible consequence: numbered sector 1 starts with `winWave` 15 (Java `10 + 5*(int)max(threat*10,1)` gives >= 25). Changes 1-2 are a functional no-op: all 45 `attackSectors` ids are already `sector-serpulo-<id>` submission presets with `has_enemy_base: true` (port `sectors.rs:302`; Java `SectorSubmissions.registerSectors` registers exactly these 45 with `captureWave <= 0` -> `generateEnemyBase = true`, `SectorSubmissions.java:27-75,126-146`). Keep Changes 1-2 for fidelity (cheap) but the acceptance is Change 3's threat. FIXED in place: test `serpulo_attack_sectors_marked` (no marked cell lacks a preset) and the verify expectation (sector 0 threat 0.8). Java not needed (source-unambiguous).

**Symptom / acceptance.** No numbered Serpulo sector is an enemy base: Java
marks 45 cells from `assets/planets/serpulo.json` `attackSectors`; the port
never reads that list. Numbered-sector threat stays 0 (Low) until the first
capture because `updateBaseCoverage` never runs at boot. After: sector 0 of
Serpulo reports `has_enemy_base == true` and every numbered sector's threat
equals Java `updateBaseCoverage` at boot. Acceptance: Rust tests below; the
planet page shows enemy-base sectors with the red/attack icon and threat
`Medium`+ next to them.

**Java.**
* `J/type/Planet.java:476-490`:
  ```java
  public @Nullable PlanetData getData(){
      if(loadPlanetData && data == null){
          Fi file = tree.get("planets/" + name + ".json");
          if(file.exists()){
              data = JsonIO.read(PlanetData.class, file.readString());
              for(int i : data.attackSectors){
                  if(i >= 0 && i < sectors.size){
                      sectors.get(i).generateEnemyBase = true;
  ```
  (called from `SectorPreset.java:89` during content init, i.e. before any
  campaign state exists).
* Serpulo list (`/home/clintonnguyen/g/code_examples/Mindustry/core/assets/planets/serpulo.json`,
  key `attackSectors`), exactly:
  `[0,6,13,16,19,20,24,27,30,47,55,66,67,69,76,92,94,111,116,127,133,161,162,176,180,185,191,192,197,200,207,225,230,240,242,243,244,245,246,247,248,254,259,263,265]`.
  No other planet json exists.
* `J/type/Planet.java:458-473 init`: `sector.loadInfo()` for all, then
  `generator.generateSector(sector)` for all (Serpulo: base no-op), then
  `updateBaseCoverage()`.

**Port today.** `core/content/registries/planets.rs:304-307`
(`load_planet_data`, `preset_remap`), `:697 planet.preset_remap = vec![...]`
(serpulo); content sector cells carry `generate_enemy_base` (l.200).
`core/game/planet.rs:162-163` copies `threat`/`generate_enemy_base` into the
runtime `Sector`; `update_base_coverage_from_neighbors` at `planet.rs:466`.
`Campaign::from_registry` at `core/game/universe.rs:318-366`.
Where sector infos are loaded at boot: `grep -n "load_info\|load_all" client/rust/mind-gdext/src/campaign.rs client/rust/mind-core/src/game/universe.rs`.

**Changes.**
1. `planets.rs`: add `pub attack_sectors: Vec<i32>` to `PlanetDef` (default
   empty, doc: "`PlanetData.attackSectors` (`planets/<name>.json`)"); in the
   serpulo block next to `preset_remap` set it to the list above (copy it
   verbatim; add a doc line citing the json path).
2. In the same registration function, after the serpulo `PlanetDef` has its
   sector cells (`with_sector_grid(3)` creates them), mark each listed cell:
   `for &i in &planet.attack_sectors { if i >= 0 && (i as usize) < planet.sectors.len() { planet.sectors[i as usize].generate_enemy_base = true; } }`.
   If the cells are created later than the `PlanetDef` literal, do the loop
   where `register_sector_submissions` (`sectors.rs:263`) marks its cells
   (same pattern: `registry.planet_mut(serpulo)`), BEFORE it.
3. `universe.rs Campaign::from_registry`: after `planets.insert(...)` for a
   planet, call `planet.update_base_coverage_from_neighbors()` on it (Java
   init order). Then find where `MindCampaign` loads sector infos at boot
   (the grep above); immediately after the info load, call
   `update_base_coverage_from_neighbors()` for every planet again
   (`has_enemy_base()` depends on `save`/`info.attack`).

**Tests** (`core/game/universe.rs` tests, `create_base_content` registry):
* `serpulo_attack_sectors_marked`: serpulo sector 0 and 265
  `generate_enemy_base == true`; sector 1 false; every marked cell has
  `preset.is_some()` (count of marked cells with `preset.is_none()` == 0 —
  CORRECTED 2026-10-14 round 3: all 45 `attackSectors` ids are hidden-map
  submission presets, `SectorSubmissions.java:27-75` + `registerSector` l.126-146).
* `boot_threat_matches_update_base_coverage`: build `Campaign::from_registry`,
  take serpulo sector 1 (`preset.is_none()`), recompute by hand
  `sum = 1 + 0.9 * (# neighbours with generate_enemy_base) + (0.88 if has_enemy_base)`,
  `threat = clamp(sum / 5, 0.3, 1.2)`; assert equal (1e-6).
* Run `cargo test -p mind-core --lib --features msav-import` and
  `cargo test -p mind-headless --lib ui_goldens_match`: the UI campaign golden
  will change (threat/enemy-base fields). Regenerate per env §2 and name
  "Planet.getData attackSectors + init updateBaseCoverage" in the body.

**Verify in-engine** (after `--import`):
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'var s = get_node("/root/Spine/Ui/UiRoot").dialog("planet").campaign_section("sectors"); return s.filter(func(x): return x.id == 0)[0]'
$G --screenshot runs/<date>-s<N>-H05-T1/planet.png
```
Expect sector 0 `has_enemy_base: true` (already true before T1 — it is the
`sector-serpulo-0` submission preset) and `threat` 0.8 (preset difficulty 8 /
10); every `preset == null` sector `threat` in 0.3..1.2 (today all 272 rows
read 0.0). The `campaign_section` call works from the menu (do not call it
while a sector is loaded). Compare with `runs/20261010-234250-ref-PAR-0070-combat-fx/java/step-140-planet.png`
(red-tinted enemy cells come only with T20b; here only icons/threat).

**Pitfalls.** Submission presets (`sector-serpulo-76`, ...) are presets with
hidden maps: `has_enemy_base` for them already comes from `capture_wave == 0`;
the attack list just also flags the cell — harmless. Do not touch
`preset_remap`.

**Commit.** `PAR-0112: Serpulo attackSectors and boot-time base coverage (H05 T1)`

---

### T2. Generated sectors use their sector rect, seed, size and threat (PAR-0112 step 1)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED. `load_sector("serpulo", 1|2|5)`: identical floor fingerprint (`hash` of `tile_debug(i,j).floor` on a stride-8 grid = 601855000 for all three), 128x128 (`tile_debug(127,127)` = `dune-wall`, `tile_debug(128,0)` returns `{}` — out of bounds is an empty dict, not an error), no core (`best_core_position()` (-1,-1)); screenshots `runs/20261014-s3-verify3/H05-T2-sector1.png`, `H05-T2-sector2.png` (same map). Verify recipe dry-run: the `campaign_section` filter works from the menu (fields `preset`, `has_save` exist) and returns `[1,2,3,4,5,7,8,9,10,11,12,14]`; `prime_sector_load` does not exist yet (T2 adds it). Java not observed (no reachable numbered sector in the seeded profile, see T4).

**Symptom / acceptance.** Every non-preset sector is the same 128x128 map
(`gdext/sim_host.rs:1820 const SECTOR_SIZE: i32 = 128`, `generate_sector(..., 1, ...)`,
`core/maps/planet/mod.rs:52` ignores `sector`). After: terrain projects the
sector's own rect, `rand` seeds with `sector.id + seedOffset(0) + baseSeed`,
size = `getSectorSize`, and the generator sees the campaign threat and
enemy-base flag. Acceptance: tests below; two numbered sectors look different
in-engine; `$H.tile_debug(size-1, size-1)` succeeds and `tile_debug(size, 0)`
is out of bounds.

**Java.**
* `J/type/Sector.java:42-53`: `rect = tile.corners.length == 0 ? new SectorRect(1f, Vec3.Zero, Vec3.Y, Vec3.X, 0f) : makeRect();`
* `J/type/Sector.java:255-282`:
  ```java
  Vec3[] corners = new Vec3[tile.corners.length];
  for(i) corners[i] = tile.corners[i].v.cpy().setLength(planet.radius);
  Tmp.v33.setZero(); for(Vec3 c : corners) Tmp.v33.add(c);
  Vec3 center = Tmp.v33.scl(1f / corners.length).cpy();
  float radius = Tmp.v33.dst(corners[0]) * 0.98f;      // Tmp.v33 == center here
  plane.set(corners[0], corners[2], corners[4]);
  Vec3 planeTop = plane.project(center.cpy().add(0f, 1f, 0f)).sub(center).setLength(radius);
  Vec3 planeRight = plane.project(center.cpy().rotate(Vec3.Y, -4f)).sub(center).setLength(radius);
  Vec3 first = corners[1].cpy().sub(center);
  float angle = first.angle(planeTop);                  // degrees
  return new SectorRect(radius, center, planeTop, planeRight, angle);
  ```
* Arc `Plane` (`ARC/extensions/g3d/src/arc/math/geom/Plane.java:53-75`):
  `set(p1,p2,p3)`: `normal = (p1 - p2).crs(p2 - p3).nor(); d = -p1.dot(normal)`;
  `project(v)`: `npd = normal.dot(v) + d; return v - npd * normal` (in place).
* `Vec3.rotate(Vec3.Y, deg)` (quaternion, right-handed, `ARC/arc-core/src/arc/math/geom/Vec3.java:437`):
  for axis Y: `x' = x*cos + z*sin; y' = y; z' = -x*sin + z*cos`, θ = deg in radians.
  `Vec3.angle(other)` = `acos(clamp(dot/(len*len2), -1, 1))` in degrees (read `Vec3.java angle`; use its exact formula).
* `J/maps/generators/PlanetGenerator.java:173-180`:
  `getSizeScl() = 3200`; `getSectorSize(s) = (int)(s.rect.radius * getSizeScl())`, `+1` if odd.
  Erekir `2000*1.07*6/5`, Tantros 2000 (already in the port as `size_scl`).
* `PlanetGenerator.java:182-196 generate(tiles, sec, params)`: `seed = params.seedOffset + baseSeed`;
  `rand.setSeed(sec.id + params.seedOffset + baseSeed)`; tile position
  `sector.rect.project(x / (float)width, y / (float)height)`.
  `World.loadSector` uses `new WorldParams()` (`seedOffset = 0`).

**Port today.** `core/maps/generators/planet.rs`: `SectorRect { center, top, right, radius }`
(no `rotation`), `SectorView` trait, `FlatSectorView` (`rect()` returns the
default plane with only `radius`), `PlanetGenerator::prepare` already seeds
`sector.id() + seed_offset + base_seed` and `get_sector_size`. Generators hold
`pub sector: FlatSectorView` (`serpulo.rs:382`, `erekir.rs:143`,
`tantros.rs:31`, `asteroid.rs:32`). `PlanetGrid::create(n)`
(`core/render/g3d/grid.rs`; `Ptile { id, tiles, corners: Vec<usize>, v }`,
`Corner { v }` — corners are indices into `grid.corners`). `PlanetDef.radius`
and `sector_tiles` (`content/registries/planets.rs:232-234`).

**Changes.**
1. `SectorRect`: add `pub rotation: f32` (Default 0.0). Keep `project`.
2. `FlatSectorView`: add `pub rect: Option<SectorRect>`; `rect()` returns it
   when `Some`, else today's default. `Default` sets `None` (all existing
   tests unchanged).
3. New `pub fn make_rect(grid: &PlanetGrid, tile: usize, planet_radius: f32) -> SectorRect`
   in `core/maps/generators/planet.rs` (doc cites `Sector.java:255-282`):
   transcribe the Java above with plain f32 ops in Java order (`Vec3.len()`
   is `sqrt(x*x+y*y+z*z)`; `setLength(l)` = `scl(l / len)`; no `mul_add`).
   A tile with 0 corners returns `SectorRect { radius: 1.0, ..default }`.
4. `WorldGenerator` trait (`core/maps/generators/mod.rs`): add
   `fn set_sector(&mut self, _view: planet::FlatSectorView) {}` and
   `fn sector_size_scl(&self) -> Option<f32> { None }`. Implement both for the
   four planet generators (`self.sector = view;` / `Some(self.planet.size_scl)`).
5. `core/maps/planet/mod.rs`: replace `generate_sector` with
   ```rust
   pub struct SectorGenExtras { pub threat: f32, pub has_enemy_base: bool, pub has_base: bool }
   pub fn generate_sector(planet: &str, sector: u16, content: &ContentRegistry,
                          extras: SectorGenExtras) -> Option<GeneratedSector>
   ```
   Body: `def = content.planet_by_name(planet)?`; `grid = PlanetGrid::create(def.sector_tiles as usize)`;
   `rect = make_rect(&grid, sector as usize, def.radius)`; `generator = generator_for(planet, 0)?`;
   `size = generator.sector_size_scl().map(|scl| { let r = (rect.radius * scl) as i32; if r % 2 == 0 { r } else { r + 1 } }).unwrap_or(128)`;
   `generator.set_sector(FlatSectorView { id: sector as u32, radius: rect.radius, threat: extras.threat,
   tile_v: grid.tiles[sector as usize].v, has_enemy_base: extras.has_enemy_base, has_base: extras.has_base, rect: Some(rect) })`;
   `params = WorldParams { seed_offset: 0, width: size, height: size, ..Default }`; then the
   existing body (grid, `generate`, `generate_rules`, `end_map_load`).
   `GeneratedSector` keeps `grid`, `rules` and gains
   `pub generator: Box<dyn WorldGenerator>` (T4-T6 use it after the sim exists)
   and `pub rect: SectorRect`, `pub sides: usize` (= `grid.tiles[sector].corners.len()`, T3).
   Remove the `let _ = sector;` lines. Update the two tests in that file to the
   new signature (`SectorGenExtras { threat: 0.0, has_enemy_base: false, has_base: false }`).
   Fix every other caller: `grep -rn "generate_sector(" client/rust`.
6. Extras source. The campaign threat lives in `MindCampaign`, but
   `load_sector` runs on `MindSimHost` and is called first by the UI and by
   `replay_lost_sector` from inside `MindCampaign` (a call back into
   `MindCampaign` from `load_sector` would panic with a re-entrant bind). So:
   * `gdext/sim_host.rs`: field `pending_gen_extras: Option<(String, u16, mind_core::maps::planet::SectorGenExtras)>`
     plus `#[func] pub fn set_sector_gen_extras(&mut self, planet: GString, sector: i32, threat: f32, has_enemy_base: bool, has_base: bool)`
     and a Rust-only `pub fn set_sector_gen_extras_rs(...)`.
   * `load_sector_world`: when generating, use `pending_gen_extras.take()` if
     planet+sector match; else fall back to a pure core function
     `mind_core::game::planet::static_gen_extras(registry, planet, sector) -> SectorGenExtras`
     (new; doc: "`Planet.updateBaseCoverage` for an unsaved sector") computing
     neighbours from `PlanetGrid::create(def.sector_tiles)` `tiles[id].tiles`,
     `has_enemy_base = cell.generate_enemy_base && cell.preset.is_none()`,
     threat with the same formula as `update_base_coverage_from_neighbors`
     (preset branch included), `has_base = false`.
   * `gdext/campaign.rs`: `#[func] pub fn prime_sector_load(&mut self, planet: GString, sector: i32) -> bool`
     reads the campaign `Sector` (`threat`, `has_enemy_base()`, `has_base()`)
     and calls `host.bind_mut().set_sector_gen_extras_rs(...)`. Call the Rust
     setter also right before `replay_lost_sector` in `start_sector_with_loadout`
     (`campaign.rs:~688`).
   * `ui/dialogs/planet_dialog.gd _launch` (l.1610): before
     `sim_host.call("load_sector", ...)`, `if campaign != null and campaign.has_method("prime_sector_load"): campaign.call("prime_sector_load", planet_name, sector_id)`.
7. `load_sector_world`: delete `SECTOR_SIZE`; build `Sim::new(1, grid.width(), grid.height(), ...)`
   from the chosen grid's size (read how `Sim::new` uses the size; the grid
   is swapped in right after). Keep the generated `GeneratedSector` (minus the
   moved grid/rules) in a local for T4-T6.

**Tests** (`core/maps/planet/mod.rs` / `generators/planet.rs` tests):
* `make_rect_center_projects`: serpulo grid, tile 1: `rect.project(0.5, 0.5) == rect.center` (1e-5),
  `|top| == radius` (1e-4), `top · plane_normal` ≈ 0 (1e-4).
* `sectors_differ_by_id`: serpulo sectors 1 and 2 -> the floor vectors differ.
* `sector_size_is_even_and_from_rect`: serpulo sector 1 size `% 2 == 0` and
  equals `(make_rect(..).radius * 3200) as i32` rounded up to even; print the
  value in the test name comment for the record.
* `static_gen_extras_matches_campaign`: for serpulo sectors 0..20 without
  preset, `static_gen_extras(...).threat` equals the T1 boot campaign threat.
* `cargo run -q -p mind-headless -- parity goldens`: regenerate only goldens
  that run the planet generator (name them in the body: "sector rect/size").

**Verify in-engine** (fresh `--import`):
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'return get_node("/root/Spine/Ui/UiRoot").dialog("planet").campaign_section("sectors").filter(func(s): return s.preset == null and not s.has_save).map(func(s): return s.id).slice(0, 6)'
# pick two ids A, B from that list; for each:
$G --eval "$MC.prime_sector_load(\"serpulo\", A); $H.load_sector(\"serpulo\", A); return $H.tile_debug(0,0)"
$G --screenshot runs/<date>-s<N>-H05-T2/sector-A.png
```
(Field names `preset`/`has_save`: print one dict first and adapt.) Do NOT call
`start_sector` here (no core until T4). `grep -c panic` on the log = 0.

**Pitfalls.** Preset sectors keep `load_preset_map_grid`; only the generator
branch changes. A ~300x300 generation takes noticeably longer (fine). Never
call `get_state_json()` on these maps.

**Commit.** `PAR-0112: generated sectors use their sector rect, seed, size and threat (H05 T2)`

---

### T3. Generated-sector hexagon darkness (`World.getDarkness` sector term)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED by code (`world/build.rs:198` uses wall + border darkness only; no polygon term) and visually (the generated 128x128 map is a square with only border fade, `H05-T2-sector1.png`). FIXED in place: the verify pair `place_block(1,1)` / centre was non-discriminating ((0,0)/(1,1) are `shale-wall`; `place_block` bypasses the player gate) — rewritten to use an open-floor tile outside the polygon through the player gate. JDK + Arc jar exist: oracle values added to the Tests paragraph.

**Symptom / acceptance.** Java numbered sectors are hexagonal (pentagonal for
the 12 pentagon cells): tiles outside the projected polygon are dark and
unbuildable (`Build.validPlace` rejects darkness >= 3); the port draws and
allows the full square. After: `world::darkness::get_darkness(grid, content, x, y)`
includes the polygon term for generated sectors; the build gate uses it;
H03's renderer/minimap call the same function (seam §6).

**Java** `J/core/World.java:484-536` (only the sector branch is new):
```java
if(state.hasSector() && state.getSector().preset == null){
    int circleBlend = 5;
    float offset = state.getSector().rect.rotation + 90;
    float angle = Angles.angle(x, y, tiles.width/2, tiles.height/2) + offset;
    int sides = state.getSector().tile.corners.length;
    float step = 360f / sides;
    float prev = Mathf.round(angle, step);          // (int)(angle/step)*step
    float next = prev + step;
    float length = state.getSector().getSize()/2f;
    float rawDst = Intersector.distanceLinePoint(Tmp.v1.trns(prev, length), Tmp.v2.trns(next, length),
        Tmp.v3.set(x - tiles.width/2, y - tiles.height/2).rotate(offset)) / Mathf.sqrt3 - 1;
    rawDst += Noise.noise(x, y, 11f, 7f) + Noise.noise(x, y, 22f, 15f);
    int circleDst = (int)(rawDst - (length - circleBlend));
    if(circleDst > 0) dark = Math.max(circleDst, dark);
}
```
* `Angles.angle(x1,y1,x2,y2)` = `atan2(y2-y1, x2-x1)` in degrees, normalised to `[0,360)` (`ARC/arc-core/src/arc/math/Angles.java`).
* `Vec2.trns(deg, len)` = `(len*cosDeg, len*sinDeg)`; `Vec2.rotate(deg)` CCW.
* `Intersector.distanceLinePoint(sx,sy,ex,ey,px,py)` (`Intersector.java:267-270`):
  `|(px-sx)*(ey-sy) - (py-sy)*(ex-sx)| / sqrt((ex-sx)^2 + (ey-sy)^2)`.
* `Mathf.round(value, step)` (`Mathf.java:508`) = `(int)(value / step) * step`.
* `Mathf.sqrt3` = `1.7320508`.
* Arc `Noise.noise(x,y,scl,mag)` = `snoise(x,y,scl,mag) / 2`, `snoise = rawNoise(x/scl, y/scl) * mag`
  — 2-D Perlin with static tables initialised by `init()` from
  `new java.util.Random(100)` (`ARC/arc-core/src/arc/util/noise/Noise.java`, 298 l.).
  Mindustry never calls `Noise.setSeed`.

**Port today.** `core/world/darkness.rs` (`get_wall_darkness` l.99,
`get_static_darkness` l.119, `border_darkness` l.134); `core/world/build.rs:198`
combines wall + border for the build gate; `WorldGrid` has `border_darkness`
and `limit_area` (`core/world/mod.rs:156`). No Arc `Noise` port exists
(`grep -rn "fn raw_noise" core/` = nothing). Java `Random`:
`crate::random::JavaRandom` (`core/random.rs`).

**Changes.**
1. New `core/math/arc_noise.rs` (header + `pub mod arc_noise;` in
   `core/math/mod.rs`): transcribe `Noise.java` fully (tables `p`, `g1`, `g2`,
   `init()` using `JavaRandom::new(100)` with the exact `nextDouble`/`nextInt`
   call order of `init`, `rawNoise(x)`, `rawNoise(x, y)`, `snoise`, `noise`).
   Build the tables once (`std::sync::OnceLock`). Values in f64 like Java;
   return f32 where Java casts.
2. `core/world/mod.rs WorldGrid`: add
   `pub sector_polygon: Option<SectorPolygon>` with
   `#[derive(Debug, Clone, Copy, PartialEq)] pub struct SectorPolygon { pub rotation: f32, pub sides: i32, pub size: i32 }`
   (default `None`; any `Clone`/serialization derive on `WorldGrid` must keep compiling).
3. `core/world/darkness.rs`: `pub fn sector_polygon_darkness(grid: &WorldGrid, x: i32, y: i32) -> f32`
   (the Java block, f32 math, `tiles.width/2` integer division) and
   `pub fn get_darkness(grid: &WorldGrid, content: &ContentRegistry, x: i32, y: i32) -> f32`
   = max(`border_darkness(grid, grid.border_darkness, grid.limit_area, x, y)`,
   `sector_polygon_darkness`, `get_static_darkness`) in Java order.
4. `core/world/build.rs:198`: replace the two-term `dark` with
   `get_wall_darkness(..) as f32` max `get_darkness(..)` (keep the wall term:
   it is the port's `tile.isDarkened()` source).
5. `gdext/sim_host.rs load_sector_world`: for the generator branch set
   `sim.grid.sector_polygon = Some(SectorPolygon { rotation: generated.rect.rotation, sides: generated.sides as i32, size: grid width })`
   (`getSize()` = the generated size). Presets and saves: `None`. A resumed
   save of a generated sector also needs it: when `resume_file` and the
   planet sector has `preset == None` (check through the registry planet
   cells), compute the rect with `make_rect` and set it too.

**Tests.** `arc_noise_matches_java`: hard-code 3 values you compute with a
tiny Java program ONLY if a JDK is installed (`which javac`); otherwise
assert determinism + range `|noise(…,11,7)| <= 7` and record "no JDK oracle".
(Verified 2026-10-14 round 3: `/usr/bin/javac` exists and the Arc jar is
`/home/clintonnguyen/g/code_examples/Arc/arc-core/build/libs/arc-core-1.0.jar`;
`Noise.noise(x,y,11f,7f)` / `Noise.noise(x,y,22f,15f)` = (10,20): 0.19717087 /
-0.7115014; (150,150): 0.4026778 / -1.1208266; (37,211): -0.762948 /
-0.89685315; (0,0): 0.0 / 0.0. Use these as the oracle values.)
`sector_polygon_dark_outside`: 300x300 grid, sides 6, rotation 0: centre
tile darkness 0, corner tile (0,0) darkness >= 3.

**Verify in-engine.** Load a numbered sector (T2 recipe), screenshot zoomed
out (`$C.set_camera_scale(0.5)` if that `#[func]` exists): the playable area
must look hexagonal once H03 switches the renderer to `get_darkness`; until
then verify the gate: `return $H.place_block(1, 1, "copper-wall")` must be
`false` and `place_block(size/2, size/2, "copper-wall")` true (the API reads
the tile back, env §4.2). CORRECTED 2026-10-14 round 3: that pair cannot fail
today — (0,0)/(1,1) of a generated map are static walls (`shale-wall`), and
env §4.1 says `place_block` bypasses the campaign placement gate. Pick an
open-floor tile outside the polygon (`tile_debug(x,y).kind == "AirBlock"`
near a corner, e.g. scan the diagonal `x = y = 2..size/5`) and test it
through the player gate (`$H.valid_place_preview(x,y,"copper-wall",0)` once
H02 T3 landed, else `apply_sim_command_json` place with `"player":0,"team":1`
and read `player_build_plans_json()`): false outside, true at the centre.

**Pitfalls.** The noise makes the edge ragged: do not "fix" it. Keep
`border_darkness` untouched (other callers).

**Commit.** `PAR-0112: generated sectors are bounded by the sector polygon darkness (H05 T3)`

---

### T4. Launch loadout core at the generated spawn (PAR-0112 step 2)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED. `load_sector("serpulo",1)` + `start_sector("serpulo",1)`: `get_hud_state()` `gameOver: true, hasCore: false, sectorName: "serpulo"` (sic), `has_player_unit()` false, `best_core_position()` (-1,-1), `dialog_stack()` `["restart"]` (`runs/20261014-s3-verify3/H05-T4-sector1-start.png`). The UI-path half is unavailable on this profile (note added in place). Java not observed: every numbered sector next to Ground Zero/captured presets hovers "Locked" (`java-08-numbered.png`, `java-10-hover-*.png`).

**Symptom / acceptance.** A generated sector loads with no player core (Java
places the launch loadout during generation), so the port ends in an instant
Game Over. After: launching to a numbered sector places
`universe.getLastLoadout()` at the generator spawn, stocked with
`universe.getLaunchResources()`; the player unit spawns there (H00).
Acceptance: `$H.best_core_position()` != `(-1,-1)`, `core_items_json()` =
the launch resources, `$H.has_player_unit()` true, `get_hud_state().hasCore` true.

**Java.**
* `SerpuloPlanetGenerator.java:719 Schematics.placeLaunchLoadout(spawn.x, spawn.y);`
  (also `ErekirPlanetGenerator.java:428 (spawnX, spawnY)`,
  `TantrosPlanetGenerator.java:81 (width/2, height/2)`, `AsteroidGenerator.java:138 (sx, sy)`).
* `game/Schematics.java:465-469`:
  `placeLoadout(universe.getLastLoadout(), x, y, state.rules.defaultTeam); ... world.tile(x, y).build.items.add(universe.getLaunchResources());`
* `placeLoadout(schem, x, y, team)` = `placeLoadout(..., check = true)` (`Schematics.java:475-512`):
  core tile anchored at `(x, y)`, tiles sorted by `-schematicPriority`, non-core
  footprints cleared first, `configureAny(config)`, cores registered.
* `game/Universe.java:129-132 getLastLoadout`: `lastLoadout ?: (sector.planet.generator.defaultLoadout ?: Loadouts.basicShard)`;
  Serpulo default `basicShard`, Erekir `basicBastion` (`ErekirPlanetGenerator.java:33`),
  Asteroid `basicNucleus` (`AsteroidGenerator.java:28`). The free launch calls
  `universe.clearLoadoutInfo()` first (lastLoadout = null, launch resources empty).
* Order matters: the loadout is placed BEFORE `basegen.generate` (T5), whose
  `isTaken` keeps enemy parts out of the player core's build radius.

**Port today.** `serpulo.rs:883 self.spawn = (...)`; `Sim::place_loadout(&schem, x, y, team, check)`
(`core/sim/mod.rs:851`); `MindSimHost.place_launch_loadout(schematic_index, stacks_json)`
(`gdext/sim_host.rs:3228`, loads `Schematics` from `user://schematics`, needs an
existing core); `start_sector_with_loadout` calls it only for the launcher
branch (`gdext/campaign.rs:~840`); `ui/dialogs/planet_dialog.gd _launch` (l.1610)
calls `load_sector` BEFORE `start_sector*`. Default loadout lookup:
`Schematics::get_default_loadout(core_block_id)` (`core/game/schematics.rs:705`).

**Changes.**
1. `core/maps/generators/planet.rs` (or a new `core/maps/planet/structures.rs`,
   `pub mod structures;` in `planet/mod.rs`):
   ```rust
   /// What `Schematics.placeLaunchLoadout` needs (`Universe.getLastLoadout/getLaunchResources`).
   pub struct LaunchLoadout { pub schematic: crate::game::schematic::Schematic, pub stacks: Vec<(crate::content::ItemId, i32)> }
   /// Live-world context for generator steps that place buildings (Java runs them inside generate()).
   pub struct StructureCtx<'a> {
       pub sim: &'a mut crate::sim::Sim,
       pub content: &'a ContentRegistry,
       pub bases: Option<&'a crate::ai::BaseRegistry>,
       pub loadout: Option<&'a LaunchLoadout>,
       pub rules: &'a mut crate::game::rules::Rules,
       pub player_team: u8,
       pub wave_team: u8,
   }
   ```
   `WorldGenerator`: `fn player_spawn(&self) -> Option<(i32, i32)> { None }` and
   `fn place_structures(&mut self, _ctx: &mut StructureCtx) {}`.
   Serpulo/Erekir/Tantros/Asteroid return their spawn (Tantros: `(w/2, h/2)`).
   Serpulo `place_structures` (this task): `place_launch_loadout(ctx, self.spawn)`
   where the shared helper does `ctx.sim.place_loadout(&lo.schematic, x as i16, y as i16, ctx.player_team, true)`,
   then stocks the first returned core entity's `ItemModule` with `lo.stacks`
   capped at the core's `storage_capacity` exactly like
   `MindSimHost::place_launch_loadout` does today (move that stocking code
   into a `pub fn stock_core(sim, entity, stacks)` in mind-core and call it
   from both places). Implement the same one-line `place_structures` for the
   other three generators (their spawns).
2. `gdext/sim_host.rs`: field `pending_launch: Option<(i32, String)>` (schematic
   index or -1, stacks json) + `#[func] pub fn set_pending_launch(&mut self, schematic_index: i32, stacks_json: GString)`.
   In `load_sector_world` (generator branch only), after
   `install_live_block_runtime` and the `BaseCatalog` insert: resolve the
   loadout — index >= 0 -> `Schematics` loaded like `place_launch_loadout`
   (factor that loading into a private `fn load_user_schematics(&self) -> Schematics`);
   index -1 or no pending -> the planet default (`basicShard`/`basicBastion`/
   `basicNucleus` by planet name through `get_default_loadout(core id)`;
   if `None`, log a warning and skip). Build `StructureCtx` (bases from the
   `BaseCatalog` resource: clone the `Arc` first), call
   `generated.generator.place_structures(&mut ctx)`, write `ctx.rules` back
   into `self.sector_rules`, set `self.generated_loadout_placed = true`
   (new field, reset at the top of `load_sector_world`), then
   `self.sim.refresh_unit_runtime()`. Clear `pending_launch`.
   Add `pub fn generated_loadout_placed(&self) -> bool`.
3. `gdext/campaign.rs start_sector_with_loadout`: skip the existing
   `place_launch_loadout` call when `host.bind().generated_loadout_placed()`.
4. `planet_dialog.gd _launch`: before `load_sector`, call
   `sim_host.call("set_pending_launch", loadout, JSON.stringify(core_stacks))`
   for the launcher branch (compute `core_stacks` exactly as today, BEFORE
   `load_sector`; move that block up) and `set_pending_launch(-1, "[]")` for the
   free launch. `remove_sector_items` stays where it is.

**Tests.** `generated_sector_reports_spawn` (serpulo sector 1:
`player_spawn().is_some()`, spawn tile not solid);
`place_structures_places_shard` (a `runtime_sim()`-style Sim built from the
generated grid — see `core/sim/production_tests.rs runtime_sim()` for how to
install the live runtime — with `Loadouts` basicShard: a `core-shard` building
exists at the spawn and holds the given stacks).

**Verify in-engine** (fresh `--import`; generated sector id A from T2):
```bash
$G --eval "$MC.prime_sector_load(\"serpulo\", A); $H.set_pending_launch(-1, \"[]\"); $H.load_sector(\"serpulo\", A); return $H.best_core_position()" \
   --eval "return $MC.start_sector(\"serpulo\", A)" \
   --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
$G --eval "return $MC.get_hud_state()"            # hasCore true
$G --eval "return $H.has_player_unit()"            # true (H00)
$G --screenshot runs/<date>-s<N>-H05-T4/landed.png
```
Then the real UI path: give Ground Zero a foundation core is NOT needed —
use a sector neighbouring an imported based sector with core >= 4 if the
planet page offers `Launch` (T3 of G05 rules); record the result either way.
(Checked 2026-10-14 round 3: on the clean import no `preset == null` sector is
`selectable` in the planet view, and in the seeded Java profile every
numbered sector hovered shows "Locked" — `SerpuloPlanetGenerator.java:91-97
allowLanding` needs a based, un-attacked neighbour with `bestCoreType.size >= 4`.
Expect the UI path to be unavailable on this profile.)
Also verify `step 600` without Game Over: `$H.set_paused(true)`, `$H.step(600)`,
`get_hud_state().hasCore` still true.

**Pitfalls.** `place_loadout(check = true)` removes environment walls under
non-core footprints (Java comment says so). Core id for the default lookup:
`registry.block_id("core-shard")` etc. Never hold a `bind()` of the host across
`MindCampaign` calls (env §3).

**Commit.** `PAR-0112: place the launch loadout core at the generated sector spawn (H05 T4)`

---

### T5. Enemy bases in generated attack sectors (PAR-0112 step 3)

> **Coordinator decision (2026-10-14, after round-3 verification).** Keep T5 but as the LAST task of this plan, headless-only: vanilla Serpulo never runs `BaseGenerator` (all attack sectors are authored maps), so it has no in-engine trigger. Execute it only after every other H05 task.

> **In-engine verification (2026-10-14, session 3, round 3):** SYMPTOM DOES NOT EXIST IN VANILLA — coordinator decision needed. Java never runs `BaseGenerator` on vanilla Serpulo: `hasEnemyBase()` for a preset-less cell needs `generateEnemyBase` (`Sector.java:145`), which only `SectorSubmissions.registerSector` sets — and only on the 45 cells it turns into hidden-map presets (`World.loadSector` loads their map, not the generator); `PlanetGenerator.generateSector` is a no-op (`PlanetGenerator.java:30`); the only other writer is the debug toggle (`PlanetDialog.java:655`); Erekir/Tantros/Asteroid generators have no `basegen`. In the port, attack sector 0 already loads its hidden map: `start_sector("serpulo",0)` -> `attack: true, enemyCores: 5, hasCore: true` (player `core-foundation` at (185,71), 100 copper), env 233, player alpha spawned (`runs/20261014-s3-verify3/H05-T5-sector0-hidden-preset.png`). The acceptance "sector 0 shows enemy cores" is therefore already met by the preset path and cannot test T5. Recommendation: make T5 low priority (mod/debug parity only) or keep it headless-only with a forced `has_enemy_base` extras test; T6 does not depend on T5's enemy half (only on `try_place`).

**Symptom / acceptance.** An attack (enemy-base) numbered sector has no enemy
cores/turrets; Java places core loadouts on every enemy room plus random base
parts, walls, cleared unit paths, and configures power. After T1 there are
45 such cells. Acceptance: on sector 0 (or the first `has_enemy_base` id with
`preset == null` the T1 test finds), `get_hud_state()` shows attack mode and
>= 1 enemy core; turrets of team 2 exist near the enemy core
(`building_team_at`), and `rules.spawns` is air-only when no ground spawn remains.

**Java.**
* `SerpuloPlanetGenerator.java:721-741`:
  ```java
  for(Room espawn : enemies) tiles.getn(espawn.x, espawn.y).setOverlay(Blocks.spawn);
  if(sector.hasEnemyBase()){
      basegen.generate(tiles, enemies.map(r -> tiles.getn(r.x, r.y)), tiles.get(spawn.x, spawn.y), state.rules.waveTeam, sector, difficulty);
      state.rules.attackMode = sector.info.attack = true;
  }else{ state.rules.winWave = sector.info.winWave = 10 + 5 * (int)Math.max(difficulty * 10, 1); }
  ```
  `postGenerate` (`:745-752`, called by `World.loadSector` after
  `loadGenerator`): if `hasEnemyBase`: `basegen.postGenerate()`; and if
  `spawner.countGroundSpawns() == 0`:
  `state.rules.spawns = Waves.generate(sector.threat, new Rand(sector.id), state.rules.attackMode, true, false);`
* `J/maps/generators/BaseGenerator.java` (whole file, 284 l.; transcribe it):
  * `generate` (42-160): return if `bases.cores` empty; `Mathf.rand.setSeed(sector.id); Mathf.rand.nextDouble();`
    `bracketRange = 0.17`, `baseChance = lerp(0.7, 2.1, difficulty)`, `wallAngle = 70`,
    `resourceChance = 0.5 * baseChance`, `nonResourceChance = 0.002 * baseChance`,
    `passes = difficulty < 0.4 ? 1 : difficulty < 0.8 ? 3 : 5`;
    `wall = getDifficultyWall(1, difficulty)`, `wallLarge = getDifficultyWall(2, difficulty)`;
    per core tile: `clearOverlay()`, `placeLoadout(bases.cores.getFrac((difficulty + Mathf.rand.range(0.4f)) / 1.4f).schematic, x, y, team, false)`,
    then `entity.items.add(item, entity.block.itemCapacity)` for EVERY item in content order;
    `passes` x `pass(...)` random schematics (resource branch / non-resource branch exactly as Java 79-93);
    the disabled `if(false)` wall-replace pass is skipped (keep a comment);
    small walls pass (98-127) and large walls pass (130-146);
    path clearing (151-168): `Astar.pathfind(core, spawn, cost, passable)` with
    cost `t.team() == waveTeam && !t.within(core, 80) ? 100000 : t.floor().hasSurface() ? 1 : 10`,
    passable `!t.block().isStatic()`; each path tile not within 80 world units of
    that core: remove waveTeam blocks on it and its 8 neighbours.
  * `postGenerate` (162-175): every centre tile of `waveTeam`: PowerNode ->
    `configureAny(new Point2[0])` then `placed()` (relinks lasers);
    Battery -> `power.status = 1`.
  * `pass(cons)` (177-180): `cores.first().circle(range = 180, ...)` (`Geometry.circle` disc order: x outer -r..r, y inner).
  * `getDifficultyWall(size, difficulty)` (33-40): blocks that are `Wall`, vanilla,
    `size == size`, `!insulated`, `buildVisibility == shown`, not `Door`,
    `isOnPlanet(state.getPlanet())`, sorted by `buildTime` (stable), `getFrac(difficulty * 0.91f)`.
  * `tryPlace(part, x, y, team, rand, posc)` (196-262): `rotation = random.range(2)`;
    `axis = ((int)(w/2f), (int)(h/2f))`; `result = Schematics.rotate(part.schematic, rotation)`;
    `rotator.set(part.centerX, part.centerY).rotateAround(axis, rotation * 90)`;
    `cx = x - (int)rotator.x; cy = y - (int)rotator.y`; reject if any tile
    `isTaken(block, realX, realY)` or (`oilExtractor` && `sumAttribute(oil) <= 0.001`);
    then `posc` callback per tile; if `part.required` is an Item: for every Drill
    tile, `iterateTaken` -> `set(placed, item)` on `hasSurface` floors and on a
    random neighbour `tiles.getc(ex + random.range(1), ey + random.range(1))`;
    `Schematics.place(result, cx + result.width/2, cy + result.height/2, team, false)`;
    then fill each placed Drill with `item` x `itemCapacity`. Return true.
  * `set(tile, item)`: `bases.ores[item]` -> overlay, else `bases.oreFloors[item]` -> floor.
  * `isTaken(block, x, y)`: `anyEnemyCoresWithin(waveTeam, x*8 + block.offset, y*8 + block.offset, rules.enemyCoreBuildRadius + 8)`
    (cores of any team != waveTeam whose `team.rules().protectCores`; `block.offset = ((size + 1) % 2) * 4`);
    else any tile in the padded footprint (`pad = 1`, offset `-(size-1)/2`)
    `overlaps(x, y)` = out of bounds || `!tile.block().alwaysReplace` || `world.getDarkness(x, y) > 0`.
* `Schematics.place(schem, x, y, team, overwrite=false)` (`Schematics.java:518-532`):
  origin `x - w/2, y - h/2`; skip tiles failing `Build.validPlace`; `setBlock` + `configureAny`.

**Port today.** `core/ai/base_registry.rs` (`BaseRegistry { cores, parts, req_parts, ores, ore_floors }`,
`for_resource(BaseResource)`, `BasePart { schematic, center_x, center_y, required, core, tier }`),
loaded in gdext as the `BaseCatalog` resource (`gdext/sim_host.rs:5417 load_base_catalog`,
inserted in `load_sector_world`); `core/maps/generators/base.rs` (stub +
`SchematicHooks`/`BaseRegistryView` trait seams — replace them);
`core/game/schematics.rs` `place(ctx, schem, x, y, team, overwrite)` (l.183),
`place_loadout` (l.214), `Schematics::rotate(schem, times, registry)` (l.810, check
it accepts negative `times`); `core/maps/generators/astar.rs pathfind(tiles, start, end, cost, passable)`;
`maps/generators/basic.rs:1079 circle`; `WorldCtx` ops (`core/world/ops.rs`:
`set_overlay` l.98, `set_floor` l.68, `set_block` l.130, `remove_block` l.200);
`SimRuntime::place_schematic` builds a `WorldCtx` inline (`core/sim/runtime.rs:399`);
`serpulo.rs generate_rules` (l.1349) already sets `attack_mode` and the spawns
with `airOnly = attack`, `naval = self.naval`.

**Changes.**
1. `core/sim/runtime.rs`: `pub fn with_world_ctx<R>(&mut self, world: &mut World, grid: &mut WorldGrid, f: impl FnOnce(&mut WorldCtx<'_>) -> R) -> R`
   (same construction as `place_schematic`), and `Sim::with_world_ctx` in
   `core/sim/mod.rs` that forwards (returns `None` without a runtime) and
   calls `refresh_unit_runtime()` afterwards.
2. Rewrite `core/maps/generators/base.rs` as a faithful port operating on
   `&mut StructureCtx` (drop the old trait seams; fix their users found by
   `grep -rn "SchematicHooks\|BaseRegistryView\|EmptyBaseRegistry" client/rust`):
   `pub fn generate(ctx, cores: &[(i32,i32)], spawn: (i32,i32), team: u8, sector_id: u32, difficulty: f32) -> BaseGenState`,
   `pub fn post_generate(ctx, state: &BaseGenState)`, `pub fn try_place(ctx, part, x, y, team, rand: &mut dyn BaseRand, posc: Option<&mut dyn FnMut(i32,i32)>) -> bool`
   where `BaseRand` is a tiny trait with `range_int(i32) -> i32` implemented for
   both `ArcRand` (enemy base, `Mathf.rand`) and the generator `SimRng`
   (ruins, T6: `range(n)` = `random(-n, n)` via `rand_range_int`).
   `getDifficultyWall` reads `BlockDef` fields (`kind`, `size`, `insulated`,
   `build_visibility`, `build_time`; vanilla = all base content; door kind —
   `grep -n "Door" client/rust/mind-core/src/content/registries/blocks/mod.rs`;
   `isOnPlanet` — `grep -rn "fn is_on_planet\|supports_env" client/rust/mind-core/src/world`).
   Darkness via T3 `get_darkness`. `alwaysReplace` via the port's block
   predicate (`grep -rn "always_replace" client/rust/mind-core/src/world`).
   Enemy-core proximity: iterate core buildings of teams != `wave_team`
   (team rules `protectCores` default true) from the ECS.
3. Serpulo `place_structures` (after T4's loadout): set spawn overlays (move
   the overlay loop from `generate` here, keeping it before the base), then if
   `self.sector.has_enemy_base()` and `ctx.bases` is `Some`: `base::generate(...)`
   with the enemy rooms (`self.enemy_spawns`), `self.spawn`, `ctx.wave_team`,
   `self.sector.id`, `self.sector.threat`; `ctx.rules.attack_mode = true`;
   then `base::post_generate`; then count ground spawns (overlay `spawn`
   tiles in `ctx.sim.grid`) and if 0:
   `ctx.rules.spawns = Waves::generate_with(threat, &mut ArcRand::new(id as u64), attack_mode, true, false)` mapped
   with the existing `spawn_group_to_json`. The `sector.info.attack = true`
   write is campaign state: `start_sector_with_loadout` already adopts
   `rules.attack_mode`; check `grep -n "info.attack" client/rust/mind-core/src/game/play.rs`
   and record if nothing sets it.
4. `load_sector_world` already passes `bases` (T4). If the catalog is
   missing log `warn!` once (Java early-returns too).

**Tests** (`core/maps/generators/base.rs`, using the real catalog: read the
`.msch` files under `REPO/assets/baseparts` the way `load_base_catalog` does
but with `std::fs`; skip the test with a printed note if the directory is
absent):
* `get_frac_clamps`; `difficulty_wall_order` (size 1, difficulty 0 -> the
  cheapest shown wall, e.g. `copper-wall`; difficulty 1 -> index `(0.91*n) as usize`).
* `enemy_base_places_cores_and_parts`: generate serpulo sector 0 with
  extras from `static_gen_extras`, run `place_structures` on a live Sim:
  >= 1 core of team 2 per enemy room, core items full, > 0 non-core team-2
  buildings, every placed part tile outside the player core radius.
* `post_generate_charges_batteries`.

**Verify in-engine.**
```bash
$G --eval "$MC.prime_sector_load(\"serpulo\", 0); $H.set_pending_launch(-1, \"[]\"); return $H.load_sector(\"serpulo\", 0)" \
   --eval "return $MC.start_sector(\"serpulo\", 0)" \
   --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
$G --eval "return $MC.get_hud_state()"   # attack mode, enemy cores > 0 (read the keys)
$G --eval "return JSON.parse_string($MC.get_rules_json()).attackMode"
$G --screenshot runs/<date>-s<N>-H05-T5/enemy-base.png   # center the camera on an enemy core first: $C.center_on_tile(x, y)
```
(If sector 0 has a preset in this build, use the T1 test's first id.)
Let 1200 ticks run (`set_paused(true)`, `step(600)` x2): the enemy base must
not self-destruct and the log must have 0 `panic`.

**Pitfalls.** `Mathf.rand` is a GLOBAL Arc Rand in Java; here a local
`ArcRand::new(sector_id)` + one `next_double()` reproduces it because nothing
else draws from it during generation. `placeLoadout(..., check = false)` for
enemy cores (no clearing). Item fill uses `block.itemCapacity` (core
`item_capacity`), not storage capacity — read the def field.

**Commit.** `PAR-0112: enemy bases (BaseGenerator) for generated attack sectors (H05 T5)`

---

### T6. Derelict ruins in generated sectors (PAR-0112 step 4)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED. Numbered sector 1 loaded: a scan of all 16384 tiles with `building_team_at` returns `-1` everywhere (no buildings at all, so no ruins and no core). Whether sector 1's Java ruin roll is > 0 is unknown (generator not observable in Java here); the test in the task picks a sector with a positive roll. Java not observed.

**Symptom / acceptance.** Java scatters 0-4 derelict base parts near ores
with scrap ore around and partial debris; the port skips them. After: on a
numbered sector where the ruin roll is > 0, team-0 buildings exist and some
are damaged. Acceptance: test below; in-engine `building_team_at` reports 0
at a ruin; `derelict` hint (`derelictmouse`) can fire there.

**Java** `SerpuloPlanetGenerator.java:633-711` (transcribe exactly; it runs
on the generator `rand` right after the decoration pass):
`difficulty = sector.threat; ruinCount = rand.random(-2, 4);` if > 0:
candidate list of tiles `padding = 25` inside, `!solid && (drop != null || floor.liquidDrop != null)`,
`ints.shuffle(rand)`; for each until `placed == ruinCount`: skip within 18
tiles of the spawn; `range = difficulty + rand.random(0.4f)`;
`part = overlay itemDrop ? bases.forResource(tile.drop()).getFrac(range)
 : liquidDrop && rand.chance(0.05) ? bases.forResource(liquid).getFrac(range)
 : rand.chance(0.05) ? bases.parts.getFrac(range) : null`;
`BaseGenerator.tryPlace(part, x, y, Team.derelict, rand, posc)` with `posc`
= oreScrap overlay on the tile (if `hasSurface`) and on d8 neighbours at
distance 1 (chance 0.4) and 2 (chance 0.2); on success `placed++` and the
debris circle `debrisRadius = max(w, h)/2 + 3`: for each centre building in
the circle: derelict && `rand.chance(lerp(0.05, 0.5, dst/debrisRadius))` ->
`remove()`, else `rand.chance(0.5)` -> `health -= rand.random(health * 0.9f)`.

**Port today.** The decoration pass ends at `serpulo.rs:~1311`; the rand is
`self.planet.base.rand` (SimRng, MapGen stream). `IntSeq.shuffle(rand)` —
check whether `BasicGenerator`/`SimRng` has an Arc-compatible shuffle
(`grep -n "fn shuffle" client/rust/mind-core/src`); Arc `IntSeq.shuffle`:
`for i = size-1 downTo 0: ii = rand.random(i); swap(i, ii)`.

**Changes.**
1. In `SerpuloPlanetGenerator::generate`, at the old "Ruins are skipped"
   comment, compute and store (do not place) the ruin plan inputs that depend
   only on tiles: `self.ruin_count`, and the candidate list — but the rand
   draws must happen in Java order, and placement needs the live world, so
   instead: store nothing here; move the whole ruin block into
   `place_structures` as its FIRST step (before T4's loadout), continuing with
   the SAME `self.planet.base.rand` state (it is not used after the
   decoration pass in `generate`; verify: no rand draw between the old
   comment and the end of `generate`). The "remove invalid ores" loop stays
   in `generate` (ruins only add ores on `hasSurface` floors).
2. Implement with T5's `try_place(..., team 0, &mut serpulo_rand, Some(posc))`
   and `Sim::with_world_ctx` for overlay writes, removals and health.

**Tests.** `ruins_placed_when_roll_positive`: find (loop ids 1..60) a serpulo
sector without preset whose ruin roll > 0, run generation + structures:
team-0 buildings exist, none within 18 tiles of the spawn.

**Verify in-engine.** Load that sector id via the T4 recipe; find a team-0
building by scanning `building_team_at` around ore tiles is too slow — use a
Rust-side `debug` log line in `place_structures` printing each ruin centre,
read it from `$U/logs`, then `$C.center_on_tile(x, y)` + screenshot
`runs/<date>-s<N>-H05-T6/ruin.png`.

**Pitfalls.** Order inside `place_structures`: ruins -> launch loadout ->
spawn overlays -> enemy base -> postGenerate -> spawn regeneration (Java order).

**Commit.** `Fixes PAR-0112: derelict ruins, loadout and enemy bases make generated sectors playable (H05 T6)`
(Use `Fixes` only if T2-T6 all landed and the T4/T5 in-engine checks passed;
else `PAR-0112: ...`.)

---

### T7. Sector notifications: invasion / off-play capture / loss toasts

> **Coordinator note (2026-10-14).** You own all three toasts (invasion, off-play capture, loss); H04 T4 no longer toasts. Do not add a `sector_invaded` signal.

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED by code + menu toast fact corrected. `gdext/campaign.rs:548-553` logs only `report.events.len()`; `#[func] run_turn` (l.1043) discards the report. 20 `$MC.run_turn()` calls in Ground Zero produced no observable event (the func returns a bool and logs nothing) — the verify step needs a log line (added to Change 2 in place). FIXED in place: `ui_root.gd _on_toast` does NOT suppress the label in the menu (only the sound): `MindUi.toast("TOAST-IN-MENU probe","warning")` at the main menu draws it (`runs/20261014-s3-verify3/H05-T7-toast-in-menu.png`); Java `HudFragment.showToast` returns in the menu (`HudFragment.java:753-754`).

**Symptom / acceptance.** While playing, Java shows a toast when another
sector is invaded, captured or lost; the port drops the turn events
(`gdext/campaign.rs:548` logs `report.events.len()` only). After:
`sector.attacked` toasts with the warning icon on `CampaignEvent::SectorInvasion`;
`sector.capture` / `sector.lost` for non-played sectors once H04 emits them.

**Java** `ui/fragments/HudFragment.java:261-275`:
```java
Events.on(SectorCaptureEvent.class, e -> { if(e.sector.isBeingPlayed()) ui.announce("@sector.capture.current", 5f); else showToast(Core.bundle.format("sector.capture", e.sector.name())); });
Events.on(SectorLoseEvent.class, e -> showToast(Icon.warning, Core.bundle.format("sector.lost", e.sector.name())));
Events.on(SectorInvasionEvent.class, e -> showToast(Icon.warning, Core.bundle.format("sector.attacked", e.sector.name())));
```
Bundle: `sector.attacked = Sector [accent]{0}[white] under attack!`,
`sector.capture = Sector [accent]{0}[white] Captured!`, `sector.lost = Sector [accent]{0}[white] lost!`.

**Port today.** `core/game/universe.rs:281-295 CampaignEvent { Turn, SectorInvasion { planet, sector, win_wave } }`;
`gdext/ui/hud.rs:249 #[func] push_toast(text, icon)` and the bundle
formatter used at `hud.rs:429`; sector display names: the campaign view's
`name` rule (`grep -n "fn sector_display_name\|fn display_name" client/rust/mind-core/src/game client/rust/mind-gdext/src`).

**Changes.**
1. `gdext/campaign.rs` both `run_turn` call sites (l.548 pump and l.1043
   `#[func] run_turn`): iterate `report.events`; for `SectorInvasion` build
   the text with the live bundle `format("sector.attacked", [name])` and call
   `/root/MindHud`'s `push_toast(text, "warning")` via `get_node_or_null` +
   `call` (MindHud is not bound here). For future variants
   (`SectorLose`/`SectorCapture` emitted by H04 for off-play sectors) add
   match arms only if the variants exist when you start (`grep -n "enum CampaignEvent" -A20 client/rust/mind-core/src/game/universe.rs`);
   otherwise record the seam (§6).
2. Do not toast while `state.isMenu()` (Java `showToast` returns in the
   menu, `HudFragment.java:753-754`). CORRECTED 2026-10-14 round 3: the port's
   `ui_root.gd _on_toast` (l.408-414) guards only the `uiNotify` sound; the
   label is still shown in the menu (`MindUi.toast("…","warning")` at the main
   menu draws it, `runs/20261014-s3-verify3/H05-T7-toast-in-menu.png`). Make
   `_on_toast` `return` when `menu_group.visible` (MindHud toasts reach it via
   `hud_fragment.gd:69-70,426` -> `MindUi.toast`). Also `log::info!` each
   event's planet/sector in Change 1 so the verify step can grep the log
   (`#[func] run_turn` returns only a bool).

**Tests.** None in Rust (glue). Parse gate.

**Verify in-engine.** In a based sector with invasions enabled, force turns:
`for i in 1..20: $MC.run_turn()` (separate evals) until the log prints an
invasion, screenshot the toast `runs/<date>-s<N>-H05-T7/invasion.png`. If no
invasion occurs in 20 turns, record "not reproduced" (invasion odds are
random) and keep the code.

**Commit.** `Gameplay H05 T7: sector invasion toasts from campaign turns`

---

### T8. Research dialog fidelity (false G05 claims) + live spend replay (PAR-0145, PAR-0047)

> **In-engine verification (2026-10-14, session 3, round 3):** PARTLY (2 of 5 visual symptoms not reproduced; live spend already works). (1) NOT REPRODUCED: the Dim is visible at `Color(0,0,0,0.9)` with `full_dialog = true` both in game (`runs/20261014-s3-verify3/H05-T8-tree.png`) and from the menu (`H05-T8-tree-from-menu.png`); Change 1 stays harmless. (2) CONFIRMED ~96 px title icon vs Java's 32 px icon + centred accent label (`java-11-techtree.png`). (3) NOT REPRODUCED: `research_tree_json("").items` = the based sectors' items (menu: 461 copper / 500 lead from the Ground Zero core; in game the live core); it was empty only because the tutorial core was empty — Java's list is empty on its profile too (`java-11-techtree.png`). Change 4 is conditional and stays a no-op. (4) CONFIRMED PopupPanel (`H05-T8-root-selector.png`) vs Java BaseDialog "Tech Tree Selection" with 300x60 icon rows (`java-12-techtree-select.png`). (5) CONFIRMED: the blank square is the Ground Zero sector node (`sector-groundZero-ui` missing); Java draws `Icon.terrain` — FIXED in place (Change 5 reduced to that fallback; the style mapping already exists). Live spend (PAR-0145) WORKS today: with 300 scrap in the GZ core, `dialog("research")._on_node_pressed(98)` (Scrap Wall; the node was off-screen) -> node `locked: false`, core scrap 100 (`H05-T8-researched.png`). Recipe: node `index` from `research_tree_json`; off-screen nodes -> call the handler.

**Symptom / acceptance.** Evidence `runs/20261013-s5-G05-T1c/hover-locked-reqs.png`
vs Java `runs/20261009-065416-ref-ground-zero/java/r05-01-tree.png`:
(1) the port shows the menu undimmed behind the tree (Java dims black9 —
`ResearchDialog` is a plain `BaseDialog`, `super("")`); (2) the title button
draws a ~96 px icon on top; Java: 32 px icon, accent label centred, 300-wide
`Tex.button`; (3) the `@globalitems` list is empty on a profile with stored
items; (4) the root selector is a PopupPanel; Java opens a BaseDialog titled
`@techtree.select`; (5) some locked nodes render as blank squares.
Acceptance: screenshot parity on those five points + a live click that spends
items and unlocks a node (PAR-0145/PAR-0047 verified).

**Java.**
* `ResearchDialog.java` ctor (first ~70 l. after `public ResearchDialog()`):
  ```java
  super("");
  titleTable.remove(); titleTable.clear(); titleTable.top();
  titleTable.button(b -> {
      b.imageDraw(() -> root.node.icon()).padRight(8).size(iconMed);     // iconMed = 32
      b.add().growX();
      b.label(() -> root.node.localizedName()).color(Pal.accent);
      b.add().growX();
      b.add().size(iconMed);
  }, () -> new BaseDialog("@techtree.select"){{
      cont.pane(t -> t.table(Tex.button, in -> {
          in.defaults().width(300f).height(60f);
          for(TechNode node : TechTree.roots){
              if(node.requiresUnlock && !node.content.unlockedHost() && node != getPrefRoot()) continue;
              in.button(node.localizedName(), node.icon(), Styles.flatTogglet, iconMed, () -> { ...switch root...; hide(); })
                .marginLeft(12f).checked(node == lastNode).row();
      ...
  margin(0f).marginBottom(8);
  cont.stack(titleTable, view = new View(), itemDisplay = new ItemsDisplay()).grow();
  ```
  Find the width/height of the title button cell in the same block
  (`grep -n "titleTable" J/ui/dialogs/ResearchDialog.java`) and copy it.
* `ui/ItemsDisplay.java:28-66`: `table(Tex.button)` margin 10 / left 12 / top 15 / right 12;
  collapser (0.3 s) with rows `formatAmount(n)` + 24 px icon (pad 4/4) + lightGray name, item-id order, only `items.has(item)`;
  header `c.button("@globalitems", Icon.downOpen, Styles.flatTogglet, col::toggle)` minWidth 200, margin 12, padBottom 4,
  icon `upOpen` when collapsed; `collapsed` defaults to `false` (field initial value) — the list is OPEN.
  Items = the planet's total over based sectors (`ResearchDialog.rebuildItems`).
* Node button style (`ResearchDialog.java:505-513`):
  `up = !locked ? Tex.buttonOver : !selectable || (!canSpend && !net.client()) ? Tex.buttonRed : Tex.button`;
  image = `node.selectable ? content.uiIcon : Icon.lock`; image colour
  `!locked ? white : node.selectable ? gray : Pal.gray`.

**Port today.** `ui/dialogs/research_dialog.gd` (`_ready` l.23-31 sets
`full_dialog = true`; `_build` l.33-80; `_open_root_selector` l.134-163;
`_items_display` `MindResearchItemsDisplay`); node drawing in the view class
(`grep -rn "class_name MindResearchView" client/ui`); tree JSON
`MindCampaign.research_tree_json(root)` (`gdext/campaign.rs:1348`, `items`
key, node `locked`/`selectable`/`can_spend`).

**Changes.**
1. `research_dialog.gd _ready`: remove `full_dialog = true` (BaseDialog
   default = black9 Dim + `defaultDialog`). If the panel then shrinks to its
   minimum, make the Panel fill the viewport WITHOUT switching the style:
   set the `Center/Panel` anchors to full rect and `custom_minimum_size` to
   the viewport size (Java `BaseDialog` is `setFillParent(true)`), keeping
   the Dim visible.
2. Title button: `HBoxContainer` inside the button: `TextureRect` 32x32
   (`root.icon`), expand spacer, `Label` with accent colour
   (`MindStyles.ACCENT` or the theme accent), expand spacer, 32 px empty
   spacer; button style `Tex.button` equivalent (`"defaultb"`/"button" theme
   variation — use whatever `MindWidgets.button` uses), width from Java.
3. Root selector: replace the PopupPanel with a transient `MindDialog`-less
   overlay ONLY if adding a manifest dialog is not acceptable; preferred: a
   new manifest dialog `research_root_select` (scene instance of
   `mind_dialog_base.tscn`, title `@techtree.select`, rows 300x60
   `flatTogglet` icon+name buttons, close button) registered in all three
   places (env §3.1: `ui/dialogs_manifest.json`, `scenes/ui/ui_root.tscn`,
   `EXPECTED_*_DIALOGS` in `core/ui/manifest.rs`) AND in `expected_pause`
   (`manifest.rs:403`, value `false`). It emits the chosen root back via
   `MindUi.open_dialog("research", {"root": name})` or a signal the research
   dialog connects to; filter roots like Java (`requiresUnlock && !unlocked && != prefRoot` skipped).
4. Items display: check why `_tree.items` is empty: print
   `JSON.parse_string($MC.research_tree_json("")).items` on the imported
   profile. If empty, fix the Rust side to sum `Sector.items()` over based
   sectors of the root's planet (Java `rebuildItems`), item-id order. Default
   expanded. Rows per Java.
5. Node style: map `locked/selectable/can_spend` to the three styles and the
   icon/tint rules above; blank squares mean the region lookup failed —
   fall back to `Icon.lock` glyph exactly when `!selectable`, and log the
   missing region name once. (Verified 2026-10-14 round 3: the style/tint/lock
   mapping ALREADY exists in `ui/widgets/research_view.gd` `_style_for`
   l.135-150, `_image_color` l.126-132, lock glyph l.119-122. The blank square
   is the `Ground Zero` sector node: its icon `sector-groundZero-ui` is not in
   the atlas. Java `SectorPreset.loadIcon` (`SectorPreset.java:132-134`) uses
   `Core.atlas.find("sector-" + name, Icon.terrain.getRegion())`, so the
   fallback for `type == "sector"` nodes is the `terrain` glyph, not `lock`
   (`java-11-techtree.png`). This change reduces to that fallback.)

**Tests.** Parse gate; `cargo test -p mind-core --lib ui::manifest` if a
dialog was added.

**Verify in-engine** (fresh `--import`, in Ground Zero via env §4.1):
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("research", "")'; sleep 2
$G --screenshot runs/<date>-s<N>-H05-T8/tree.png     # compare with java r05-01-tree.png
# live spend (PAR-0145): pick a locked, selectable node with requirements the core holds; seed items first:
$G --eval "$H.debug_add_core_items(\"copper\", 2000); $H.debug_add_core_items(\"lead\", 2000); return true"
# click it through its node (find the node control by name in MindResearchView), then:
$G --eval "return JSON.parse_string($MC.research_tree_json(\"\")).nodes.filter(func(n): return n.name == \"<node>\")[0]"
$G --screenshot runs/<date>-s<N>-H05-T8/researched.png   # compare java r05-14-researched.png
```
Expected: the node turns unlocked (Tex.buttonOver), the items row shines,
core copper decreased by the requirement.

**Commit.** `Fixes PAR-0145: research dialog dim, title, root selector, items and node styles; live spend verified (H05 T8)`

---

### T9. Arc `Strings.autoFixed` truncation (stat values prerequisite)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED by code (`core/ui/stat_display.rs:24-38` rounds half-up via `java_round`; Arc `Strings.java:761-773` floors). JDK oracle run against the Arc jar (`javac -cp arc-core-1.0.jar`): 0.291666->"0.291", 0.6666667->"0.666", 1.3333334->"1.333", 1.5->"1.5", 2->"2", 0.4->"0.4", 60/7 (2)->"8.57", 17.5/60->"0.291", but **-1.25 (max 1) -> "-1.3"** — FIXED in place.

**Symptom / acceptance.** `core/ui/stat_display.rs:25 auto_fixed` rounds half
up; Arc truncates. Every stat/tooltip number with more decimals than `max`
is off by one in the last digit (e.g. duo build time 0.29167 s -> Java
`0.291`, port `0.292`).

**Java** `ARC/arc-core/src/arc/util/Strings.java:761-811`:
```java
public static String autoFixed(float value, int max){
    value = (float)Mathf.floor(value * Mathf.pow(10, max) + 0.001f) / Mathf.pow(10, max);
    int precision = Math.abs(Mathf.floor(value) - value) < 0.0001f ? 0 :
                    Math.abs(Mathf.floor(value * 10) - value * 10) < 0.0001f ? 1 :
                    Math.abs(Mathf.floor(value * 100) - value * 100) < 0.0001f ? 2 :
                    Math.abs(Mathf.floor(value * 1000) - value * 1000) < 0.0001f ? 3 : 4;
    return fixed(value, Math.min(max, precision));
}
fixedBuilder(d, places): negative = d < 0; d = |d|; dec = (int)(float)(d * Math.pow(10, places) + 0.0001f);
  places == 0 -> "-"? + dec; else insert "." at len - places (leading "0." + zeros when len <= places).
```
`Mathf.floor(float)` returns `int` (`(int)Math.floor`); `Mathf.pow` is float.

**Changes.** Rewrite `auto_fixed(value: f32, decimals: usize) -> String` and
add `pub fn fixed(d: f32, places: usize) -> String` in `stat_display.rs`,
f32 arithmetic in Java order (`Math.pow(10, places)` in `fixedBuilder` is
double: compute `(d as f64 * 10f64.powi(p) + 0.0001f32 as f64) as f32 as i32`).
Keep `fix_value` = `auto_fixed(v, 3)`.

**Tests.** `auto_fixed_matches_arc`: `0.291666 -> "0.291"`, `0.6666667 -> "0.666"`,
`1.3333334 -> "1.333"`, `1.5 -> "1.5"`, `2.0 -> "2"`, `0.4 -> "0.4"`,
`-1.25 (max 1) -> "-1.3"` (CORRECTED 2026-10-14 round 3 with the Arc jar: `Mathf.floor(-12.499) = -13`; the other values were confirmed by the same JDK run), `60.0/7.0 (max 2) -> "8.57"`. Run the whole
mind-core suite: fix any existing test that asserted the rounded value only
if the Java value is the truncated one (cite this task).

**Commit.** `Gameplay H05 T9: Strings.autoFixed truncates like Arc`

---

### T10a. Content info (`?`) stats: model, base stats, endpoint, renderer

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED. `$MC.has_method("content_info_json")` false; `open_dialog("content", {"content":"duo"})` shows icon, a raw **`[accent]Duo`** header (markup not rendered — extra symptom, render the name through markup in Change 3), a stray `block` line and the description only (`runs/20261014-s3-verify3/H05-T10a-duo-info.png`). Java reference captured: `java-26-duo-info.png` — fill-parent `Info` dialog, `Purpose` + description, `General`: Health 250, Size 1x1, Build Time 0.291 seconds, Build Cost copper 35; `Liquids`: Liquid Capacity 20 liquid units; `Function`: Range 20 blocks, Inaccuracy 2 degrees, Firing Rate 3/sec, Targets Air/Ground Yes, Ammo (copper 9 damage 2 ammo/item, graphite 18 damage ...). Facts verified: duo `health = 250` (`Blocks.java`), `buildTime = 35 * 0.5 = 17.5` ticks (`Block.java:1453-1464`).

**Symptom / acceptance.** `content_info_dialog.gd:33` calls
`MindCampaign.content_info_json`, which does not exist -> name/description
only, no stat rows. After: Java-ordered categories/stats. Acceptance: `?` on
`duo` shows `General` with Size `1x1`, Health `250`, Build Time
`0.291 seconds`, Build Cost copper 35; `copper` item shows Explosiveness `0%`,
Flammability `0%`, Radioactivity `0%`, Charge `0%`.

**Java.**
* `world/meta/Stat.java`: 88 entries (`grep -c "new Stat(" J/world/meta/Stat.java` = 88), id = declaration index. Order and category:
  general: health, armor, size, displaySize, buildTime, buildCost, memoryCapacity,
  explosiveness, flammability, radioactivity, charge, heatCapacity, viscosity,
  temperature, flying, speed, buildSpeed, mineSpeed, mineTier, payloadCapacity,
  baseDeflectChance, lightningChance, lightningDamage, abilities, canBoost,
  boostingSpeed, maxUnits, damageMultiplier, healthMultiplier, speedMultiplier,
  reloadMultiplier, buildSpeedMultiplier, reactive, healing, immunities;
  items: itemCapacity, itemsMoved, launchTime, maxConsecutive; liquids: liquidCapacity;
  power: powerCapacity, powerUse, powerDamage, powerRange, powerConnections,
  basePowerGeneration, meltdownTime, warmupTime; crafting: tiles, input, output,
  productionTime, maxEfficiency, drillTier, drillSpeed, linkRange, instructions;
  function: weapons, bullet, speedIncrease, repairTime, repairSpeed, range,
  shootRange, inaccuracy, shots, reload, crushDamage, legSplashDamage,
  targetsAir, targetsGround, damage, status, frequency, ammo, ammoCapacity,
  ammoUse, shieldHealth, cooldownTime, regenerationRate, activationTime,
  moduleTier, unitType, receiveRate; optional: booster, boostEffect, affinities, opposites.
  **Re-count against the file** before coding (the list above has to total 88;
  if the file differs, the file wins). Label `bundle("stat." + name.toLowerCase())`, tooltip `stat.<name>.info` when present.
* `StatCat.java`: general, power, liquids, items, crafting, function, optional (label `category.<name>`).
* `StatUnit.java`: read the file; each `(name, space, icon)`; `localized() = none ? "" : bundle("unit." + name.toLowerCase())`
  (bundle has `unit.powersecond`; the port's `stat_display.rs:114` key `unit.powersec` is WRONG).
* `Stats.java`: ordered map category -> stat -> values; `add`, `remove`, `replace`;
  `toMap()` sorts categories and stats by id (values keep insertion order);
  `useCategories`; `timePeriod` (-1). Helpers `add(stat, float, unit)` = number;
  `addPercent(stat, v)` = `number((int)(v*100), percent)`; `add(stat, bool)`; `add(stat, fmt, args)` (`@` placeholders).
* `StatValues.java:35-145`: `string`, `bool` (`@yes`/`@no`), `fixValue = autoFixed(v, 3)` (T9),
  `number(v, unit)` -> `(icon? icon+" ") + fixValue(v)` then `(space?" ") + unit.localized()`;
  `squared`, `multiplierModifier`, `percentModifier`, `items(displayName, stacks)`,
  `items(timePeriod, stacks)` (`amount/(timePeriod/60)` + `unit.persecond`),
  `items(timePeriod, filter)`, `liquid(l, amount, perSecond)`, `liquids(filter, amount, perSecond)`,
  `liquids(timePeriod, perSecond, stacks)`.
* Base `setStats`: `world/Block.java:644-678` (size `"@x@"`; synthetic -> health,
  armor if > 0; `canBeBuilt && requirements` -> buildTime `/60` seconds and
  buildCost `items(false, requirements)`; instantTransfer -> itemsMoved instant
  + maxConsecutive; every consumer `display(stats)` in declaration order;
  liquidCapacity if hasLiquids; itemCapacity if hasItems (or `none. unabletoclog`)),
  consumers `world/consumers/Consume*.java display`, `type/Item.java setStats`
  (addPercent explosiveness, flammability, radioactivity, charge),
  `type/Liquid.java setStats` (+temperature, heatCapacity, viscosity),
  `type/StatusEffect.java:98-137`, `type/UnitType.java:790-853` minus
  weapons/abilities (T10e).
* `ui/dialogs/ContentInfoDialog.java:35-135` layout: header icon 48 + accent
  name; description `[lightgray]` width 500 padLeft 10 under `@category.purpose`;
  per category (sorted) an accent label when `useCategories`; per stat row
  padLeft 10: `"[lightgray]" + label + ":[] "` then cells with 10 px gaps;
  details `[gray]` width 400 padTop 20; credit gray padTop 40.
* Dump of all 80 `setStats` bodies (use while writing T10a-e):
  `cd J && for f in $(grep -rln "public void setStats()" .); do awk '/public void setStats\(\)/{p=1;d=0} p{print FILENAME":"FNR": "$0; n=gsub(/{/,"{"); m=gsub(/}/,"}"); d+=n-m; if(d==0 && /}/){p=0}}' $f; done > /tmp/setstats.txt`.

**Port today.** `ui/dialogs/content_info_dialog.gd` (139 l., `_render_stats`
l.87 expects `{category,label,value}` rows); `core/world/stats.rs` (dead,
wrong `StatCat`); `core/ui/stat_display.rs`; block data via
`core/world/block.rs BlockTable::build_default(&registry)` (l.240) and
`BlockView` (`def()`, `consumers()`, `kind_data()`); markup:
`mind_core::ui::text::render_markup(s, iconc)` (`core/ui/text.rs`), used by
`MindAssets.bundle_markup` (`gdext/assets/atlas.rs`).

**Changes.**
1. New directory module `core/ui/content_stats/` (`mod.rs` + family files
   later): `StatCat`, `Stat` (with `ALL: [Stat; 88]`, `name()`, `category()`),
   `StatUnit` (`key()`, `space()`, `icon()`), and
   ```rust
   #[derive(Serialize)] #[serde(tag = "k")]
   pub enum Cell { Text { t: String }, Icon { region: String, amount: Option<String>, size: u8, content: Option<String> }, Space { w: f32 }, Row, Panel { cells: Vec<Cell> } }
   pub struct Stats { pub use_categories: bool, pub time_period: f32, map: Vec<(StatCat, Vec<(Stat, Vec<Vec<Cell>>)>)> }
   pub struct StatCtx<'a> { pub registry: &'a ContentRegistry, pub blocks: &'a BlockTable, pub bundle: &'a dyn Fn(&str) -> String }
   pub mod values;   // the StatValues helpers
   pub fn content_stats(ctx: &StatCtx, content: ContentRef) -> Stats
   ```
   Dispatch by content type; Block -> base then a family hook (empty here).
   Leave `core/world/stats.rs` with a doc line pointing to the new module.
2. `gdext/campaign.rs #[func] content_info_json(&mut self, name: GString) -> GString`:
   resolve block/item/liquid/unit/status/sector/planet by name; cache a
   `BlockTable` in `MindCampaign` (build once per registry); emit
   `{"content","type","localized","icon","description","details","credit","use_categories","locked","hide_details","categories":[{"name","label","stats":[{"name","label","tooltip","values":[[cells…]]}]}]}`;
   Text cells through `render_markup` (if `MindCampaign` cannot reach the
   `Iconc` table, add `#[func] MindAssets.markup_text(raw) -> GString` and
   let GDScript convert).
3. `content_info_dialog.gd`: render per the Java layout (one HBox per stat,
   Icon cells as `TextureRect` 24/32 px with an outline amount label, click ->
   `open_content_info(content)`, `Row` -> new line, 10 px spacer after each
   value). Keep the bundle fallback only when the endpoint returns `{}`.

**Tests** (`core/ui/content_stats/mod.rs`): `stat_ids` (`Health as usize == 0`,
`ItemCapacity.category() == Items`, last = `Opposites`, `ALL.len() == 88`);
`copper_item_stats` (4 rows, `"0"` + `"%"`, no space); `duo_block_stats` —
size `"1x1"`, health `"250"` (`Blocks.java` duo `health = 250`), buildTime
`"0.291"` + `" seconds"` (35 copper x cost 0.5 x buildCostMultiplier = 17.5
ticks — confirm the formula in `Block.java init` `buildCost` lines and
`Items.java` copper `cost`), buildCost one Icon `item-copper-ui` amount `"35"`;
`cryofluid_liquid_stats` (`Liquids.java` cryofluid fields; viscosity default
0.5 -> `"50"`).

**Verify in-engine.**
```bash
$G --eval "return JSON.parse_string($MC.content_info_json(\"copper\")).categories[0].stats.size()"   # 4
$G --eval 'return get_node("/root/MindUi").open_dialog("content", JSON.stringify({"content":"duo"}))'; sleep 2
$G --screenshot runs/<date>-s<N>-H05-T10a/duo-info.png
```

**Pitfalls.** `Strings.format` uses `@` placeholders. Region names:
`unit-<n>-ui`, `item-<n>-ui`, `liquid-<n>-ui`, `status-<n>-ui`. No world access in the endpoint.

**Commit.** `Gameplay H05 T10a: Stats model and base content stats for the content info dialog`

### T10b. Content stats: production, crafting, power

> **In-engine verification (2026-10-14, session 3, round 3):** NOT CHECKABLE in engine before T10a (no stat endpoint). Code facts unchanged.

Java (each calls `super.setStats()` first; use `/tmp/setstats.txt`):
`GenericCrafter` (sets `timePeriod = craftTime` BEFORE super), `AttributeCrafter`,
`Separator`, `HeatCrafter`, `HeatProducer`, `Drill` (`drillTier` drillables,
`drillSpeed = 60/drillTime*size*size` itemsSecond, liquid boost), `BurstDrill`,
`BeamDrill`, `Pump`, `SolidPump`, `Fracker`, `WallCrafter`, `PowerGenerator`
(`basePowerGeneration` = `powerProduction*60` powerSecond), `ConsumeGenerator`,
`SolarGenerator`, `ThermalGenerator`, `NuclearReactor`, `ImpactReactor`,
`VariableReactor`, `HeaterGenerator`, `PowerNode` (laserRange blocks,
maxNodes), `BeamNode`, `PowerVoid`, `ItemSource`; StatValues `drillables`,
`blocks(attr…)`, `speedBoosters`, `itemBoosters`, `boosters`,
`itemEffMultiplier`, `liquidEffMultiplier`, `displayItemPercent`
(`StatValues.java:325-660`). Port data: `core/world/block_kind_data.rs`
variants; power defs `grep -rn "pub struct .*Def\b" client/rust/mind-core/src/world/blocks/power`.
File `core/ui/content_stats/production.rs`, one `fn <family>_stats` per class.
Tests: `mechanical_drill_stats` drillSpeed `60/600*2*2 = 0.4` -> `"0.4"`;
`graphite_press_stats` productionTime `"1.5"` seconds, output graphite
`1/(90/60)` -> autoFixed truncation `"0.666"`, input coal 2 -> `"1.333"`.
**Commit.** `Gameplay H05 T10b: production, crafting and power content stats`

### T10c. Content stats: distribution, storage, logic, defense (non-turret)

> **In-engine verification (2026-10-14, session 3, round 3):** NOT CHECKABLE in engine before T10a.

`Conveyor` (itemsMoved = `displayedSpeed` itemsSecond), `StackConveyor`,
`Duct`, `DuctRouter`, `DuctBridge`, `DuctJunction` (removes itemCapacity),
`OverflowDuct`, `Junction`, `ItemBridge`, `BufferedItemBridge`, `MassDriver`,
`DirectionalUnloader`, `Unloader`, `CoreBlock`, `LaunchPad`, `LandingPad`,
`LiquidJunction` (removes liquidCapacity), `LogicBlock`, `MemoryBlock`,
`LogicDisplay`, `Wall`, `ShieldWall`, `MendProjector`, `OverdriveProjector`,
`ForceProjector`, `RegenProjector`, `ShockMine`, `ShockwaveTower`,
`BuildTurret`. File `content_stats/logistics.rs`. Tests: `conveyor_stats`
itemsMoved `"5"` (titanium `"10"`, `Blocks.java displayedSpeed`); `mender_stats`
one concrete value read from `MendProjector.setStats`.
**Commit.** `Gameplay H05 T10c: logistics, storage, logic and defense content stats`

### T10d. Content stats: unit factories and payload blocks

> **In-engine verification (2026-10-14, session 3, round 3):** NOT CHECKABLE in engine before T10a.

`UnitFactory`, `Reconstructor` (output = upgrade pairs icon -> arrow -> icon,
only `unlockedNow` pairs, `Styles.grayPanel` boxes with 40 px icons ->
`Cell::Panel`), `UnitAssembler`, `UnitAssemblerModule`, `RepairTurret`,
`RepairTower`, `PayloadConveyor`, `PayloadMassDriver`, `Constructor`. Port:
`BlockDef.unit_plans`, `reconstructor_upgrades`, `assembler_plans`.
Test: `additive_reconstructor_lists_unlocked_upgrades` (all unlocked: dagger->mace first).
**Commit.** `Gameplay H05 T10d: unit factory and payload content stats`

### T10e. Content stats: turrets + ammo, unit weapons and abilities

> **In-engine verification (2026-10-14, session 3, round 3):** NOT CHECKABLE in engine before T10a. Java reference for the acceptance exists: `runs/20261014-s3-verify3/java-26-duo-info.png` (copper `9 damage`, `2 ammo/item`; graphite `18 damage`; ammo panels use a dark `grayPanel`-like box).

`BaseTurret` (shootRange blocks, activationTime), `Turret`, `ReloadTurret`,
`ItemTurret` (removes itemCapacity; `ammo = StatValues.ammo(ammoTypes)`;
ammoCapacity `maxAmmo/max(ammoPerShot,1)` shots), `LiquidTurret`,
`PowerTurret`, `LaserTurret`, `ContinuousTurret`, `ContinuousLiquidTurret`,
`PayloadAmmoTurret`, `PointDefenseTurret`, `TractorBeamTurret`;
`StatValues.ammo` (from l.700 to EOF), `weapons(unit, weapons)` (l.655),
`abilities` (l.682), `statusText` (l.229). Port: `core/world/blocks/defense/turrets/`,
`core/weapons/`, unit registry. Split `T10e-1` turrets+ammo / `T10e-2`
weapons+abilities if large. Test: `duo_ammo_lists_copper_graphite_silicon`
(insertion order; copper damage `9`).
**Commit.** `Gameplay H05 T10e: turret ammo and unit weapon content stats`

---

### T11a. Custom Rules dialog: rule rows, search, edit menu (PAR-0132 part 1)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED (`runs/20261014-s3-verify3/H05-T11a-rules.png` vs `step-316-rules.png`): a tab rail with raw keys (`rules.title`, `rules.environment`, ..., body `@rules.title`), no rows, no search, centred panel (Java is fill-parent). The verify `open_dialog("custom_rules", JSON.stringify({"rules": JSON.parse_string($MC.get_rules_json())}))` works as written.

**Symptom / acceptance.** `custom_rules_dialog.gd` (53 l.) shows only a tab
rail. Java (reference `runs/20261010-234250-ref-PAR-0070-combat-fx/java/step-316-rules.png`):
a search field, then one scrolling list of categories (accent title + 3 px
accent rule) with check/number/text/team rows. After: every Java row exists,
edits change a working `Rules` copy, and the caller receives it on hide.

**Java** `J/ui/dialogs/CustomRulesDialog.java`:
* ctor (52-110): title `@mode.custom`, `setFillParent(true)`, `shown(setup)`,
  close button, `@edit` (Icon.pencil) button -> `@edit.menu` dialog with
  `@copy.clipboard` (rules JSON without spawns), `@load.clipboard`
  (keeps spawns/objectives; error `@rules.invaliddata`), `@settings.reset`
  (`resetter`), buttons 280x64 `Styles.cleart`.
* `setup` (122-139): `Icon.zoom` + field (`@players.search`, lower-cased,
  spaces collapsed) then `cont.pane(main)`.
* `setupMain` (140-345): categories in order **waves, resourcesbuilding,
  unit, enemy, environment, light, music, planet, teams** with the exact rows
  listed in the Java file (transcribe every `check/number/numberi/text/team`
  call: label key, setter field, getter, enable condition, min/max, `*60`/`/60`
  and `*tilesize` scale factors). Buttons: `@configure` (loadout dialog,
  999999 cap, reset `copper x100`), `@bannedblocks`, `@bannedunits`,
  `@rules.weather` (weather dialog), ambient light colour picker, planet
  buttons (`flatTogglet` 140x50, 3 per row, `planet.applyRules(rules, true)`,
  `@rules.anyenv`), per-team collapsers (`Team.baseTeams`, 260x55 `togglet`).
  `showRuleEditRule` adds `@rules.allowedit` at the top of "teams".
* `addToMain` (347-353): title label accent padTop 20 padRight 100 padBottom -3,
  accent image height 3 padBottom 20, then the category table.
* Row builders (393-470): `check` (checkbox, disabled when !condition);
  `number`/`numberi` (label white/gray by condition, field width 120 padRight 50/100,
  validated by min/max and `canParsePositiveFloat`); `text` (width 300);
  `team` (label + one 60x60 `squareTogglei` per base team tinted with its colour);
  `ruleInfo` -> tooltip `<key>.info` when the bundle has it.
* Search filter: a row is skipped unless `bundle(key).toLowerCase().contains(ruleSearch)`.

**Port today.** `ui/dialogs/custom_rules_dialog.gd`; rules JSON is serde
camelCase (`core/io/json/rules.rs`, e.g. `waveTimer`, `winWave`,
`waveSpacing`, `teams`); dialogs to reuse: `loadout_dialog.gd`,
`banned_content_dialog` (editor scenes), `picker` (colour); bundle keys
`rules.*` exist (`grep -n "^rules\." REPO/assets/bundles/bundle.properties | wc -l`).

**Changes.**
1. Context: `open_dialog("custom_rules", {"rules": <json>, "reset": <json>, "show_rule_edit": bool, "return_to": "<caller>"})`.
   Keep a `_rules: Dictionary` working copy (parsed JSON).
2. Data-driven rows: a GDScript const array per category, each entry
   `{"kind":"check|number|numberi|text|team|button", "key":"@rules.waves", "field":"waves", "scale":1.0, "min":0, "max":INF, "cond":"waves"}`
   where `cond` names a tiny GDScript predicate function (`_cond_waves()` =
   `_rules.waves`, etc. — one function per distinct Java condition). Transcribe
   ALL Java rows, in order. Team sub-rules write `_rules.teams[<team id>].<field>`
   (check the JSON shape of `teams` with `$MC.get_rules_json()` first).
3. Layout per `addToMain` + row builders; search field re-filters.
4. `@edit` menu with the three actions (`DisplayServer.clipboard_get/set`).
5. On hide emit `rules_edited(json: String)` (signal) — consumers: T11b and
   the existing custom-game flow (`grep -rn "custom_rules" client/ui`), which
   must keep working.

**Tests.** Parse gate. Rust: none.

**Verify in-engine.** Menu -> Play -> Custom Game -> pick a map -> Customize
(or `open_dialog("custom_rules", JSON.stringify({"rules": JSON.parse_string($MC.get_rules_json())}))`),
screenshot `runs/<date>-s<N>-H05-T11a/rules.png` vs `step-316-rules.png`;
type `wave` in search -> only wave rows remain.

**Commit.** `PAR-0132: Custom Rules dialog rule rows, search and edit menu (H05 T11a)`

### T11b. Pause-menu rules button and live rule application (PAR-0132 part 2)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED by code; in-engine recipe BLOCKED (note added in place). `set_rules_json` (`gdext/campaign.rs:1119-1133`) writes only `self.session.rules`; the pump mirror is now at l.570 (`self.session.rules = runtime.session.rules.clone()`). The campaign pause menu correctly has no book (`allowEditRules` false; `runs/20261014-s3-verify3/H05-T11b-pause.png`); Java shows the book bottom-left in a sandbox custom game (`java-23-escape-inventory.png`, `step-374-pause.png`). The port's Custom Game path never loads the map (`H05-T11b-sandbox.png`), so no non-campaign session exists to click the button in — coordinator: custom-map loading has no owner in round H.

**Java** `J/ui/dialogs/PausedDialog.java:20-37`:
```java
stack(cont, new Table(t -> {
    t.bottom().left();
    t.button(Icon.book, () -> {
        Rules toEdit = Vars.state.rules.copy();
        rulesDialog.show(toEdit, () -> state.rules.copy());
        rulesDialog.hidden(() -> { Vars.state.rules = toEdit; Call.setRules(toEdit); });
    }).size(70f).tooltip("@customize").visible(() -> state.rules.allowEditRules && (net.server() || !net.active()));
})).grow().row();
```
Reference frame `step-374-pause.png` (book button bottom-left).

**Port today.** `paused_dialog.gd` (no button); `MindCampaign.set_rules_json`
(`gdext/campaign.rs:1119`) assigns `self.session.rules` only — the next
`pump_runtime` copies `runtime.session.rules` back over it (`campaign.rs:565`),
so an edit during play is lost. World re-derivation:
`CampaignRuntime.rules_world_dirty = true` -> `Sim::apply_dirty_rules_to_world`
(`core/sim/runtime.rs:1382-1395`).

**Changes.**
1. `set_rules_json`: after `apply_set_rules` succeeds, if a live runtime
   exists (`host.bind_mut().take_campaign_runtime()` pattern used at
   `campaign.rs:999`), write the same rules into `runtime.session.rules`, set
   `runtime.rules_world_dirty = true`, put the runtime back. Keep the
   `allowEditRules` guard (`guard_set_rules`).
2. `paused_dialog.gd`: a 70x70 icon button (`book` glyph, tooltip
   `@customize`) anchored bottom-left of the dialog, visible only when
   `JSON.parse_string($MC.get_rules_json()).allowEditRules` (and not a
   network client — `/root/Spine/MindNet` state if exposed; else always);
   pressed -> open `custom_rules` with `{"rules": current, "reset": current}`
   stacked over the pause menu; on `rules_edited(json)` -> `set_rules_json(json)`.

**Verify in-engine.** Sandbox/custom game with `allowEditRules` (Custom Game
-> Sandbox), Esc -> book -> untick `@rules.fire`, close ->
`return JSON.parse_string($MC.get_rules_json()).fire` = false after
`$H.step(10)`. Screenshot the pause menu vs `step-374-pause.png`.
BLOCKED as written (verified 2026-10-14 round 3): the port's Custom Game flow
never loads the map. `map_play_dialog.gd _play` emits `play_requested` (no
listener anywhere) and calls `MindCampaign.start_map_mode` (does not exist)
-> fallback `start_map` (`gdext/campaign.rs:954`) only runs `play_map` on the
session: from the menu the result is an empty grey world with the previous
HUD, `modeName` "sandbox" but `infiniteResources` false
(`runs/20261014-s3-verify3/H05-T11b-sandbox.png`). Until a custom-map load
exists, verify headless: a mind-gdext-free test that `set_rules_json` on a
session with a live `CampaignRuntime` changes `runtime.session.rules` and
survives one `pump_runtime`-equivalent mirror, plus the parse gate and a
screenshot of the button in a non-campaign session if one can be reached.

**Commit.** `Fixes PAR-0132: in-game rule editing from the pause menu applies live (H05 T11b)`

---

### T12. Hints remainder

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED by code. `hints_fragment.gd:145-147` `(preset != "fungalPass" or not fungal_pass_saved)` (Java `&&` of both negations); `sim_host.rs:5284 let cores_destroyed = 0i64`; no Rust producer of `"cannotupgrade"`/`"factorycontrol"` (only the doc comment `sim/mod.rs:48`); completion l.224-227 is the eased `command_mode && selection > 0`. Dependency added: the factorycontrol in-engine check needs H08 T6b (today `command_tap` is units-only, so right-clicking with a selected factory emits nothing).

**Symptom / acceptance.** (a) `unitSelectControl` shows on Fungal Pass; (b)
`serpuloCoreZone` never shows in attack maps (`cores_destroyed` is always 0);
(c) `cannotUpgrade` and (d) `factoryControl` never complete/show; (e)
`unitSelectControl` completes without a command target.

**Java** `ui/fragments/HintsFragment.java`:
* `:199-203` unitSelectControl shown: `isSerpulo() && units > 3 && !net.active() && !player.dead() && state.getSector() != SectorPresets.fungalPass.sector && SectorPresets.fungalPass.sector.save == null`;
  complete: `control.input.commandMode && selectedUnits.size > 0 && selectedUnits.first().controller() instanceof CommandAI ai && ai.targetPos != null`.
* `:285` serpuloCoreZone: `!attackMode || stats.getDestroyed(coreShard) + getDestroyed(coreFoundation) + getDestroyed(coreNucleus) > 0`.
* `:84` `Events.run(Trigger.cannotUpgrade, () -> events.add("cannotupgrade"))`;
  fired by `world/blocks/units/Reconstructor.java:252-257` when
  `upgrade != null && !upgrade.unlockedNowHost() && !team.isAI()`.
* `:91-95` `BuildingCommandEvent` with a `PayloadBlockBuild` -> `"factorycontrol"`
  (fired by `InputHandler.java:491` for commanded buildings).

**Port today.** `ui/fragments/hints_fragment.gd:145-147` (`or`), `:186`,
`:175`, `:254`; `gdext/sim_host.rs:5284 let cores_destroyed = 0i64;` in the
hints JSON; `GameStats.destroyed_block_count` (`core/io/json/rules.rs:856`,
filled by `core/game/stats.rs:113`); `Sim.hint_events.events` (`core/sim/mod.rs:219`,
`FireCreated` pattern at l.909-916); building command dispatch
`core/sim/mod.rs:1405-1427` (`SimCommand` command-building branch);
Reconstructor update `core/world/blocks/units/behavior.rs:451-475` (no
unlock gate today); `$H.command_targets_json()` (`[{id,x,y,attack}]`);
`$I.selection_json()` (id array).

**Changes.**
1. `hints_fragment.gd:147`: `and preset != "fungalPass" and not bool(hints.get("fungal_pass_saved", false))`.
2. `sim_host.rs` hints JSON: `cores_destroyed` = sum of
   `destroyed_block_count["core-shard"|"core-foundation"|"core-nucleus"]`
   from the live `CampaignRuntime` session stats (read the resource the same
   way the surrounding code reads the session; `grep -n "CampaignRuntime" client/rust/mind-gdext/src/sim_host.rs`).
3. factorycontrol: in the command-building branch (`sim/mod.rs:~1424`) after
   `apply_command_building`, if the block kind is a payload block (UnitFactory,
   Reconstructor, UnitAssembler, PayloadConveyor/Router excluded — use the Java
   class list: `PayloadBlock` subclasses; match `BlockKind` variants by name)
   insert `"factorycontrol"` into `self.hint_events.events`.
4. cannotupgrade: define `pub struct CannotUpgradeFired;` (Resource) next to
   `FireCreated`; drain it in `Sim::tick` like `FireCreated` into
   `hint_events.events.insert("cannotupgrade")`. The Reconstructor unlock gate
   itself is H02's (seam §6): if H02 has landed a
   `unlocked_now_host` check in `ReconstructorBehavior` (`grep -n "unlock" client/rust/mind-core/src/world/blocks/units/behavior.rs`),
   insert the resource there; otherwise leave a `// H02 seam` comment at the
   upgrade site and record it.
5. unitSelectControl completion: `_I.command_mode()` and the first id of
   `selection_json()` appears in `command_targets_json()` (non-null x/y).

**Tests.** `factory_command_records_hint` (sim with a ground-factory, apply
the command-building `SimCommand`, assert the event).

**Verify in-engine.** Ground Zero: place `ground-factory` via `place_block`,
select it in command mode and right-click a tile (bridge input recipe) ->
`return JSON.parse_string($H.hint_state_json())` lists `factorycontrol`.

**Commit.** `Gameplay H05 T12: remaining hint predicates and hint events (fungal pass, cores destroyed, factory control, command targets)`

---

### T13. Schematics dialog cards per Java (PAR-0143) + `schematic-background` in the atlas

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED + 2 new findings. Port cards (`runs/20261014-s3-verify3/H05-T13-cards-ingame.png`): no action row, name badge at the bottom, no tiled background, import button in the search row, and the `basicNucleus` image overflows its card. Java (`java-02-schematics.png`, `java-03b-schematics-row3.png`): `Tags:` row, 50 px action row (info/upload/pencil/trash), top badge, tiled `schematic-background`, Back + Import in the bottom row. NEW: the port lists the 4 vanilla loadouts (`basicBastion`, `basicFoundation`, `basicNucleus`; none are user files — the 22 `.msch` files in `$U/schematics` have other names) — Change 5 added in place. NEW: the Java `Tags:` row is missing (note 6 added).

**Symptom / acceptance.** Port evidence `runs/20261013-s5-G05-T10b/cards.png`
vs `SchematicsDialog.java:150-205`: missing per-card 50 px action row
(info / upload / pencil / trash), name badge at the bottom instead of the top,
`togglet` instead of `flati` with `Tex.pane`, import button in the search row
instead of the bottom button row, a non-Java detail panel, menu click
selects instead of opening the info dialog; tiled background missing
(sprite not packed).

**Java** (pasted in the inventory research; key layout):
```java
sel[0] = t.button(b -> {
    b.top(); b.margin(0f);
    b.table(buttons -> { buttons.left(); buttons.defaults().size(50f);
        buttons.button(Icon.info, Styles.emptyi, () -> showInfo(s)).tooltip("@info.title");
        buttons.button(Icon.upload, Styles.emptyi, () -> showExport(s)).tooltip("@editor.export");
        buttons.button(Icon.pencil, Styles.emptyi, () -> showEdit(s)).tooltip("@schematic.edit");
        buttons.button(Icon.trash, Styles.emptyi, () -> ui.showConfirm("@confirm", "@schematic.delete.confirm", () -> { schematics.remove(s); rebuildPane.run(); })).tooltip("@save.delete");
    }).growX().height(50f);
    b.row();
    b.stack(new SchematicImage(s).setScaling(Scaling.fit), new Table(n -> { n.top();
        n.table(Styles.black3, c -> { Label label = c.add(s.name()).style(Styles.outlineLabel).color(Color.white).top().growX().maxWidth(200f - 8f).get();
            label.setEllipsis(true); label.setAlignment(Align.center); }).growX().margin(1).pad(4).maxWidth(Scl.scl(200f - 8f)).padBottom(0);
    })).size(200f);
}, () -> { if(sel[0].childrenPressed()) return;
    if(state.isMenu()) showInfo(s); else if(!state.rules.schematicsAllowed) ui.showInfo("@schematic.disabled"); else { control.input.useSchematic(s); hide(); } })
 .pad(4).style(Styles.flati).get();
sel[0].getStyle().up = Tex.pane;
```
ctor: `buttons.button("@schematic.import", Icon.download, this::showImport)` after `addCloseButton()`;
search `ignoreSymbols` regex strip on both sides; Enter (`Binding.chat`) in the search field uses `firstSchematic`.
`showInfo(s)` = `SchematicInfoDialog` (name, image, requirements, power).

**Port today.** `ui/dialogs/schematics_dialog.gd` (cards l.141-167, import
l.69-72, detail panel `_rebuild_detail`, actions l.155-246 reusable);
`ui/widgets/schematic_image.gd` (background via
`MindWidgets.icon_texture("schematic-background")` which misses). Atlas
packing: `bash tools/pack.sh pack`, manifest of included sprites in
`client/rust/mind-tools` (`grep -rn "sprites/" client/rust/mind-tools/src | head`).

**Changes.**
1. Atlas: add `sprites/schematic-background.png` to the packed set (find the
   include list/glob in mind-tools; if sprites outside `blocks/` etc. are
   excluded by a rule, add an explicit entry) or load it as a plain
   `Texture2D` from the assets dir in `schematic_image.gd` (`load()` of the
   resolved asset path) — prefer the atlas if the rule is a simple list.
2. Cards per Java: Button with `flati`-equivalent style and `Tex.pane` up
   (theme variation `"pane"` panel inside a flat button if the theme has no
   such button style), VBox: 50 px action HBox (4 icon buttons `emptyi`,
   tooltips), then a 200x200 stack (image + TOP badge `black3` outline label,
   ellipsis, max width 192). Action presses must not trigger the card press.
3. Move the import button to the bottom button row after Back; remove the
   detail panel; menu click opens an info view: reuse the detail panel
   content as a `schematic_info` dialog only if one exists
   (`grep -n "schematic_info" client/ui/dialogs_manifest.json`); otherwise add
   the manifest dialog `schematic_info` (all three registrations +
   `expected_pause` false) showing name, `MindSchematicImage`, requirements.
4. Search: strip symbols with Java's `ignoreSymbols` (`grep -n "ignoreSymbols" J/ui/dialogs/SchematicsDialog.java`); Enter uses the first match.
5. (added 2026-10-14 round 3) Do not list the vanilla loadouts. The port's
   `Schematics::load_loadouts` (`core/game/schematics.rs:555-566`) pushes
   basicShard/Foundation/Nucleus/Bastion into `all`, so the dialog shows
   `basicBastion`, `basicFoundation`, `basicNucleus` cards
   (`runs/20261014-s3-verify3/H05-T13-cards-ingame.png`). Java's
   `loadLoadouts` (`Schematics.java:108-109`) -> `checkLoadout` (l.325-338)
   puts them only in `loadouts`/`defaultLoadouts`; `schematics.all()` holds
   the user files (`java-03b-schematics-row3.png`: "Switch on/off" -> "glass"
   -> "graphite"). Filter them out in `MindCampaign.list_schematics`
   (`gdext/campaign.rs:2237`) with the schematic's own index kept in each row
   (`MindInput.use_schematic(index)` and the export/delete actions use `all`
   indices — keep a `"index"` field and use it everywhere instead of the row
   position), or move the loadouts out of `all` if every `all` index user is
   updated in the same commit (grep `\.all\b` in `core/game/schematics.rs`
   and `gdext/`). Pick the first; record the choice.
6. (noted 2026-10-14 round 3, not a change here) Java shows a `Tags:` row
   (tag toggles + pencil, `SchematicsDialog.java` `rebuildTags`) under the
   search field (`java-02-schematics.png`); the port has none. Out of this
   task's scope unless trivial — list it in the summary as a follow-up.

**Verify in-engine.** Menu -> Schematics: screenshot
`runs/<date>-s<N>-H05-T13/cards.png`; in Ground Zero open schematics, click
a card -> placement preview follows the mouse (G08 seam).

**Commit.** `Fixes PAR-0143: schematics cards with action row, top name badge and Java click behaviour (H05 T13)`

---

### T14. Planet globe: drag-rotate, wheel zoom, no camera snap on select, last sector, new-preset reveal

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED by code: `ui/planet_view.gd` has no drag/scroll handlers, `focus_tile` (l.322-331) moves the camera, and no `<planet>-last-sector` key is written anywhere (`grep -rn "last-sector"` finds only `last-sector-save`). Java drag-rotates the globe (`java-09-rotated.png` after a 300 px drag). Verify dry-run: `/root/Spine/Planet/PlanetView` exists (script `planet_view.gd`); `drag`/`cam_pos` do not exist yet (T14 adds them).

**Symptom / acceptance.** The globe cannot be dragged or zoomed; selecting
snaps the camera (`ui/planet_view.gd:322 focus_tile`); the page never opens
at the last played sector (`<planet>-last-sector` is never written); newly
unlocked presets are not revealed. After: Java behaviour. Acceptance: drag
300 px right rotates the camera azimuth by `300/9*speed` degrees; wheel down
x3 -> zoom 1.3; reopening the page looks at the last played sector.

**Java** `ui/dialogs/PlanetDialog.java`:
* drag (114-138): ignored while revealing (`showing() && newPresets.peek() != lastSector`), then `newPresets.clear()`;
  `pos = state.camPos; upV = pos.angle(Vec3.Y); speed = 1 - |upV - 90| / 90;`
  `pos.rotate(camUp, cx / 9 * speed);` `amount = cy / 10; amount = clamp(upV + amount, 1, 179) - upV;`
  `pos.rotate(camUp.cpy().rotate(camDir, 90), amount);` (Arc y-up drag: `cy = -relative.y` in Godot).
* wheel (140-147): `zoom = clamp(zoom + amountY / 10, planet.minZoom, planet.maxZoom)` (`maxZoom` 2);
  `state.zoom = lerpDelta(state.zoom, zoom, 0.4)`.
* camera distance (`graphics/g3d/PlanetRenderer.java:65`):
  `camPos.setLength((radius + camRadius) * 4 + (zoom - 1) * (radius + camRadius) * 2)`.
* `show()` (238-291): `lookAt(state.planet.getLastSector())`; selecting does
  NOT move the camera (only `launching` lerps `lookAt(selected, 0.1)`).
* `uiAlpha = lerpDelta(uiAlpha, zoom < 1.9 ? 1 : 0, 0.1)`.
* New presets (270-288, 986-1003): on show, every preset with `unlocked() && sector.preset == preset && !alwaysUnlocked && !sector.info.shown && requireUnlock && !sector.hasBase() && preset.planet == state.planet`
  -> add, `info.shown = true`, `saveInfo()`; if any: append `getLastSector()`, reverse; per frame while non-empty: `to = peek()`, `presetShow += delta`, `lookAt(to, 0.11)`, `zoom = 0.75`;
  at `presetShow >= 20` && `size > 1` (once): `ui.announce(Iconc.lockOpen + " [accent]" + to.name(), 2)`;
  at `presetShow > 144` pop + reset. Esc during reveal truncates to the last element.
* `type/Planet.java:312-321`: `getLastSector()` = `sectors.get(min(settings.getInt(name + "-last-sector", startSector), size - 1))`;
  `setLastSector(s)` = `settings.put(name + "-last-sector", s.id)` — called by `Control.playSector` (`Control.java:448`).

**Port today.** `ui/planet_view.gd` (683 l.: `camera`, `CAM_LENGTH`,
`focus_tile` l.322, `pick` l.412); `planet_dialog.gd _input` (picking),
`MindCampaign.planet_view` (`gdext/campaign.rs:3307`, no `min_zoom`);
`PlanetDef.min_zoom` (`planets.rs:333`); `SectorInfo.shown`;
`MindUi.announce(text, duration)`.

**Changes.**
1. `planet_view.gd`: state `cam_pos: Vector3`, `zoom := 1.0`, `zoom_target := 1.0`;
   `_process`: `zoom = lerp(zoom, zoom_target, 1 - pow(1 - 0.4, delta * 60))`,
   camera position = `cam_pos.normalized() * ((r + cr) * 4 + (zoom - 1) * (r + cr) * 2)`,
   `look_at(Vector3.ZERO, Vector3.UP)`; `drag(relative)` per Java
   (`cam_up = camera.global_transform.basis.y`, `cam_dir = -basis.z`,
   `Vector3.rotated(axis.normalized(), deg_to_rad(a))`); `scroll(amount_y)`;
   `look_at_tile(tile, alpha := 1.0)` (slerp); remove the camera move from `focus_tile`.
2. `MindCampaign.planet_view`: add `min_zoom` and `max_zoom` (2.0).
3. `planet_dialog.gd _input`: left-drag motion -> `view.drag(event.relative)`,
   `_dragged = true` (a release after a drag must not select); wheel ->
   `view.scroll(±1)`; guard with the top-of-stack check used by T2 of G05
   (`is_top()` or equivalent — grep `func is_top` in `ui/mind_dialog.gd`).
4. Last sector: in `gdext/campaign.rs start_sector_with_loadout` and the
   resume path (`play_sector` `#[func]`), write
   `self.settings.put_i32(&format!("{planet_name}-last-sector"), sector)` and
   persist (cite `Control.java:448`). On page show call
   `view.look_at_tile(last)` using `MindUi.settings_get_i32` (or a new
   `planet_view` field `last_sector`).
5. New presets: `pub fn new_presets(planet: &mut Planet, registry) -> Vec<u16>`
   in `core/game/planet.rs` (Java filter; marks `info.shown`), `#[func] MindCampaign.take_new_presets(planet) -> PackedInt32Array`
   (saves infos, persists), reveal loop in `planet_dialog.gd _process`
   (ticks = `delta * 60`), called deferred from `shown()`.

**Tests.** `new_presets_marks_shown` (`core/game/planet.rs`).

**Verify in-engine.**
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'var v = get_node("/root/Spine/Planet/PlanetView"); v.drag(Vector2(300, 0)); return v.cam_pos'
$G --screenshot runs/<date>-s<N>-H05-T14/dragged.png
```

**Pitfalls.** Camera exactly on the Y axis is degenerate — keep the 1..179° clamp.

**Commit.** `Gameplay H05 T14: planet globe drag/zoom, last sector, no snap on select, new preset reveal`

---

### T15. Planet page: rename pencil and sector icon picker

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED. No pencil/icon button; the title row shows the preset art (`runs/20261014-s3-verify3/H05-T15-gz-panel.png`) where Java shows the `Icon.noneSmall` button for Ground Zero (`java-06-gz-panel.png`). Port-today paragraph corrected in place. Java rename/icon picker on a numbered sector not observed (no selectable numbered sector).

**Java** `PlanetDialog.java:1199-1283`: title row `[accent]name` padLeft 3;
pencil (`Icon.pencilSmall`, `clearNonei`, 40x40, padLeft 4) only when
`preset == null || !preset.requireUnlock` ->
`ui.showTextInput("@sectors.rename", "@name", 32, sector.name(), v -> { sector.setName(v); updateSelected(); rebuildList(); })`;
icon button 40x40 (contentIcon uiIcon, else `Icon.icons[info.icon + "Small"]`,
else `Icon.noneSmall`), tooltip `@sector.changeicon`, opens a fill-parent
`Dialog("")` with 48 px `squareTogglei` buttons: `Icon.none` (both null),
`accessibleIcons` (`J/Vars.java:164-169`), then per `defaultContentIcons`
type (item, liquid, block, unit, status) a 3 px accent rule + every
`!isHidden() && unlocked()` content (iconMed 32); `cols = min(20, screenW / 52)`;
choice -> `saveInfo(); hide(); updateSelected(); rebuildList()`; `@back` 210x64.

**Port today.** (Verified 2026-10-14 round 3) the title row
(`planet_dialog.gd _build_sector_panel`, ~l.968-986) adds a plain,
non-interactive `MindWidgets.image("sector-<preset>")` after the name (Ground
Zero shows its preset art); Java shows the 40x40 icon BUTTON with
`contentIcon`/`Icon.icons[info.icon+"Small"]`/`Icon.noneSmall` — the slashed
circle for Ground Zero (`java-06-gz-panel.png`, `step-141-sector.png`), never
the preset art. Replace that image with the icon button.
`SectorInfo.name/icon/content_icon`; `SectorView.named/icon/content_icon`
(`core/ui/campaign.rs`); `Sector::save_info`; `Campaign::sector_mut`;
`MindUi.show_text_input(title, message, max, default, numeric, allow_empty)` +
`text_input_result`; `ui/dialogs/icon_select_dialog.gd` (menu mode only).

**Changes.** `#[func] set_sector_name(planet, id, name) -> bool` and
`set_sector_icon(planet, id, icon, content_icon) -> bool` in
`gdext/campaign.rs` (empty = None; `save_info` + `persist`; push to the live
runtime copy when it is the played sector); title-row pencil/icon buttons in
`planet_dialog.gd _build_sector_panel`; a `"sector"` mode in
`icon_select_dialog.gd` (context `{"mode":"sector","icon","content_icon"}`,
none button, accessible icons, five content sections from
`MindCampaign.database_json` rows filtered unlocked/non-hidden, emits
`sector_icon_selected(icon, content_icon)`); fix its title: Java
`IconSelectDialog` has none (`@icons` key does not exist).

**Verify.** Rename a based numbered sector (a T4 sector you played) to
"Base A", `tools/session-play.sh N --play`, reopen: panel + list show it;
Ground Zero shows only the icon button (`requireUnlock` true). Restore the
profile (env §4.3).

**Commit.** `Gameplay H05 T15: planet page sector rename and icon picker`

---

### T16. Sector stats dialog with Abandon

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED. Port Stats for Ground Zero is a `full_text` dump with only `Wave: 1` and an OK button (`runs/20261014-s3-verify3/H05-T16-stats.png`); Java (`java-07-gz-stats.png`): description, `Time: 0:02:30`, `Wave: 1`, `Threat: Low`, `Resources:` icons, the lockdown line, Back + Abandon. Reference path corrected in place.

**Java** `PlanetDialog.java:1026-1147` showStats/displayItems + `1149-1171 abandonSectorConfirm`:
```
dialog = BaseDialog(sector.name()); cont.pane(c -> { c.defaults().padBottom(5)
 if preset && description: add(displayDescription).width(420).wrap().left()
 add(bundle("sectors.time") + " [accent]" + sector.save.getPlayTime())         // Strings.formatMillis
 if info.attempts > 0: "sectors.attempts [accent]n"
 if info.waves && hasBase(): "sectors.wave [accent]n"
 if isAttacked() || !hasBase(): "sectors.threat [accent]" + displayThreat()
 if save != null && info.resources.any(): "@sectors.resources" + 24 px icons padRight 3, padLeft 10
 if isAttacked() && !isBeingPlayed(): formatIcons(bundle("sector.lockdown")) wrap fillX padBottom 10
 displayItems(production, "@sectors.production"); displayItems(export, "@sectors.export" + destination line); if hasBase(): displayItems(imports, "@sectors.import" + sources)
 if hasBase() && items.total > 0: "@sectors.stored", rows icon + formatAmount(max(n,0)) lightGray, 4 per row, padLeft 10 })
addCloseButton(); if hasBase(): buttons.button("@sector.abandon", Icon.cancel, () -> abandonSectorConfirm(sector, dialog::hide))
displayItems: content.items() order; total = (int)(stat.mean * 60); total > 1 -> icon + formatAmount(total) + " " + bundle("unit.perminute") lightGray; 3 per row
```
`Strings.formatMillis` (`ARC/.../Strings.java:813`): `h:mm:ss`, hours unpadded.
Abandon: confirm `@sector.abandon.confirm`; played sector -> hide page, kill
player cores after 7 ticks; else clear items, `hasCore = false`, production, `saveInfo()`.

**Port today.** `SectorStatsView` (`core/ui/campaign.rs`, rows in IndexMap
order), `planet_dialog.gd _show_sector_stats` (`full_text` dump),
`MindCampaign.abandon_sector()` (played sector only), `clear_sector_record`
(`gdext/campaign.rs:3669`), save meta `SaveIo::get_meta` (`SaveMeta.time_played`).

**Changes.** Sort rate lists + stored by item id; add `playtime_ms`,
`destination`, `import_sources` (port `SectorInfo.eachImport`),
`lockdown`, `preset_description_key`; regenerate the UI golden; fill
`playtime_ms` in `campaign_views_json` from the save meta (played sector:
total playtime). New dialog `sector_stats` (scene instance of
`mind_dialog_base.tscn`; manifest entry `{"name":"sector_stats","scene":"res://scenes/ui/dialogs/sector_stats_dialog.tscn","pause":false,"menu_openable":false,"args":["planet:Str","sector:Int"]}`;
static instance in `scenes/ui/ui_root.tscn`; `EXPECTED_*_DIALOGS` AND
`expected_pause` `false` in `core/ui/manifest.rs`). `#[func] abandon_sector_at(planet, id) -> bool`
(delegates to `abandon_sector()` for the played sector, else
`clear_sector_record`-like clear of items/has_core/production + persist).
`@stats` opens it; delete `_show_sector_stats`.

**Tests.** `cargo test -p mind-core --lib ui::manifest`; `ui_goldens_match`;
`sector_stats_rows_in_item_order`.

**Verify.** Planet page -> Ground Zero -> Stats; screenshot; check `Time:`.
Java reference: `runs/20261014-s3-verify3/java-07-gz-stats.png` (description,
`Time: 0:02:30`, `Wave: 1`, `Threat: Low`, Resources icons, lockdown line,
Back + Abandon). `step-142-stats.png` does NOT show the dialog (same frame as
step-141).

**Commit.** `Gameplay H05 T16: sector stats dialog with item rows, playtime and abandon`

---

### T17. Core Database: icon planet tabs, current-planet default (PAR-0144)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED. On Onset the port database shows text tabs `All / Erekir / Serpulo` with `All` selected (`runs/20261014-s3-verify3/H05-T17-database-onset.png`, centred panel); Java (`java-04-database.png`, from the menu): 32 px icon tabs (eye, Erekir, Serpulo) with the eye (`All`) selected, fill-parent dialog. Java's in-game current-planet default was not observed (source `DatabaseDialog.java:37-125`). Reference path corrected in place.

**Java** `DatabaseDialog.java:37-125`: `shown`: if campaign and the tab list
contains the current planet -> select it, else in game `rules.planet`; tab
buttons `clearNoneTogglei` 50x50, image 32: sun -> `Icon.eyeSmall`, planet
-> `Icon.icons.get(p.icon, Icon.commandRally)` tinted `p.iconColor`; tooltip
`@all` / localized name; 10 per row; locked entries `Icon.lock` `Pal.gray`;
in game banned -> `Icon.cancel` scarlet 60%, banned sorted last.
Reference `runs/20261014-s3-verify3/java-04-database.png` (CORRECTED 2026-10-14
round 3: `menu-03-database.png` is the planet page and `menu-03b-database.png`
the main menu; neither shows the database).

**Port today.** `database_dialog.gd:71 _build_tabs` (text buttons);
`core/ui/database.rs:65 DatabaseTabView { name, localized, color, is_all }`;
`PlanetDef.icon` (`planets.rs:339`).

**Changes.** `DatabaseTabView.icon: String` (`"eyeSmall"` for sun, else
`def.icon`); 50x50 icon toggle tabs via `MindWidgets.glyph(icon, 32, color)`;
default tab from `MindCampaign.get_sector_state()` planet when in a
campaign; banned cross only if `rules.banned_blocks` is exposed
(`grep -n banned client/rust/mind-core/src/game/rules.rs`), else record.
**Tests.** `database_tab_icons`. Regenerate a database golden if one exists.
**Commit.** `Fixes PAR-0144: database planet tabs render icons and default to the current planet (H05 T17)`

---

### T18. Settings fan-out between MindUi and MindCampaign (PAR-0164)

> **Coordinator override (2026-10-14): WITHDRAWN — do not execute.** PAR-0164 is fixed by H04 T11 (one shared `settings_hub` store, the Java `Core.settings` model); a signal fan-out between two stores would be thrown away. Skip; write `skipped: H04 T11 owns PAR-0164` in your summary. If H04 T11 is on `main`, only run this task's in-engine acceptance (flip `campaignselect` via MindUi, read via MindCampaign) and record it.

> **In-engine verification (2026-10-14, session 3, round 3):** Withdrawal marker present (Coordinator override above). H04 T11 is not on `main` (`main` = `ffbe84dd`), so the optional acceptance was not run.

**Symptom / acceptance.** `MindUi.settings_put_flag/text/...`
(`gdext/ui/ui_host.rs:730-754`) write MindUi's `SettingsStore` and flush;
`MindCampaign` holds its own store loaded at boot (`gdext/campaign.rs:189`)
and keeps stale values (e.g. `saveinterval`, `campaignselect`) and can write
them back. After: a value set through MindUi is visible to `MindCampaign` in
the same session, and MindUi sees keys `MindCampaign` wrote.
Acceptance (ledger `to`): flip `campaignselect` through Settings, then read
it through `MindCampaign` in the same session.

**Changes.**
1. `ui_host.rs`: `#[signal] fn setting_changed(key: GString);` emitted after
   every `settings_put_*` (and `remove`).
2. `campaign.rs`: in `ready()` connect `/root/MindUi` `setting_changed` to a
   `#[func] fn on_ui_setting_changed(&mut self, key: GString)` that copies the
   value: `#[func] MindUi.settings_raw_json(key) -> GString` is not needed if
   both stores flush to disk immediately — simplest exact approach: re-read
   the single key from a fresh `SettingsStore::load(&NativeFs, &self.paths)`
   and `self.settings.put(key, value)` (or `remove`), WITHOUT marking dirty
   if the API allows (else accept a redundant flush).
3. Reverse: after `MindCampaign::persist` flushes, emit
   `#[signal] settings_flushed()`; MindUi connects and reloads its store from
   disk (safe: MindUi always flushes immediately, so it has no unsaved state —
   verify that `persist_settings` is called by every MindUi put).
4. Guard re-entrancy: signal handlers must not call back into the emitter
   while it is bound — use `call_deferred` connections (`ConnectFlags::DEFERRED`).

**Tests.** None in Rust (gdext glue); in-engine check.

**Verify in-engine.**
`$G --eval 'get_node("/root/MindUi").settings_put_flag("campaignselect", true); return true'`, `sleep 1`,
then a `MindCampaign` getter that reads it (`grep -n "campaignselect" client/rust/mind-gdext/src/campaign.rs`) returns the new value.

**Commit.** `Fixes PAR-0164: MindUi setting changes reach MindCampaign and back (H05 T18)`

---

### T19. Dialog hide fade-out (0.1 s)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED by code: `ui/mind_dialog.gd hide_dialog` (now l.137-146) sets `visible = false` immediately; Java `UI.java:132-133` `setHideAction(sequence(fadeOut(0.1f)))`.

**Java** `core/UI.java:132-133`: `Dialog.setHideAction(() -> sequence(fadeOut(0.1f)))`
(read the exact lines). Port `ui/mind_dialog.gd:119-121` hides instantly.
**Changes.** `hide_dialog()`: tween `modulate.a` to 0 over 0.1 s then
`visible = false` and restore alpha; a re-show during the fade cancels the
tween. `MindUi`'s stack bookkeeping stays synchronous (unchanged). Emit
`uiBack` only where Java does (G07 owns playback).
**Verify.** Open/close settings; screenshot mid-fade is optional; parse gate.
**Commit.** `Gameplay H05 T19: dialogs fade out over 0.1 s like Arc`

---

### T20a. Planet globe hex mesh with generator colours and the planet shader (PAR-0142 part 1)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED visually: port Serpulo/Erekir globes (`runs/20261014-s3-verify3/H05-T15-gz-panel.png`, `H05-T20-planet.png`) are a textured sphere with overlay rings, no lit hex mesh/grid lines; Java `java-05-planet.png`/`java-06-gz-panel.png` show the shaded hex mesh with sector grid lines and the red enemy-base tint. Code facts unchanged.

**Java.** `HexMesh(planet, 6)` = `MeshBuilder.buildHex(generator, 6, radius, 0.2)`
(`graphics/g3d/MeshBuilder.java:84-190`): `grid = PlanetGrid.create(divisions)`,
heights per corner `(1 + getHeight(corner.v) * intensity) * radius`; per tile
(skip if `skip(v)`): normal from corners 0,2,4; colour `getColor(tile.v)`;
emissive `getEmissiveColor(tile.v)` when `isEmissive()`; indexed when
`tiles*6 < 65535`; triangles (0,1,2),(0,2,3),(0,3,4)(,(0,4,5)).
`SerpuloPlanetGenerator.getColor` (133-138): `block = getBlock(p, true); salt -> sand; out = mapColor, a = 1 - albedo`.
Albedo (`Blocks.java`): deep-water, shallow-water, tainted-water,
deep-tainted-water, darksand-tainted-water, sand-water, darksand-water,
arkycite-floor 0.9; snow 0.7; ice 0.65; ice-snow, ice-wall 0.6; default 0.
Shader `core/assets/shaders/planet.vert`: `emissive = a_emissive.a * u_emissive * min(pow(max(0, (1 - norc.r) * 1.2), 3), 1.1)`;
`v_col = vec4(mix(a_color.rgb, a_emissive.rgb, emissive), 1) * vec4(mix(norc, vec3(1), emissive), 1)`.
Reference `runs/20261010-234250-ref-PAR-0070-combat-fx/java/step-140-planet.png`.
**Port.** `planet_view.gd:481 _build_body` (billboard), `client/shaders/planet.gdshader`,
`core/render/g3d/mesh_data.rs`, `HexMesher` trait, `serpulo.rs:512 get_color` (black),
`BlockDef.map_color`, no `albedo`.
**Changes.** `BlockDef.albedo`; Serpulo/Erekir `get_color`;
`mesh_data.rs build_hex(mesher, divisions, radius, intensity) -> MeshData`
(+normals, emissive); `#[func] MindCampaign.planet_mesh(planet) -> Dictionary`;
`planet_view.gd` `ArrayMesh` (VERTEX, NORMAL, COLOR, CUSTOM0 emissive, INDEX)
with the fixed shader; keep depth so back-side grid lines stay hidden.
**Tests.** `build_hex_counts`; `serpulo_color_alpha_from_albedo` (deep water alpha 0.1).
**Commit.** `PAR-0142: planet globe hex mesh with generator colours and planet shader (H05 T20a)`

### T20b. Serpulo emissive sector glow (PAR-0142 part 2)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED visually (no red/emissive enemy-base cells on the port globe; Java shows them, `java-05-planet.png`).

`SerpuloPlanetGenerator.java:141-183 getEmissiveColor` (enemy bases crux
colour; captured sectors sharded colour x `info.lightCoverage`) and
`getBlock(p, true)` visual branch (`:211-225`, derelict megabase near
`basePos`). Pass per-sector state (`has_enemy_base && !captured`,
`has_base`, `tile_v`, preset difficulty, `light_coverage`) into
`planet_mesh`; rebuild on capture/loss (Java `reloadMeshAsync`,
`SerpuloPlanetGenerator.java:72-85`).
**Commit.** `PAR-0142: Serpulo emissive sector glow on the globe (H05 T20b)`

### T20c. Planet bloom (PAR-0142 part 3)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED visually (Java has a bloom halo on the limb, `java-05-planet.png`; the port's purple rim is a flat atmosphere).

`PlanetRenderer.java:31` `Bloom(w/4, h/4, true, false)` around the planet
render. Port: `WorldEnvironment` glow on the `/root/Spine/Planet` scene;
record tuned values vs the Java frame and Arc `Bloom.java` defaults.
**Commit.** `Fixes PAR-0142: planet globe bloom (H05 T20c)`

---

### T21. Campaign UI cosmetic residuals

> **In-engine verification (2026-10-14, session 3, round 3):** PARTLY checked by code. FIXED in place: the layout-stub bullet (`MindRowTreeLayout`/`MindTreeLayout` are used by `sector_select_dialog.gd`; only radial/branch are unused). Limb culling/pulse/black8 not checked separately.

* Cull globe sector icons behind the limb (`tile_v · camera_dir <= 0`, `planet_view.gd _add_icon`).
* Unlocked-preset ring pulse `derelict -> white` with `Mathf.absin(Time.time, 10, 1)` (`PlanetDialog.java:461-466`; `absin = |sin(t/scl)| * mag`, ticks).
* Sector list background `Styles.black8`.
* `uiAlpha` fade of icons/labels when `zoom >= 1.9` (T14 state).
* Remove unused `ui/layout/*.gd` stubs if `grep -rn "MindTreeLayout\|MindRowTreeLayout\|MindRadialTreeLayout" client` finds no users.
  (Verified 2026-10-14 round 3: `MindRowTreeLayout` — and its base
  `MindTreeLayout` — IS used by `ui/dialogs/sector_select_dialog.gd:15,47`;
  only `radial_tree_layout.gd` and `branch_tree_layout.gd` (+ `.uid`) are
  unused.)
**Commit.** `Gameplay H05 T21: campaign UI cosmetic residuals`

### T22. Host dialog layout (PAR-0151, low priority)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED (`runs/20261014-s3-verify3/H05-T22-host.png` vs `step-375-host.png`): raw `mode.survival` ... `mode.editor` buttons, no colour swatch, no `?`, `Host` as a bottom-row button, empty name field (Java pre-fills the player name).

**Java** `ui/dialogs/HostDialog.java:16-86` (`w = 300`): row 1 `@name` +
field (max 40, writes `name`) + 54 px `squarei` colour swatch -> `PaletteDialog`
(`color-0`); row 2 `@server.port` + field (default `port` setting, 6567,
valid 1..65535); row 3 empty 65 px cell + `@host` button `w x 70` +
`?` 65x70 (`@host.info`); `shown`: `getBoolOnce("hostinfo")` -> `@host.info`.
No game-mode buttons. Reference `step-375-host.png`.
**Port.** `ui/dialogs/host_dialog.gd` (mode buttons `MODES`, l.14-48).
**Changes.** Remove the mode row (keep `host_requested(name, mode)`
emitting the current rules' mode so callers keep working), add the swatch
(open `palette` dialog), `?` button, `hostinfo` once-flag, layout widths per Java.
**Commit.** `Fixes PAR-0151: host dialog layout matches HostDialog (H05 T22)`

---

### T23. Block palette keeps the previous planet's blocks after a sector switch (added after the 2026-10-14 in-engine verification)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED (reported by session 2, reproduced and root-caused here). A fresh entry into Erekir Onset (`erekir` sector 10, first sector after a clean import) shows the Erekir palette (7 distribution blocks: ducts; `runs/20261014-s3-verify3/H05-T23-onset-palette.png`). Entering Ground Zero first and then Onset in the same session (`load_sector`/`start_sector` while the HUD stays visible) keeps the Serpulo palette (15 distribution blocks: conveyors, junction, sorters, ...; `H05-T23-onset-after-gz.png`), and `$I.select_block_by_name("conveyor")` succeeds on Onset. Both catalog endpoints are already correct at that moment (`MindCampaign.block_catalog_json()` and `MindUi.block_catalog_json()` -> 7 distribution blocks, `rules.planet` "erekir", env 17): only the fragment's cached copy is stale. Owner: H05 (campaign UI fragment + a load signal on `MindSimHost`); H08 T5 also edits `placement_fragment.gd` (hotkeys) — keep the diff to the functions named here.

**Symptom / acceptance.** After switching sectors without the HUD being hidden
(sector-to-sector launch, the planet page "Go"/"Launch" while in game, a
lost-sector replay, any `load_sector` from a running game), the block palette
still lists the previous planet's blocks (Serpulo conveyors on Erekir, or
Erekir ducts on Serpulo), the previously selected block stays selected, and
the category is not reset. Java rebuilds the palette on every world load and
on every block unlock. Acceptance: Ground Zero -> Onset in one session shows
the Erekir palette (distribution = 7 duct blocks), category `distribution`,
no block selected; Onset -> Ground Zero shows Serpulo conveyors; researching
a block while in game makes it appear in the palette without leaving the
sector.

**Java** `J/ui/fragments/PlacementFragment.java`:
```java
Events.on(WorldLoadEvent.class, event -> {            // l.78-84
    Core.app.post(() -> {
        currentCategory = Category.distribution;
        control.input.block = null;
        rebuild();
    });
});
Events.on(UnlockEvent.class, event -> {               // l.92-96
    if(event.content instanceof Block){
        rebuild();
    }
});
Events.on(ResetEvent.class, event -> {                // l.98-100
    selectedBlocks.clear();
});
```
Filters used by `rebuild` (l.767-781): `getByCategory` = `category == cat &&
isVisible() && environmentBuildable()`; `unlocked(block)` =
`unlockedNowHost() && placeablePlayer && environmentBuildable() &&
supportsEnv(state.rules.env)`; `environmentBuildable()` =
`isOnPlanet(state.getPlanet())` (`Block.java:1117-1119`,
`UnlockableContent.java:132-134`). The port's catalog builder already applies
these (`core/ui/campaign.rs block_catalog_with`, l.430-500, planet from
`session.rules.planet` in `gdext/campaign.rs:1855-1867`).

**Port today.** `ui/fragments/placement_fragment.gd`: `_ready` connects only
`visibility_changed` to `_ensure_loaded` (l.82); `_ensure_loaded` (l.598-608)
resets `_selected_category = -1` only when the sim tick went backwards, then
`_load_catalog()` (l.612-620) re-reads `MindUi.block_catalog_json()`. Nothing
reloads the catalog when a new world is loaded while the HUD stays visible,
or when research unlocks a block mid-game. `MindSimHost` counts world loads
(`world_loads += 1` at `gdext/sim_host.rs:1786` (`load_world`/map path) and
`:1933` (end of `load_sector_world`); `pub fn world_loads()` l.2452 is NOT a
`#[func]`) and has only the `state_changed` and `world_changed` signals
(l.544-548); `world_changed` also fires on every build, so it is not a
WorldLoadEvent. Research unlocks go through `MindCampaign.research`
(`gdext/campaign.rs:1139`), which calls `invalidate_ui_block_catalog`
(l.1871) for MindUi's cache only.

**Changes.**
1. `gdext/sim_host.rs`: add `#[signal] fn world_loaded();` next to
   `world_changed` and emit it right after each `self.world_loads += 1`
   (both sites; doc: "`EventType.WorldLoadEvent`"). Emit AFTER
   `emit_state()`/`emit_world_changed()` so listeners read the new state.
2. `gdext/campaign.rs`: add `#[signal] fn block_unlocked();` and emit it from
   `research` when the researched content is a block, and from the
   auto-unlock path if it unlocks blocks (grep `research auto-unlocked`).
   Never emit while `self` holds a bind of the host (env §3).
3. `placement_fragment.gd _ready`: connect `/root/Spine/SimHost`
   `world_loaded` to `_on_world_loaded` and `/root/Spine/MindCampaign`
   `block_unlocked` to `_on_block_unlocked` (guard `has_signal`, avoid double
   connects like l.86-87). Both handlers use `call_deferred` (Java
   `Core.app.post`).
   * `_on_world_loaded()`: `_selected_category = -1` (the existing
     `_build_categories` fallback picks `distribution`, l.648-652), clear the
     selected block through the input facade
     (`/root/Spine/Input.select_block_by_name("air")`, the deselect H08 T5
     uses), then `_load_catalog()` — even when the fragment is not visible
     (`_ensure_loaded` returns early when hidden; call `_load_catalog`
     directly or drop the visibility guard for this path).
   * `_on_block_unlocked()`: `_load_catalog()` without touching the category
     or the selection (Java `rebuild()` only).
   Keep the `visibility_changed` path (it covers menu -> game).
4. Java `ResetEvent` -> `selectedBlocks.clear()`: if the fragment keeps a
   per-category last-selected map, clear it in `_on_world_loaded` too
   (`grep -n "_last_block\|selected_blocks" placement_fragment.gd`).

**Tests.** GDScript parse gate. `cargo test -p mind-gdext --lib` (signal
declarations compile). No Rust logic test (glue).

**Verify in-engine** (env §4.1, clean `--import`, no H00 needed):
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'get_node("/root/Spine/Ui/UiRoot").dialog("planet")._sector_action_pressed("serpulo", 170, "go"); return true'; sleep 6
$G --eval 'return get_node("/root/MindUi").close_top_dialog()'        # Campaign Changes Notice, if present
$G --eval "$I.select_block_by_name(\"conveyor\"); return true"
$G --eval "$H.load_sector(\"erekir\", 10); return true" --eval "return $MC.start_sector(\"erekir\", 10)" --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'; sleep 3
$G --screenshot runs/<date>-s<N>-H05-T23/onset-after-gz.png            # ducts, not conveyors
$G --eval "return JSON.parse_string($I.get_input_state_json()).block"   # null
```
Then back to Ground Zero the same way: Serpulo conveyors. `grep -c panic` = 0.

**Pitfalls.** `select_plans` and other `get_input_state_json()` fields are
numbers, not arrays (an eval calling `.size()` on one wedged the game during
verification). Do not rebuild on `world_changed` (fires per placed block).
`Core.app.post` = one frame later: use `call_deferred`, not a timer.

**Commit.** `Gameplay H05 T23: placement palette rebuilds on world load and block unlock`

---

## 3. Order, hot files

### 3.1 Order
1. T1 -> T2 -> T3 -> T4 -> T5 -> T6 (generated sectors; each depends on the previous).
2. T7 (notifications), T8 (research), T9 -> T10a -> T10b..T10e (stats).
3. T11a -> T11b (rules), T12 (hints), T13 (schematics).
4. T14 -> T15 -> T16 (planet page), T17, T18, T19.
5. T20a -> T20b -> T20c, T21, T22.
6. T23 (palette rebuild; independent — can run any time, added 2026-10-14 round 3).

UI golden regeneration only in T1, T16, T17 (and any task that changes a
`core/ui/campaign.rs` view struct); re-run `ui_goldens_match` after every task.

### 3.2 Hot files shared with other plans (small diffs, merge often)
* `gdext/sim_host.rs` (`load_sector_world`, hints JSON): T2-T6, T12 — H01/H02/H04/H08 also edit it.
* `gdext/campaign.rs`: T2, T4, T7, T10a, T11b, T14-T16, T18, T20a — H04 owns the campaign loop there.
* `core/maps/planet/serpulo.rs`, `core/maps/generators/{base,planet}.rs`: T2-T6, T20a.
* `core/world/darkness.rs`, `core/world/build.rs`: T3 (H03 reads, H02 build gate).
* `core/sim/mod.rs` / `core/sim/runtime.rs`: T5 (`with_world_ctx`), T12.
* `core/content/registries/planets.rs`, `blocks/mod.rs`: T1, T20a (`albedo`).
* `ui/dialogs/planet_dialog.gd`: T2, T4, T14-T16.

---

## 4. Ledger changes to report (coordinator applies)

| Item | New status / note |
|---|---|
| PAR-0112 | `fixing` after T2-T5; `verified` after T6 + in-engine T4/T5 evidence (note: attackSectors + boot threat (T1), sector rect/size/darkness (T2-T3), loadout, BaseGenerator, ruins) |
| PAR-0145 | `verified` after T8 live spend replay (`runs/<date>-s<N>-H05-T8/researched.png`) |
| PAR-0047 | `verified` with T8 (click + hover halves) |
| PAR-0132 | `fixing` after T11a, `verified` after T11b |
| PAR-0143 | `verified` after T13 |
| PAR-0144 | `verified` after T17 |
| PAR-0164 | `verified` after T18 |
| PAR-0142 | `fixing` after T20a, `verified` after T20c |
| PAR-0151 | `verified` after T22 |
| new | "Serpulo attackSectors never marked; numbered-sector threat 0 until first capture" -> T1 |
| new | "Generated sectors are square: sector-polygon darkness missing (build gate + render)" -> T3 (render half H03) |
| new | "No toast for sector invasions (turn events dropped)" -> T7 |
| new | "Strings.autoFixed rounds instead of truncating" -> T9 |
| new | "Content info dialog shows no stats" -> T10a-e |
| new | "In-game rule edits are overwritten by the runtime mirror (set_rules_json)" -> T11b |
| new | "Hints: unitSelectControl fungal-pass predicate inverted; cores_destroyed 0; cannotupgrade/factorycontrol never fire" -> T12 |
| new | "Planet globe cannot be dragged/zoomed; last sector never stored; no preset reveal" -> T14 |
| new | "Planet page lacks rename/icon picker; stats dialog is a text dump" -> T15, T16 |
| new | "Dialogs hide instantly (Java fades 0.1 s)" -> T19 |
| new | "Block palette keeps the previous planet's blocks after an in-session sector switch (no WorldLoadEvent/UnlockEvent rebuild)" -> T23 (added 2026-10-14 round 3) |

---

## 5. Done criteria

* Each task committed (or reported blocked with the reason), one commit per
  task, subjects as given.
* `cargo test -p mind-core --lib --features msav-import`,
  `cargo test -p mind-headless --lib` (only H00-listed failures),
  `ui_goldens_match`, `parity goldens`, clippy `-D warnings`, the GDScript
  parse gate: all green.
* In-engine on a clean re-import: a numbered sector launched from the planet
  page lands with a core + player unit, survives 1200 ticks; an attack
  numbered sector shows enemy cores/turrets; research tree dimmed with items
  and a live spend; `?` on duo shows stats; pause-menu rule edit applies;
  0 `panic` in the log.
* Summary `REPO/docs/H05-campaign-ui_summary.md` per env §6.

---

## 6. Cross-area seams

**Provided by H05 (others consume):**
* T3: `mind_core::world::darkness::get_darkness(grid, content, x, y) -> f32` and
  `WorldGrid.sector_polygon`. **H03** switches the world renderer's darkness
  layer and the minimap (`gdext/render/minimap.rs:130,179,243,289,508` call
  `border_darkness` today) to `get_darkness`. **H02** placement already goes
  through `world/build.rs` (T3 edits it).
* T2: `mind_core::maps::planet::generate_sector(planet, sector, content, SectorGenExtras)`
  (new signature) — **H09** editor `SectorGenerateDialog` Apply (ledger call
  c873: returns false today) should call it.
* T2: `MindSimHost.set_sector_gen_extras(...)`, `MindCampaign.prime_sector_load(planet, id)`;
  T4: `MindSimHost.set_pending_launch(index, stacks_json)`,
  `generated_loadout_placed()`. **H04** (landing/launch pads, invasions) must
  call `prime_sector_load` + `set_pending_launch` before any `load_sector` it
  triggers for a numbered sector.
* T5: `Sim::with_world_ctx(|ctx| ...)` (generic live-world edit helper).
* T12: `CannotUpgradeFired` resource (drained into hint events).
* T18: `MindUi` signal `setting_changed(key)`, `MindCampaign` signal `settings_flushed()`.

**Needed by H05 (from others):**
* **H00**: `rules.env` 233 + player spawn (T4 acceptance `has_player_unit`).
* **H04**: off-play `CampaignEvent::SectorLose` / `SectorCapture` variants
  from `Campaign::run_turn` (T7 adds the toasts when they exist);
  `sector.info.attack = true` for generated attack sectors if the play flow
  does not set it from `rules.attack_mode` (T5 records); Load-dialog save previews remain H04's.
* **H02**: Reconstructor `unlockedNowHost` gate (`Reconstructor.java:252-257`)
  inserting `CannotUpgradeFired` (T12).
* **H03**: renderer darkness via `get_darkness` (above); toast/announce visuals.
* **H07**: `MindUi.play_ui_sound` playback (`uiUnlock` T8, `uiBack` T19).
* **H08**: `planetMap`/`research` key bindings and `isRebuildSelecting`
  (hints); `use_schematic` already present.
* **H06**: logic editor dialog (not here); `@rules.alloweditworldprocessors`
  row in T11a only edits the rule.
