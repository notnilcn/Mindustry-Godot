// SPDX-License-Identifier: GPL-3.0-only

//! World/sector loading helpers (grids, saves, presets, spawn counts).
//! Upstream: core/src/mindustry/core/World.java

use super::*;

/// A grid loaded from a save/preset-map file plus the rules the container
/// carried (MSAV map rules / MGRS save rules), when any.
pub(super) struct LoadedGrid {
    grid: WorldGrid,
    rules: Option<Rules>,
    /// Decoded building payloads `(tile index, base)` awaiting ECS spawn.
    pending_buildings: Vec<(usize, mind_core::world::building_io::DecodedBase)>,
    /// Team build plans carried by the save (`SaveReadState.team_plans`); the
    /// campaign runtime applies them (`apply_team_plans`).
    plans: Vec<(i32, Vec<mind_core::io::save::state::TeamPlan>)>,
    /// Resume half of the decoded save (units/fires/puddles, markers, stats,
    /// meta tags); `None` for generated worlds. The world half above was moved
    /// out (T2).
    save: Option<mind_core::io::sim_io::DecodedSave>,
}

/// Sector-only inputs of the shared [`MindSimHost::finish_world_load`] tail;
/// the map path leaves every one unset.
#[derive(Default)]
pub(super) struct WorldLoadExtras {
    /// Generated-sector polygon `(rotation, sides)` (`World.getDarkness`,
    /// `World.java:505-529`); the size is the loaded grid's width.
    polygon: Option<(f32, i32)>,
    /// `Schematics.placeLaunchLoadout` for a generated sector, run once the
    /// fresh sim + runtimes are live (H05 T4).
    generated_loadout: Option<GeneratedLoadout>,
    /// The save supplied the world: stash the resume metadata + saved tick.
    resume: bool,
}

/// The deferred generated-sector loadout step (`H05 T4`).
pub(super) type GeneratedLoadout = Box<dyn FnOnce(&mut MindSimHost)>;

/// `World.setSectorRules` (`World.java:300`): `state.map` is named after the
/// preset (`preset.localizedName`) or `"<planet>; Sector <id>"` for a
/// generated sector (L02 T4).
pub(super) fn sector_map_name(
    registry: &ContentRegistry,
    planet_name: &str,
    sector: i32,
) -> String {
    let Some(planet) = registry.planet_by_name(planet_name) else {
        return String::new();
    };
    let preset = planet
        .sectors
        .get(sector.max(0) as usize)
        .and_then(|cell| cell.preset)
        .and_then(|preset| registry.sector(preset));
    match preset {
        Some(preset) => preset.unlock.localized_name.clone(),
        None => format!("{}; Sector {sector}", planet.unlock.localized_name),
    }
}

/// Native path of a campaign sector save (`user://saves/sector-<planet>-<id>.msav`).
pub(super) fn sector_save_path(planet_name: &str, sector: u16) -> String {
    let root = ProjectSettings::singleton()
        .globalize_path("user://")
        .to_string();
    std::path::Path::new(&root)
        .join("saves")
        .join(format!("sector-{planet_name}-{sector}.msav"))
        .to_string_lossy()
        .into_owned()
}

/// Loads `Vars.bases`: reads every `baseparts/*.msch` under the resolved
/// assets dir through the host file access and classifies them (PAR-0139).
pub(super) fn load_base_catalog(registry: &ContentRegistry) -> Option<mind_core::ai::BaseRegistry> {
    let dir = format!("{}/baseparts", crate::assets::loader::resolve_assets_dir());
    let files = DirAccess::get_files_at(&dir);
    if files.is_empty() {
        log::warn!("base catalog: no `.msch` files under `{dir}`");
        return None;
    }
    let mut schematics = Vec::new();
    for name in files.as_slice() {
        let name = name.to_string();
        if !name.ends_with(".msch") {
            continue;
        }
        let path = format!("{dir}/{name}");
        let bytes = FileAccess::get_file_as_bytes(&path);
        match mind_core::game::schematics::read(bytes.as_slice(), registry) {
            Ok(schematic) => schematics.push(schematic),
            Err(error) => log::warn!("base catalog: `{path}` rejected: {error}"),
        }
    }
    if schematics.is_empty() {
        return None;
    }
    let mut bases = mind_core::ai::BaseRegistry::new();
    bases.load(registry, schematics);
    log::info!(
        "base catalog: {} core part(s), {} part(s), {} resource part(s)",
        bases.cores.len(),
        bases.parts.len(),
        bases.req_parts.len()
    );
    Some(bases)
}

/// `World.loadSector` preset branch: resolves the preset's
/// `maps/<planet>/<map>.msav` path. Returns `None` when the sector cell has no
/// preset or the file is absent (the caller then uses the planet generator).
///
/// The caller loads the resolved path through [`load_map_grid_file`] — a preset
/// map is a *map* load (`FileMapGenerator.generate` -> `SaveIO.load(map.file,
/// world.new FilterContext(map))`), so its `genfilters` scatter/ore stack runs;
/// [`load_grid_file`] stays the plain save/resume reader.
pub(super) fn resolve_preset_map_path(
    planet_name: &str,
    sector: u16,
    registry: &ContentRegistry,
) -> Option<String> {
    // Resolve `sector.preset` and its map name (`SectorPreset.initialize`).
    let (owner_name, map_name) = {
        let planet = registry.planet_by_name(planet_name)?;
        let cell = planet.sectors.get(sector as usize)?;
        let preset = registry.sector(cell.preset?)?;
        let owner = registry
            .planet(preset.planet)
            .map(|planet| planet.name.clone())
            .unwrap_or_else(|| planet_name.to_owned());
        let map = preset
            .file_name
            .clone()
            .unwrap_or_else(|| preset.name.clone());
        (owner, map)
    };
    let path = format!(
        "{}/maps/{owner_name}/{map_name}.msav",
        crate::assets::loader::resolve_assets_dir()
    );
    if !FileAccess::file_exists(&path) {
        log::warn!("load_sector: preset map missing `{path}`");
        return None;
    }
    Some(path)
}

/// Reads an `.msav`/`MGRS` file into a fresh [`WorldGrid`] through the save
/// reader, preserving the container's rules and the resume payload (units,
/// fires, puddles, markers, stats, meta tags). Plain loads (Load Game, sector
/// resume) never re-run generation filters.
pub(super) fn load_grid_file(path: &str, registry: &mut ContentRegistry) -> Option<LoadedGrid> {
    let bytes = FileAccess::get_file_as_bytes(path);
    let save = match mind_core::io::sim_io::decode_save_bytes(bytes.as_slice(), registry) {
        Ok(save) => save,
        Err(error) => {
            log::warn!("load_map_file: `{path}` failed: {error}");
            return None;
        }
    };
    let loaded = take_loaded_grid(save);
    Some(loaded)
}

/// [`load_grid_file`] for map loads: runs the `World.FilterContext` step with
/// `seed` (`World.loadMap`, `World.java:341-357,661-695`), so the map's
/// `genfilters`/default stack lands ore and boulders before `endMapLoad`. Both
/// the editor map load and a campaign preset launch (`FileMapGenerator`) go
/// through this path.
pub(super) fn load_map_grid_file(
    path: &str,
    registry: &mut ContentRegistry,
    seed: u64,
) -> Option<LoadedGrid> {
    let bytes = FileAccess::get_file_as_bytes(path);
    let save = match mind_core::io::sim_io::decode_map_bytes(bytes.as_slice(), registry, seed) {
        Ok(save) => save,
        Err(error) => {
            log::warn!("load_map_file: `{path}` failed: {error}");
            return None;
        }
    };
    if let Some(seed) = save.map_gen_seed {
        log::info!("load_map_file: `{path}` gen seed {seed}");
    }
    Some(take_loaded_grid(save))
}

/// Moves the world half out of a decoded save (callers keep their current
/// shape); the resume half stays in `save` for the sector-resume path.
pub(super) fn take_loaded_grid(mut save: mind_core::io::sim_io::DecodedSave) -> LoadedGrid {
    let grid = std::mem::take(&mut save.grid);
    let pending_buildings = std::mem::take(&mut save.pending);
    let plans = std::mem::take(&mut save.plans);
    let rules = save.rules.take();
    LoadedGrid {
        grid,
        rules,
        pending_buildings,
        plans,
        save: Some(save),
    }
}

/// Counts enemy spawn overlays (`BlockPalette.is_spawn`) on a loaded grid.
pub(super) fn count_spawns(grid: &WorldGrid, content: &ContentRegistry) -> i32 {
    grid.tiles
        .array()
        .iter()
        .filter(|tile| {
            content
                .block(tile.overlay)
                .is_some_and(|def| def.kind == BlockKind::SpawnBlock)
        })
        .count() as i32
}

impl MindSimHost {
    /// Rust-only variant (called by `MindCampaign` through a sibling bind, and
    /// by the lost-sector replay before its `load_sector`).
    pub fn set_sector_gen_extras_rs(
        &mut self,
        planet: &str,
        sector: i32,
        extras: mind_core::maps::planet::SectorGenExtras,
    ) {
        if sector < 0 {
            return;
        }
        self.pending_gen_extras = Some((planet.to_owned(), sector as u16, extras));
    }

    /// Sets the resume source for the next `load_sector` (Rust-only; called by
    /// `MindCampaign.prime_sector_load`). The campaign resolves it from the
    /// sector's meta-derived save association, so a save copied under another
    /// name is still the resume source (`Control.playSector` loads
    /// `sector.save`, `Control.java:450-456`); an empty path means "no
    /// associated save, so this is a fresh launch" (CG-SAVE-05). The host only
    /// falls back to the canonical `saves/sector-<planet>-<id>.msav` when it
    /// was never primed (a direct host call).
    pub fn set_sector_resume_file(&mut self, path: GString) {
        self.pending_sector_save = Some(path.to_string());
    }

    /// Takes the resume file the last `load_sector` could not decode
    /// (`Control.playSector`'s `catch(SaveException)`, `Control.java:555-561`):
    /// the campaign clears the sector's save association, deletes the slot and
    /// reports `@save.corrupted` (CG-SAVE-06).
    pub fn take_corrupt_sector_save(&mut self) -> Option<String> {
        self.corrupt_sector_save.take()
    }

    /// The single-core loadout schematic of `core` (`Schematic::single_core`).
    pub(super) fn single_core_schematic(
        &self,
        core: &str,
    ) -> Option<mind_core::game::schematic::Schematic> {
        let registry = self.content_snapshot.as_ref()?;
        let id = registry.block_id(core)?;
        let size = registry.block(id).map(|def| def.size).unwrap_or(1);
        Some(mind_core::game::schematic::Schematic::single_core(id, size))
    }

    /// Whether the last `load_sector` placed the generated sector's launch
    /// loadout during the world load (H05 T4).
    pub fn generated_loadout_placed(&self) -> bool {
        self.generated_loadout_placed
    }

    /// Loads the user schematic directory (`dataDirectory/schematics`) into a
    /// fresh registry (`Schematics.get()`; used by `place_launch_loadout` and
    /// the generated-sector launch resolution).
    pub(super) fn load_user_schematics(&self) -> mind_core::game::schematics::Schematics {
        use mind_core::game::schematics::Schematics;
        let mut schematics = Schematics::new();
        let Some(registry) = self.content_snapshot.as_ref() else {
            return schematics;
        };
        // Java's `schematicDirectory` is `dataDirectory/schematics`; `user://`
        // globalizes to the same data root the campaign facade loads from.
        let paths = mind_core::io::fs::Paths::new(
            ProjectSettings::singleton()
                .globalize_path("user://")
                .to_string(),
        );
        if let Err(error) = schematics.load(
            &mind_core::io::fs::NativeFs,
            &paths.root().join("schematics"),
            registry,
        ) {
            log::warn!("load_user_schematics: schematic load failed: {error}");
        }
        schematics
    }

    /// Resolves the launch loadout for a generated sector (H05 T4): the
    /// pending schematic index or the planet default (`getDefaultLoadout` on
    /// the planet's `defaultCore`), with the pending launch-resource stacks.
    pub(super) fn resolve_generated_loadout(
        &mut self,
        planet_name: &str,
    ) -> Option<mind_core::maps::generators::planet::LaunchLoadout> {
        let (index, stacks_json) = self.pending_launch.take().unwrap_or((-1, String::new()));
        let core_override = self
            .pending_launch_core
            .take()
            .and_then(|core| self.single_core_schematic(&core));
        let schematics = self.load_user_schematics();
        let registry = self.content_snapshot.as_ref()?;
        let index = if core_override.is_some() {
            None
        } else if index >= 0 {
            Some(index as usize)
        } else {
            registry
                .planet_by_name(planet_name)
                .and_then(|def| def.default_core.clone())
                .and_then(|core| registry.block_id(&core))
                .and_then(|core| schematics.get_default_loadout(core))
        };
        let Some(schematic) =
            core_override.or_else(|| index.and_then(|index| schematics.all.get(index).cloned()))
        else {
            log::warn!("load_sector: no launch loadout for `{planet_name}`");
            return None;
        };
        let stacks: Vec<mind_core::io::json::content_serde::JsonItemStack> =
            serde_json::from_str(&stacks_json).unwrap_or_default();
        let stacks = stacks
            .iter()
            .filter_map(|stack| {
                let name = stack.item.as_deref()?;
                Some((registry.item_id(name)?, stack.amount))
            })
            .collect();
        Some(mind_core::maps::generators::planet::LaunchLoadout { schematic, stacks })
    }

    /// A fresh generation seed for the next `load_map_world` (`Mathf.rand`
    /// re-rolls every map load; the port keeps the draw recorded instead).
    ///
    /// The value is mixed with the monotonic world-load counter so two loads
    /// requested within the same clock tick still re-roll; in-engine replays
    /// can pin it through `MindSimHost::set_next_map_gen_seed`.
    pub(super) fn next_map_gen_seed(&mut self) -> u64 {
        if let Some(seed) = self.pending_map_gen_seed.take() {
            return seed;
        }
        let ticks = godot::classes::Time::singleton().get_ticks_usec();
        ticks ^ self.world_loads.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }

    /// Pins the generation seed the next `load_map_world` will use and report
    /// (`World.FilterContext`; the value recorded in the state dump as
    /// `map_gen_seed`). Rust-only replay/debug seam; the default draw is a
    /// fresh seed per load.
    pub fn set_next_map_gen_seed(&mut self, seed: u64) {
        self.pending_map_gen_seed = Some(seed);
    }

    /// Resolves a `load_map_world` path argument: absolute paths as-is,
    /// `res://`/`user://` through `ProjectSettings.globalize_path`, and a bare
    /// map name through the `MindPreview` registry rows (`path` key).
    pub(super) fn resolve_map_path(&self, path: &GString) -> Option<String> {
        let raw = path.to_string();
        if raw.starts_with("res://") || raw.starts_with("user://") {
            return Some(
                ProjectSettings::singleton()
                    .globalize_path(&raw)
                    .to_string(),
            );
        }
        if std::path::Path::new(&raw).is_absolute() {
            return Some(raw);
        }
        if raw.is_empty() {
            return None;
        }
        let preview = self.preview.as_ref()?;
        let resolved = preview.bind().map_path(GString::from(raw.as_str()));
        (!resolved.is_empty()).then(|| resolved.to_string())
    }

    /// Shared `World.loadSector` build: `use_save` selects the resume source
    /// (the lost-sector replay reloads fresh with `false`).
    pub(super) fn load_sector_world(
        &mut self,
        planet_name: &str,
        sector: i32,
        use_save: bool,
    ) -> bool {
        if sector < 0 {
            return false;
        }
        // A new world invalidates the previous campaign runtime.
        self.sim.ecs.0.remove_resource::<CampaignRuntime>();
        // Resume metadata is per load; clear before this load decides.
        self.sector_resume = None;
        // The loadout placement flag is per load (H05 T4).
        self.generated_loadout_placed = false;
        // Take the registry out so the generator can borrow it while the sim is
        // rebuilt, then hand it back (content boot is expensive; never drop it).
        let Some(mut registry) = self.content_snapshot.take() else {
            log::warn!("load_sector: content registry unavailable");
            return false;
        };
        // `Control.playSector`: the save slot the campaign associated with this
        // sector is the resume source. `MindCampaign.prime_sector_load` primes
        // the exact file (meta association, so a renamed copy works —
        // CG-SAVE-05) or an empty path for "no base here"; `sector_save_path`
        // is the canonical fallback for a direct host call that never primed.
        let resume_path = match self.pending_sector_save.take() {
            Some(path) => path,
            None => sector_save_path(planet_name, sector as u16),
        };
        self.corrupt_sector_save = None;
        let resume_file =
            use_save && !resume_path.is_empty() && std::path::Path::new(&resume_path).is_file();
        let mut resumed = false;
        let mut loaded = None;
        if resume_file {
            log::info!("load_sector: resuming `{planet_name}:{sector}` from `{resume_path}`");
            loaded = load_grid_file(&resume_path, &mut registry);
            if loaded.is_none() {
                // `SaveIO.load(Fi)`'s backup fallback (`SaveIO.java:147-160`):
                // a torn primary save with a readable `-backup` still resumes.
                let backup =
                    mind_core::io::SaveIo::backup_file_for(std::path::Path::new(&resume_path));
                if backup.is_file() {
                    log::warn!(
                        "load_sector: `{resume_path}` failed to decode; trying backup `{}`",
                        backup.display()
                    );
                    loaded = load_grid_file(backup.to_string_lossy().as_ref(), &mut registry);
                }
            }
            resumed = loaded.is_some();
            if !resumed {
                // `Control.playSector`'s `catch(SaveException)`
                // (`Control.java:555-561`): the slot was readable when the
                // saves were listed but no longer decodes and has no readable
                // backup. Hand it to the campaign (clear association, delete
                // slot, `@save.corrupted`) and fall through to a fresh world
                // like Java's replay — never resume a regenerated world under
                // the stale record (CG-SAVE-06).
                log::error!(
                    "load_sector: `{planet_name}:{sector}` save `{resume_path}` is unreadable; \
                     replaying the sector fresh (`@save.corrupted`)"
                );
                self.corrupt_sector_save = Some(resume_path.clone());
            }
        }
        if loaded.is_none()
            && let Some(preset_path) =
                resolve_preset_map_path(planet_name, sector as u16, &registry)
        {
            // `World.loadSector` -> `sector.preset.generator.generate` ->
            // `SaveIO.load(map.file, world.new FilterContext(map))`
            // (`FileMapGenerator.java:71-82`): a preset map loads through the
            // map-load context, so its `genfilters` stack (boulder/prop
            // scatters) runs before `endMapLoad` — exactly like a custom-game
            // map load. Java's `filter.randomize()` draws from the global rand;
            // the port draws a recorded seed per load (`map_gen_seed`).
            let seed = self.next_map_gen_seed();
            loaded = load_map_grid_file(&preset_path, &mut registry, seed);
            if let Some(loaded) = loaded.as_ref() {
                log::info!(
                    "load_sector: loaded preset map `{preset_path}` ({}x{}, gen seed {seed})",
                    loaded.grid.tiles.width,
                    loaded.grid.tiles.height
                );
            }
        }
        let generated_sector = if loaded.is_none() {
            // H05 T2: the campaign primes the live threat/enemy-base flags;
            // the static fallback reproduces `updateBaseCoverage` for an
            // unsaved cell (`SectorGenExtras`).
            let extras = match self.pending_gen_extras.take() {
                Some((planet, id, extras)) if planet == planet_name && id == sector as u16 => {
                    extras
                }
                other => {
                    self.pending_gen_extras = other;
                    mind_core::game::planet::static_gen_extras(
                        &registry,
                        planet_name,
                        sector as u16,
                    )
                }
            };
            mind_core::maps::planet::generate_sector(planet_name, sector as u16, &registry, extras)
        } else {
            None
        };
        // The generator survives the grid/rules move so T4-T6 can run its
        // structure passes on the live sim.
        let mut generated_ctx: Option<(
            Box<dyn mind_core::maps::generators::WorldGenerator>,
            mind_core::maps::generators::planet::SectorRect,
            usize,
        )> = None;
        let loaded = match loaded {
            Some(loaded) => loaded,
            None => match generated_sector {
                Some(generated) => {
                    let mind_core::maps::planet::GeneratedSector {
                        grid,
                        rules,
                        generator,
                        rect,
                        sides,
                    } = generated;
                    generated_ctx = Some((generator, rect, sides));
                    LoadedGrid {
                        grid,
                        rules: Some(rules),
                        pending_buildings: Vec::new(),
                        plans: Vec::new(),
                        save: None,
                    }
                }
                None => {
                    self.content_snapshot = Some(registry);
                    log::warn!("load_sector: no map or generator for planet `{planet_name}`");
                    return false;
                }
            },
        };
        // A resume save only counts when the save file supplied the world (a
        // failed decode reports `corrupt_sector_save` and loads the preset map
        // or generator instead; a preset map is not a resume).
        let resume_from_save = resumed && loaded.save.is_some();
        // H05 T3: a generated sector is bounded by its projected polygon
        // (`World.getDarkness` sector term, `World.java:505-529`); presets and
        // saves of preset sectors keep the square map. A resumed generated
        // sector needs the same polygon as a fresh generation. Only
        // `(rotation, sides)` are decided here: the size is the loaded grid's.
        let polygon = if let Some((_, rect, sides)) = &generated_ctx {
            Some((rect.rotation, *sides as i32))
        } else if resume_from_save
            && let Some(def) = registry.planet_by_name(planet_name)
            && def
                .sectors
                .get(sector as usize)
                .is_some_and(|cell| cell.preset.is_none())
        {
            let planet_grid =
                mind_core::render::g3d::grid::PlanetGrid::create(def.sector_tiles as usize);
            let rect = mind_core::maps::generators::planet::make_rect(
                &planet_grid,
                sector as usize,
                def.radius,
            );
            let sides = planet_grid
                .tiles
                .get(sector as usize)
                .map(|tile| tile.corners.len())
                .unwrap_or(0);
            Some((rect.rotation, sides as i32))
        } else {
            None
        };
        // H05 T4: a generated sector places its launch loadout during the world
        // load (`Schematics.placeLaunchLoadout` runs inside `generate()`;
        // T5/T6 add the enemy base and ruins to the same pass). The closure
        // runs from the shared tail once the fresh sim is live.
        let generated_loadout: Option<GeneratedLoadout> =
            generated_ctx.take().map(|(generator, _, _)| {
                let planet = planet_name.to_owned();
                Box::new(move |host: &mut MindSimHost| {
                    let mut generator = generator;
                    if let Some(loadout) = host.resolve_generated_loadout(&planet) {
                        let bases = host
                            .sim
                            .ecs
                            .0
                            .get_resource::<mind_core::game::runtime::BaseCatalog>()
                            .map(|catalog| catalog.0.clone());
                        let mut rules = host.sector_rules.take().unwrap_or_default();
                        // `state.rules.defaultTeam` (`Schematics.placeLaunchLoadout`):
                        // the fresh sim has no `CampaignRuntime`/`BuildRules` yet, so
                        // the load's own rules carry the team.
                        let player_team = rules.default_team;
                        let wave_team = rules.wave_team;
                        if let Some(content) = host.content_snapshot.as_ref() {
                            let mut ctx = mind_core::maps::generators::planet::StructureCtx {
                                sim: &mut host.sim,
                                content,
                                bases: bases.as_deref(),
                                loadout: Some(&loadout),
                                rules: &mut rules,
                                player_team,
                                wave_team,
                            };
                            generator.place_structures(&mut ctx);
                            host.generated_loadout_placed = true;
                        }
                        host.sector_rules = Some(rules);
                    }
                    host.pending_launch = None;
                    host.pending_launch_core = None;
                    host.sim.refresh_unit_runtime();
                }) as GeneratedLoadout
            });
        let map_name = sector_map_name(&registry, planet_name, sector);
        let loaded_ok = self.finish_world_load(
            registry,
            loaded,
            WorldLoadExtras {
                polygon,
                generated_loadout,
                resume: resume_from_save,
            },
        );
        if loaded_ok {
            // `World.setSectorRules` runs after a resume too (`World.java:266,292,300`).
            self.sim.set_map_name(map_name);
            log::info!("loaded campaign sector `{planet_name}:{sector}`");
        }
        loaded_ok
    }

    /// Shared world-load tail of `load_sector_world`/`load_map_world`
    /// (`World.loadMap`'s building half): swaps the grid into a fresh `Sim`,
    /// installs the live runtimes and materializes the map's buildings. The
    /// sector-only steps (polygon, launch loadout, resume metadata) arrive via
    /// [`WorldLoadExtras`] so the two callers cannot drift.
    pub(super) fn finish_world_load(
        &mut self,
        registry: ContentRegistry,
        loaded: LoadedGrid,
        extras: WorldLoadExtras,
    ) -> bool {
        let resume_from_save = extras.resume && loaded.save.is_some();
        let LoadedGrid {
            grid,
            rules,
            pending_buildings,
            plans,
            save,
        } = loaded;
        self.sector_spawns = count_spawns(&grid, &registry);
        self.sector_rules = rules;
        self.sector_plans = plans;
        // `Vars.bases` (PAR-0139): load before the registry moves into the
        // content snapshot; the resource is inserted once the sim exists.
        let base_catalog = load_base_catalog(&registry);
        self.content_snapshot = Some(registry);

        // Reuse a fresh sim and swap in the loaded/generated grid; a new
        // iteration keeps no ECS entities from the previous world.
        let mut sim = Sim::new(1, grid.width(), grid.height(), BlockId::AIR, BlockId::AIR);
        sim.grid = grid;
        // `World.loadMap`'s `FilterContext` seed (recorded from
        // `decode_map_bytes`; surfaced in the state dump as `map_gen_seed`).
        if let Some(seed) = save.as_ref().and_then(|save| save.map_gen_seed) {
            sim.set_map_gen_seed(seed);
        }
        if let Some((rotation, sides)) = extras.polygon {
            sim.grid.sector_polygon = Some(mind_core::world::SectorPolygon {
                rotation,
                sides,
                size: sim.grid.tiles.width,
            });
        }
        let _ = sim.set_phase(mind_core::game::State::Playing);
        self.runner =
            FixedStepRunner::for_rate(sim.config().fixed_hz, sim.config().max_ticks_per_frame);
        self.sim = sim;
        self.player = None;
        // I07 T4: give the fresh sim the mod-extended block table before any
        // entity/placement work (`Blocks::new` is vanilla-only).
        if let Some(registry) = self.content_snapshot.as_ref()
            && let Err(error) = self.sim.sync_blocks_from_content(registry)
        {
            log::warn!("load_sector: mod block table sync failed: {error}");
        }
        // H06 T20: resolve `@content` constants from the point the globals
        // exist — before the map/save processors are decoded below — exactly
        // like `Vars.logicVars` at content init (`Vars.java:369`).
        self.install_logic_globals();
        // A campaign sector is live play: its placed blocks must execute their
        // plan-07 behaviors (mining, transport, crafting, power). The runtime
        // resources are installed before any entity exists so ECS indices stay
        // stable.
        self.install_live_block_runtime();
        if let Some(catalog) = base_catalog {
            self.sim
                .ecs
                .0
                .insert_resource(mind_core::game::runtime::BaseCatalog(std::sync::Arc::new(
                    catalog,
                )));
        }
        // H05 T4: the generated-sector launch loadout (`load_sector_world`).
        if let Some(generated_loadout) = extras.generated_loadout {
            generated_loadout(self);
        }
        // `World.loadMap`'s building half: the reader decoded the map's tile
        // entities into the pending queue while the grid was built, so spawn
        // the ECS buildings (cores included) the campaign runtime/teams read.
        let materialized = self.sim.materialize_map_buildings(&pending_buildings);
        if materialized > 0 {
            log::info!("world load: materialized {materialized} map building(s)");
        }
        // T2: the save's world entities (units/fires/puddles) restore into the
        // live runtime; the resume metadata (wavetime/tick/stats/markers) is
        // handed to `MindCampaign` after `play_sector`.
        if let Some(save) = save.as_ref() {
            self.sim.apply_decoded_entities(save);
        }
        if resume_from_save && let Some(save) = save.as_ref() {
            if let Some(tick) = mind_core::io::sim_io::meta_tick(&save.tags) {
                self.sim.state.tick = tick;
            }
            self.sector_resume = Some(mind_core::io::sim_io::SectorResumeMeta {
                wavetime: save
                    .tags
                    .get("wavetime")
                    .and_then(|v| v.parse::<f32>().ok()),
                tick: mind_core::io::sim_io::meta_tick(&save.tags),
                stats: save.stats.clone(),
                markers: save.markers.clone(),
                static_fog: save.static_fog.clone(),
            });
        }
        // The fresh `Sim` carries no IO executor; re-wire the seams so a queued
        // `request_save`/`request_load` drains at the next `IoSet` boundary.
        self.install_sim_seams();
        self.sim.reset_logic_presentation();
        // Map locales (`state.mapLocales`, from the save/map `locales` meta
        // tag; H06 T4). Preset maps without the tag fall back to the global
        // bundle in `run_locale_print`.
        let map_locales = save
            .as_ref()
            .and_then(|save| save.tags.get("locales"))
            .and_then(|text| {
                mind_core::io::json::JsonIo::read::<mind_core::io::json::MapLocales>(text).ok()
            })
            .unwrap_or_default();
        if let Some(mut text) = self
            .sim
            .ecs
            .0
            .get_resource_mut::<mind_core::logic::world::LogicText>()
        {
            text.map_locales = map_locales;
        }
        self.world_dirty = true;
        self.world_loads += 1;
        self.emit_state();
        self.emit_world_changed();
        self.emit_world_loaded();
        true
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Loads a scenario (path may be `res://`, `user://` or native) and resets
    /// the sim; returns `false` on read/parse/apply failure.
    #[func]
    pub fn load_scenario(&mut self, path: GString) -> bool {
        let requested = path.to_string();
        let native = ProjectSettings::singleton().globalize_path(&requested);
        let scenario = match Scenario::read(native.to_string()) {
            Ok(scenario) => scenario,
            Err(err) => {
                log::error!("load_scenario `{requested}` failed: {err}");
                return false;
            }
        };
        let sim = match Sim::from_scenario(&scenario) {
            Ok(sim) => sim,
            Err(err) => {
                log::error!("load_scenario `{requested}`: sim build failed: {err}");
                return false;
            }
        };
        let player = match ScenarioPlayer::new(&scenario, sim.content()) {
            Ok(player) => player,
            Err(err) => {
                log::error!("load_scenario `{requested}`: command resolve failed: {err}");
                return false;
            }
        };

        log::info!(
            "loaded scenario `{}` ({} steps, seed {}) from `{requested}`",
            scenario.name,
            player.total_steps(),
            scenario.seed
        );
        self.runner =
            FixedStepRunner::for_rate(sim.config().fixed_hz, sim.config().max_ticks_per_frame);
        self.sim = sim;
        self.player = Some(player);
        self.install_sim_seams();
        self.sim.reset_logic_presentation();
        self.world_dirty = true;
        self.world_loads += 1;
        self.emit_state();
        self.emit_world_changed();
        self.emit_world_loaded();
        true
    }

    /// Primes the campaign extras (`SectorGenExtras`) the next generated
    /// numbered sector reads at load time (H05 T2;
    /// `MindCampaign.prime_sector_load`). Only for `planet`/`sector` matching
    /// the next `load_sector`; otherwise the static fallback applies.
    #[func]
    pub fn set_sector_gen_extras(
        &mut self,
        planet: GString,
        sector: i32,
        threat: f32,
        has_enemy_base: bool,
        has_base: bool,
    ) {
        self.set_sector_gen_extras_rs(
            &planet.to_string(),
            sector,
            mind_core::maps::planet::SectorGenExtras {
                threat,
                has_enemy_base,
                has_base,
            },
        );
    }

    /// Sets the launch loadout the next generated sector places
    /// (`Control.playNewSector`/`Universe.getLastLoadout`; H05 T4).
    /// `schematic_index = -1` = planet default (free launch); `stacks_json` is
    /// the launch-resources array (`[{item, amount}]`).
    #[func]
    pub fn set_pending_launch(&mut self, schematic_index: i32, stacks_json: GString) {
        self.pending_launch = Some((schematic_index, stacks_json.to_string()));
    }

    /// `Universe.updateLoadout((CoreBlock)launchBlock)` +
    /// `updateLaunchResources(resources)` (Accelerator.java:275-277) for the
    /// next sector: the launch loadout becomes a single `core` block
    /// (`Universe.java:118-121`) stocked with `stacks_json`.
    #[func]
    pub fn set_pending_launch_core(&mut self, core: GString, stacks_json: GString) {
        self.pending_launch_core = Some(core.to_string());
        self.pending_launch = Some((-1, stacks_json.to_string()));
    }

    /// Generates and installs a campaign sector's world into the sim
    /// (`Control.playNewSector` world half), preferring an existing sector save
    /// (`user://saves/sector-<planet>-<id>.msav`, resume) over the preset map
    /// and the planet generator. The campaign play-flow state itself lives in
    /// `MindCampaign`; this only owns the tile grid the renderer draws.
    /// Returns `false` when the planet generator, map and save are all missing.
    #[func]
    pub fn load_sector(&mut self, planet: GString, sector: i32) -> bool {
        let planet_name = planet.to_string();
        self.load_sector_world(&planet_name, sector, true)
    }

    /// Loads a custom map file into the sim (`Control.playMap`'s world half,
    /// `Control.java:400-426`): the grid, buildings and cores of `path`, with
    /// no sector preset/generator fallback.
    ///
    /// `path` is an absolute path, a `res://`/`user://` path, or a bare map
    /// name registered with `MindPreview`. Returns `false` (and sets
    /// `last_map_error` to `map.invalid`) when the file is missing or fails to
    /// decode, exactly like `World.loadMap`'s error branch
    /// (`World.java:355-364`).
    #[func]
    pub fn load_map_world(&mut self, path: GString) -> bool {
        self.last_map_error = None;
        let Some(resolved) = self.resolve_map_path(&path) else {
            log::warn!("load_map_world: unknown map `{path}`");
            self.last_map_error = Some(String::from("map.invalid"));
            return false;
        };
        // A new world invalidates the previous campaign runtime.
        self.sim.ecs.0.remove_resource::<CampaignRuntime>();
        // Resume metadata is per load; clear before this load decides.
        self.sector_resume = None;
        // A non-sector load cannot report a corrupt sector slot.
        self.corrupt_sector_save = None;
        // The loadout placement flag is per load (H05 T4).
        self.generated_loadout_placed = false;
        let Some(mut registry) = self.content_snapshot.take() else {
            log::warn!("load_map_world: content registry unavailable");
            self.last_map_error = Some(String::from("map.invalid"));
            return false;
        };
        // `World.loadMap` loads through `new FilterContext(map)`: the map's
        // `genfilters` stack (or the default one) is applied to the decoded
        // tiles (`World.java:341-357,661-695`). Java's `filter.randomize()`
        // draws from the global `Mathf.rand`, so every load re-rolls; the port
        // draws a fresh seed per load and records it (`map_gen_seed`) so the
        // world is reproducible from seed + map (`GenerateFilter.java:91-92`).
        let seed = self.next_map_gen_seed();
        let Some(loaded) = load_map_grid_file(&resolved, &mut registry, seed) else {
            self.content_snapshot = Some(registry);
            self.last_map_error = Some(String::from("map.invalid"));
            return false;
        };
        // `state.map = map` (`World.java:369`); `Map.name()` = the `name` tag
        // (`Map.java:127-129`).
        let map_name = loaded
            .save
            .as_ref()
            .and_then(|save| save.tags.get("name").cloned())
            .unwrap_or_default();
        let loaded_ok = self.finish_world_load(registry, loaded, WorldLoadExtras::default());
        if loaded_ok {
            self.sim.set_map_name(map_name);
        }
        loaded_ok
    }

    /// `map.invalid` (or `None`) from the last failed `load_map_world`
    /// (`World.loadMap`'s `ui.showErrorMessage("@map.invalid")` branch).
    #[func]
    pub fn last_map_error(&self) -> GString {
        GString::from(self.last_map_error.as_deref().unwrap_or(""))
    }
}
