// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: plan 00 §3.5/§3.10 spine inspector (no upstream Java file).

//! Read-only state-inspector overlay for the M4 spine.
//!
//! Reads the cheap `MindSimHost` accessors (tick, checksum, update id, phase,
//! selected block, group counts) every 250 ms and on `state_changed` while
//! visible. The full `get_state_json()` dump is deliberately not used: it
//! serializes the whole grid and entity list (megabytes per call) and would
//! stall the frame loop. This node never writes sim state.

use godot::classes::{Control, IControl, ItemList, Label, Texture2D, TextureRect};
use godot::prelude::*;

/// Poll interval in seconds (plan §3.10 extension contract).
const POLL_INTERVAL: f64 = 0.25;

/// `MindStateInspector` — the debug spine overlay.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindStateInspector {
    base: Base<Control>,
    /// Path to the sim host; overridable when this scene is reused elsewhere.
    #[export]
    pub host_path: NodePath,
    /// Path to the camera used for the cursor-tile readout.
    #[export]
    pub camera_path: NodePath,
    /// Path to the STDB autoload whose `net` page this inspector renders.
    #[export]
    pub net_path: NodePath,
    /// Path to the plan-18 audio driver (`/root/Spine/MindAudio`).
    #[export]
    pub audio_path: NodePath,
    /// Path to the plan-19 editor facade (`/root/Spine/MindEditor`).
    #[export]
    pub editor_path: NodePath,
    /// Content type listed in `ContentList`, e.g. `item`, `block`.
    #[export]
    pub content_type: GString,
    host: Option<Gd<Node>>,
    camera: Option<Gd<Node>>,
    elapsed: f64,
}

#[godot_api]
impl MindStateInspector {
    fn label(&self, path: &str) -> Option<Gd<Label>> {
        self.base().get_node_or_null(path)?.try_cast::<Label>().ok()
    }

    fn item_list(&self, path: &str) -> Option<Gd<ItemList>> {
        self.base()
            .get_node_or_null(path)?
            .try_cast::<ItemList>()
            .ok()
    }

    fn preview(&self, path: &str) -> Option<Gd<TextureRect>> {
        self.base()
            .get_node_or_null(path)?
            .try_cast::<TextureRect>()
            .ok()
    }

    fn host_node(&self) -> Option<Gd<Node>> {
        self.host.clone()
    }

    fn refresh(&mut self) {
        let Some(mut host) = self.host_node() else {
            return;
        };
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("tick: {}", host.call("get_tick", &[]).to::<i64>()));
        lines.push(format!(
            "update: {}  state: {}",
            host.call("get_update_id", &[]).to::<i64>(),
            host.call("get_state", &[]).to::<GString>(),
        ));
        lines.push(format!(
            "paused: {}",
            if host.call("is_paused", &[]).to::<bool>() {
                "true"
            } else {
                "false"
            }
        ));
        lines.push(format!(
            "selected: {}",
            host.call("selected_block", &[]).to::<GString>()
        ));
        lines.push(format!("cursor: {}", self.cursor_tile_text()));
        lines.push(format!(
            "checksum: {}",
            host.call("get_checksum", &[]).to::<GString>()
        ));
        lines.push(format!("groups: {}", Self::group_counts_text(&mut host)));
        if let Some(mut label) = self.label("Label") {
            label.set_text(&lines.join("\n"));
        }
        self.refresh_audio();
        self.refresh_net();
        self.refresh_editor();
    }

    /// Live per-group entity counts (plan 05 M9 inspector surface).
    fn group_counts_text(host: &mut Gd<Node>) -> String {
        let counts = host.call("get_group_counts", &[]).to::<VarDictionary>();
        let mut parts: Vec<String> = Vec::new();
        for key in ["all", "build", "unit", "bullet", "player"] {
            if let Some(value) = counts.get(key) {
                parts.push(format!("{key}={}", value.to::<i64>()));
            }
        }
        parts.join(" ")
    }

    /// Plan-18 §3.10 `Audio` row: bus/music/lowpass/voice state.
    fn refresh_audio(&mut self) {
        let Some(mut audio) = self.base().get_node_or_null(&self.audio_path.clone()) else {
            if let Some(mut label) = self.label("Audio") {
                label.set_text("audio: -");
            }
            return;
        };
        if !audio.has_method("stats") {
            if let Some(mut label) = self.label("Audio") {
                label.set_text("audio: -");
            }
            return;
        }
        let stats = audio.call("stats", &[]).to::<VarDictionary>();
        let paused = stats
            .get("sound_paused")
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        let track = stats
            .get("current_track")
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        let text = format!(
            "audio: {}  track={}  wet={:.2}  voices={}  loops={}",
            if paused { "paused" } else { "play" },
            if track.is_empty() {
                "none".to_string()
            } else {
                track.to_string()
            },
            stats
                .get("lowpass_wet")
                .map(|value| value.to::<f32>())
                .unwrap_or(0.0),
            stats
                .get("voices")
                .map(|value| value.to::<i64>())
                .unwrap_or(0),
            stats
                .get("loop_sounds")
                .map(|value| value.to::<i64>())
                .unwrap_or(0),
        );
        if let Some(mut label) = self.label("Audio") {
            label.set_text(&text);
        }
    }

    /// Plan-19 M3 inspector `Editor` tab: tool/brush/selection/op stack.
    fn refresh_editor(&mut self) {
        let Some(mut editor) = self.base().get_node_or_null(&self.editor_path.clone()) else {
            if let Some(mut label) = self.label("Editor") {
                label.set_text("editor: -");
            }
            return;
        };
        if !editor.has_method("status") {
            if let Some(mut label) = self.label("Editor") {
                label.set_text("editor: -");
            }
            return;
        }
        let status = editor.call("status", &[]).to::<VarDictionary>();
        let text = format!(
            "editor: {}  brush={:.1}  block={}\nops={} undo={} redo={}  {}x{}",
            status
                .get("tool")
                .map(|value| value.to::<GString>())
                .unwrap_or_default(),
            status
                .get("brush_size")
                .map(|value| value.to::<f32>())
                .unwrap_or(1.0),
            status
                .get("draw_block")
                .map(|value| value.to::<GString>())
                .unwrap_or_default(),
            status
                .get("retained_ops")
                .map(|value| value.to::<i64>())
                .unwrap_or(0),
            if status
                .get("can_undo")
                .map(|value| value.to::<bool>())
                .unwrap_or(false)
            {
                "y"
            } else {
                "n"
            },
            if status
                .get("can_redo")
                .map(|value| value.to::<bool>())
                .unwrap_or(false)
            {
                "y"
            } else {
                "n"
            },
            status
                .get("width")
                .map(|value| value.to::<i64>())
                .unwrap_or(0),
            status
                .get("height")
                .map(|value| value.to::<i64>())
                .unwrap_or(0),
        );
        if let Some(mut label) = self.label("Editor") {
            label.set_text(&text);
        }
    }

    /// `net` page (plan 01 §3.11): connector state, wave flags, relay counters.
    fn refresh_net(&mut self) {
        let Some(mut connector) = self.base().get_node_or_null(&self.net_path.clone()) else {
            if let Some(mut label) = self.label("NetLabel") {
                label.set_text("net: no StdbConnector autoload");
            }
            return;
        };
        let identity = connector.call("local_identity_hex", &[]).to::<GString>();
        let identity_text = if identity.is_empty() {
            "-".to_string()
        } else {
            identity.to_string().chars().take(8).collect::<String>()
        };
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!(
            "net: {}  {}",
            connector.call("state", &[]).to::<GString>(),
            identity_text
        ));
        lines.push(format!(
            "waves: base={} lobby={} game={}",
            wave_text(&mut connector, "base"),
            wave_text(&mut connector, "lobby"),
            wave_text(&mut connector, "game"),
        ));
        let order_error = connector.call("relay_order_error", &[]).to::<GString>();
        lines.push(format!(
            "relay: applied={} last={} err={}",
            connector.call("dev_relay_applied_count", &[]).to::<i64>(),
            connector.call("dev_last_command_id", &[]).to::<i64>(),
            if order_error.is_empty() {
                "-".to_string()
            } else {
                order_error.to_string()
            },
        ));
        if let Some(mut label) = self.label("NetLabel") {
            label.set_text(&lines.join("\n"));
        }
    }

    /// Content registry readout (plan 02 §3.7): per-type counts plus the ordered
    /// entries of `content_type`.
    fn refresh_content(&mut self) {
        let Some(mut host) = self.host_node() else {
            return;
        };
        let counts = host.call("content_counts", &[]).to::<VarDictionary>();
        if counts.is_empty() {
            if let Some(mut label) = self.label("ContentCounts") {
                label.set_text("content: -");
            }
            if let Some(mut list) = self.item_list("ContentList") {
                list.clear();
            }
            return;
        }
        let mut lines: Vec<String> = vec!["content".to_string()];
        for key in counts.keys_array().iter_shared() {
            if let Some(value) = counts.get(&key.to::<GString>()) {
                lines.push(format!("{}: {}", key.to::<GString>(), value.to::<i64>()));
            }
        }
        if let Some(mut label) = self.label("ContentCounts") {
            label.set_text(&lines.join("\n"));
        }
        let names = host
            .call("content_list", &[self.content_type.clone().to_variant()])
            .to::<PackedStringArray>();
        if let Some(mut list) = self.item_list("ContentList") {
            list.clear();
            for name in names.as_slice() {
                list.add_item(name);
            }
        }
    }

    fn cursor_tile_text(&mut self) -> String {
        if self.camera.is_none() {
            self.camera = self.base().get_node_or_null(&self.camera_path.clone());
        }
        let Some(mut camera) = self.camera.clone() else {
            return "-".to_string();
        };
        let Some(viewport) = self.base().get_viewport() else {
            return "-".to_string();
        };
        let mouse = viewport.get_mouse_position();
        if !camera.has_method("screen_to_tile") {
            return "-".to_string();
        }
        let tile = camera
            .call(
                "screen_to_tile",
                &[mouse.x.to_variant(), mouse.y.to_variant()],
            )
            .try_to::<Vector2i>()
            .unwrap_or_default();
        format!("({}, {})", tile.x, tile.y)
    }

    /// Resolves the `MindAssets` autoload (plan 03 M5).
    fn assets_node(&self) -> Option<Gd<Node>> {
        self.base().get_node_or_null("/root/MindAssets")
    }

    /// M5 inspector fixture: displays an atlas region's texture.
    #[func]
    pub fn show_region(&mut self, region_name: GString) -> bool {
        let Some(mut assets) = self.assets_node() else {
            return false;
        };
        let texture = assets
            .call("find_region", &[region_name.to_variant()])
            .try_to::<Gd<Texture2D>>()
            .ok();
        let Some(texture) = texture else {
            return false;
        };
        if let Some(mut preview) = self.preview("RegionPreview") {
            preview.set_texture(&texture);
        }
        true
    }

    /// M5 inspector fixture: displays a content UI icon by looking up the
    /// generated `ui/<type>-<name>-ui` region.
    #[func]
    pub fn show_icon(&mut self, icon_name: GString) -> bool {
        for prefix in ["block", "unit", "item", "liquid", "status"] {
            let region = GString::from(format!("ui/{prefix}-{}-ui", icon_name).as_str());
            if self.show_region(region) {
                return true;
            }
        }
        false
    }

    #[func]
    fn on_state_changed(&mut self, _tick: i64, _checksum: GString) {
        if self.base().is_visible() {
            self.refresh();
        }
    }

    #[func]
    fn on_visibility_changed(&mut self) {
        if !self.base().is_visible() {
            return;
        }
        self.elapsed = 0.0;
        self.refresh();
        self.refresh_content();
    }
}

fn wave_text(connector: &mut Gd<Node>, wave: &str) -> &'static str {
    if connector
        .call("wave_applied_state", &[GString::from(wave).to_variant()])
        .to::<bool>()
    {
        "on"
    } else {
        "off"
    }
}

#[godot_api]
impl IControl for MindStateInspector {
    fn ready(&mut self) {
        self.host = self.base().get_node_or_null(&self.host_path.clone());
        if self.host.is_none() {
            self.host = self.base().get_node_or_null("/root/Spine/SimHost");
        }
        if let Some(mut host) = self.host.clone() {
            let callable = self.base().callable("on_state_changed");
            if !host.is_connected("state_changed", &callable) {
                host.connect("state_changed", &callable);
            }
        }
        self.camera = self.base().get_node_or_null(&self.camera_path.clone());
        if self.camera.is_none() {
            self.camera = self.base().get_node_or_null("/root/Spine/World/Camera2D");
        }
        let callable = self.base().callable("on_visibility_changed");
        self.base_mut().connect("visibility_changed", &callable);
        if self.base().is_visible() {
            self.refresh();
            self.refresh_content();
        }
    }

    fn process(&mut self, delta: f64) {
        // A hidden overlay must not poll: the refresh is debug-only.
        if !self.base().is_visible() {
            return;
        }
        self.elapsed += delta;
        if self.elapsed >= POLL_INTERVAL {
            self.elapsed = 0.0;
            self.refresh();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_interval_is_250_ms() {
        assert_eq!(POLL_INTERVAL, 0.25);
    }

    #[test]
    fn kit_helpers_do_not_panic_without_autoloads() {
        use crate::ui::kit;
        // Pure helper smoke: no scene tree in unit tests, so lookups return None.
        assert!(kit::ui().is_none());
    }
}
