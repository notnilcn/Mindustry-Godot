// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map objectives: completion, live stats, marker payloads and the fog query.
//! Upstream: game/MapObjectives.java; core/src/mindustry/core/Renderer.java (markers, ~352-360)

use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Completes an objective by index (`complete_objective` relay target).
    #[func]
    pub fn complete_objective(&mut self, index: i64) -> bool {
        if index < 0 {
            return false;
        }
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                let completed = runtime
                    .session
                    .objectives
                    .complete(index as usize, &mut runtime.session.rules);
                self.session.objectives = runtime.session.objectives.clone();
                host.bind_mut().put_campaign_runtime(runtime);
                return completed;
            }
        }
        self.session
            .objectives
            .complete(index as usize, &mut self.session.rules)
    }

    /// Per-objective state rows (`index`, `type`, `completed`, `countup`).
    #[func]
    pub fn get_objective_state(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        for index in 0..self.session.objectives.len() {
            let mut row = VarDictionary::new();
            row.set(&GString::from("index"), &(index as i64).to_variant());
            if let Some(kind) = self.session.objectives.type_name(index) {
                row.set(&GString::from("type"), &kind.as_str().to_variant());
            }
            row.set(
                &GString::from("completed"),
                &self.session.objectives.is_completed(index).to_variant(),
            );
            row.set(
                &GString::from("countup"),
                &self.session.objectives.timer_countup(index).to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Debug seam: `{completed, countup}` straight from the live runtime
    /// executor (the facade mirror is stale while the frame pump is paused).
    #[func]
    pub fn objective_runtime_debug(&mut self, index: i64) -> GString {
        if index < 0 {
            return GString::from("null");
        }
        let index = index as usize;
        let Some(mut host) = self.host() else {
            return GString::from("null");
        };
        let Some(runtime) = host.bind_mut().take_campaign_runtime() else {
            return GString::from("null");
        };
        let completed = runtime.session.objectives.is_completed(index);
        let countup = runtime.session.objectives.timer_countup(index);
        host.bind_mut().put_campaign_runtime(runtime);
        GString::from(
            serde_json::json!({"completed": completed, "countup": countup})
                .to_string()
                .as_str(),
        )
    }

    /// Live counters `Objective.text()` reads on top of the objective data:
    /// `state.stats.placedBlockCount`/`enemyUnitsDestroyed` (`MapObjectives.java:442,501`),
    /// the running `UnitCount` objective's `TeamData.countType` (`:470`), and
    /// `DestroyBlocksObjective.progress()` (`:646`, a live `world.build` scan).
    ///
    /// The HUD feeds these into its `ObjectiveLocale`; count-based objective
    /// text shows the **remaining** count, so they must be live, not persisted.
    #[func]
    pub fn objective_live_stats_json(&mut self) -> GString {
        let mut out = serde_json::Map::new();
        let mut unit_counts = serde_json::Map::new();
        let mut blocks_progress = serde_json::Map::new();
        let mut timer_countup = serde_json::Map::new();
        let taken = self
            .host()
            .and_then(|mut host| host.bind_mut().take_campaign_runtime());
        {
            let session = taken
                .as_ref()
                .map_or(&self.session, |runtime| &runtime.session);
            out.insert(
                "placedBlocks".to_owned(),
                serde_json::to_value(&session.stats.placed_block_count).unwrap_or_default(),
            );
            out.insert(
                "enemyUnitsDestroyed".to_owned(),
                serde_json::json!(session.stats.enemy_units_destroyed),
            );
            let team = i64::from(session.rules.default_team);
            // `unit_debug_json` is a host round-trip plus a JSON parse; fetch it
            // at most once per call, and only when a `UnitCount` objective runs
            // (finding #8). The row filter itself is the pure
            // [`count_unit_rows`].
            let mut unit_rows: Option<Vec<serde_json::Value>> = None;
            for index in session.objectives.running_indices() {
                match session.objectives.get(index) {
                    Some(mind_core::io::json::objectives::MapObjective::Timer(_)) => {
                        timer_countup.insert(
                            index.to_string(),
                            serde_json::json!(session.objectives.timer_countup(index)),
                        );
                    }
                    Some(mind_core::io::json::objectives::MapObjective::UnitCount(objective)) => {
                        let unit = objective.unit.as_deref().unwrap_or("");
                        // `state.rules.defaultTeam.data().countType(unit)`.
                        let rows = unit_rows.get_or_insert_with(|| {
                            self.host()
                                .map(|mut host| host.call("unit_debug_json", &[]).to_string())
                                .and_then(|json| {
                                    serde_json::from_str::<Vec<serde_json::Value>>(&json).ok()
                                })
                                .unwrap_or_default()
                        });
                        let count = count_unit_rows(rows, unit, team);
                        unit_counts.insert(unit.to_owned(), serde_json::json!(count));
                    }
                    Some(mind_core::io::json::objectives::MapObjective::DestroyBlocks(
                        objective,
                    )) => {
                        let block = objective.block.as_deref().unwrap_or("");
                        let block_id = self
                            .registry
                            .as_ref()
                            .and_then(|registry| registry.block_id(block))
                            .map(|id| i64::from(id.raw()));
                        // `progress()`: a position counts when its building is
                        // gone or belongs to another team/block.
                        let progress = self
                            .host()
                            .map(|mut host| {
                                let mut progress = 0i64;
                                for pos in &objective.positions {
                                    let dict = host
                                        .call(
                                            "building_debug",
                                            &[pos.x.to_variant(), pos.y.to_variant()],
                                        )
                                        .try_to::<VarDictionary>()
                                        .ok();
                                    let read = |key: &str| {
                                        dict.as_ref().and_then(|dict| {
                                            dict.get(&GString::from(key).to_variant())
                                                .and_then(|value| value.try_to::<i64>().ok())
                                        })
                                    };
                                    let intact = read("block").is_some()
                                        && read("block") == block_id
                                        && read("team") == Some(i64::from(objective.team));
                                    if !intact {
                                        progress += 1;
                                    }
                                }
                                progress
                            })
                            .unwrap_or(0);
                        blocks_progress.insert(index.to_string(), serde_json::json!(progress));
                    }
                    _ => {}
                }
            }
        }
        if let Some(runtime) = taken
            && let Some(mut host) = self.host()
        {
            host.bind_mut().put_campaign_runtime(runtime);
        }
        out.insert(
            "unitCounts".to_owned(),
            serde_json::Value::Object(unit_counts),
        );
        out.insert(
            "timerCountup".to_owned(),
            serde_json::Value::Object(timer_countup),
        );
        out.insert(
            "destroyBlocksProgress".to_owned(),
            serde_json::Value::Object(blocks_progress),
        );
        GString::from(serde_json::Value::Object(out).to_string().as_str())
    }

    /// `MapObjectives` world-marker draw list for the running objectives
    /// (view-only; `MindObjectiveMarkers` projects the positions through the
    /// camera).
    ///
    /// Every marker class `Renderer.java:352-360` feeds to
    /// `ObjectiveMarker.draw` is forwarded — `ShapeText`, `Shape`, `Text`,
    /// `Point`, `Line`, `Texture` and `Quad` — behind the `world != -1` gate
    /// (`Renderer.java:356`). `Light` markers only implement `drawLight`
    /// (`MapObjectives.java:1416-1430`; the base `draw` is empty), so they
    /// contribute no world geometry. Text-bearing classes carry their
    /// localized text (`ObjectiveMarker.fetchText`).
    #[func]
    pub fn objective_markers_json(&self) -> GString {
        use mind_core::game::map_markers::{MarkerKind, marker_index};
        use mind_core::io::json::objectives::{ObjectiveMarker as M, TextureHolder};

        fn color_json(color: mind_core::content::Rgba) -> serde_json::Value {
            serde_json::json!([color.r, color.g, color.b, color.a])
        }
        fn holder_json(holder: &TextureHolder) -> serde_json::Value {
            serde_json::json!({
                "string": holder.string,
                "content": holder.content,
                "building": holder.building,
            })
        }

        let mut rows: Vec<serde_json::Value> = Vec::new();
        for index in self.session.objectives.running_indices() {
            let Some(objective) = self.session.objectives.get(index) else {
                continue;
            };
            for marker in &objective.common().markers {
                // `marker.world != -1`: `@IndexBool` is 1/-1, so only -1 hides.
                if marker_index(marker, MarkerKind::World) == -1 {
                    continue;
                }
                let row = match marker {
                    M::Point(m) => serde_json::json!({
                        "class": "Point",
                        "x": m.pos.x,
                        "y": m.pos.y,
                        "radius": m.radius,
                        "stroke": m.stroke,
                        "color": color_json(m.color.0),
                        "autoscale": m.autoscale,
                        "drawLayer": m.draw_layer,
                    }),
                    M::ShapeText(m) => serde_json::json!({
                        "class": "ShapeText",
                        "x": m.pos.x,
                        "y": m.pos.y,
                        "text": self.localized_marker_text(&m.text),
                        "radius": m.radius,
                        "rotation": m.rotation,
                        "sides": m.sides,
                        "color": color_json(m.color.0),
                        "textHeight": m.text_height,
                        "fontSize": m.font_size,
                        "flags": m.flags,
                        "textAlign": m.text_align,
                        "lineAlign": m.line_align,
                        "autoscale": m.autoscale,
                        "drawLayer": m.draw_layer,
                    }),
                    M::Shape(m) => serde_json::json!({
                        "class": "Shape",
                        "x": m.pos.x,
                        "y": m.pos.y,
                        "radius": m.radius,
                        "rotation": m.rotation,
                        "stroke": m.stroke,
                        "startAngle": m.start_angle,
                        "endAngle": m.end_angle,
                        "fill": m.fill,
                        "outline": m.outline,
                        "sides": m.sides,
                        "color": color_json(m.color.0),
                        "autoscale": m.autoscale,
                        "drawLayer": m.draw_layer,
                    }),
                    M::Text(m) => serde_json::json!({
                        "class": "Text",
                        "x": m.pos.x,
                        "y": m.pos.y,
                        "text": self.localized_marker_text(&m.text),
                        "fontSize": m.font_size,
                        "flags": m.flags,
                        "textAlign": m.text_align,
                        "lineAlign": m.line_align,
                        "autoscale": m.autoscale,
                        "drawLayer": m.draw_layer,
                    }),
                    M::Line(m) => serde_json::json!({
                        "class": "Line",
                        "x": m.pos.x,
                        "y": m.pos.y,
                        "endX": m.end_pos.x,
                        "endY": m.end_pos.y,
                        "stroke": m.stroke,
                        "outline": m.outline,
                        "color1": color_json(m.color1.0),
                        "color2": color_json(m.color2.0),
                        "autoscale": m.autoscale,
                        "drawLayer": m.draw_layer,
                    }),
                    M::Texture(m) => serde_json::json!({
                        "class": "Texture",
                        "x": m.pos.x,
                        "y": m.pos.y,
                        "rotation": m.rotation,
                        "width": m.width,
                        "height": m.height,
                        "texture": holder_json(&m.texture),
                        "color": color_json(m.color.0),
                        "autoscale": m.autoscale,
                        "drawLayer": m.draw_layer,
                    }),
                    M::Quad(m) => serde_json::json!({
                        "class": "Quad",
                        "vertices": m.vertices,
                        "texture": holder_json(&m.texture),
                        "autoscale": m.autoscale,
                        "drawLayer": m.draw_layer,
                    }),
                    // `LightMarker`: no world geometry (see the doc comment).
                    M::Light(_) => continue,
                };
                rows.push(row);
            }
        }
        GString::from(serde_json::Value::Array(rows).to_string().as_str())
    }

    /// Fog query placeholder (fog state is plan 12 M7 `FogControl`, not yet
    /// bound to this facade); reports the rules fog flags. The live probe is
    /// `MindSimHost.fog_debug_json` (this facade does not own the `Sim`).
    #[func]
    pub fn fog_query(&self, team: i64, x: i32, y: i32) -> VarDictionary {
        let mut dict = VarDictionary::new();
        dict.set(&GString::from("team"), &team.to_variant());
        dict.set(&GString::from("x"), &x.to_variant());
        dict.set(&GString::from("y"), &y.to_variant());
        dict.set(&GString::from("fog"), &self.session.rules.fog.to_variant());
        dict.set(
            &GString::from("staticFog"),
            &self.session.rules.static_fog.to_variant(),
        );
        dict.set(&GString::from("discovered"), &false.to_variant());
        dict
    }
}

impl MindCampaign {
    /// `ShapeTextMarker.fetchText`: a leading `@` resolves through the live
    /// bundle; Arc markup and `:icon:` tokens are stripped for the plain world
    /// label (atlas-backed icons arrive with the label pass follow-up).
    fn localized_marker_text(&self, raw: &str) -> String {
        let resolved = match raw.trim().strip_prefix('@') {
            Some(key) => {
                let value = self.bundle.get(key);
                if value.is_empty() || value == key {
                    raw.to_owned()
                } else {
                    value.to_owned()
                }
            }
            None => raw.to_owned(),
        };
        // Arc markup and `:icon:` tokens stay in: the world-label pass draws
        // them as coloured runs with content-icon glyphs (`WorldLabel`).
        resolved
    }
}

/// `TeamData.countType(unit)` over the `unit_debug_json` rows: the live units
/// whose content type and team match (`MapObjectives.java:470`). Pure, so the
/// once-per-call hoist in [`MindCampaign::objective_live_stats_json`] keeps an
/// exact-output test (finding #8).
fn count_unit_rows(rows: &[serde_json::Value], unit: &str, team: i64) -> usize {
    rows.iter()
        .filter(|row| {
            row.get("type").and_then(serde_json::Value::as_str) == Some(unit)
                && row.get("team").and_then(serde_json::Value::as_i64) == Some(team)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Finding #8: the `UnitCount` row filter counts live units by content
    /// type and team only (a same-type unit on another team does not count).
    #[test]
    fn unit_count_rows_filter_by_type_and_team() {
        let rows = serde_json::json!([
            {"type": "dagger", "team": 1},
            {"type": "dagger", "team": 1},
            {"type": "dagger", "team": 2},
            {"type": "mace", "team": 1},
            {"type": "dagger"},
            {"type": "dagger", "team": "1"},
        ]);
        let rows = rows.as_array().cloned().unwrap_or_default();
        assert_eq!(count_unit_rows(&rows, "dagger", 1), 2);
        assert_eq!(count_unit_rows(&rows, "dagger", 2), 1);
        assert_eq!(count_unit_rows(&rows, "mace", 1), 1);
        assert_eq!(count_unit_rows(&rows, "flare", 1), 0);
        assert_eq!(count_unit_rows(&[], "dagger", 1), 0);
    }
}
