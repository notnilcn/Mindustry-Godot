// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server host control (plan 22 §3.6/§3.11).
//!
//! The multiplayer host/join API is plan 21 (not built); this module defines the
//! small [`HostControl`] seam the console commands call and a local,
//! Godot-free [`LocalHost`] that boots the plan-04 synthetic world so
//! `host`/`save`/`load`/`runwave` work end-to-end today. `LocalHost` uses the
//! fixture world until plan 06/07 supply real maps/buildings; plan 21 swaps in
//! the networked host behind the trait.

use std::path::Path;

use mind_core::content::ContentRegistry;
use mind_core::io::fs::FileSystem;
use mind_core::io::save::fixture::{FixtureContext, FixtureSink, FixtureWorld};
use mind_core::io::save::versions::v1::base_meta_tags;
use mind_core::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};

/// Host failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// The requested map is not `synthetic` and no map file matched.
    UnknownMap(String),
    /// No world is loaded (the command needs `host` first).
    NoWorld,
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::UnknownMap(map) => write!(f, "unknown map: {map}"),
            HostError::NoWorld => write!(f, "no world loaded; use `host <map> [mode]` first"),
        }
    }
}

impl std::error::Error for HostError {}

/// Host state snapshot for `status`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostStatus {
    /// Whether a world is loaded.
    pub hosting: bool,
    /// Loaded map name.
    pub map: Option<String>,
    /// Game mode.
    pub mode: Option<String>,
    /// Current wave.
    pub wave: i32,
    /// Current tick.
    pub tick: u64,
    /// Connected players (plan 21; always 0 locally).
    pub players: usize,
}

/// The console-facing host seam (plan 21 `HostController` stands behind this).
pub trait HostControl {
    /// Loads `map` in `mode` and starts hosting.
    fn host(&mut self, map: &str, mode: &str) -> Result<(), HostError>;
    /// Stops hosting (world unloaded).
    fn stop(&mut self);
    /// Current host state.
    fn status(&self) -> HostStatus;
    /// Forces the next wave.
    fn run_wave(&mut self) -> Result<(), HostError>;
}

/// A local, Godot-free host over the plan-04 synthetic world.
pub struct LocalHost {
    registry: &'static ContentRegistry,
    maps_dir: std::path::PathBuf,
    world: Option<FixtureWorld<'static>>,
    map: Option<String>,
    mode: Option<String>,
}

impl std::fmt::Debug for LocalHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalHost")
            .field("hosting", &self.world.is_some())
            .field("map", &self.map)
            .field("mode", &self.mode)
            .finish()
    }
}

impl LocalHost {
    /// Creates a host over `registry` that looks for map files in `maps_dir`.
    pub fn new(
        registry: &'static ContentRegistry,
        maps_dir: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            registry,
            maps_dir: maps_dir.into(),
            world: None,
            map: None,
            mode: None,
        }
    }

    /// The loaded content registry.
    pub fn registry(&self) -> &'static ContentRegistry {
        self.registry
    }

    /// Whether a world is loaded.
    pub fn is_hosting(&self) -> bool {
        self.world.is_some()
    }

    /// Advances the loaded world by `ticks`.
    pub fn tick(&mut self, ticks: u64) {
        if let Some(world) = &mut self.world {
            for _ in 0..ticks {
                world.tick();
            }
        }
    }

    /// Saves the loaded world to `path` (`save <slot>`).
    pub fn save(&self, fs: &dyn FileSystem, path: &Path) -> Result<(), HostError> {
        let world = self.world.as_ref().ok_or(HostError::NoWorld)?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut tags = base_meta_tags(
            world.width,
            world.height,
            world.wave,
            self.map.as_deref().unwrap_or("world"),
        );
        tags.insert("tick".to_owned(), world.tick.to_string());
        let mut ctx = WriteContext::meta_only(tags);
        ctx.content = Some(self.registry);
        ctx.map = Some(world);
        ctx.entities = Some(world);
        SaveIo::save(fs, path, &ctx, &SaveOptions::new()).map_err(|error| {
            log::warn!("save failed: {error}");
            HostError::NoWorld
        })
    }

    /// Loads `path` into a fresh synthetic-backed world (`load <slot>`).
    pub fn load(&mut self, fs: &dyn FileSystem, path: &Path) -> Result<(), HostError> {
        if !fs.exists(path) {
            return Err(HostError::UnknownMap(path.display().to_string()));
        }
        let cell = std::cell::RefCell::new(FixtureWorld::new(self.registry, 0, 0));
        let mut context = FixtureContext(&cell);
        let mut sink = FixtureSink(&cell);
        let mut load_registry = super::boot_content().map_err(|error| {
            log::warn!("load content boot failed: {error}");
            HostError::NoWorld
        })?;
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut load_registry),
            entities: Some(&mut sink),
            ..SaveReadState::default()
        };
        if let Err(error) = SaveIo::load(fs, path, &mut state) {
            log::warn!("load failed: {error}");
            return Err(HostError::NoWorld);
        }
        let tags = state.tags.clone();
        let plans = state.team_plans.clone();
        drop(state);
        let mut world = cell.into_inner();
        world.apply_meta(&tags);
        world.apply_team_plans(plans);
        self.world = Some(world);
        self.map = Some(
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("loaded")
                .to_owned(),
        );
        self.mode.get_or_insert_with(|| "survival".to_owned());
        Ok(())
    }
}

impl HostControl for LocalHost {
    fn host(&mut self, map: &str, mode: &str) -> Result<(), HostError> {
        let name = if map.is_empty() { "synthetic" } else { map };
        let world = if name == "synthetic" {
            FixtureWorld::synthetic(self.registry, 64, 64)
        } else {
            // Accept a bare map name or a direct `.msav` path in `maps_dir`.
            let candidate = self.maps_dir.join(name);
            let candidate = if candidate.extension().is_some() {
                candidate
            } else {
                candidate.with_extension("msav")
            };
            if !candidate.exists() {
                return Err(HostError::UnknownMap(name.to_owned()));
            }
            let mut loader = LocalHost {
                registry: self.registry,
                maps_dir: self.maps_dir.clone(),
                world: None,
                map: None,
                mode: None,
            };
            loader.load(&mind_core::io::fs::NativeFs, &candidate)?;
            loader.world.take().ok_or(HostError::NoWorld)?
        };
        self.world = Some(world);
        self.map = Some(name.to_owned());
        self.mode = Some(mode.to_owned());
        Ok(())
    }

    fn stop(&mut self) {
        self.world = None;
        self.map = None;
        self.mode = None;
    }

    fn status(&self) -> HostStatus {
        match &self.world {
            Some(world) => HostStatus {
                hosting: true,
                map: self.map.clone(),
                mode: self.mode.clone(),
                wave: world.wave,
                tick: world.tick,
                players: 0,
            },
            None => HostStatus::default(),
        }
    }

    fn run_wave(&mut self) -> Result<(), HostError> {
        let world = self.world.as_mut().ok_or(HostError::NoWorld)?;
        world.wave = world.wave.saturating_add(1);
        Ok(())
    }
}
