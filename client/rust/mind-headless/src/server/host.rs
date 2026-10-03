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
    /// A known built-in/sector-preset map was requested, but the real-world
    /// loader (plan 06/12/19) is not wired into the headless host yet.
    NeedsWorldLoader(String),
    /// No world is loaded (the command needs `host` first).
    NoWorld,
    /// The plan-21 host/relay call failed (`serve` online mode).
    Net(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::UnknownMap(map) => write!(f, "unknown map: {map}"),
            HostError::NeedsWorldLoader(map) => write!(
                f,
                "`{map}` is a known map/sector preset, but the real-world loader \
                 (plan 06/12/19) is not wired into the headless host yet; use \
                 `host synthetic` or place a `{map}.msav` under config/maps/"
            ),
            HostError::NoWorld => write!(f, "no world loaded; use `host <map> [mode]` first"),
            HostError::Net(reason) => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for HostError {}

/// Admin/moderation failure (plan 21 §3.10 seam).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminError {
    /// No live STDB connection: the command needs a database (or offline mode
    /// falls back to the JSON mirror).
    Offline,
    /// Under D2 the action has no representation (IP/subnet bans: no peer IPs).
    Unsupported(String),
    /// The player/identity could not be resolved.
    UnknownTarget(String),
    /// The reducer send failed.
    Connector(String),
    /// No match is currently hosted.
    NoMatch,
}

impl std::fmt::Display for AdminError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdminError::Offline => {
                write!(
                    f,
                    "requires a live STDB connection (plan 21); unavailable offline"
                )
            }
            AdminError::Unsupported(reason) => write!(f, "{reason}"),
            AdminError::UnknownTarget(target) => write!(f, "no player found matching `{target}`"),
            AdminError::Connector(reason) => write!(f, "STDB reducer failed: {reason}"),
            AdminError::NoMatch => write!(f, "no match is hosted"),
        }
    }
}

impl std::error::Error for AdminError {}

/// A live player row for the `players`/`status` view (plan 21 §3.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerView {
    /// Identity hex (64 lowercase hex chars).
    pub identity: String,
    /// Last-known username (`""` when unresolved).
    pub name: String,
    /// Team index, when assigned.
    pub team: Option<u8>,
    /// Whether an admin record exists.
    pub admin: bool,
    /// Whether the member is currently connected (last `relay_member` row).
    pub connected: bool,
    /// Membership role name (`player`/`spectator`).
    pub role: String,
    /// Placeholder for the upstream mobile/modded flags (always false here).
    pub mobile: bool,
    /// Placeholder for locale (`""` when unknown).
    pub locale: String,
}

/// The console-facing admin/moderation seam (plan 21 §3.10 `Admins`).
///
/// The dedicated server implements this over the `mind-stdb` reducer set; the
/// offline mirror is the fallback when no database is reachable (P22-6).
pub trait Admins {
    /// Broadcasts a server `System` chat message (`say`).
    fn say(&mut self, message: &str) -> Result<(), AdminError>;
    /// Kicks a connected player (`kick`).
    fn kick(&mut self, target: &str) -> Result<(), AdminError>;
    /// Bans a player (`ban [kind] <target>`).
    fn ban(&mut self, kind: crate::server::admin::BanKind, target: &str) -> Result<(), AdminError>;
    /// Removes every ban for a target (`unban`).
    fn unban(&mut self, target: &str) -> Result<(), AdminError>;
    /// Removes an identity ban (`pardon`).
    fn pardon(&mut self, target: &str) -> Result<(), AdminError>;
    /// Grants/revokes admin (`admin add|remove`).
    fn grant(&mut self, target: &str, on: bool) -> Result<(), AdminError>;
    /// Enables/disables the global whitelist (`config whitelist`).
    fn set_whitelist_enabled(&mut self, enabled: bool) -> Result<(), AdminError>;
    /// Adds/removes a whitelist identity (`whitelist add|remove`).
    fn whitelist(&mut self, target: &str, on: bool) -> Result<(), AdminError>;
    /// Sets the connection player cap (`playerlimit [off/number]`).
    fn player_limit(&mut self, limit: Option<u16>) -> Result<(), AdminError>;
    /// Offline `search`/`info` scan (mirror + live players).
    fn search(&self, query: &str) -> Vec<PlayerView>;
    /// Snapshot mirrors for `admins`/`bans` listing.
    fn mirror(&self) -> &crate::server::admin::AdminMirror;
}

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
    /// Live player views (`players`/`status`); empty for a local host.
    fn players(&self) -> Vec<PlayerView> {
        Vec::new()
    }
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

    /// Distinguishes an unknown name from a known built-in/sector preset the
    /// headless host cannot yet build (plan 06/12/19 loader not wired).
    fn missing_map_error(&self, name: &str) -> HostError {
        let known_builtin = mind_core::maps::DEFAULT_MAP_NAMES.contains(&name);
        let known_sector = self.registry.sector_by_name(name).is_some();
        if known_builtin || known_sector {
            HostError::NeedsWorldLoader(name.to_owned())
        } else {
            HostError::UnknownMap(name.to_owned())
        }
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
                return Err(self.missing_map_error(name));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> &'static ContentRegistry {
        Box::leak(Box::new(
            crate::server::boot_content().expect("content boot"),
        ))
    }

    /// `host groundZero` / `host maze`: plan 06/19 names are recognised and
    /// delegated to the (unwired) real-world loader rather than reported unknown.
    #[test]
    fn known_map_names_delegate_to_world_loader() {
        let dir = std::env::temp_dir().join(format!("mind-host-map-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut host = LocalHost::new(registry(), dir.clone());
        assert!(matches!(
            host.host("groundZero", "survival"),
            Err(HostError::NeedsWorldLoader(_))
        ));
        assert!(matches!(
            host.host("maze", "survival"),
            Err(HostError::NeedsWorldLoader(_))
        ));
        assert!(matches!(
            host.host("no-such-map-xyz", "survival"),
            Err(HostError::UnknownMap(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn synthetic_host_roundtrips_status() {
        let dir = std::env::temp_dir().join(format!("mind-host-syn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut host = LocalHost::new(registry(), dir.clone());
        host.host("synthetic", "survival").expect("host");
        assert!(host.status().hosting);
        assert_eq!(host.status().map.as_deref(), Some("synthetic"));
        // `FixtureWorld::synthetic` starts at wave 3 (save-roundtrip fixture), so
        // assert the increment rather than an absolute value.
        let before = host.status().wave;
        host.run_wave().expect("wave");
        assert_eq!(host.status().wave, before + 1);
        host.stop();
        assert!(!host.status().hosting);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
