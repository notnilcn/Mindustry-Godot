// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Generate-dialog filters and the sector-generate requests.
//! Upstream: core/src/mindustry/editor/MapGenerateDialog.java

use super::*;

#[godot_api(secondary)]
impl MindEditor {
    // --- M4/M5/M6 dialog data (plan 19 M4-M6) ---

    /// `MapGenerateDialog.filters`: the `genfilters` tag JSON, or the default
    /// stack when unset.
    #[func]
    pub fn filters_json(&self) -> GString {
        if let Some(tag) = self.editor.tags.get("genfilters")
            && !tag.trim().is_empty()
        {
            return GString::from(tag.as_str());
        }
        match self.content.as_ref() {
            Some(content) => GString::from(
                mind_core::maps::filters::write_filters(
                    content,
                    &mind_core::maps::filters::default_filter_stack(content),
                )
                .as_str(),
            ),
            None => GString::from("[]"),
        }
    }

    /// Stores the `genfilters` tag from the generate dialog.
    #[func]
    pub fn set_filters_json(&mut self, json: GString) -> bool {
        // GDScript JSON round-trips turn integral numbers into floats; restore
        // the `int` fields before the tag keeps an unparsable `"seed": 0.0`.
        let normalized = mind_core::maps::filters::normalize_filter_numbers(&json.to_string());
        self.editor.tags.insert("genfilters".to_owned(), normalized);
        self.clear_error();
        true
    }

    /// `MapGenerateDialog.applyToEditor`: run the tag filters over the world and
    /// clear the op stack.
    #[func]
    pub fn apply_filters(&mut self) -> bool {
        self.mark_tiles_changed();
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let tag = self.editor.tags.get("genfilters").cloned();
        let mut filters = mind_core::editor::generate::filters_from_tag(content, tag.as_deref());
        mind_core::editor::generate::apply_filters_to_editor(
            &mut self.editor,
            &mut self.grid,
            content,
            &mut filters,
            0,
        );
        self.clear_error();
        true
    }

    /// `MapGenerateDialog.rebuildFilters`: one row per tag filter with its
    /// `FilterOption` descriptors and the resolved values (`filter.options()`
    /// plus each option's getter, class defaults merged).
    #[func]
    pub fn filter_options_json(&self) -> GString {
        let Some(content) = self.content() else {
            return GString::from("[]");
        };
        let tag = self
            .editor
            .tags
            .get("genfilters")
            .cloned()
            .unwrap_or_default();
        GString::from(mind_core::maps::filters::filter_options_json(content, &tag).as_str())
    }

    /// `MapGenerateDialog.showAdd`: `Maps.allFilterTypes` rows (class tag,
    /// `simpleName`, icon block, `isPost`).
    #[func]
    pub fn filter_types_json(&self) -> GString {
        GString::from(mind_core::maps::filters::filter_types_json().as_str())
    }

    /// `Maps.addDefaultOres` as a `genfilters` JSON fragment (the `@add`
    /// picker's "Default Ores" row).
    #[func]
    pub fn default_ores_json(&self) -> GString {
        let Some(content) = self.content() else {
            return GString::from("[]");
        };
        let filters = mind_core::maps::filters::default_ore_filters(content);
        GString::from(mind_core::maps::filters::write_filters(content, &filters).as_str())
    }

    /// `FilterOption.BlockOption` picker rows: every block the named predicate
    /// accepts (`content.blocks()` + `Boolf<Block>`), in content-id order.
    #[func]
    pub fn filter_blocks_json(&self, predicate: GString) -> GString {
        let Some(content) = self.content() else {
            return GString::from("[]");
        };
        let Some(predicate) = BlockPredicate::from_name(&predicate.to_string()) else {
            return GString::from("[]");
        };
        let rows: Vec<serde_json::Value> = content
            .blocks()
            .iter()
            .filter(|def| predicate.test(content, def.id))
            .map(|def| {
                serde_json::json!({
                    "name": def.name,
                    "id": def.id.raw(),
                    "ui_icon": format!("block-{}-ui", def.name),
                })
            })
            .collect();
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| "[]".to_owned())
                .as_str(),
        )
    }

    /// `MapGenerateDialog.update()`: the live filter preview over the editor
    /// grid, colored with the map-preview palette (`MapIO.colorFor`).
    #[func]
    pub fn generate_preview_texture(&mut self) -> Option<Gd<Texture2D>> {
        if self.palette.is_none() {
            let content = self.content()?;
            let mut palette = BlockPalette::of(content);
            palette.apply_block_colors(&crate::render::minimap::load_block_colors());
            self.palette = Some(palette);
        }
        let snapshot = mind_core::editor::generate::EditorSnapshot::capture(&self.grid);
        let tag = self.editor.tags.get("genfilters").cloned();
        let content = self.content()?;
        let palette = self.palette.as_ref()?;
        let mut filters = mind_core::editor::generate::filters_from_tag(content, tag.as_deref());
        let image = mind_core::editor::generate::generate_preview_with_palette(
            &snapshot,
            &mut filters,
            content,
            palette,
            0,
        );
        preview::bind_texture(&image)
    }

    /// `SectorGenerateDialog.apply`: generation is owned by the planet/sector
    /// host (plan 06 `WorldGrid::load_sector`); the dialogue records the request
    /// and reports deferred until that host is wired in-engine.
    #[func]
    pub fn sector_generate(&mut self, planet: GString, sector: i64, seed: i64) -> bool {
        let planet_name = planet.to_string();
        let Ok(sector_id) = u16::try_from(sector) else {
            self.last_error_key = Some("@editor.sectorgenerate".to_owned());
            self.last_error_message = Some(format!("invalid sector {sector}"));
            return false;
        };
        let seed_offset = seed.max(0) as u64;
        let Some(content) = self.content() else {
            return false;
        };
        // `World.loadSector` -> `PlanetGenerator.generate` (the structure
        // passes stay sim-side; the editor gets the generated terrain).
        let extras = mind_core::game::planet::static_gen_extras(content, &planet_name, sector_id);
        let Some(generated) = mind_core::maps::planet::generate_sector_with_seed(
            &planet_name,
            sector_id,
            content,
            extras,
            seed_offset,
        ) else {
            self.last_error_key = Some("@editor.sectorgenerate".to_owned());
            self.last_error_message = Some(format!("no generator for `{planet_name}`"));
            return false;
        };
        let generated_rules = generated.rules.clone();
        // `editor.load(() -> world.loadSector(...))` + `logic.reset()`: the
        // generated grid replaces the editor world and its entities.
        self.grid = generated.grid;
        self.ecs = MindWorld::new();
        // Java writes the generated `state.rules`; the port's editor keeps the
        // map rules in the `rules` tag, and clears `sector` + the extra filters
        // (`state.rules.sector = null`, `editor.tags.put("genfilters", "{}")`).
        if let Ok(json) = mind_core::io::json::JsonIo::write(&generated_rules) {
            self.editor.tags.insert("rules".to_owned(), json);
        }
        self.editor
            .tags
            .insert("genfilters".to_owned(), "{}".to_owned());
        self.editor.tags.shift_remove("sector");
        self.file = None;
        self.editor.saved = false;
        self.mark_tiles_changed();
        self.clear_error();
        true
    }

    /// `SectorGenerateDialog`'s planet picker (`:39-41`): every planet with a
    /// generator, non-empty sectors and `accessible`.
    #[func]
    pub fn sector_planets_json(&self) -> GString {
        let Some(content) = self.content() else {
            return GString::from("[]");
        };
        let rows: Vec<serde_json::Value> = content
            .planets()
            .iter()
            .filter(|def| {
                def.generator != GeneratorKind::None && !def.sectors.is_empty() && def.accessible
            })
            .map(|def| {
                serde_json::json!({
                    "name": def.name,
                    "sectors": def.sectors.len(),
                })
            })
            .collect();
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| "[]".to_owned())
                .as_str(),
        )
    }

    /// `Sector.getSize()` for the picker's `[ NxN ]` label
    /// (`Sector.java:203-205`); `0` when the planet/cell is unknown.
    #[func]
    pub fn sector_size(&self, planet: GString, sector: i64) -> i64 {
        let Some(content) = self.content() else {
            return 0;
        };
        let Ok(sector_id) = u16::try_from(sector) else {
            return 0;
        };
        mind_core::maps::planet::sector_size(&planet.to_string(), sector_id, content)
            .map(|size| size as i64)
            .unwrap_or(0)
    }
}
