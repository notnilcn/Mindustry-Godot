// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Schematic library: loading, listing, selecting and placing schematics.
//! Upstream: game/Schematics.java

use super::cache::dir_mtime_key;
use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Lists the decoded schematics (vanilla loadouts + `schematics/*.msch`).
    #[func]
    pub fn list_schematics(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        let Some(registry) = &self.registry else {
            return out;
        };
        let schematics = self.load_schematics(registry);
        for schematic in &schematics.all {
            let mut row = VarDictionary::new();
            row.set(
                &GString::from("name"),
                &schematic.name().as_str().to_variant(),
            );
            row.set(&GString::from("width"), &schematic.width.to_variant());
            row.set(&GString::from("height"), &schematic.height.to_variant());
            row.set(
                &GString::from("tiles"),
                &(schematic.tiles.len() as i64).to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Captures a schematic from the live world selection (I-5): reads the
    /// selected tile rectangle and returns its base64 `.msch` payload.
    #[func]
    pub fn write_schematic_selection(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> GString {
        let Some(host) = self.host() else {
            return GString::from("");
        };
        let Some(registry) = self.registry.as_ref() else {
            return GString::from("");
        };
        let tiles = host.bind().tile_region(x0, y0, x1, y1);
        let Some(min_x) = tiles.iter().map(|(x, _, _)| *x as i32).min() else {
            return GString::from("");
        };
        let Some(min_y) = tiles.iter().map(|(_, y, _)| *y as i32).min() else {
            return GString::from("");
        };
        let Some(max_x) = tiles.iter().map(|(x, _, _)| *x as i32).max() else {
            return GString::from("");
        };
        let Some(max_y) = tiles.iter().map(|(_, y, _)| *y as i32).max() else {
            return GString::from("");
        };
        let width = max_x - min_x + 1;
        let height = max_y - min_y + 1;
        let mut schematic = mind_core::game::schematic::Schematic {
            tiles: Vec::new(),
            labels: Vec::new(),
            tags: Default::default(),
            width,
            height,
            file: None,
            mod_name: None,
        };
        for (x, y, block) in tiles {
            if block == 0 {
                continue;
            }
            schematic.tiles.push(mind_core::game::schematic::Stile::new(
                mind_core::content::BlockId::new(block),
                (x as i32 - min_x) as i16,
                (y as i32 - min_y) as i16,
                Default::default(),
                0,
            ));
        }
        match mind_core::game::schematics::write_base64(&schematic, registry) {
            Ok(encoded) => GString::from(encoded.as_str()),
            Err(error) => {
                log::warn!("write_schematic_selection failed: {error}");
                GString::from("")
            }
        }
    }

    /// Places a base64 `.msch` schematic centered at `(x, y)` into the live
    /// world (I-5); rotation `0` only until the rotate relay command lands.
    #[func]
    pub fn place_schematic_base64(&mut self, b64: GString, x: i32, y: i32) -> bool {
        let Some(registry) = self.registry.as_ref() else {
            return false;
        };
        let Ok(schematic) = mind_core::game::schematics::read_base64(&b64.to_string(), registry)
        else {
            return false;
        };
        let Some(mut host) = self.host() else {
            return false;
        };
        let mut placed = false;
        let mut host = host.bind_mut();
        // With a live player unit the tiles queue as construction plans
        // (`InputHandler` placement path); otherwise they place instantly
        // (editor/preview worlds, T18).
        let player_plans = host.player_can_build();
        for stile in &schematic.tiles {
            if stile.block == mind_core::content::BlockId::AIR {
                continue;
            }
            let Ok(tile_x) = i16::try_from(x + stile.x as i32) else {
                continue;
            };
            let Ok(tile_y) = i16::try_from(y + stile.y as i32) else {
                continue;
            };
            placed |= if player_plans {
                host.queue_player_place_at(tile_x, tile_y, stile.block, stile.rotation)
            } else {
                host.place_block_at(tile_x, tile_y, stile.block, stile.rotation)
            };
        }
        placed
    }
}

impl MindCampaign {
    /// The schematic library (`Schematics.load`: vanilla loadouts plus every
    /// `.msch` under the data dir's `schematics/`; Java's `schematicDirectory`
    /// is `dataDirectory/schematics`, which the profile import extracts).
    ///
    /// Memoized on the directory mtime (finding #9): dialogs call
    /// `campaign_views_json` repeatedly per rebuild and every uncached call
    /// re-reads every file. The port only creates/deletes `.msch` files (the
    /// schematic dialog writes a fresh name; `remove` deletes), so a directory
    /// mtime catches every content change; a same-file in-place rewrite by an
    /// external tool would not be seen.
    pub(super) fn load_schematics(&self, registry: &ContentRegistry) -> Rc<Schematics> {
        let dir = self.paths.root().join("schematics");
        let key = dir_mtime_key(&dir);
        if let Some(cached) = self.schematics_cache.borrow().get(key) {
            return Rc::clone(cached);
        }
        let mut schematics = Schematics::new();
        if let Err(error) = schematics.load(&NativeFs, &dir, registry) {
            log::warn!(
                "failed to load schematics from `{}`: {error}",
                dir.display()
            );
        }
        let schematics = Rc::new(schematics);
        self.schematics_cache
            .borrow_mut()
            .store(key, Rc::clone(&schematics));
        schematics
    }

    /// The `index`-th loaded schematic (`list_schematics` order) for
    /// `MindInput.use_schematic` (G08 T6b, consumed by G05 T10b).
    pub fn schematic_at(&self, index: i32) -> Option<mind_core::game::schematic::Schematic> {
        let registry = self.registry.as_ref()?;
        self.load_schematics(registry)
            .all
            .get(index.max(0) as usize)
            .cloned()
    }
}
