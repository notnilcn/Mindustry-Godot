<!-- SPDX-License-Identifier: GPL-3.0-only -->
# I04 — Campaign UI and meta screens: what H05 did not reach

Round I, written 2026-10-15. Read `docs/I-environment.md` fully, then
`docs/I-INDEX.md` §3, then this plan.

H05 (round H) landed T1-T4, T6-T9, T10a and T10b. This plan carries everything else.
Every task below was re-verified against `main` @ `b560e9b1` on 2026-10-15 and is
self-contained. Do not open the H05 plan.

Moved elsewhere: H05 T11a/T11b (custom rules) -> I06 T3/T4; H05 T18 (settings fan-out) and
the settings store -> I05 T1; H05 T22 (host dialog) -> I08 T12.

Dropped (verified against Java 2026-10-15): H05 T7's "off-play `sector.capture` / `sector.lost`
toasts". In the current Java source `SectorLoseEvent` is never constructed (the only matches
are its declaration `J/game/EventType.java:143` and two listeners), and `SectorCaptureEvent`
is fired only by `J/core/Logic.java:409` for `state.rules.sector`, the played sector. Java
therefore never shows these off-play toasts. Nothing to do.

---

## 1. State on `main` (verified 2026-10-15)

| H05 task | Commit | Exists now |
|---|---|---|
| T1 attack sectors + boot threat | `ba24fe66` | |
| T2 sector rect/size/seed/threat | `ac60dadc` | `generate_sector(planet, sector, content, SectorGenExtras)` |
| T3 sector polygon darkness | `ab9f87e8` | `world::darkness::get_darkness`, `WorldGrid.sector_polygon` |
| T4 launch loadout at the spawn | `eb78d250` | |
| T6 derelict ruins | `a00a4cad` | `core/maps/generators/base.rs`: `get_frac` :19, `trait BaseRand` :30, `try_place` :80; `Sim::with_world_ctx` (`core/sim/mod.rs:1010`, `runtime.rs:464`) |
| T7 sector notifications (invasion) | `6781351f` | `sector.attacked` toast via `MindAssets.bundle_format` |
| T8 research dialog + live spend | `2f413cab` | PAR-0145 fixed |
| T9 Arc `autoFixed` | `df856e3f` | |
| T10a stats model + base stats | `5efcdb5a` | `core/ui/content_stats/{mod.rs (Stat, StatCat, StatUnit, Cell, Stats, StatCtx), values.rs (number, items, liquids, drillables, speed_boosters, ...)}`, `#[func] MindCampaign.content_info_json` |
| T10b production/crafting/power | `703b2b22` | `core/ui/content_stats/production.rs` `family_block_stats(ctx, id)` :46 -> `time_period_before_super` + `family_body` (dispatch on `def.kind`); behaviours reachable through the `BuildingBehavior: Any` downcast |

---

## 2. Tasks

Order: T1 (quick, user-visible) -> T2 -> T3 -> T4 -> T5 -> T6 -> T7 -> T8 -> T9 -> T10 -> T11
-> T12 -> T13 -> T14 -> T15 -> T16 -> T17 (last: headless only).

### T1 — Block palette rebuilds on world load and block unlock (was H05 T23)

**Evidence (2026-10-14):** entering Ground Zero, then Erekir Onset in the same session keeps the
Serpulo palette (15 distribution blocks; `runs/20261014-s3-verify3/H05-T23-onset-after-gz.png`)
and `$I.select_block_by_name("conveyor")` succeeds on Onset. Both catalog endpoints are already
correct (`MindCampaign.block_catalog_json()`, `MindUi.block_catalog_json()`); only the fragment's
cached copy is stale.

**Java** (`J/ui/fragments/PlacementFragment.java`): `WorldLoadEvent` (`:78-84`) ->
`Core.app.post(() -> { currentCategory = Category.distribution; control.input.block = null; rebuild(); })`;
`UnlockEvent` with a `Block` (`:92-96`) -> `rebuild()`; `ResetEvent` (`:98-100`) -> `selectedBlocks.clear()`.

**Port today.** `ui/fragments/placement_fragment.gd`: `_ready` connects only
`visibility_changed` -> `_ensure_loaded` (`_ensure_loaded` resets the category only when the
tick went backwards, then `_load_catalog()` re-reads `MindUi.block_catalog_json()`).
`MindSimHost` increments `world_loads` at the end of each world load (grep
`self.world_loads += 1` in `gdext/sim_host.rs`: the map/save path and the end of
`load_sector_world`) and has only the `state_changed`/`world_changed` signals
(`world_changed` fires on every build, so it is not a world-load event). Research unlocks go
through `MindCampaign.research` -> `invalidate_ui_block_catalog` (MindUi's cache only).

**Changes.**
1. `gdext/sim_host.rs`: `#[signal] fn world_loaded();` emitted right after each
   `self.world_loads += 1`, AFTER `emit_state()`/`emit_world_changed()` (doc: `EventType.WorldLoadEvent`).
   (If I06 T1 has factored a shared `finish_world_load`, emit it there once.)
2. `gdext/campaign.rs`: `#[signal] fn block_unlocked();` emitted from `research` when the
   researched content is a block, and from any auto-unlock path that unlocks blocks. Never emit
   while holding a bind of the host.
3. `placement_fragment.gd _ready`: connect `SimHost.world_loaded` -> `_on_world_loaded` and
   `MindCampaign.block_unlocked` -> `_on_block_unlocked` (guard `has_signal`; avoid double
   connects). Both deferred (`CONNECT_DEFERRED`; Java `Core.app.post`).
   `_on_world_loaded`: `_selected_category = -1` (the fallback picks distribution), deselect via
   `/root/Spine/Input.select_block_by_name("air")`, clear any per-category last-selected map,
   `_load_catalog()` even when hidden. `_on_block_unlocked`: `_load_catalog()` only.

**Verify in-engine** (clean `--import`): Go to Ground Zero, select `conveyor`, then
`load_sector("erekir", 10)` + `start_sector("erekir", 10)` + hide the menu; screenshot -> ducts;
`JSON.parse_string($I.get_input_state_json()).block` is null. Back to Ground Zero -> conveyors.
Do NOT call `.size()` on `get_input_state_json()` fields (they are numbers).

**Commit.** `Gameplay I04 T1: placement palette rebuilds on world load and block unlock`

---

### T2 — Unlock toast in the HUD (Java `Control` UnlockEvent -> `HudFragment.showUnlock`)

**Java.** `J/core/Control.java:160-165`: `Events.on(UnlockEvent.class, e -> { if(e.content.showUnlock()) ui.hudfrag.showUnlock(e.content); ... })`;
`UnlockableContent.showUnlock()` (`J/ctype/UnlockableContent.java:242`) and the
`StatusEffect` override (`J/type/StatusEffect.java:140`); `HudFragment.showUnlock(content)`
(`J/ui/fragments/HudFragment.java:786-850`: a top-centre toast with the content icon, name and
`@unlocked`, merging several unlocks that arrive together; read it all).
UnlockEvents fire from `UnlockableContent.unlock()` (research spends, campaign item pickups via
`checkAutoUnlocks`, etc.).

**Port today.** `#[func] MindHud.push_unlock(name)` (`gdext/ui/hud.rs:265`) emits `unlock`, and
the HUD fragment handles it, but nothing calls `push_unlock`. Research spends write the registry
directly; the sim-produced `UnlockRef`s go through `MindCampaign.take_unlocked_content()`.

**Changes.** One producer: every place that marks content unlocked during play (the research spend
in `MindCampaign.research`, the drain of `take_unlocked_content`, `checkAutoUnlocks` equivalents:
`grep -rn "fn unlock\b\|fn mark_unlocked\|unlocked_content" client/rust/mind-gdext/src/campaign.rs`)
calls `MindHud.push_unlock(name)` when `show_unlock(content)` is true (port Java's rule). Check
the HUD fragment's `unlock` handler against `HudFragment.showUnlock` (layout, merge window,
duration) and fix any differences.

**Verify.** Ground Zero with an unresearched block: research it from the research dialog -> the
unlock toast shows its icon and `Unlocked` (screenshot).

**Commit.** `Gameplay I04 T2: unlocking content shows the HUD unlock toast`

---

### T3 — Hints remainder (was H05 T12)

**Evidence (2026-10-14):** `ui/fragments/hints_fragment.gd:147` uses
`(preset != "fungalPass" or not fungal_pass_saved)` (Java: `&&` of both negations);
`gdext/sim_host.rs:7124 let cores_destroyed = 0i64;` (fed to the hints JSON at :7149);
no producer of `"cannotupgrade"` or `"factorycontrol"`; the `unitSelectControl` completion is
the eased `command_mode && selection > 0`.

**Java** (`J/ui/fragments/HintsFragment.java`):
* `unitSelectControl` (`:199-203`) visible: `isSerpulo() && units > 3 && !net.active() && !player.dead() && state.getSector() != SectorPresets.fungalPass.sector && SectorPresets.fungalPass.sector.save == null`;
  complete: `control.input.commandMode && selectedUnits.size > 0 && selectedUnits.first().controller() instanceof CommandAI ai && ai.targetPos != null`.
* `serpuloCoreZone` (`:285`): `!attackMode || stats.getDestroyed(coreShard) + getDestroyed(coreFoundation) + getDestroyed(coreNucleus) > 0`.
* `:84` `Events.run(Trigger.cannotUpgrade, () -> events.add("cannotupgrade"))`, fired by
  `J/world/blocks/units/Reconstructor.java:252-257` when `upgrade != null && !upgrade.unlockedNowHost() && !team.isAI()`.
* `:91-95` `BuildingCommandEvent` with a `PayloadBlockBuild` -> `"factorycontrol"` (fired by
  `InputHandler.java:491` for commanded buildings).

**Changes.**
1. `hints_fragment.gd:147`: `and preset != "fungalPass" and not bool(hints.get("fungal_pass_saved", false))`.
2. `sim_host.rs` hints JSON: `cores_destroyed` = the sum of the session's
   `destroyed_block_count["core-shard"|"core-foundation"|"core-nucleus"]` (name-keyed since
   H09 T10; read the live `CampaignRuntime` the way the surrounding code does).
3. `factorycontrol`: in the command-building branch of `Sim` command handling
   (`grep -n "apply_command_building" core/sim/mod.rs`), after the command, if the block is a
   `PayloadBlock` subclass in Java (UnitFactory, Reconstructor, UnitAssembler; `PayloadConveyor`
   / `PayloadRouter` are `PayloadBlock`s too: check `J/world/blocks/payloads/PayloadConveyor.java`
   `extends` and follow Java exactly), insert `"factorycontrol"` into `self.hint_events.events`.
4. `cannotupgrade`: a `CannotUpgradeFired` resource next to `FireCreated`
   (`grep -n "FireCreated" core/sim/mod.rs`), set by the Reconstructor when the Java condition
   holds (add the `unlockedNowHost` gate to the reconstructor upgrade path in
   `core/world/blocks/units/behavior.rs` if it is missing: the upgrade must NOT happen when the
   target is locked for a non-AI team), drained in `Sim::tick` into `hint_events`.
5. `unitSelectControl` completion: `$I.command_mode()` and the first selected id has a
   non-null target in `command_targets_json()`.

**Tests.** `factory_command_records_hint`, `locked_upgrade_blocks_and_fires_hint`.

**Verify.** Ground Zero: `place_block` a `ground-factory`, select it in command mode, right-click a
tile -> `JSON.parse_string($H.hint_state_json())` lists `factorycontrol`.

**Commit.** `Gameplay I04 T3: remaining hint predicates and hint events`

---

### T4 — Content info stats: distribution, storage, logic, defense (was H05 T10c)

**Template.** `core/ui/content_stats/production.rs` (T10b): `family_body` dispatches on
`def.kind` and each family has `fn <family>_stats(ctx, stats, def, ...)`. Create
`core/ui/content_stats/logistics.rs` and call it from `family_body` (or a sibling dispatcher)
AFTER the base block stats, exactly like the Java class chain (`super.setStats()` first, then the
family's adds/removes).

**Java `setStats` sites** (read each; transcribe every `stats.add/remove/replace`):
`distribution/Conveyor.java:56` (itemsMoved = `displayedSpeed`, `StatUnit.itemsSecond`),
`StackConveyor.java:64`, `Duct.java:59`, `DuctRouter.java:48`, `DuctBridge.java:21`,
`DuctJunction.java:41` (removes itemCapacity), `OverflowDuct.java:41`, `Junction.java:30`,
`ItemBridge.java:73`, `BufferedItemBridge.java:24`, `MassDriver.java:69`,
`DirectionalUnloader.java:59`, `storage/Unloader.java:61`, `storage/CoreBlock.java:125`,
`campaign/LaunchPad.java:59`, `campaign/LandingPad.java:119`, `liquid/LiquidJunction.java:21`
(removes liquidCapacity), `logic/LogicBlock.java:175`, `logic/MemoryBlock.java:30`,
`logic/LogicDisplay.java:72`, `defense/Wall.java:53`, `defense/ShieldWall.java:31`,
`defense/MendProjector.java:58`, `defense/OverdriveProjector.java:63`,
`defense/ForceProjector.java:108`, `defense/RegenProjector.java:91`, `defense/ShockMine.java:41`,
`defense/ShockwaveTower.java:51`, `defense/BuildTurret.java:81` (all paths under
`J/world/blocks/`). `StatValues` helpers: `J/world/meta/StatValues.java` (`number` :56,
`items` :145-169, `liquids` :119-137, `blocks(...)` :498-502, `boosters` :536,
`speedBoosters` :603, `itemBoosters` :628). Port values live in the behaviour structs
(`displayedSpeed`, bridge `range`/`transportTime`, duct `speed`, mender `reload/range/healPercent/phaseBoost`, ...):
find each with `grep -n "<javaFieldName>\|<snake_name>" core/world/blocks/<family>/`.

**Tests.** `conveyor_stats` (Items Moved = `displayedSpeed`: `conveyor` 5, `titanium-conveyor` 10
(`J/content/Blocks.java:2074,2082`), formatted through `values::number`, which applies Arc
`autoFixed`, so the cell text is `"5"` / `"10"`); `mender_stats` (one concrete value from
`MendProjector.setStats`); `duct_junction_has_no_item_capacity`.

**Verify.** Core Database -> `titanium-conveyor` `?` -> the Items Moved row; screenshot next to a
Java capture if one exists (`ls REPO/runs/*ref*/java | grep -i info`).

**Commit.** `Gameplay I04 T4: logistics, storage, logic and defense content stats`

---

### T5 — Content stats: unit factories and payload blocks (was H05 T10d)

**Java.** `units/UnitFactory.java:146`, `units/Reconstructor.java:92` (output = upgrade pairs:
icon -> arrow -> icon for each `unlockedNow` pair, each pair in a `Styles.grayPanel` box with 40 px
icons), `units/UnitAssembler.java:206`, `units/UnitAssemblerModule.java:32`,
`units/RepairTurret.java:71`, `units/RepairTower.java:36`, `payloads/PayloadConveyor.java:62`,
`payloads/PayloadMassDriver.java:81`, `payloads/Constructor.java:42`.
Port data: `BlockDef.unit_plans`, `reconstructor_upgrades`, `assembler_plans` (grep the exact
names in `core/content/registries/blocks/`). Panels map to `Cell::Panel` (add the variant if
missing, with a renderer case in `ui/dialogs/content_info_dialog.gd`).

**Tests.** `additive_reconstructor_lists_unlocked_upgrades` (all unlocked: dagger -> mace first).

**Commit.** `Gameplay I04 T5: unit factory and payload content stats`

---

### T6 — Content stats: turrets + ammo; unit weapons and abilities (was H05 T10e)

**Java.** Turrets: `defense/turrets/BaseTurret.java:93` (shootRange in blocks, activation),
`Turret.java:185`, `ReloadTurret.java:17`, `ItemTurret.java:49` (removes itemCapacity;
`stats.add(Stat.ammo, StatValues.ammo(ammoTypes))`; ammo capacity `maxAmmo / max(ammoPerShot,1)` shots),
`LiquidTurret.java:37`, `PowerTurret.java:17`, `LaserTurret.java:26`, `ContinuousTurret.java:29`,
`ContinuousLiquidTurret.java:32`, `PayloadAmmoTurret.java:51`, `PointDefenseTurret.java:51`,
`TractorBeamTurret.java:57`. `StatValues.ammo(map, showUnit)` (`StatValues.java:701` to the end of
the file: damage, splash, lightning, fragments, status (`statusText` :229), knockback, pierce,
homing, reload multiplier, ammo multiplier, ...; nested fragment bullets in sub-panels).
Units: `type/UnitType.java:790` `setStats`, `StatValues.weapons(unit, weapons)` (:655),
`abilities` (:682). Reference: `runs/20261014-s3-verify3/java-26-duo-info.png` (copper `9 damage`,
`2 ammo/item`; graphite `18 damage`; ammo panels in a dark `grayPanel`-like box).

**Port.** `core/world/blocks/defense/turrets/` (configs incl. ammo maps), `core/weapons/`,
bullet defs (`BulletDef`), unit registry. Split into two commits: turrets+ammo, then
weapons+abilities.

**Tests.** `duo_ammo_lists_copper_graphite_silicon` (insertion order; copper damage `9`);
`dagger_weapon_stats`.

**Verify.** Database -> duo `?`: screenshot next to `java-26-duo-info.png`.

**Commits.** `Gameplay I04 T6: turret and ammo content stats`, `Gameplay I04 T6: unit weapon and ability content stats`

---

### T7 — Schematics dialog per Java: cards, tags row, info dialog (Fixes PAR-0143) (was H05 T13)

**Evidence (2026-10-14):** port cards (`runs/20261014-s3-verify3/H05-T13-cards-ingame.png`) have no
action row, a bottom name badge, no tiled background, the import button in the search row, and the
`basicNucleus` image overflows. Java (`java-02-schematics.png`, `java-03b-schematics-row3.png`):
`Tags:` row, a 50 px action row (info/upload/pencil/trash), a top badge, the tiled
`schematic-background`, Back + Import in the bottom row. The port also lists the 3-4 vanilla
loadouts (`basicBastion`, `basicFoundation`, `basicNucleus`), which Java never shows.

**Java** (`J/ui/dialogs/SchematicsDialog.java`, read it; card at `:150-205`):
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
Ctor: `buttons.button("@schematic.import", Icon.download, this::showImport)` after `addCloseButton()`;
the search strips `ignoreSymbols` on both sides; Enter uses the first schematic. `rebuildTags`
builds the `Tags:` row (tag toggles + a pencil to edit tags). `showInfo(s)` =
`SchematicInfoDialog` (name, image, requirements, power).

**Port today.** `ui/dialogs/schematics_dialog.gd` (cards, import in the search row, a detail
panel); `ui/widgets/schematic_image.gd` (`MindWidgets.icon_texture("schematic-background")`
misses: the sprite is not packed); `core/game/schematics.rs:556` `load_loadouts` pushes the
loadouts into `all`; `#[func] MindCampaign.list_schematics` (`gdext/campaign.rs:2896`).

**Changes.**
1. Atlas: add `sprites/schematic-background.png` to the packed set (find the include rule in
   `client/rust/mind-tools/src`, `grep -rn "sprites/" client/rust/mind-tools/src | head`). If the
   rule cannot be extended simply, load it as a plain texture from the assets dir in
   `schematic_image.gd`.
2. Cards exactly per Java (button with the `pane` background; 50 px action row with four
   `emptyi` buttons and tooltips; a 200x200 stack with the image and the TOP `black3` badge,
   ellipsis, max width 192). Action presses must not trigger the card press.
3. Import button in the bottom row after Back; remove the detail panel; a `schematic_info`
   dialog (all three registrations; pause false) showing name, image, requirements and power.
4. Search per Java (`ignoreSymbols`, Enter = first match).
5. Hide the vanilla loadouts: filter them out in `list_schematics` and keep each row's real `all`
   index in an `"index"` field; every action (`use_schematic`, export, delete) uses that field.
6. The `Tags:` row per `rebuildTags` (tag toggles filter the cards; the pencil edits tags).

**Verify.** Menu -> Schematics screenshot vs `java-02-schematics.png`; in Ground Zero, clicking a
card starts placement (preview follows the mouse).

**Commit.** `Fixes PAR-0143: schematics cards, tags row and info dialog per SchematicsDialog`

---

### T8 — Planet globe: drag, zoom, last sector, no snap on select, new-preset reveal (was H05 T14)

**Evidence (2026-10-14):** `ui/planet_view.gd` has no drag/scroll handlers, `focus_tile` (:322)
moves the camera, and no `<planet>-last-sector` key is written anywhere. Java drag-rotates the
globe (`java-09-rotated.png` after a 300 px drag).

**Java** (`J/ui/dialogs/PlanetDialog.java`):
* drag (`:114-138`): ignored while revealing (`showing() && newPresets.peek() != lastSector`), then
  `newPresets.clear()`; `pos = state.camPos; upV = pos.angle(Vec3.Y); speed = 1 - |upV - 90| / 90;`
  `pos.rotate(camUp, cx / 9 * speed);` `amount = cy / 10; amount = clamp(upV + amount, 1, 179) - upV;`
  `pos.rotate(camUp.cpy().rotate(camDir, 90), amount);` (Arc y-up drag: `cy = -relative.y` in Godot).
* wheel (`:140-147`): `zoom = clamp(zoom + amountY / 10, planet.minZoom, planet.maxZoom)` (`maxZoom` 2);
  `state.zoom = lerpDelta(state.zoom, zoom, 0.4)`.
* camera distance (`J/graphics/g3d/PlanetRenderer.java:65`):
  `camPos.setLength((radius + camRadius) * 4 + (zoom - 1) * (radius + camRadius) * 2)`.
* `show()` (`:238-291`): `lookAt(state.planet.getLastSector())`; selecting does NOT move the
  camera (only `launching` lerps `lookAt(selected, 0.1)`). `uiAlpha = lerpDelta(uiAlpha, zoom < 1.9 ? 1 : 0, 0.1)`.
* New presets (`:270-288`, `:986-1003`): on show, every preset with `unlocked() && sector.preset == preset && !alwaysUnlocked && !sector.info.shown && requireUnlock && !sector.hasBase() && preset.planet == state.planet`
  -> add, `info.shown = true`, `saveInfo()`; if any: append `getLastSector()`, reverse; per frame
  while non-empty: `to = peek()`, `presetShow += delta`, `lookAt(to, 0.11)`, `zoom = 0.75`; at
  `presetShow >= 20 && size > 1` (once): `ui.announce(Iconc.lockOpen + " [accent]" + to.name(), 2)`;
  at `presetShow > 144` pop and reset. Escape during the reveal truncates to the last element.
* `J/type/Planet.java:312-321`: `getLastSector()` = `sectors.get(min(settings.getInt(name + "-last-sector", startSector), size - 1))`;
  `setLastSector(s)` = `settings.put(name + "-last-sector", s.id)`, called by `Control.playSector`
  (`J/core/Control.java:448`).

**Port today.** `ui/planet_view.gd` (`camera`, `CAM_LENGTH`, `focus_tile` :322, `pick`),
`planet_dialog.gd _input` (picking), `MindCampaign.planet_view` (no `min_zoom`/`max_zoom`),
`PlanetDef.min_zoom`, `SectorInfo.shown`, `MindUi.announce(text, duration)`.

**Changes.**
1. `planet_view.gd`: `cam_pos: Vector3`, `zoom := 1.0`, `zoom_target := 1.0`; in `_process`
   `zoom = lerp(zoom, zoom_target, 1 - pow(1 - 0.4, delta * 60))`, camera position =
   `cam_pos.normalized() * ((r + cr) * 4 + (zoom - 1) * (r + cr) * 2)`, `look_at(Vector3.ZERO, Vector3.UP)`;
   `drag(relative)` per Java (`cam_up = basis.y`, `cam_dir = -basis.z`,
   `Vector3.rotated(axis.normalized(), deg_to_rad(a))`); `scroll(amount_y)`;
   `look_at_tile(tile, alpha := 1.0)` (slerp); remove the camera move from `focus_tile`.
2. `MindCampaign.planet_view`: add `min_zoom` and `max_zoom` (2.0).
3. `planet_dialog.gd _input`: left-drag motion -> `view.drag(event.relative)` with `_dragged = true`
   (a release after a drag must not select); wheel -> `view.scroll(±1)`; only when the dialog
   is on top (`is_top()`).
4. Last sector: write `<planet>-last-sector` in the campaign start paths (`start_sector_with_loadout`
   and the resume path) through the settings store, and persist (cite `Control.java:448`); on page
   show call `view.look_at_tile(last)`.
5. New presets: `pub fn new_presets(planet: &mut Planet, registry) -> Vec<u16>` in
   `core/game/planet.rs` (Java filter; marks `info.shown`), `#[func] MindCampaign.take_new_presets(planet) -> PackedInt32Array`
   (saves infos, persists), and the reveal loop in `planet_dialog.gd _process` (ticks = `delta * 60`),
   called deferred from `shown()`.

**Tests.** `new_presets_marks_shown`.

**Verify.**
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'var v = get_node("/root/Spine/Planet/PlanetView"); v.drag(Vector2(300, 0)); return v.cam_pos'
$G --screenshot runs/<date>-s<N>-I04-T8/dragged.png
```
**Pitfall.** A camera exactly on the Y axis is degenerate; keep the 1..179° clamp.

**Commit.** `Gameplay I04 T8: planet globe drag/zoom, last sector, no snap on select, new preset reveal`

---

### T9 — Planet page: sector rename pencil and icon picker (was H05 T15)

**Evidence:** no pencil/icon button; the title row shows the preset art
(`runs/20261014-s3-verify3/H05-T15-gz-panel.png`) where Java shows the `Icon.noneSmall` button
for Ground Zero (`java-06-gz-panel.png`).

**Java** (`PlanetDialog.java:1199-1283`): title row `[accent]name` padLeft 3; pencil
(`Icon.pencilSmall`, `clearNonei`, 40x40, padLeft 4) only when `preset == null || !preset.requireUnlock`
-> `ui.showTextInput("@sectors.rename", "@name", 32, sector.name(), v -> { sector.setName(v); updateSelected(); rebuildList(); })`;
icon button 40x40 (`contentIcon.uiIcon`, else `Icon.icons[info.icon + "Small"]`, else `Icon.noneSmall`),
tooltip `@sector.changeicon`, opening a fill-parent `Dialog("")` with 48 px `squareTogglei` buttons:
`Icon.none` (both null), `accessibleIcons` (`J/Vars.java:164-169`), then per `defaultContentIcons`
type (item, liquid, block, unit, status) a 3 px accent rule + every `!isHidden() && unlocked()`
content (iconMed 32); `cols = min(20, screenW / 52)`; a choice -> `saveInfo(); hide(); updateSelected(); rebuildList()`; `@back` 210x64.

**Port today.** The title row in `planet_dialog.gd _build_sector_panel` adds a non-interactive
`MindWidgets.image("sector-<preset>")`; `SectorInfo.name/icon/content_icon`;
`MindUi.show_text_input(title, message, max, default, numeric, allow_empty)` + `text_input_result`;
`ui/dialogs/icon_select_dialog.gd` (menu mode only).

**Changes.** `#[func] set_sector_name(planet, id, name) -> bool` and
`set_sector_icon(planet, id, icon, content_icon) -> bool` in `gdext/campaign.rs` (empty = None;
`save_info` + persist; push to the live runtime copy when it is the played sector); the title-row
pencil/icon buttons; a `"sector"` mode in `icon_select_dialog.gd` (context
`{"mode":"sector","icon","content_icon"}`, the none button, accessible icons, five content sections
from `MindCampaign.database_json` rows filtered unlocked/non-hidden, emitting
`sector_icon_selected(icon, content_icon)`); remove its title (Java `IconSelectDialog` has none).

**Verify.** Rename a numbered sector you have a base on to "Base A", `--play`, reopen: the panel and
list show it; Ground Zero shows only the icon button. Restore the profile afterwards (env §4.3).

**Commit.** `Gameplay I04 T9: planet page sector rename and icon picker`

---

### T10 — Sector stats dialog with Abandon (was H05 T16)

**Evidence:** port Stats for Ground Zero is a `full_text` dump with only `Wave: 1` and OK
(`runs/20261014-s3-verify3/H05-T16-stats.png`); Java (`java-07-gz-stats.png`): description,
`Time: 0:02:30`, `Wave: 1`, `Threat: Low`, `Resources:` icons, the lockdown line, Back + Abandon.

**Java** (`PlanetDialog.java:1026-1147` showStats/displayItems, `:1149-1171` abandonSectorConfirm):
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
`Strings.formatMillis` (`ARC/arc-core/src/arc/util/Strings.java:813`): `h:mm:ss`, hours unpadded.
Abandon: confirm `@sector.abandon.confirm`; the played sector -> hide the page and kill the player
cores after 7 ticks; otherwise clear items, `hasCore = false`, production, `saveInfo()`.

**Port today.** `SectorStatsView` (`core/ui/campaign.rs`), `planet_dialog.gd _show_sector_stats`
(:1147, a `full_text` dump), `MindCampaign.abandon_sector()` (played sector only),
`clear_sector_record` (`gdext/campaign.rs`), save meta `SaveIo::get_meta` (`SaveMeta.time_played`).

**Changes.** Sort the rate lists and stored items by item id; add `playtime_ms`, `destination`,
`import_sources` (port `SectorInfo.eachImport`), `lockdown`, `preset_description_key` to the view
and regenerate the UI golden (I-environment §2); fill `playtime_ms` from the save meta. New dialog
`sector_stats` (manifest `{"name":"sector_stats","scene":"res://scenes/ui/dialogs/sector_stats_dialog.tscn","pause":false,"menu_openable":false,"args":["planet:Str","sector:Int"]}`,
static instance in `ui_root.tscn`, `EXPECTED_*_DIALOGS` and `expected_pause` false in
`core/ui/manifest.rs`). `#[func] abandon_sector_at(planet, id) -> bool`. The `@stats` button opens
it; delete `_show_sector_stats`.

**Tests.** `cargo test -p mind-core --lib ui::manifest`; `sector_stats_rows_in_item_order`.

**Verify.** Planet page -> Ground Zero -> Stats; screenshot vs `java-07-gz-stats.png`.

**Commit.** `Gameplay I04 T10: sector stats dialog with item rows, playtime and abandon`

---

### T11 — Core Database: icon planet tabs, current-planet default (Fixes PAR-0144) (was H05 T17)

**Evidence:** text tabs `All / Erekir / Serpulo` with `All` selected
(`runs/20261014-s3-verify3/H05-T17-database-onset.png`); Java (`java-04-database.png`): 32 px icon
tabs (eye, Erekir, Serpulo), fill-parent dialog.

**Java** (`J/ui/dialogs/DatabaseDialog.java:37-125`): `shown`: in a campaign, if the tab list
contains the current planet select it, else (in game) `rules.planet`; tab buttons
`clearNoneTogglei` 50x50, image 32: sun -> `Icon.eyeSmall`, planet ->
`Icon.icons.get(p.icon, Icon.commandRally)` tinted `p.iconColor`; tooltip `@all` / the localized
name; 10 per row; locked entries `Icon.lock` in `Pal.gray`; in game, banned content shows
`Icon.cancel` in scarlet at 60% and sorts last.

**Port today.** `database_dialog.gd:71 _build_tabs` (text buttons); `core/ui/database.rs`
`DatabaseTabView { name, localized, color, is_all }`; `PlanetDef.icon`.

**Changes.** `DatabaseTabView.icon` (`"eyeSmall"` for the sun, else `def.icon`); 50x50 icon toggle
tabs (`MindWidgets.glyph(icon, 32, color)`); default tab from `MindCampaign.get_sector_state()`
planet in a campaign; the banned cross when `rules.banned_blocks` is exposed; the fill-parent
layout. **Tests.** `database_tab_icons`.

**Commit.** `Fixes PAR-0144: database planet tabs render icons and default to the current planet`

---

### T12 — Dialogs fade out over 0.1 s (was H05 T19)

**Java.** `J/core/UI.java:132-133`: `Dialog.setHideAction(() -> sequence(fadeOut(0.1f)))`.
**Port today.** `ui/mind_dialog.gd:151` `hide_dialog()` sets `visible = false` immediately.
**Changes.** Tween `modulate.a` to 0 over 0.1 s, then `visible = false`, restore alpha; a re-show
during the fade cancels the tween; `MindUi`'s stack bookkeeping stays synchronous; keep the
`uiBack` emit where it is.
**Commit.** `Gameplay I04 T12: dialogs fade out over 0.1 s like Arc`

---

### T13 — Planet globe hex mesh with generator colours and the planet shader (PAR-0142 part 1) (was H05 T20a)

**Evidence:** the port globe is a textured sphere with overlay rings
(`runs/20261014-s3-verify3/H05-T20-planet.png`); Java shows a shaded hex mesh with sector grid
lines and the red enemy-base tint (`java-05-planet.png`).

**Java.** `HexMesh(planet, 6)` = `MeshBuilder.buildHex(generator, 6, radius, 0.2)`
(`J/graphics/g3d/MeshBuilder.java:84-190`): `grid = PlanetGrid.create(divisions)`, heights per
corner `(1 + getHeight(corner.v) * intensity) * radius`; per tile (skip if `skip(v)`): normal from
corners 0,2,4; colour `getColor(tile.v)`; emissive `getEmissiveColor(tile.v)` when `isEmissive()`;
indexed when `tiles*6 < 65535`; triangles (0,1,2),(0,2,3),(0,3,4)(,(0,4,5)).
`SerpuloPlanetGenerator.getColor` (`:133-138`): `block = getBlock(p, true)`; salt -> sand;
`out = mapColor`, `a = 1 - albedo`. Albedo (`Blocks.java`): deep-water, shallow-water,
tainted-water, deep-tainted-water, darksand-tainted-water, sand-water, darksand-water,
arkycite-floor 0.9; snow 0.7; ice 0.65; ice-snow, ice-wall 0.6; default 0. Shader
`core/assets/shaders/planet.vert`: `emissive = a_emissive.a * u_emissive * min(pow(max(0, (1 - norc.r) * 1.2), 3), 1.1)`;
`v_col = vec4(mix(a_color.rgb, a_emissive.rgb, emissive), 1) * vec4(mix(norc, vec3(1), emissive), 1)`.
Reference `runs/20261010-234250-ref-PAR-0070-combat-fx/java/step-140-planet.png`.

**Port.** `planet_view.gd _build_body` (billboard), `client/shaders/planet.gdshader`,
`core/render/g3d/mesh_data.rs`, the `HexMesher` trait, `serpulo.rs get_color` (returns black),
`BlockDef.map_color`, no `albedo` field.

**Changes.** `BlockDef.albedo` (content data); Serpulo/Erekir `get_color`;
`mesh_data.rs build_hex(mesher, divisions, radius, intensity) -> MeshData` (+ normals, emissive);
`#[func] MindCampaign.planet_mesh(planet) -> Dictionary`; `planet_view.gd` `ArrayMesh`
(VERTEX, NORMAL, COLOR, CUSTOM0 emissive, INDEX) with the fixed shader; keep depth so back-side grid
lines stay hidden. `parity/golden_content.json` changes (albedo): regenerate per env §2.
**Tests.** `build_hex_counts`; `serpulo_color_alpha_from_albedo` (deep water alpha 0.1).
**Commit.** `PAR-0142: planet globe hex mesh with generator colours and planet shader`

### T14 — Serpulo emissive sector glow (PAR-0142 part 2) (was H05 T20b)

`SerpuloPlanetGenerator.java:141-183 getEmissiveColor` (enemy bases: the crux colour; captured
sectors: the sharded colour x `info.lightCoverage`) and the `getBlock(p, true)` visual branch
(`:211-225`, derelict megabase near `basePos`). Pass per-sector state (`has_enemy_base && !captured`,
`has_base`, `tile_v`, preset difficulty, `light_coverage`) into `planet_mesh`; rebuild on capture/loss
(Java `reloadMeshAsync`, `SerpuloPlanetGenerator.java:72-85`).
**Commit.** `PAR-0142: Serpulo emissive sector glow on the globe`

### T15 — Planet bloom (Fixes PAR-0142) (was H05 T20c)

`PlanetRenderer.java:31` `Bloom(w/4, h/4, true, false)` around the planet render. Port: a
`WorldEnvironment` glow on the `/root/Spine/Planet` scene; record the tuned values next to the Java
frame and the Arc `Bloom` defaults (`ARC/arc-core/src/arc/graphics/g2d/Bloom.java`).
**Commit.** `Fixes PAR-0142: planet globe bloom`

### T16 — Campaign UI cosmetic residuals (was H05 T21)

* Cull globe sector icons behind the limb (`tile_v · camera_dir <= 0`, `planet_view.gd _add_icon`).
* Unlocked-preset ring pulse `derelict -> white` with `Mathf.absin(Time.time, 10, 1)`
  (`PlanetDialog.java:461-466`; `absin = |sin(t/scl)| * mag`, ticks).
* Sector list background `Styles.black8`.
* `uiAlpha` fade of icons/labels when `zoom >= 1.9` (T8 state).
* Remove `ui/layout/radial_tree_layout.gd` and `branch_tree_layout.gd` (+ `.uid`) if still unused
  (`MindRowTreeLayout`/`MindTreeLayout` ARE used by `sector_select_dialog.gd`).
**Commit.** `Gameplay I04 T16: campaign UI cosmetic residuals`

---

### T17 — Enemy bases in generated attack sectors (PAR-0112 step 3) (was H05 T5) — LAST, headless only

**Why headless only.** Vanilla Java never runs `BaseGenerator` on Serpulo: a preset-less cell gets
`hasEnemyBase()` only through `generateEnemyBase` (`Sector.java:145`), set by
`SectorSubmissions.registerSector` only on the 45 cells it turns into hidden-map presets (which load
their map, not the generator); `PlanetGenerator.generateSector` is a no-op (`PlanetGenerator.java:30`);
the other writer is the debug toggle (`PlanetDialog.java:655`). In the port, sector 0 already loads
its hidden map (`start_sector("serpulo",0)` -> attack, 5 enemy cores). So this task is for mod/debug
parity and is verified with a forced `has_enemy_base` test.

**Java.**
* `SerpuloPlanetGenerator.java:721-741`: spawn overlays for enemy rooms; if `sector.hasEnemyBase()`:
  `basegen.generate(tiles, enemies.map(r -> tiles.getn(r.x, r.y)), tiles.get(spawn.x, spawn.y), state.rules.waveTeam, sector, difficulty); state.rules.attackMode = sector.info.attack = true;`
  else `state.rules.winWave = sector.info.winWave = 10 + 5 * (int)Math.max(difficulty * 10, 1)`.
  `postGenerate` (`:745-752`): if `hasEnemyBase`: `basegen.postGenerate()`; and if
  `spawner.countGroundSpawns() == 0`: `state.rules.spawns = Waves.generate(sector.threat, new Rand(sector.id), state.rules.attackMode, true, false);`
* `J/maps/generators/BaseGenerator.java` (284 lines; transcribe `generate` :42-160, `postGenerate`
  :162-175, `pass` :177-180 (`Geometry.circle` order), `getDifficultyWall` :33-40):
  `Mathf.rand.setSeed(sector.id); Mathf.rand.nextDouble();` `bracketRange = 0.17`,
  `baseChance = lerp(0.7, 2.1, difficulty)`, `wallAngle = 70`, `resourceChance = 0.5 * baseChance`,
  `nonResourceChance = 0.002 * baseChance`, `passes = difficulty < 0.4 ? 1 : difficulty < 0.8 ? 3 : 5`;
  per core tile: `clearOverlay()`, `placeLoadout(bases.cores.getFrac((difficulty + Mathf.rand.range(0.4f)) / 1.4f).schematic, x, y, team, false)`,
  then every item filled to `entity.block.itemCapacity` in content order; `passes` x `pass(...)`; the
  disabled `if(false)` wall pass is skipped (keep a comment); small walls (`:98-127`) and large walls
  (`:130-146`); path clearing (`:151-168`): `Astar.pathfind(core, spawn, cost, passable)` with cost
  `t.team() == waveTeam && !t.within(core, 80) ? 100000 : t.floor().hasSurface() ? 1 : 10`, passable
  `!t.block().isStatic()`; each path tile beyond 80 world units of that core removes waveTeam blocks on
  it and its 8 neighbours. `postGenerate`: every waveTeam centre tile: PowerNode ->
  `configureAny(new Point2[0])` then `placed()`; Battery -> `power.status = 1`.

**Port today.** `try_place`, `get_frac`, `BaseRand`, `Sim::with_world_ctx` landed with H05 T6;
`core/ai/base_registry.rs` (`BaseRegistry { cores, parts, req_parts, ores, ore_floors }`); the
`BaseCatalog` resource (`gdext/sim_host.rs load_base_catalog`, inserted in `load_sector_world`);
`core/game/schematics.rs` `place`, `place_loadout`, `Schematics::rotate`;
`core/maps/generators/astar.rs pathfind`; `serpulo.rs generate_rules` sets `attack_mode` and air-only spawns.

**Changes.** `base::generate(ctx, cores, spawn, team, sector_id, difficulty) -> BaseGenState`,
`base::post_generate(ctx, &state)`, `get_difficulty_wall`; Serpulo `place_structures` calls them after
the loadout (spawn overlays first) when `self.sector.has_enemy_base()` and `ctx.bases` is `Some`, sets
`rules.attack_mode`, and regenerates ground-less spawns with `Waves::generate_with(threat, &mut ArcRand::new(id as u64), attack_mode, true, false)`.

**Tests** (real catalog from `REPO/assets/baseparts`; skip with a printed note if absent):
`get_frac_clamps`; `difficulty_wall_order`; `enemy_base_places_cores_and_parts` (forced
`has_enemy_base` extras: >= 1 team-2 core per enemy room, core items full, > 0 other team-2
buildings outside the player core radius); `post_generate_charges_batteries`.

**Pitfalls.** Java's `Mathf.rand` is global; a local `ArcRand::new(sector_id)` + one `next_double()`
reproduces it because nothing else draws during generation. `placeLoadout(..., check = false)` for
enemy cores. Item fill uses the core's `item_capacity`.

**Commit.** `PAR-0112: enemy bases (BaseGenerator) for generated attack sectors`
(PAR-0112 can then be closed by the coordinator: steps 1, 2, 4 landed in round H.)

---

## 3. Order and hot files

T1..T17 as listed. Hot files: `ui/fragments/placement_fragment.gd` (T1; I05 T6 hotkeys also edit it:
keep the diff to the named functions), `gdext/campaign.rs`, `gdext/sim_host.rs` (T1 signal, T3 hint
JSON), `ui/dialogs/planet_dialog.gd` (T8-T10), UI goldens (T10).

## 4. Ledger changes to report

| Item | After |
|---|---|
| PAR-0143 | verified (T7) |
| PAR-0144 | verified (T11) |
| PAR-0142 | verified (T13-T15) |
| PAR-0112 | verified (T17 headless + round-H steps) |
| new | "Palette keeps the previous planet's blocks" -> verified (T1) |
| new | "Unlock toast never shows" -> verified (T2) |

## 5. Done criteria

All tasks committed with their checks; suites green; UI goldens regenerated where views changed;
summary in `REPO/docs/I04-campaign-ui_summary.md`.
