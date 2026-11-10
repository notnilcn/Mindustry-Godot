// SPDX-License-Identifier: GPL-3.0-only

//! Boot and sim-seam installation for the host (logic globals, IO executor, audio sink).
//! Upstream: core/src/mindustry/ClientLauncher.java (boot order)

use super::*;

impl MindSimHost {
    /// Installs the content-initialized logic globals (`GlobalVars`) on the sim.
    ///
    /// `LAssembler.var` resolves constants from the global `Vars.logicVars`
    /// (`J/logic/LAssembler.java:59-61`), created once in `Vars.init`
    /// (`J/core/Vars.java:369`) and filled with the content by
    /// `ContentLoader.java:107` — long before any map or save is read. Map/save
    /// buildings are decoded by `materialize_map_buildings` (`LogicBuild.read`
    /// -> `read_compressed` -> `update_code`), so this must run BEFORE them or
    /// `@sharded`/`@duo`/`@scatter` become plain null variables (H06 T20).
    /// Idempotent.
    pub(super) fn install_logic_globals(&mut self) {
        if self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::logic::globals::GlobalVars>()
            .is_some()
        {
            return;
        }
        if let Some(registry) = self.content_snapshot.as_ref() {
            mind_core::logic::globals::GlobalVars::install_world(&mut self.sim.ecs.0, registry);
        }
        // `@clientLocale`/`@clientName` from the shared settings store
        // (`GlobalVars.java:211-216` reads `player.locale()`/`player.name()`).
        let info = mind_core::logic::globals::LogicClientInfo {
            locale: crate::settings_hub::with_settings(|store| {
                store.get_string(mind_core::io::settings::KEY_LOCALE, "en")
            }),
            name: crate::settings_hub::with_settings(|store| {
                store.get_string(mind_core::io::settings::KEY_PLAYER_NAME, "")
            }),
        };
        self.sim.ecs.0.insert_resource(info);
    }

    /// Wires the host seams that live on the `Sim` value: the content-initialized
    /// logic globals (plan-13 M7), the live-sim IO executor (plan 04 §3.10) and
    /// the audio sink (plan-18).
    ///
    /// `load_scenario`/`load_sector` install a brand-new `Sim`, so this must run
    /// again after every replacement; otherwise `request_save`/`request_load`
    /// queue into an `IoQueue` with no handler and never drain.
    pub(super) fn install_sim_seams(&mut self) {
        self.install_logic_globals();
        if let Some(registry) = self.content_snapshot.take() {
            // Live world-processor sinks (G06 T4): rules/markers/wave live in
            // the campaign session; the event log + message timers are sim
            // resources (headless=false so messages/cutscenes reach the host).
            match self
                .sim
                .ecs
                .0
                .get_resource_mut::<mind_core::logic::world::LogicWorldState>()
            {
                Some(mut state) => {
                    state.headless = false;
                    state.is_host = true;
                }
                None => {
                    self.sim
                        .ecs
                        .0
                        .insert_resource(mind_core::logic::world::LogicWorldState {
                            headless: false,
                            ..mind_core::logic::world::LogicWorldState::new()
                        });
                }
            }
            if self
                .sim
                .ecs
                .0
                .get_resource::<mind_core::logic::world::MessageState>()
                .is_none()
            {
                self.sim
                    .ecs
                    .0
                    .insert_resource(mind_core::logic::world::MessageState::default());
            }
            if self
                .sim
                .ecs
                .0
                .get_resource::<mind_core::logic::world::LogicContentIndex>()
                .is_none()
            {
                self.sim.ecs.0.insert_resource(
                    mind_core::logic::world::LogicContentIndex::from_content(&registry),
                );
            }
            // World-processor text sources (H06 T4): `Core.bundle`, `Iconc`
            // and `player.locale()` for `message`/`localeprint`/`printchar`.
            // The map-scoped locales are replaced per sector load.
            let assets_dir = crate::assets::loader::resolve_assets_dir();
            let locale = crate::assets::loader::resolve_locale();
            let map_locales = self
                .sim
                .ecs
                .0
                .get_resource::<mind_core::logic::world::LogicText>()
                .map(|text| text.map_locales.clone())
                .unwrap_or_default();
            self.sim
                .ecs
                .0
                .insert_resource(mind_core::logic::world::LogicText {
                    bundle: Some(crate::assets::bundle::load_bundle(&assets_dir, &locale)),
                    iconc: Some(crate::assets::bundle::load_iconc(&assets_dir)),
                    map_locales,
                    locale,
                });
            self.content_snapshot = Some(registry);
        }

        // The handler boots its content lazily on the first IO call, so a
        // session that never saves pays nothing.
        self.sim
            .set_io_handler(Box::new(mind_core::io::SimIoHandler::new()));

        // Sim systems emit `AudioEvent`s unconditionally; `MindAudio` drains this
        // log once per frame.
        self.sim
            .ecs
            .0
            .insert_resource(AudioSinkRes::new(self.audio_log.clone()));

        // Combat FX (`Effect.at`/`Effect.shake`/decals) go to `MindFx`'s bus;
        // a fresh `Sim` carries a fresh noop-sink unit runtime, so this runs
        // after every replacement.
        self.install_fx_sink();
    }

    /// Installs the live `MindFx` bus on the sim's unit runtime so every
    /// `FxSink` call reaches the view instead of the noop sink (the scene-wired
    /// `fx` dependency; `FxBus` is `Send + Sync`). No-op when the FX host or
    /// unit runtime is absent.
    pub(super) fn install_fx_sink(&mut self) {
        let Some(fx) = self.fx.as_ref() else {
            return;
        };
        let bus = fx.bind().bus_handle();
        // Reactor/generator explosions (`ExplosionSink`) share the same bus so
        // their effect/sound/shake/scorch reach the view (PAR-0399).
        self.sim.ecs.0.insert_resource(
            mind_core::world::blocks::power::explosion::ExplosionSinkRes(Some(Box::new(
                crate::fx::explosion_sink::HostExplosionSink { bus: bus.clone() },
            ))),
        );
        if let Some(mut runtime) = self.sim.unit_runtime_mut() {
            runtime.fx = bus;
        }
    }

    /// Installs the opt-in live block runtime (plan-07 `BlockTable` + behaviors)
    /// over the host's content snapshot.
    ///
    /// The default `Sim` spine stores a bare `BuildingComp` per placed block, so
    /// buildings are inert (`update_buildings` is a no-op without the runtime
    /// resources). Installing the runtime makes every subsequent place/break go
    /// through the real tile ops and lets the scheduled building/power systems
    /// run. Idempotent; returns `false` when the content snapshot is unavailable
    /// or the table fails to build.
    pub(super) fn install_live_block_runtime(&mut self) -> bool {
        // I07 T4: install over the host's content snapshot (base + mods) so
        // runtime placement/config lookups address the same registry the
        // command entry points do. Without a snapshot, fall back to vanilla.
        if !self.sim.has_block_runtime() {
            let result = match self.content_snapshot.as_ref() {
                Some(content) => self.sim.install_block_runtime_from(
                    mind_core::sim::runtime::SimRuntimeConfig::default(),
                    content.clone(),
                ),
                None => self.sim.install_block_runtime(),
            };
            if let Err(error) = result {
                log::warn!("live block runtime install failed: {error}");
                return false;
            }
        }
        // EV-0048: the live unit/combat runtime rides on the same content/world;
        // a failed install keeps buildings working but leaves units inert.
        if !self.sim.has_unit_runtime() {
            match self.content_snapshot.as_ref() {
                Some(content) => self.sim.install_unit_runtime_from(content.clone()),
                None => {
                    if let Err(error) = self.sim.install_unit_runtime() {
                        log::warn!("live unit runtime install failed: {error}");
                        return false;
                    }
                }
            }
        }
        true
    }
}
