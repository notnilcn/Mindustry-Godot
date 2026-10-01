# 03 — ASSETS (offline pipeline + runtime loading) implementation plan

> Ports `tools:pack` (ImagePacker/Generators/ImageTileGenerator/AssetsProcess) and the runtime asset layer (`FileTree`, atlas, bundles, icons, sounds/musics registration) from Mindustry (Java + Arc) to Rust (`mind-tools` CLI + `mind-atlas` packer + `mind-core::assets` + `mind-gdext::assets`).
> Region names, bundle keys and file paths are parity ABI and stay byte-identical. Pixel-identical generated sprites are **not** required (see §2.3).

---

## 1. Header block

| Field | Value |
|---|---|
| **Status** | 🔶 In progress on `lane/03-assets` — M0 complete 2026-10-02 (workspace, migration, provenance). Executable only after `00_FOUNDATION_IMPLEMENTATION_PLAN.md` and `02_CONTENT_IMPLEMENTATION_PLAN.md` milestones are green. |
| **Phase** | P1 (Platform & content) |
| **Depends on** | `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (workspace, spine, `mind-headless`, MCP bridge, CI), `02_CONTENT_IMPLEMENTATION_PLAN.md` (`ContentType`, content registry, `Block`/`UnitType`/`Item`/`Team`/`SectorPreset` metadata needed by generators and `@Load`). |
| **Blocks** | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md`, `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md`, `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md`, `14_UI_IMPLEMENTATION_PLAN.md`, `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md`, `17_FX_PARTS_IMPLEMENTATION_PLAN.md`, `18_AUDIO_IMPLEMENTATION_PLAN.md`, `20_MODS_IMPLEMENTATION_PLAN.md` — every consumer of sprites/regions/bundles/sounds/icons. |
| **Sources** | `Mindustry/core/assets/AGENTS.md`; `Mindustry/core/assets-raw/AGENTS.md`; `Mindustry/tools/AGENTS.md`; `Mindustry/annotations/AGENTS.md` (`AssetsProcess`, `LoadRegionProcessor`); `Mindustry/core/src/mindustry/graphics/AGENTS.md`; `Mindustry/core/src/mindustry/ui/AGENTS.md`; `Mindustry/core/src/mindustry/audio/AGENTS.md`; `tools/src/mindustry/tools/{ImagePacker,Generators,ImageTileGenerator}.java`; `tools/build.gradle`; `core/src/mindustry/core/FileTree.java`; `core/src/mindustry/Vars.java` (`loadSettings` bundle chain); `core/src/mindustry/ui/Fonts.java`; `core/src/mindustry/ctype/UnlockableContent.java`; `core/src/mindustry/graphics/MultiPacker.java`; `core/src/mindustry/mod/{Mods,DataBundleLoader}.java`; `core/src/mindustry/ClientLauncher.java` (boot stages); annotations `LoadRegionProcessor.java` / `AssetsProcess.java`; `core/src/mindustry/world/blocks/TileBitmask.java`; `tests/src/test/java/DataAssetTests.java`. Arc is **not** checked out beside `Mindustry/`; Arc classes named here (`Pixmaps`, `PixmapPacker`, `TexturePacker`, `TextureAtlas`, `I18NBundle`) are behavioral oracles to be fetched/reimplemented (§8 R3). |
| **Extends spine** | (a) asset boot stage machine hooked into the plan-00 fixed-step client boot and `mind-headless`; (b) `AssetMounted`/`AssetsReady` events on the plan-00 event bus; (c) `MindAssets` gdext singleton + `probe(name)` debug API for MCP; (d) `mind-headless assets *` scenarios; (e) a minimal on-screen asset inspector fixture (draws one `AtlasTexture` + icon glyph) used by the MCP scenario. |

---

## 2. Scope & parity definition

### 2.1 What this plan covers

1. **Offline pack** — a `mind-tools pack` replacement for `gradlew tools:pack`:
   - staging copy of sprite sources, generator passes (`Generators.run()`), AA pass, texture packing into pages, `icons.properties` rewrite, `Sounds`/`Musics`/`Tex`/`Icon`/`Iconc` id generation, fallback 2048 atlas, shader asset preparation, `locales` list, asset manifest.
   - generator parity: autotiles (47-slice), splashes, bubbles, fluid/gas animation frames, cliff masks (256), cracks, block icons + outlines + team recolor + `block-<name>-full` + `ui/block-<name>-ui` + `block_colors.png`, shallows, item/liquid/status icons, sector icons, team icons, unit icons + parts + treads + wrecks + `unit-<name>-full` + `ui/unit-<name>-ui`, ore variants, floor edges, scorches.
2. **Runtime loading layer**:
   - `FileTree` equivalent (mod files first, then internal), used for sprites, shaders, bundles, sounds, music, cursors and loose textures.
   - `Core.atlas` equivalent: region index built from the generated atlas manifest; `find(name[, fallback])` with `found()` semantics; loose textures (`error.png`, `noise*.png`, `fog.png`, …).
   - `@Load` equivalent: proc-macro-driven region population with exact templating (`@`, `@size`, `#`, `#1`, `#2`, `fallback`) per `LoadRegionProcessor`.
   - `UnlockableContent.loadIcon()` lookup chain and `uiIcon`.
   - bundles: `.properties` parsing, locale parent-chain fallback, `global.properties` merge, external bundle override, `locales` enumeration, `{0}` formatting, `IntFormat` caching.
   - `icons.properties` equivalent, `Iconc` code maps, `Icon`/`Tex` registries, content-icon glyph lookup (`Fonts.registerIcon`/`getUnicodeStr` equivalents).
   - generated `Sounds`/`Musics` registries from file names (IDs, duplicate detection, keyword mangling, `none`/`unset`).
   - UI ninepatch split metadata (`.9.png`), cursors, loose textures.
3. **Shader asset layer** — GLSL sources' file format/location/loading; plan `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` owns the frame pipeline, passes and uniform application. This plan produces/loads the shader resources and a manifest that plan 16 binds to.
4. **Mod asset hooks** — the interfaces plan `20_MODS_IMPLEMENTATION_PLAN.md` calls: `FileTree::add_file`, `AtlasOverlayBuilder` (runtime pages), `Bundle::merge_assets`, sound/music overlay, `sprites/` vs `sprites-override/` naming rules, `dp-` prefix forwarding, page-type routing.

### 2.2 Definition of done

- `mind-tools pack` runs on a clean checkout, produces the full vanilla runtime asset tree plus both atlases and all id files, and is **bit-reproducible for identical inputs** (two runs → identical sha256 for every output).
- Every region referenced by content via `@Load`, `loadIcon`, or the documented suffix rules resolves; content load fails loudly on `fallback=error` misses and the missing-region count is zero.
- `mind-headless assets boot` loads the index, atlases, bundles, icons and sound registry with no Godot and dumps a golden JSON.
- The MCP scenario in §7.2c passes (region lookup + icon rendering + screenshot evidence).
- Bundle key inventory check passes (every non-English locale ⊆ English; every referenced content key present).
- `Sounds`/`Musics`/`Tex`/`Icon`/`Iconc` registries are complete, deterministic and stable across repacks (append-only ids).
- The mod overlay API is implemented and exercised by a fixture mod test owned by plan 20.

### 2.3 Deliberate deviations (all documented in `THIRD_PARTY_NOTICES.md`/plan changelog)

| # | Mindustry behavior | Port behavior | Reason |
|---|---|---|---|
| A1 | Runtime atlas `.aatls` (Arc JSON) + `sprites.png`… pages | **Atlas PNG pages + `sprites.atlas.json` region map**, page types `main`/`environment`/`ui`/`rubble` | Godot-friendly, deterministic, inspectable; region names unchanged; page layout is not ABI. |
| A2 | `mergeFontAtlas` merges the UI page into the font atlas for one-batch drawing | **Not merged.** UI regions live on the UI page and are referenced as `AtlasTexture`; fonts stay separate `FontFile`s | Arc batcher optimization only; Godot batches independently. |
| A3 | Generated sprite **pixels** derive from Arc `Pixmaps`/`Simplex`/`Ridged`/`VoronoiNoise` | Algorithms reimplemented in `mind-atlas`; visual equivalence targeted. Pixel hashes are Rust↔Rust stable only | Arc is a separate artifact; byte-identical PNGs are not part of the parity ABI. |
| A4 | `logicids.dat` (Anuken-only) | **Skipped** (locked §9 HIGH_LEVEL) | Not reproducible, not needed; logic lookups resolve by name (`13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md`). |
| A5 | `ClassMap.java`/`global.js` generation (`ScriptMainGenerator`) | Data files migrated; no JS generation here. JSON `type` name resolution is plan 20's registry | Rhino/JS script mods are OD1; plan 20 owns the replacement. |
| A6 | `block_colors.png` alpha encodes square-sprite | Same encoding retained byte-for-byte | Plan 06 `ColorMapper` + minimap parity. |
| A7 | `font.woff`/`font_jp.woff`/`monospace.woff` loaded by Arc FreeType | Converted **WOFF→TTF at pack time**; Godot `FontFile` loads TTF | Removes runtime WOFF uncertainty (see R5), keeps upstream sources. |
| A8 | Runtime `PixmapPacker` repack for mods merged into the live atlas | Mod sprites go to **separate overlay pages** appended to the index (never repacking vanilla pages) | Deterministic vanilla pages, cheap enable/disable, no reload of page 0. |
| A9 | `icon.ttf` regenerated via Fontello + FontForge | Upstream generated `icon.ttf`/`logic.ttf`/`tech.ttf` migrated as assets; only the **code tables** (`Iconc`) are generated by `mind-tools` | Fontello is a network service and the FontForge merge step is marked broken upstream (R4 / ND3). |

Deferred: map previews/maps (plan 19), planet JSON/sector art wiring (plans 12/19), baseparts (plan 12), audio playback/priority (plan 18), UI text/`:name:` expansion (plan 14), mod parsing (plan 20).

---

## 3. Target design

### 3.1 Repository layout & crates (additions to HIGH_LEVEL §2.1)

```
mindustry-godot/
  assets/                                  # runtime assets (migrated from Mindustry core/assets)
    bundles/  icons/  fonts/  cursors/  sounds/  music/  shaders/  scripts/  contributors
    sprites/                               # generated: sprites*.png + sprites.atlas.json (+ fallback/)
    loose/                                 # noise/error/fog/... loose textures (paths kept as upstream names)
  assets-raw/                              # migrated editor sources (sprites/, fontgen/, icons/)
  client/rust/
    mind-atlas/                            # pure-Rust packing library (no content, no Godot)
      src/{lib,pack_json,page,pack,pixmaps,ninepatch,whitespace}.rs
    mind-tools/                            # CLI: pack/migrate/icons/bundles/sounds/shaders/manifest
      src/{main,staging,manifest,generated_assets,shaders}.rs
      src/generate/{mod.rs,autotile,blocks,units,environment,fx,icons,teams}.rs
    mind-core/src/assets/
      mod.rs                               # Assets facade, boot stages, events
      atlas.rs                             # Region, AtlasIndex, find(), load manifest
      file_tree.rs                         # mods-first virtual FS
      bundle.rs                            # properties + chain + IntFormat
      icons.rs                             # Iconc/Icon code tables, content-icon registry
      regions.rs                           # LoadCtx + registry trait
      sounds.rs                            # Sounds/Musics name+id registry
      generated.rs                         # include!(OUT_DIR/asset_ids.rs)  [build.rs output]
    mind-gdext/src/assets/
      mod.rs  loader.rs  atlas.rs  tex.rs  fonts.rs  cursors.rs  shaders.rs
  build/assets/                            # gitignored pack outputs & manifests
  tools/                                   # host scripts: pack.sh / pack.ps1 (bash primary)
```

New crates are registered in `client/rust/Cargo.toml`. `mind-atlas` and `mind-core` stay Godot-free; `mind-tools` depends on `mind-core` + `mind-atlas` (not on `mind-gdext`); `mind-gdext` depends on `mind-core` + `mind-atlas` (runtime mod overlay packing).

### 3.2 Boot / load order (port of `ClientLauncher.setup()` asset stages)

| # | Mindustry | Port (`mind_gdext::assets::loader`) | Notes |
|---|---|---|---|
| 1 | `assets.load("sprites/error.png")` | load loose `error.png` texture; register region `"error"` | must exist before any lookup |
| 2 | `Manifest`/bundle (`Vars.loadSettings`) | build `FileTree` (mods → internal); load `Bundle` + `global.properties` + `locales` | bundle must precede content constructors (plan 02 reads keys in `new`) |
| 3 | `sprites.aatls` or fallback | load `sprites.atlas.json` + pages; build `AtlasIndex` | fallback when `atlas.fallback` setting or page open failure |
| 4 | `Musics.load()` / `Sounds.load()` | build `Sounds`/`Musics` registry from `assets.index.json`; lazy-load `AudioStreamOggVorbis` on demand | playback/priority = plan 18 |
| 5 | `content.createBaseContent()` + `loadColors()` | plan 02 base content | |
| 6 | mods (scripts, JSON) | plan 20; overlay pages/bundles applied here | A8 |
| 7 | `Fonts.mergeFontAtlas` | no-op (A2); `Fonts` load `font.ttf`, `icon.ttf`, `logic.ttf`, `tech.ttf`, `monospace.ttf` | |
| 8 | `content.init()` / `content.load()` | plan 02 lifecycle; `content.load()` resolves `@Load` fields via `LoadRegions` derive + `loadIcon()` | stage fails on `fallback=error` miss |
| 9 | `mods.loadModPatches()` | plan 20; image/bundle overlays re-applied | |
| 10 | `baseparts` / maps | plans 12/19 | out of scope |
| 11 | loose textures, cursors | `Assets::texture(name)` cache, `Input.set_custom_mouse_cursor` | |

Boot is a re-entrant stage machine so the loading screen, headless boot and MCP can each advance one stage at a time. Stages emit `AssetsStageEvent { stage, ok, elapsed_ms }` and a final `AssetsReadyEvent` on the plan-00 bus.

### 3.3 `mind-core::assets` core types (Godot-free)

```rust
pub struct Region {                 // 1:1 with Arc AtlasRegion fields used by the game
    pub name: String,               // byte-identical, '/' never present (flattenPaths)
    pub page: u8, pub x: u32, pub y: u32, pub w: u32, pub h: u32,
    pub splits: [i32; 4], pub pads: [i32; 4],          // ninepatch / stripWhitespaceCenter
    pub offsets: [i32; 2],          // whitespace-center offsets (0,0 when none)
    pub page_type: PageType,        // main | environment | ui | rubble
}
pub struct AtlasIndex { /* name -> Region (HashMap), regions_sorted: Vec<Region>, pages: Vec<Page> */ }
impl AtlasIndex {
    pub fn find(&self, name: &str) -> Option<&Region>;              // None == "not found"
    pub fn find_or(&self, name: &str, fallback: &'static str) -> &Region;
    pub fn has(&self, name: &str) -> bool;
    pub fn len(&self) -> usize;
}
pub struct FileTree { /* ordered overlay roots + internal root; normalized keys */ }
impl FileTree {
    pub fn get(&self, path: &str) -> Option<AssetFile>;             // mods first, then internal
    pub fn add_file(&mut self, path: &str, file: AssetFile);        // plan 20 hook
    pub fn resolve_sound(&self, name: &str) -> Option<String>;      // tries .ogg then .mp3
}
pub struct Bundle { /* IndexMap chain: locale -> parent -> base; global overlay; external user bundle */ }
impl Bundle {
    pub fn get(&self, key: &str) -> &str;                            // raw name if missing
    pub fn get_or_null(&self, key: &str) -> Option<&str>;
    pub fn format(&self, key: &str, args: &[&str]) -> String;        // {0} placeholders
    pub fn keys(&self) -> impl Iterator<Item = &str>;
    pub fn merge_assets(&mut self, locale: &str, props: IndexMap<String, String>); // plan 20
}
pub struct Iconc { /* name -> char, char -> name, all: String */ }
pub struct Sounds { /* field_name -> (file, id), id -> field_name; none/unset dummies */ }
pub struct LoadCtx<'a> { pub atlas: &'a AtlasIndex, pub content_name: String, pub size: u32,
                         pub indices: [usize; 2] }
pub trait LoadRegions { fn load_regions(&mut self, ctx: &LoadCtx); }
```

`Region` deliberately carries `splits`/`pads`/`offsets` so plan 14 (`NinePatchRect`) and plan 16 (stretched drawers, turret regions) need no further metadata. `find()` on a missing name returns `None`; only `find_or(name, "error")` bridges to the always-present `error` region — this makes accidental missing regions a load error rather than a silent `error.png` (verified in §7).

### 3.4 Region name grammar (ABI — byte-identical, never normalized)

- Content sprite = `<content name>` (`copper-wall`, `battery-large`); variants `<name>1..N`; tiling `<name>-tile1..N`; edges `<name>-edge` (3×3 tiles); autotiles `<name>-0..46`, variant `<name>-<v>-0..46`.
- Blocks: `@` = content name; `@size` = block size (`1..16`); suffixes `-team`, `-shadow`, `-heat`, `-liquid`, `-top`, `-bottom`, `-glow`, `-power`, `-open`, `-item`, `-arrow`, `-laser`, `-piston0/1`, `-side1/2`.
- Turrets: body `<name>`, barrel `<name>-preview`, base `<name>-base` (else `turrets/bases/block-<size>`), `<name>-heat`, `<name>-liquid`, `<name>-top`; mirrored parts `-r`/`-l` (+ generated `-r-outline`/`-l-outline`).
- Units: body `<name>`, `-preview`, `-cell`, `-base`, `-leg`, `-leg-base`, `-foot`, `-joint`, `-joint-base`, `-treads` (+ generated `<name>-treads<r>-<i>`); segmented `-segment0..N`, `<name>-segment-outline<i>`; weapons `units/weapons/<weapon>` + `-heat`, `-cell`, `-preview`, `-outline`.
- Generated: `-outline`, `-team-<team>`, `block-<name>-full`, `unit-<name>-full`, `ui/block-<name>-ui`, `ui/unit-<name>-ui`, `ui/item-<name>-ui`, `ui/liquid-<name>-ui`, `ui/status-<name>-ui`, `ui/team-<name>`, `ui/sectors/sector-<name>`, `rubble/<unit>-wreck0..2`, `rubble/scorch-*`, `rubble/cracks-*`, `block_colors`, `splash-*`, `bubble-*`, `fluid-liquid-*`, `fluid-gas-*`, `cliffmask*`.
- Buckets for `<type>-<name>-full` / `<type>-<name>-ui` use `ContentType::name()` (`item`, `block`, `liquid`, `status`, `unit`, `sector`, `weather`).
- No path separators ever appear in region names (`flattenPaths: true`); duplicate base names across folders are a pack error.

Team recolor magic colors (exact): `0xffffffff` index 0, `0xdcc6c6ff` index 1, `0x9d7f7fff` index 2; every team with a palette emits `<name>-team-<team>`; block team lookups prefer `<name>-team-<team>` over `-team`. Unit icon cells use `0xffffffff→0xffa664ff`, `0xdcc6c6ff`/`0xdcc5c5ff→0xd06b53ff`.

### 3.5 Offline pipeline (`mind-tools pack`)

Stages mirror `tools/build.gradle` `pack` + `ImagePacker.main`:

1. **`staging`** — delete `build/assets/staging/`, copy `assets-raw/sprites/**` in; normalize `\`→`/`; write `build/assets/last_pack_version` (port version).
2. **`enumerate`** — build `PackAtlas` (fake in-memory atlas over staging, port of `ImagePacker`'s `GenRegion`/`PackIndex`), then run plan-02 content creation in headless mode (equivalent of `Vars.content.createBaseContent()/init()` under `Vars.headless = true` + no-op log).
3. **`generate`** — `Generators.run()` in the original order; each generator mutates staging exactly like upstream (`save`, `replace`, `delete`):
   1. `autotiles` (per floor/wall with `autotile`, `autotileVariants`; `ImageTileGenerator` on `-autotile[N].png`; delete source; preview `<name>.png` = bottom-right 32×32 crop if absent),
   2. `splashes` (12×32px), 3. `bubbles` (16×40px), 4. `gas-frames` (`fluid.png` stencil ×3 liquid / ×4 gas frames),
   5. `cliffs` (`cliff0..7` sources → `cliffmask0..255`, 64×64, white/`dark`/`mid` mask algorithm),
   6. `cracks` (`cracks-<size>-<i>` for `BlockRenderer.maxCrackSize` × `crackRegions`, Ridged noise + 3px median filter),
   7. `block-icons` (outlines via `makeIconRegions`/`getRegionsToOutline`, per-team recolor, `-outline` regions, `block-<name>-full`, `ui/block-<name>-ui` scaled to ≤128, `block_colors.png`),
   8. `shallows`, 9. `item-icons` (+6px container and gray outline for statuses), 10. `sector-icons` (10px container, `Pal.darkerGray` outline), 11. `team-icons` (`ui/team-<name>` tinted + gray outline; `derelict` = `b7b8c9`),
   12. (`all-icons` stays disabled, matching upstream `if(false)`), 13. `unit-icons` (outlines, weapon/part composition at `Draw.scl`, tread animation slices, segmented bodies, cell recolor, `unit-<name>-full`, deterministic wrecks via Voronoi/Ridged + split angle, `ui/unit-<name>-ui` fit-scaled),
   14. `ore-icons` (shadow offset `w/tilesize−1`, variants, `block-<name>-full`, `ui/...-ui`), 15. `edges` (`edge-stencil` × floor texture), 16. `scorches` (10 sizes ×3, `ScorchGenerator` + median filter).
   - `Draw.scl` is read from the staged `scale_marker` sprite (never hardcoded). Generator RNG seeds are stable hash functions pinned in `mind-atlas::hash` (FNV-1a of `name`), **not** Java `String.hashCode` (A3).
   - Iteration is over content in ID order and files in name order; parallel generation is allowed, writes are ordered through a single deterministic writer.
4. **`move-ui-icons`** — move staged `ui/icons/` into `ui/` (port of the Gradle `copy`/`delete` pair).
5. **`antialias`** — `mind_atlas::pixmaps::antialias` (port of Arc `Pixmaps.antialias`) over every staged PNG, skipping: `*.9.png`, names containing `aaaa`, and files under a `ui/` directory that are icon intermediates (`icon-*`) — exact predicate copied from `tools/build.gradle:85`.
6. **`pack`** — `TexturePacker.process` equivalent: resolve nested `pack.json` per source group (root/environment/rubble/ui), build page sets (`main` 4096, `environment` 2048, `ui` 2048, `rubble` 4096), compute ninepatch `splits`/`pads`, `stripWhitespaceCenter` offsets, `duplicatePadding`, `bleed` (padding 2), then encode `sprites*.png` + `sprites.atlas.json`.
7. **`pack-fallback`** — rewrite page caps 4096→2048 (equivalent of the in-place JSON edit) and repeat stage 6 into `assets/sprites/fallback/`. Skipped with `--no-fallback` (upstream `-Pargs`).
8. **`ids`** — write `assets/sounds.index.json`, `assets/musics` portion, `assets/icons/icons.properties` (append-only codes), `assets/icons/icon_codes.json`, `assets/locales`, `assets/shaders/shader.index.json`, `build/assets/asset_manifest.json` (hashes).
9. **`build.rs` codegen** (not a `mind-tools` step): `mind-core/build.rs` scans `assets/sounds/**`, `assets/music/**`, `assets-raw/sprites/ui/*.png`, `assets-raw/fontgen/config.json` and `assets/icons/icons.properties`, and emits `OUT_DIR/asset_ids.rs` with `pub mod sounds/musics/tex/icon/iconc` `&'static str`/`char` constants (Java keyword mangling: append `s`; Rust consts are UPPER_SNAKE). Deterministic sorted; no content needed, so `cargo build` alone works; `mind-tools pack` only updates the inputs.

`mind-tools` subcommands: `migrate` (one-time copy of upstream `core/assets*` with provenance manifest), `pack [--only <stage>] [--timings]`, `gen-sprites`, `icons sync`, `bundles sync|locales`, `sounds index`, `shaders build|check`, `manifest verify`, `import-aatls <file>` (debug oracle: read an upstream Arc atlas if present and dump a region-name list for inventory diffing).

### 3.6 Runtime atlas binding (`mind-gdext::assets`)

- `loader.rs` owns the stage machine (§3.2) and is called from the plan-00 client boot before content load.
- `atlas.rs` loads each page PNG with `Image::load_png_from_buffer` (runtime decode; uniform for vanilla and mod pages), uploads `ImageTexture`, applies filter from the `linear` setting (linear default, nearest otherwise) and `REPEAT_DISABLED`; builds an `AtlasTexture` per region lazily and caches it (`HashMap<String, Gd<AtlasTexture>>`). Loose textures are separate `ImageTexture`s.
- `tex.rs` exposes `Tex::get(name) -> Option<Gd<Drawable>>` backed by the atlas (`bar.9` → region `bar` with splits); `Tex` constants come from `asset_ids::tex`.
- `fonts.rs` loads `FontFile` resources (TTF) for `def`, `outline`, `icon`, `iconLarge`, `tech`, `logic`, `monospace`, applies the Scl scaling rule (18px base, `iconLarge` 48px + 5px dark-gray border, `logic` 16px unscaled) and exposes `icon_glyph(name) -> (char, FontKind)` and `Icon::all`.
- `cursors.rs` sets `Input.set_custom_mouse_cursor` for `cursor`, `hand`, `ibeam`, `drill`, `unload`, `target`, `repair` with upstream hot spots.
- `shaders.rs` loads `.gdshader` files through `FileTree` into `Shader`/`ShaderMaterial` resources (plan 16 consumes; §3.7).

### 3.7 Shader asset strategy (plan 16 owns the pipeline)

- Canonical GLSL from upstream is migrated to `assets-raw/shaders/*.frag|.vert` and copied to `assets/shaders/` by pack (data/reference; mods may override through `FileTree`).
- Godot shaders live in `client/shaders/*.gdshader` (tracked, hand-ported) plus optional `*.rdshader` (`RDShaderFile`) for g3d/compute cases. `mind-tools shaders build` copies them into `assets/shaders/godot/` and writes `shader.index.json` listing: source GLSL path, gdshader path, declared uniforms, textures, stage. `shaders check` diff-checks that every uniform/texture referenced by the upstream GLSL exists in the ported shader (drift guard).
- Naming parity: the logical shader name (`water`, `fog`, `screenspace`, `planet`, …) is the key used by plan 16's `Shaders` registry; mods override `<name>` via `sprites`-style lookup in `FileTree`. Fullscreen-shader entry points and uniform application are plan 16's.
- Default translation posture is hand-port + drift check (see ND2). Surface shaders reuse `sprites/<name>.png` noise textures through the `textureName()` convention; `caustics` maps to `caustics`.

### 3.8 Mod asset hooks (interface for plan 20)

```rust
pub trait AssetOverlayProvider {
    fn sprites(&self) -> &[OverlaySprite];       // sprites/ (prefix mod name) and sprites-override/ (replace)
    fn bundles(&self) -> &[(String, IndexMap<String,String>)]; // target bundle file -> properties
    fn sounds(&self) -> &[OverlaySound];         // prefix dp- for data assets, mod-prefixed otherwise
    fn images(&self) -> &[OverlayImage];         // DataImagePacker equivalent
    fn shaders(&self) -> &[OverlayShader];
}
pub struct OverlaySprite { pub path: String, pub name: String, pub png: Vec<u8>, pub replace: bool }
```

- Prefix rule (port of `Mods.packSprites`): `sprites/` names get `<mod>-` unless the name already looks category-prefixed and contains `<mod>-` (exact upstream rule at `Mods.java:412`); `sprites-override/` never prefixes and warns when the target region is absent.
- Page routing by path substring: `blocks/environment` → `environment`, `rubble` → `rubble`, `ui` → `ui`, else `main` (port of `Mods.getPage`).
- Plan 20 calls `AtlasOverlayBuilder::add(page_type, name, pixmap, splits, pads)` (uses `mind-atlas` packer); overlay pages are appended to `AtlasIndex` as extra pages and are not merged into vanilla pages (A8). `texturescale`/`bleed` semantics are supported by `mind-atlas`.
- Bundle merge (port of `DataBundleLoader`): load `bundles/bundle*.properties` per locale into the matching chain node, snapshot originals for `unload()`; `dp-` bundle assets merge into the live bundle.
- `MindAssets::probe(name)` returns `{found, page, x, y, w, h, splits, pads}` as a `Dictionary` for MCP assertions.

### 3.9 Invariants & determinism

- `mind-core` has **no** Godot types, no filesystem reads outside the injected `FileTree`; `cargo test -p mind-core` runs with a temp asset fixture.
- Region names, bundle keys, sound names, icon codes are ABI: never reordered, never auto-cased, never path-qualified.
- `icons.properties` allocation is append-only: existing codes are preserved byte-for-byte; new content takes `min(existing)−1`.
- No `HashMap` iteration in pack output ordering; all writers sort by name (or content ID where upstream does).
- Pack output for identical inputs is bit-identical: fixed PNG encoder settings (pinned `image`/`png` crate versions), fixed deflate level, fixed generator seeds, no timestamps in PNG chunks/manifest.
- Sim paths never touch this module except content metadata; asset loading is client/boot-only and guarded by `Vars.headless`-style flags in `mind-gdext`.

### 3.10 STDB & boundaries

- No STDB tables/reducers/views are touched. Asset content is shipped in the client build; server asset handling is plan 20/21 (server asset loaders, `dp-` assets over the relay).
- `mind-tools` never links `gdext`; `mind-gdext` never parses content JSON (plan 20).
- `mind-atlas` is dependency-light (`image`, `png`, `serde`, `indexmap`) and usable in CI without a GPU.

---

## 4. Port map

| Mindustry / Arc source | Target Rust module | Notes |
|---|---|---|
| `tools/src/mindustry/tools/ImagePacker.java` | `mind-tools/src/pack_atlas.rs` + `mind-tools/src/staging.rs` + `mind-tools/src/main.rs` (CLI) | Fake atlas over staging (`GenRegion`/`PackIndex`), drive order, `icons.properties` writer, `logicids.dat` **omitted** (`--logicids` accepted, warns and exits 0). |
| `tools/src/mindustry/tools/Generators.java` | `mind-tools/src/generate/{autotile,blocks,units,environment,fx,icons,teams}.rs` | 1:1 pass order; `ScorchGenerator` in `environment.rs`; `fluid`/`gasFrame`/`liquidFrame` simplex helpers in `fx.rs`; seeds via `mind_atlas::hash`. |
| `tools/src/mindustry/tools/ImageTileGenerator.java` | `mind-tools/src/generate/autotile.rs` + `mind-atlas/src/autotile.rs` | Embedded 12×4 base64 layout decoded at first use (sha256 pinned in a test); 4×4→47 slices. |
| `core/src/mindustry/world/blocks/TileBitmask.java` | `mind-core/src/world/blocks/tile_bitmask.rs` | `values: [u8; 256]` literal, `load(name)`/`load_variants(name, n)` resolving `-<i>` / `-<v+1>-<i>` (consumed by plans 08/16). |
| `tools/build.gradle` (`pack`, AA, fallback) | `mind-tools/src/main.rs` stages + `tools/pack.sh`/`pack.ps1` | AA skip predicate, in-place 4096→2048 rewrite, `--no-fallback`. |
| Arc `Pixmaps` (outline, blend, median, antialias, flipX, replace) | `mind-atlas/src/pixmaps.rs` | Behavioral reimplementation (A3); used by generators and runtime `dp-` images. |
| Arc `PixmapPacker`/`TexturePacker`/`TextureAtlas` (pack-time) | `mind-atlas/src/{pack,page,whitespace,ninepatch}.rs` | `pack.json` semantics, page caps, splits/pads/offsets; atlas manifest writer. |
| `annotations/.../impl/AssetsProcess.java` | `mind-tools/src/generated_assets.rs` + `mind-core/build.rs` → `mind-core/src/assets/generated.rs` | `Sounds`/`Musics` names+ids; `Tex` from `assets-raw/sprites/ui/*.png`; `Iconc` from `fontgen/config.json` + `icons.properties`; `Icon.all`. Keyword→append `s`. |
| `annotations/.../misc/LoadRegionProcessor.java` | `mind-macros/` `#[derive(LoadRegions)]` + `mind-core/src/assets/regions.rs` | `@`, `@size`, `#`/`#1`/`#2`, `length`/`lengths`, `fallback` (default `"error"`); parse replacement order identical (`@size` → `@` → `#1` → `#2` → `#`). |
| `core/src/mindustry/core/FileTree.java` | `mind-core/src/assets/file_tree.rs` | keys normalized `\`→`/`, leading `/` variant lookup, mods-then-internal, `resolve_sound` tries `.ogg`/`.mp3`; `loadSound`/`loadMusic` caches live in `mind-gdext`. |
| `core/src/mindustry/Vars.java` `loadSettings()` bundle block | `mind-core/src/assets/bundle.rs` | external user bundle, locale chain, `global.properties` merge, `locales` list, `router` locale parity (low priority). |
| Arc `I18NBundle` + `core/src/mindustry/ui/IntFormat.java` | `mind-core/src/assets/bundle.rs` (`Bundle`, `IntFormat`) | parent-chain lookup, `{0}` formatting, per-frame cached formats (plan 14 uses). |
| `core/src/mindustry/ctype/UnlockableContent.java` `loadIcon()` | `mind-core/src/content/unlockable.rs` (method) + `mind-core/src/assets/atlas.rs` | exact chain: `fullOverride` → `<type>-<name>-full` → `<name>-full` → `<name>` → `<type>-<name>` → `<name>1`; `uiIcon` = `<type>-<name>-ui` else `fullIcon`. |
| `core/src/mindustry/ui/Fonts.java` (asset parts) | `mind-gdext/src/assets/fonts.rs` + `mind-core/src/assets/icons.rs` | TTF loading/scaling/outline, glyph lookup, `registerIcon`/`getUnicode`/`getUnicodeStr` equivalents, `Icon.all`; text rendering stays plan 14. |
| `core/src/mindustry/graphics/MultiPacker.java` (`PageType`) | `mind-atlas/src/page.rs` (`PageType` main/environment/ui/rubble) | page caps + runtime overlay routing. |
| `core/src/mindustry/graphics/Shaders.java` (loader/factory parts) | `mind-gdext/src/assets/shaders.rs` | `FileTree` lookup `shaders/<name>`, registry keyed by logical name; passes/uniforms = plan 16. |
| `core/assets-raw/fontgen/config.json`, `assets/icons/icons.properties` | `mind-tools/src/generated_assets.rs`, `mind-core/src/assets/icons.rs` | glyph code tables + content-icon registry; append-only. |
| `core/src/mindustry/mod/Mods.java` `packSprites`/`getPage` | `mind-gdext/src/assets/overlay.rs` (interface in §3.8) | plan 20 drives it; prefix/override/page rules ported; `dp-` handled in plan 20. |
| `core/src/mindustry/mod/DataBundleLoader.java`, `DataImagePacker.java`, `DataAudioLoader.java` | `mind-core/src/assets/bundle.rs` merge API + `mind-atlas` + `mind-gdext` audio overlay | plan 20 implements the asset types; primitives live here. |
| `tools/src/mindustry/tools/ScriptMainGenerator.java`, `core/src/mindustry/mod/Scripts.java` (asset-facing pieces: `global.js`, `ClassMap.java`) | **plan 20** (`20_MODS_IMPLEMENTATION_PLAN.md`); this plan migrates the files and reserves `mind-tools scripts` | No JS/classmap generation here (A5, OD1). |
| `tests/src/test/java/DataAssetTests.java`, `PatcherTests.java` (asset aspects), `GenericModTest.java` (asset aspects) | plan 20 tests, using this plan's `FileTree`/`Bundle`/overlay APIs | The asset-index parts of these tests are mirrored as plan-03 unit tests (§7.1a) plus plan-20 integration. |

---

## 5. Milestones & task breakdown

Smallest vertical slice first: one authored sprite (`copper-wall`) flowing source → staging → atlas page → JSON manifest → headless lookup, before any generator exists.

### M0 — Workspace, migration, provenance
- [x] Add `mind-atlas`, `mind-tools` crates; wire into `client/rust/Cargo.toml`; `mind-core/src/assets/` skeleton.
- [x] `mind-tools migrate --from <Mindustry> --to .`: copy `core/assets` → `assets/`, `core/assets-raw` → `assets-raw/`, keep every path/name; write `assets/ASSET_PROVENANCE.md` (upstream commit hash, input-tree sha256, GPL-3.0 notice) and `build/assets/migration_manifest.json`.
- [x] Trim: no `logicids.dat`, no `sprites_out/`, no `build/`, no `sprites.aatls`, no `version.properties`/`locales`/`basepartnames` from upstream (regenerated).
- [x] `tools/pack.sh` / `tools/pack.ps1` wrappers; CI job `assets-pack` (skipped when cache hit). — **scripts done; CI job deferred to orchestrator (HLP §5.2-1: orchestrator owns CI edits), see Changelog.**
- **Verify:** cleaned tree contains only hand-authored/runtime inputs; migration manifest stable across two runs. ✅ (`f3a333652b…` identical both runs; `mind-headless assets migrate-check` OK)

### M1 — Packer core + vertical slice
- [ ] `mind-atlas`: `pack.json` loading/inheritance, page allocator, `duplicatePadding`, `flattenPaths`, `stripWhitespaceCenter` (splits/offsets), ninepatch split detection, whitespace/bleed, deterministic PNG writer, `PageType` caps.
- [ ] Manifest writer (`sprites.atlas.json`) + sha256 `asset_manifest.json`.
- [ ] `mind-tools pack --only pack` over a fixture tree containing one PNG; output identical across runs.
- [ ] `mind-core::assets::atlas` loads the manifest; `mind-headless assets index --atlas <dir>` dumps JSON.
- **Verify:** determinism test + headless region lookup for `copper-wall` and `error`.

### M2 — Generators that need only filenames
- [ ] `autotiles` (with the embedded layout), `splashes`, `bubbles`, `gas-frames`, `cliffs`, `cracks`, `scorches` (`ScorchGenerator`), `edges` skeleton.
- [ ] AA pass + skip predicate; `move-ui-icons`; staging mutation semantics (`save`/`replace`/`delete`).
- **Verify:** golden image hashes for a fixed fixture subset; `TileBitmask` table test.

### M3 — Content-driven generators (needs plan 02 metadata)
- [ ] Content metadata contract from plan 02: `get_generated_icons`, `make_icon_regions`, `get_regions_to_outline`, `outline_color/radius/icon`, `team_region(s)`, autotile flags/variants, unit regions/parts/weapons/treads/segments, team palettes, item/liquid/status colors, sector list, `BlockRenderer.maxCrackSize`/`crackRegions`, `tilesize`.
- [ ] `block-icons` (outlines, team recolor, `block-<name>-full`, UI icons, `block_colors.png`), `shallows`, `item-icons`, `sector-icons`, `team-icons`, `unit-icons`, `ore-icons`.
- [ ] `icons.properties` writer (append-only, exact format) + `icon_codes.json`.
- **Verify:** content-driven region inventory check (`mind-headless assets regions --assert-complete`) on the full vanilla content set.

### M4 — Full pack, fallback, ids
- [ ] `pack-fallback` (4096→2048 rewrite), `--no-fallback`.
- [ ] `sounds index`, `musics`, `build.rs` id codegen, `locales`, `last_pack_version`.
- [ ] Shader copy + `shader.index.json` + drift check skeleton.
- **Verify:** full `mind-tools pack` in budget; determinism of every output; region-name golden committed (`assets/parity/region_names.txt`).

### M5 — Runtime `FileTree` + atlas binding + MCP probe
- [ ] `FileTree` (mods-first, normalization), `mind-gdext::assets::atlas` runtime decode + `AtlasTexture` cache, loose textures, `MindAssets.probe`.
- [ ] Plan-00 spine inspector fixture shows a region and an icon glyph.
- **Verify:** MCP scenario §7.2c green with screenshot.

### M6 — `@Load` + `loadIcon` integration
- [ ] `mind-macros` `#[derive(LoadRegions)]`; plan 02 content structs annotated; `ContentLoader.load()` calls `load_regions`.
- [ ] `loadIcon` chain + `uiIcon`; error-fallback auditing (`fallback` explicit vs `error`).
- **Verify:** headless boot of full content with zero error-fallbacks; unit tests §7.1a.

### M7 — Bundles + icons/fonts
- [ ] Properties parser (UTF-8, escapes, continuations, ordered), chain fallback, `global.properties`, external bundle, `locales`, `IntFormat`, `format`.
- [ ] `mind-tools bundles sync` (updateBundles port, PUA `\uXXXX` escaping), bundle key inventory check.
- [ ] `Iconc`/`Icon` code tables + `Fonts` TTF loading; `unicode_str` for emojis; `RichTextLabel` handshake documented for plan 14.
- [ ] WOFF→TTF conversion at pack time.
- **Verify:** bundle tests + `bundles diff` report in CI.

### M8 — Sounds/Musics + UI `Tex` + cursors
- [ ] Registry from `sounds.index.json`; duplicate detection; keyword mangling; `none`/`unset`; lazy `AudioStreamOggVorbis` cache (name-keyed).
- [ ] `Tex::get` + generated `tex` constants; ninepatch splits exposed; cursor assets.
- **Verify:** inventory equals file tree; plan-18 handshake test (registry only).

### M9 — Mod overlay API + plan-20 handshake
- [ ] `AssetOverlayProvider` implementation, runtime overlay pages, bundle merge, `dp-` hooks documented.
- [ ] Fixture mod (folder) with `sprites/`, `sprites-override/`, `bundles/`, one `dp-` image; plan 20 loads it in `mind-headless`.
- **Verify:** fixture test green; override warnings match upstream semantics.

### M10 — Budgets, CI, exit
- [ ] Benchmarks (`criterion`) and pack timing report; CI cache; `assets-pack` budget gate.
- [ ] Exit checklist §7.3 signed off with artifacts; changelog entry.

---

## 6. Data & formats

### 6.1 Asset tree (runtime)

```
assets/
  bundles/bundle.properties, bundle_<locale>.properties, global.properties, bundle_pt_BR.properties, ...
  locales                                # generated list of selectable locales
  icons/icons.properties                 # generated, append-only; "code=contentName|textureName"
  icons/icon_codes.json                  # generated from assets-raw/fontgen/config.json
  fonts/{font.ttf,font_jp.ttf,monospace.ttf,icon.ttf,logic.ttf,tech.ttf}
  cursors/{cursor,hand,ibeam,drill,unload,target,repair}.png
  sounds/{beams,block,charge,environment,explosions,loops,movement,shoot,ui}/*.ogg
  music/*.ogg
  sounds.index.json                      # generated registry (sounds + musics + ids)
  shaders/*.frag|*.vert (copied GLSL)  + shaders/godot/*.gdshader + shader.index.json
  sprites/sprites.png, sprites2.png, ... , sprites.atlas.json
  sprites/fallback/sprites*.png, sprites/fallback/sprites.atlas.json
  sprites/block_colors.png
  sprites/planets/*.png
  sprites/loose/*.png                    # noise, error, fog, ... (logical names preserved)
  scripts/base.js, scripts/global.js     # migrated; generation owned by plan 20
  contributors
assets-raw/
  sprites/**/*.png, **/pack.json
  fontgen/config.json, fontgen/extra/**, fontgen/merge.pe
  icons/*.png
  shaders/*.frag|*.vert
```

### 6.2 `pack.json` (supported keys, defaults preserved)

| Key | Default | Notes |
|---|---|---|
| `duplicatePadding` | true | pad each region on all sides |
| `combineSubdirectories` | true | subfolders without their own `pack.json` merge into the parent pass |
| `flattenPaths` | true | region names have no directories; duplicates are an error |
| `maxWidth`/`maxHeight` | 4096 (root/rubble), 2048 (`blocks/environment`, `ui`) | fallback pass rewrites 4096→2048 |
| `fast` | true | packer heuristic; ported deterministically |
| `stripWhitespaceCenter` | true | records `splits`/`pads`/`offsets` |
| `ignoredWhitespaceStrings` | `["effects/"]` | paths excluded from center stripping |
| `bleed` (port-only) | matches upstream runtime bleed (2px when linear) | used for overlay/mod pages |

Nested configs: `assets-raw/sprites/blocks/environment/pack.json`, `rubble/pack.json`, `ui/pack.json`; a folder with its own config is excluded from the parent's `combineSubdirectories` pass.

### 6.3 Atlas manifest `sprites.atlas.json`

```json
{
  "format": 1,
  "generator": "mind-tools 0.1.0",
  "pageCap": 4096,
  "fallback": false,
  "inputsHash": "sha256:…",
  "pages": [
    {"index": 0, "type": "main", "file": "sprites.png", "width": 4096, "height": 4096, "sha256": "…"},
    {"index": 1, "type": "environment", "file": "sprites2.png", "width": 2048, "height": 2048, "sha256": "…"},
    {"index": 2, "type": "ui", "file": "sprites3.png", "width": 2048, "height": 2048, "sha256": "…"},
    {"index": 3, "type": "rubble", "file": "sprites4.png", "width": 4096, "height": 4096, "sha256": "…"}
  ],
  "regions": [
    {"name": "copper-wall", "page": 0, "x": 0, "y": 0, "w": 32, "h": 32,
     "splits": [0,0,0,0], "pads": [0,0,0,0], "offsets": [0,0], "pageType": "main"}
  ]
}
```

`regions` is a flat array sorted byte-wise by `name` (deterministic); loaders build a `HashMap<name, idx>`. `splits`/`pads` order matches Arc (`left,right,top,bottom`). `inputsHash` = sha256 over sorted `(relative path, sha256(bytes))` of every staged source after generation but before packing.

### 6.4 `icons.properties` (byte-format parity)

```
63743=spawn|block-spawn-ui
…
31625=copper-wall|block-copper-wall-ui
```

- Lines `code=contentName|textureName`, `\n` terminated, no header; existing lines preserved in original order; new codes allocated descending from `0xF8FF` (min existing code − 1).
- Content iteration/order: blocks, items, liquids, units, status effects; excludes `ConstructBlock`, `air` and `UnitType.internal` without `internalGenerateSprites`.
- `textureName = <ctype>-<name>-ui`.
- Stable-id test: pack twice with a synthetic new content → existing codes unchanged, one new code added at the tail end of allocation space.

### 6.5 `sounds.index.json`

```json
{
  "format": 1,
  "sounds": [{"name": "none", "file": null, "id": -1, "category": null},
             {"name": "uiBack", "file": "sounds/ui/uiBack.ogg", "id": 0, "category": "ui"}],
  "musics": [{"name": "game1", "file": "music/game1.ogg"}]
}
```

- Walk `assets/sounds/**` recursively, sort by file name; duplicate base names across folders = hard error (mirrors `AssetsProcess`).
- `name` is the generated Java-style field name (keyword → append `s`); `file` is the real path; `.mp3` accepted by `FileTree::resolve_sound` at load time.
- IDs are append-only, dense from 0 in sorted order; `none`/`unset` are virtual dummies.
- Musics: same walk over `assets/music/**`, no ids.

### 6.6 Bundles

- Source of truth: `bundle.properties` (UTF-8, ordered, comments `#`, blank lines preserved for sync diffing).
- Parser: `key = value`, `key: value`, `key value`; continuation lines ending `\`; escapes `\t \n \r \\ \: \= \ ` and `\uXXXX`; values keep Mindustry color tags (`[accent]`, `[]`) and `\n` verbatim.
- Chain: `bundle_<lang>_<REGION>` → `bundle_<lang>` → `bundle`; missing key walks parents; empty value is treated as missing for fallback (Arc `getOrNull` behavior).
- `global.properties` is `putAll`-merged into the live bundle after locale load (overrides locale values).
- External `user://bundle.properties` (or a `bundle` file in the data dir) replaces the internal load and logs "external translation bundle has been loaded".
- `locales` file: one locale code per line derived from `bundle_*.properties` names, sorted; read by plan-14 settings.
- `format`: `{0}…{n}` replacement; `IntFormat` caches `key + rendered` results for per-frame labels (bounded LRU, 512 entries).
- `bundles sync` equivalent: rewrites locale files to match English key order, adds missing keys from English, removes stale keys, preserves comments/blank lines, escapes PUA `0xE000..0xF8FF` as `\uXXXX`, `\` as `\\`, newlines as `\n`; dry-run by default, `--apply` writes.

### 6.7 Shader manifest `shader.index.json`

```json
{"format": 1, "shaders": [
  {"name": "water", "glslFrag": "shaders/water.frag", "gdshader": "shaders/godot/water.gdshader",
   "uniforms": ["u_time", "u_useParticles"], "textures": ["noise"], "stage": "fragment"}]}
```

### 6.8 Generated Rust ids (`mind-core/build.rs`)

`OUT_DIR/asset_ids.rs`, included by `mind-core/src/assets/generated.rs`; `// @generated` banner, sorted, deterministic:

```rust
pub mod sounds { pub const NONE: &str = "none"; pub const UI_BACK: &str = "uiBack"; }
pub mod musics { pub const GAME1: &str = "game1"; }
pub mod tex { pub const BAR: &str = "bar"; }
pub mod icon { /* name -> char from fontgen/config.json */ }
pub mod iconc { /* name -> char from icons.properties + config.json */ }
```

### 6.9 Generated/managed files

| File | Producer | Committed? |
|---|---|---|
| `assets/sprites/**` (pages + `atlas.json`, `fallback/**`, `block_colors.png`) | `mind-tools pack` | no (gitignored) |
| `build/assets/**` (staging, manifests, timings) | `mind-tools` | no |
| `assets/icons/icons.properties` | `mind-tools icons` | **yes** (tracked; append-only, like upstream) |
| `assets/icons/icon_codes.json`, `assets/sounds.index.json`, `assets/locales` | `mind-tools` | no |
| `assets/shaders/godot/**`, `shader.index.json` | `mind-tools shaders` | `.gdshader` sources tracked in `client/shaders/`; copies not |
| `assets/ASSET_PROVENANCE.md`, `assets/parity/region_names.txt` | `mind-tools migrate` / first pack | yes |
| `OUT_DIR/asset_ids.rs` | `build.rs` | no |

---

## 7. Oracle & verification (REQUIRED)

### 7.1a Ported tests (Mindustry → Rust)

| Mindustry test / behavior | Rust test |
|---|---|
| `ApplicationTests.launchApplication` asset bootstrap | `mind-headless assets boot` + `cargo test -p mind-headless assets_boot` |
| `AssetsProcess` sound name mangling/duplicates | `mind-core::assets::sounds::tests::names_ids_and_duplicates` |
| `LoadRegionProcessor` templating | `mind-core::assets::regions::tests::templating_matches_processor` (`@`, `@size`, `#`, `#1`, `#2`, fallback) |
| `UnlockableContent.loadIcon` contract | `mind-core::content::unlockable::tests::load_icon_chain` |
| `ImagePacker` icons allocation | `mind-tools::generated_assets::tests::icons_properties_append_only` |
| `ImageTileGenerator` (47 slices) | `mind-tools::generate::autotile::tests::slice_layout_and_hash` |
| `TileBitmask.values` | `mind-core::world::blocks::tile_bitmask::tests::table_shape_and_ids` |
| `FileTree.get` | `mind-core::assets::file_tree::tests::mods_first_backslash_normalization` |
| `Vars.loadSettings` bundle chain | `mind-core::assets::bundle::tests::locale_chain_and_global_merge` |
| Arc `I18NBundle.format` | `mind-core::assets::bundle::tests::format_placeholders` |
| Arc `Pixmaps.outline`/`blend`/`median`/`antialias` | `mind-atlas::pixmaps::tests::golden_ops` (fixture PNGs) |
| `PixmapPacker` strip/splits | `mind-atlas::pack::tests::strip_whitespace_splits_and_determinism` |
| `.9.png` splitting | `mind-atlas::ninepatch::tests::split_detection` |
| `DataAssetTests` asset aspects (`dp-` bundle/image) | plan 20 tests using `Bundle::merge_assets` / overlay builder (cross-referenced from `20_MODS_IMPLEMENTATION_PLAN.md`) |
| `PatcherTests` asset aspects | plan 20 `patch_image_and_bundle` |
| `GenericModTest` asset aspects | offline fixture mod in plan 20 (no HTTP) |

### 7.1b Headless harness (`mind-headless`) scenarios

| Command | Asserts |
|---|---|
| `mind-headless assets migrate-check` | migrated `assets/` matches `migration_manifest.json`; forbidden generated files absent (`logicids.dat`, `sprites.aatls`, `version.properties`). |
| `mind-headless assets pack-check --manifest build/assets/asset_manifest.json` | every output hash present and matching; `inputsHash` matches a fresh staging pass. |
| `mind-headless assets boot --atlas assets/sprites --dump build/assets/boot.json` | stage timings; page count; `AtlasIndex::len ≥ 18000`; `error` region present; bundle `en` loaded; sound index loaded; dump golden JSON (regions count, icon code count, locales). |
| `mind-headless assets regions --assert-complete` | every `@Load`/`loadIcon`/suffix-derived name for every content item resolves; exits non-zero listing misses (a miss with explicit `fallback` is reported, a `fallback=error` miss fails). |
| `mind-headless assets bundle-diff` | `bundle.properties` keys present in every shipped locale? report only (locales may be partial); no locale may contain keys absent from English; `global.properties` merged. |
| `mind-headless assets sounds-check` | registry == recursive file listing, ids dense/append-only, duplicates rejected. |
| `mind-headless assets fallback-boot --atlas assets/sprites/fallback` | fallback atlas boots and every region ≤ 2048 fits. |
| `mind-headless assets determinism --runs 2` | two full packs into temp dirs produce identical sha256 for every output (CI gate). |
| `mind-headless dump assets` | golden checksum of region names + icon codes + sound ids; diffed in CI. |

Scenarios live in `mind-headless/src/scenarios/assets_*.rs` and are registered under the names `assets_migrate`, `assets_pack`, `assets_boot`, `assets_regions`, `assets_bundles`, `assets_sounds`, `assets_determinism`.

### 7.1c MCP playtest scenario (open-godot-mcp)

Preconditions: `mind-tools pack` outputs present; plan-00 spine scene exists (`res://scenes/spine.tscn`) and boots `MindAssets`; `godot_health` ok.

1. `godot_health check` → `ok: true`.
2. `godot_game play(scene="res://scenes/spine.tscn", frozen=false)`; wait for `AssetsReadyEvent` (poll `godot_log get source=game count=50` for `[assets] ready pages=… regions=…`).
3. Region lookup: `godot_exec eval`:
   ```gdscript
   var p = MindAssets.probe("copper-wall")
   return [p.found, p.pageType, p.w, p.h, p.splits]
   ```
   expect `found == true`, `w == 32`, `h == 32`.
4. Negative lookup: `MindAssets.probe("this-region-does-not-exist").found` must be `false` (no silent `error` fallback).
5. Transparency check (proves the PNG actually decoded): `MindAssets.find_region("blank")` → `texture.get_image().get_pixel(x + w/2, y + h/2).a == 0`; `copper-wall` center or border sample has `a > 0`.
6. Icon check: `godot_exec eval` on the spine inspector node `/root/Spine/StateInspector` calling `show_region("copper-wall")` (fixture method added in M5) then `show_icon("copperWall")` (renders the `Iconc` glyph with the icon font). `godot_screenshot game` saves evidence; verify the captured image differs from the pre-call screenshot (non-blank pixels in the inspector rect).
7. Bundle check: `godot_exec eval` returns `MindAssets.bundle_get("block.copper-wall.name")` == `"Copper Wall"` and `MindAssets.bundle_get("definitely.missing.key") == "definitely.missing.key"`.
8. Duplicate-file check: `godot_exec eval` returns `MindAssets.atlas_duplicates().is_empty()`.
9. `godot_log errors` → empty; `godot_game stop`.

The exact eval strings are recorded in `assets/parity/mcp_assets_scenario.md` when M5 lands, so the scenario is copy-pasteable in CI.

### 7.1d Performance budget & measurement

| Budget | Target | Measurement |
|---|---|---|
| Full cold `mind-tools pack` (full vanilla, 8-core WSL2 Ubuntu dev box) | ≤ 90 s; regression gate +25% | `mind-tools pack --timings` → `build/assets/pack_timings.json`; CI `assets-pack` job |
| Incremental atlas-only repack (`--only pack`) | ≤ 20 s | same |
| Runtime assets-ready (index + 4 pages decode + bundles + fonts + sounds index) | ≤ 1500 ms | `mind-headless assets boot --time`; in-engine `AssetsReadyEvent.elapsed_ms` |
| `AtlasIndex::find` | ≤ 200 ns/lookup (1M mixed-name bench) | `criterion` bench `atlas_lookup` |
| Bundle `get`/`format` | ≤ 500 ns / ≤ 5 µs | `criterion` benches `bundle_get`, `bundle_format` |
| Atlas metadata resident | ≤ 12 MB for vanilla | `assets boot --dump` sizes + `dhat` run |
| GPU page memory | ≤ 350 MB worst case (4 pages max cap), expected ≤ 140 MB | page dimensions in manifest |
| `icons.properties` goldens | stable across repacks | determinism scenario |

### 7.1e Exit criteria checklist

- [ ] `mind-tools pack` outputs bit-reproducible (`assets determinism --runs 2`).
- [ ] Zero unresolved `fallback=error` region lookups across all vanilla content.
- [ ] Every Mindustry region name in `assets/parity/region_names.txt` resolves; inventory diff vs the documented suffix rules is empty.
- [ ] Bundle key inventory: no locale key missing from English; English keys all reachable; `global.properties` merged; locale-chain fallback tests green.
- [ ] `icons.properties` append-only behavior proven; every content icon code unique; `Iconc.codes`/`codeToName` complete.
- [ ] `Sounds`/`Musics` registry matches the file tree; duplicates rejected; `none`/`unset` present; ids stable across repacks.
- [ ] Fallback 2048 atlas generated and bootable.
- [ ] `.9.png` splits present for every ninepatch; plan 14 renders one correctly (handshake test).
- [ ] Shader manifest validated; no uniform drift (`shaders check`).
- [ ] Mod overlay fixture (sprites/, sprites-override/, bundle, `dp-` image) loads headlessly; plan-20 interface stable.
- [ ] MCP scenario §7.1c passes with screenshot + eval evidence.
- [ ] Budgets §7.1d met; CI `assets-pack` gated.
- [ ] `logicids.dat` absent; no pack output committed; GPL-3.0 headers/attribution in place; `ASSET_PROVENANCE.md` written.
- [ ] `cargo test -p mind-core` passes without Godot or network.

---

## 8. Risks & open decisions

| # | Risk / decision | Default being planned against | Status |
|---|---|---|---|
| R1 | Arc is not checked out locally; `Pixmaps`/`PixmapPacker`/AA semantics must be exact enough for visual parity | Fetch the Arc revision pinned in `Mindustry/gradle.properties` at execution and port semantics into `mind-atlas` (small math/pixmap utilities only, attribution added); generator outputs are not byte-ABI, so small deviations are acceptable | Tracked |
| R2 | PNG encoder determinism across crate versions | Pin `png`/`image` versions in `Cargo.lock`; determinism CI catches drift | Tracked |
| R3 | Generated sprite visual drift vs upstream (noise/median/outline algorithms) | Golden-image review per generator milestone; deviations noted in the plan changelog | Tracked |
| R4 | `PixmapPacker.stripWhitespaceCenter` + ninepatch split behavior is subtle | Port from Arc source; fixture tests for a conveyor arrow/turret barrel; plan 14/16 consume `splits`/`pads` only | Tracked |
| R5 | WOFF→TTF conversion fidelity | `mind-tools fonts` decompresses WOFF (zlib tables) to TTF; validated by glyph-count/table checks; fallback is shipping woff if Godot loads it | Tracked |
| R6 | Runtime PNG decode cost / export pipeline (Godot import vs runtime) | Runtime `Image.load_png_from_buffer` for all pages (mod-friendly, uniform); revisit in `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` if decode exceeds budget on mobile | Tracked |
| R7 | Atlas JSON size vs memory for ~20k regions | Flat sorted array + `HashMap` at load; budget measured; gzip optional during export (read transparently) | Tracked |
| R8 | Content-icon rendering in Godot text (Arc injects atlas regions as font glyphs) | `Iconc` char + `AtlasTexture`-per-icon API for plan 14's `formatIcons`; `[img]`/custom BBCode effect implementation owned by plan 14 | Handshake needed with `14_UI_IMPLEMENTATION_PLAN.md` |
| R9 | Mod overlay page layout differs from vanilla (A8) | Overlay pages appended; plan 20 never mutates vanilla pages; `AtlasIndex` supports duplicate detection with `sprites-override` semantics | Handshake needed with `20_MODS_IMPLEMENTATION_PLAN.md` |
| R10 | Vendored asset tree vs referencing upstream checkout | **NEEDS USER DECISION** — default: vendor `assets/` + `assets-raw/` into `mindustry-godot/` via `mind-tools migrate`, with `MIND_UPSTREAM` env override to re-migrate/diff against a local Mindustry checkout. Vendoring makes CI/determinism tests self-contained | **NEEDS USER DECISION** |
| R11 | Shader GLSL→Godot translation method | **Locked 2026-10-01 (NUD-12=C, hybrid): hand-port the complex/critical shaders to `.gdshader`; an automated transpile pass is allowed for simple shaders, but its output is committed as reviewed code (never generated at build time); `mind-tools shaders check` gates all drift.** Plan `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` consumes the manifest | locked |
| R12 | Icon-font regeneration (Fontello/FontForge replacement) | **NEEDS USER DECISION** — default: reuse upstream tracked `icon.ttf`/`logic.ttf`/`tech.ttf`; `mind-tools` only generates code tables; a Rust font builder is out of scope until a new icon is needed | **NEEDS USER DECISION** |
| R13 | `assets/` vendoring size (~hundreds of MB, LFS or not) | Default: plain git (assets ~tens of MB after excluding generated atlases); Git LFS only if the repo grows past practical limits | Tracked (ties to R10) |
| R14 | Cross-platform path/case handling (Windows + WSL/Linux) | `FileTree` keys normalized `\`→`/`; case-sensitive lookups (Godot import is case-sensitive on some targets); CI runs a case-collision check over `assets/` | Tracked |

---

## 9. References

Read in full for this plan (paths relative to `C:\Users\Clinton\g\code_examples\`):

- `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 locked decisions, §2 architecture, §4 template, §6–§9 conventions)
- `mindustry-godot/PRELIMINARY_PLAN.md`
- `Mindustry/core/assets/AGENTS.md`
- `Mindustry/core/assets-raw/AGENTS.md`
- `Mindustry/tools/AGENTS.md`
- `Mindustry/annotations/AGENTS.md`
- `Mindustry/core/src/mindustry/graphics/AGENTS.md`
- `Mindustry/core/src/mindustry/ui/AGENTS.md`
- `Mindustry/core/src/mindustry/audio/AGENTS.md`
- `Mindustry/core/src/mindustry/ctype/AGENTS.md`
- `Mindustry/core/src/mindustry/core/AGENTS.md`
- `Mindustry/core/src/mindustry/world/AGENTS.md`, `Mindustry/core/src/mindustry/world/blocks/AGENTS.md`
- `Mindustry/core/src/mindustry/mod/AGENTS.md`
- `Mindustry/tests/AGENTS.md`

Source files read (ported/skimmed):

- `Mindustry/tools/src/mindustry/tools/ImagePacker.java`
- `Mindustry/tools/src/mindustry/tools/Generators.java`
- `Mindustry/tools/src/mindustry/tools/ImageTileGenerator.java`
- `Mindustry/tools/build.gradle` (`pack`, AA, fallback, `updateBundles`)
- `Mindustry/core/src/mindustry/core/FileTree.java`
- `Mindustry/core/src/mindustry/Vars.java` (bundle/`loadSettings`)
- `Mindustry/core/src/mindustry/ui/Fonts.java`
- `Mindustry/core/src/mindustry/ctype/UnlockableContent.java`
- `Mindustry/core/src/mindustry/graphics/MultiPacker.java`, `Shaders.java`
- `Mindustry/core/src/mindustry/world/blocks/TileBitmask.java`, `Autotiler.java`
- `Mindustry/core/src/mindustry/mod/Mods.java`, `DataBundleLoader.java`
- `Mindustry/core/src/mindustry/ClientLauncher.java`
- `Mindustry/annotations/src/main/java/mindustry/annotations/impl/AssetsProcess.java`
- `Mindustry/annotations/src/main/java/mindustry/annotations/misc/LoadRegionProcessor.java`
- `Mindustry/core/assets-raw/sprites/pack.json` (+ nested configs listed in `assets-raw/AGENTS.md`)
- `Mindustry/core/assets/bundles/global.properties`, `Mindustry/core/assets/icons/icons.properties`
- `Mindustry/tests/src/test/java/DataAssetTests.java`, `ApplicationTests.java` (bootstrap)

Arc classes referenced as oracles (must be fetched/attributed at execution): `arc.graphics.Pixmap`, `arc.graphics.Pixmaps`, `arc.packer.TexturePacker`/`PixmapPacker`, `arc.graphics.g2d.TextureAtlas`, `arc.util.serialization.JsonValue`, `arc.util.I18NBundle`, `arc.util.PropertiesUtils`, `arc.audio.{Sound,Music}`.

---

## Changelog

- **2026-10-02 — M0 (lane/03-assets).** Workspace + migration landed. New crates `mind-atlas` (packing library; `manifest`, `migrate` trim rules) and `mind-tools` (`migrate` subcommand); `mind-core::assets` doc skeleton; `mind-headless assets migrate-check` + `paths::find_repo_root`. Vendored upstream `core/assets` (662 files) + `core/assets-raw` (2315 files) at commit `2cd7aeecf1378b3db456be9bfde8691b3cdc1bcc` → `assets/` + `assets-raw/` (2977 files, 61M+4.2M); trim rules in `mind-atlas/src/migrate.rs` (no `logicids.dat`/`sprites.aatls`/`version.properties`/`locales`/`basepartnames`/generated pages). Provenance at `assets/ASSET_PROVENANCE.md` (tree hash `cc949a8c…e264`); manifest `build/assets/migration_manifest.json` (gitignored) byte-identical across two runs (`f3a333652b…`). Evidence: `cargo test -p mind-atlas` 3/3, `cargo clippy -p mind-atlas -p mind-tools -p mind-headless --all-targets -D warnings` clean, `cargo fmt --check` clean, `mind-headless assets migrate-check` OK. Plan-text corrections: (a) CI `assets-pack` job deferred to the orchestrator (HLP §5.2-1 reserves CI edits); `tools/pack.sh`/`.ps1` wrappers landed instead. (b) §6.1 `sprites/loose/` dropped — loose textures keep upstream paths `assets/sprites/*.png` (`sprites/error.png` is FileTree parity ABI); gitignore selects only generated `sprites*.png` pages. (c) Arc fetched at pinned `archash=7445105cd2` (R1) as a sibling reference checkout for porting oracles; attribution to land with the pixmap/packer ports (M1).
- (not started) — generated 2026-10-01 as part of the initial plan set.
