// SPDX-License-Identifier: GPL-3.0-only

//! Place/break command entry points and the sim command queue.
//! Upstream: core/src/mindustry/input/InputHandler.java (place/break commands)

use super::*;

/// Typed field reads over one parsed JSON command payload (H06).
///
/// Both JSON entry points ([`MindSimHost::apply_sim_command_json`] and
/// [`MindSimHost::submit_player_command_json`]) share these. Each accessor
/// reproduces the exact `serde_json` chain its call sites used, so the accepted
/// shapes and defaults are unchanged (`as_u64` + `as` for the content ids,
/// `as_i64` + `try_from` for the tile coordinates).
struct JsonArgs<'a>(&'a serde_json::Value);

impl<'a> JsonArgs<'a> {
    /// `get(k).and_then(|v| v.as_i64()).and_then(|v| i16::try_from(v).ok())`.
    fn i16(&self, k: &str) -> Option<i16> {
        self.0
            .get(k)
            .and_then(|v| v.as_i64())
            .and_then(|v| i16::try_from(v).ok())
    }

    /// `get(k).and_then(|v| v.as_i64())`.
    fn i64(&self, k: &str) -> Option<i64> {
        self.0.get(k).and_then(|v| v.as_i64())
    }

    /// `get(k).and_then(|v| v.as_u64()).unwrap_or(default) as u16`.
    fn u16_or(&self, k: &str, default: u64) -> u16 {
        self.0.get(k).and_then(|v| v.as_u64()).unwrap_or(default) as u16
    }

    /// `get(k).and_then(|v| v.as_u64()).unwrap_or(default) as u8`.
    fn u8_or(&self, k: &str, default: u64) -> u8 {
        self.0.get(k).and_then(|v| v.as_u64()).unwrap_or(default) as u8
    }

    /// `get(k).and_then(|v| v.as_i64()).unwrap_or(default).clamp(0, 3) as i8`
    /// (the `rotation` quarter-turn field).
    fn rotation(&self, k: &str, default: i64) -> i8 {
        self.0
            .get(k)
            .and_then(|v| v.as_i64())
            .unwrap_or(default)
            .clamp(0, 3) as i8
    }

    /// `get(k).and_then(|v| v.as_f64())`.
    fn f64(&self, k: &str) -> Option<f64> {
        self.0.get(k).and_then(|v| v.as_f64())
    }

    /// `get(k).and_then(|v| v.as_str())`.
    fn str(&self, k: &str) -> Option<&'a str> {
        self.0.get(k).and_then(|v| v.as_str())
    }

    /// `get(k).and_then(|v| v.as_bool())`.
    fn bool(&self, k: &str) -> Option<bool> {
        self.0.get(k).and_then(|v| v.as_bool())
    }

    /// `get(k).and_then(|v| v.as_bool()).unwrap_or(default)`.
    fn bool_or(&self, k: &str, default: bool) -> bool {
        self.0.get(k).and_then(|v| v.as_bool()).unwrap_or(default)
    }

    /// `get(k).and_then(|v| v.as_array()).map(|rows| rows.iter()
    /// .filter_map(|row| row.as_i64().map(|id| id as i32)).collect())
    /// .unwrap_or_default()`.
    fn i32_list(&self, k: &str) -> smallvec::SmallVec<[i32; 32]> {
        self.0
            .get(k)
            .and_then(|v| v.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row.as_i64().map(|id| id as i32))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl MindSimHost {
    /// Whether `(x, y)` now holds `block` anchored there: the tile's block id
    /// matches and the building entity's center tile is `(x, y)` (`T7`, the
    /// `place_block` readback; PAR-0165).
    pub(super) fn tile_holds_block(&self, x: i16, y: i16, block: BlockId) -> bool {
        let pos = mind_core::world::TilePos::new(x, y);
        self.sim.grid.block_at(pos) == Some(block)
            && self
                .sim
                .grid
                .entity_at(pos)
                .and_then(|entity| {
                    self.world()
                        .get::<mind_core::entities::comp::Building>(entity)
                })
                .is_some_and(|building| building.tile == pos)
    }

    /// Enqueues a canonical [`SimCommand`] to apply at the next tick start
    /// (plan 21 §3.5). Rust-only seam used by the `MindNet` relay runtime for
    /// foreign rows and predictions.
    pub fn enqueue_sim_command(&mut self, command: SimCommand) {
        self.pending_commands.push_back(command);
    }

    /// Single outbound seam for player actions (I08 T6): into the relay
    /// mailbox when a match installed one, else the local command queue.
    pub fn submit_player_command(&mut self, command: SimCommand) {
        if let Some(outbox) = self.relay_outbox.as_ref()
            && let Ok(mut queue) = outbox.lock()
        {
            queue.push(command);
            return;
        }
        self.enqueue_sim_command(command);
    }

    /// Installs the relay mailbox (called by `MindNet` while a match is
    /// active).
    pub fn install_relay_outbox(
        &mut self,
        outbox: std::sync::Arc<std::sync::Mutex<Vec<SimCommand>>>,
    ) {
        self.relay_outbox = Some(outbox);
    }

    /// Removes the relay mailbox (single-player again).
    pub fn clear_relay_outbox(&mut self) {
        self.relay_outbox = None;
    }

    /// Runs `f` against the live content registry (relay name resolution and
    /// the content hash; I08 T4/T6). `None` before a registry is installed.
    pub fn with_registry<R>(
        &self,
        f: impl FnOnce(&mind_core::content::ContentRegistry) -> R,
    ) -> Option<R> {
        self.content_snapshot.as_ref().map(f)
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Places a block at `(x, y)` immediately (API path; MCP §7c step 5).
    #[func]
    pub fn place_block(&mut self, x: i32, y: i32, block: GString) -> bool {
        let name = block.to_string();
        let id = match self.sim.content().id(&name) {
            Ok(id) => id,
            Err(_) => {
                log::warn!("place_block: unknown block `{name}`");
                return false;
            }
        };
        let Some((x, y)) = crate::gdutil::coords::tile_i16(x, y) else {
            log::warn!("place_block: ({x}, {y}) does not fit in i16");
            return false;
        };
        let applied = self.apply_command(Command::Place { x, y, block: id });
        // PAR-0165 (T7): `apply_command` returns `Ok` even when the live
        // runtime rejected the placement, so read the tile back and only
        // report success when the block really sits anchored at `(x, y)`.
        let placed = applied && self.tile_holds_block(x, y, id);
        if placed {
            self.mark_world_changed();
        }
        placed
    }

    /// Breaks the block at `(x, y)` immediately (API path; MCP §7c step 5).
    #[func]
    pub fn break_block(&mut self, x: i32, y: i32) -> bool {
        let Some((x, y)) = crate::gdutil::coords::tile_i16(x, y) else {
            log::warn!("break_block: ({x}, {y}) does not fit in i16");
            return false;
        };
        let applied = self.apply_command(Command::Break { x, y });
        if applied {
            self.mark_world_changed();
        }
        applied
    }

    /// Places `block` at `(x, y)` with an explicit rotation (`0..3`) through the
    /// same live-runtime path as [`Self::place_block`]. `Command::Place` always
    /// carries rotation 0, so rotated belts/conduits need this test seam
    /// (MCP §7c has no rotation field).
    #[func]
    pub fn place_block_rot(&mut self, x: i32, y: i32, block: GString, rot: i32) -> bool {
        let name = block.to_string();
        let id = match self.sim.content().id(&name) {
            Ok(id) => id,
            Err(_) => {
                log::warn!("place_block_rot: unknown block `{name}`");
                return false;
            }
        };
        let Some((x, y)) = crate::gdutil::coords::tile_i16(x, y) else {
            log::warn!("place_block_rot: ({x}, {y}) does not fit in i16");
            return false;
        };
        let team = self.sim.live_default_team();
        let applied = self
            .sim
            .place_for_team(x, y, id, rot.clamp(0, 3) as u8, team);
        let placed = applied && self.tile_holds_block(x, y, id);
        if placed {
            self.emit_state();
            self.emit_world_changed();
        }
        placed
    }

    /// Placement ghost validity (`Build.validPlace(type, player.team(), x, y,
    /// rot)`, `InputHandler.java:1455`); H08 consumes it. Out-of-range ints and
    /// unknown block names return `false`.
    #[func]
    pub fn valid_place_preview(&self, x: i32, y: i32, block: GString, rotation: i32) -> bool {
        let name = block.to_string();
        let Ok(id) = self.sim.content().id(&name) else {
            log::debug!("valid_place_preview: unknown block `{name}`");
            return false;
        };
        let Some((x, y)) = crate::gdutil::coords::tile_i16(x, y) else {
            return false;
        };
        self.sim
            .valid_place_player(x, y, id, rotation.clamp(0, 3) as u8)
    }

    /// Number of relay commands queued for the next tick boundary.
    #[func]
    pub fn pending_command_count(&self) -> i64 {
        self.pending_commands.len() as i64
    }

    /// MCP/dev helper: parse a small JSON command and enqueue it.
    ///
    /// Shape: `{"op":"place","x":1,"y":2,"block":<u16>,"rotation":0}` or
    /// `{"op":"break","x":1,"y":2}`.
    #[func]
    pub fn apply_sim_command_json(&mut self, cmd_json: GString) -> bool {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&cmd_json.to_string()) else {
            log::warn!("apply_sim_command_json: invalid JSON");
            return false;
        };
        let args = JsonArgs(&value);
        let op = args.str("op").unwrap_or("");
        let (x, y) = (args.i16("x"), args.i16("y"));
        // Optional issuing player: `Some` routes place/break through the
        // player unit's build queue like live input (PAR-0098).
        let player = args.i64("player").map(|id| id as i32);
        let command = match op {
            "place" => {
                let (Some(x), Some(y)) = (x, y) else {
                    log::warn!("apply_sim_command_json: place needs x,y");
                    return false;
                };
                let block = args.u16_or("block", 0);
                let rotation = args.rotation("rotation", 0);
                // The staged team (when given) drives the pre-check so enemy
                // staging works; otherwise the live default team is used.
                let team = args.u8_or("team", 0);
                let check_team = if value.get("team").is_some() {
                    team
                } else {
                    self.sim.live_default_team()
                };
                // PAR-0165 (T7): reject invalid placements at enqueue time so
                // callers get an honest `false` instead of a silently dropped
                // command (`Sim::command` logs rejections at debug level).
                if !self.sim.can_place_for_team(
                    x,
                    y,
                    BlockId::new(block),
                    rotation as u8,
                    check_team,
                ) {
                    log::warn!("apply_sim_command_json: place rejected at ({x}, {y})");
                    return false;
                }
                SimCommand::Place {
                    x,
                    y,
                    block,
                    rotation,
                    team,
                    player,
                }
            }
            "break" => {
                let (Some(x), Some(y)) = (x, y) else {
                    log::warn!("apply_sim_command_json: break needs x,y");
                    return false;
                };
                SimCommand::Break { x, y, player }
            }
            "spawn_unit" => {
                let unit = args.u16_or("unit", 0);
                let team = args.u8_or("team", 1);
                let fx = args.f64("px").or_else(|| x.map(f64::from)).unwrap_or(0.0);
                let fy = args.f64("py").or_else(|| y.map(f64::from)).unwrap_or(0.0);
                SimCommand::SpawnUnit {
                    unit,
                    x: fx as f32,
                    y: fy as f32,
                    team,
                }
            }
            "unit_command" => {
                let units: smallvec::SmallVec<[i32; 32]> = args.i32_list("units");
                if units.is_empty() {
                    log::warn!("apply_sim_command_json: unit_command without units");
                    return false;
                }
                let command = args.u16_or("command", 0);
                // `unit_command` targets: `x`/`y` are the raw wire pair (world
                // pixels for a move, or the lossless `pack_wire_target` pair);
                // the optional `target_unit` key packs an EntitySeq attack
                // target for recipes (H03 blocker #2: ids do not fit an f32).
                let fx = args.f64("x").unwrap_or(0.0);
                let fy = args.f64("y").unwrap_or(0.0);
                let (x, y) = args
                    .i64("target_unit")
                    .map(|id| {
                        mind_core::sim::pack_wire_target(
                            &mind_core::input::action::CommandTarget::Unit(id as i32),
                        )
                    })
                    .unwrap_or((fx as f32, fy as f32));
                SimCommand::UnitCommand {
                    units,
                    command,
                    x,
                    y,
                    // `queue` (I01 T13): append instead of replace.
                    queue: args.bool_or("queue", false),
                }
            }
            // `InputHandler.setUnitStance` (G01 T11): `units` + `stance` id in
            // `content_list("unitStance")` order, `enabled`.
            "unit_stance" => {
                let units: smallvec::SmallVec<[i32; 32]> = args.i32_list("units");
                let stance = args.u16_or("stance", 0);
                let enabled = args.bool_or("enabled", true);
                SimCommand::UnitStance {
                    units,
                    stance,
                    enabled,
                }
            }
            // `InputHandler.unitControl`/`unitClear` (G01 T9).
            "unit_control" => SimCommand::UnitControl {
                unit: args.i64("unit").map(|v| v as i32),
            },
            "unit_clear" => SimCommand::UnitClear,
            other => {
                log::warn!("apply_sim_command_json: unsupported op `{other}`");
                return false;
            }
        };
        self.pending_commands.push_back(command);
        true
    }

    /// MCP/test helper: parse a small JSON command and route it through the
    /// single outbound seam [`Self::submit_player_command`] (I08 T6), so the
    /// relay path can be driven in-engine.
    ///
    /// Shape: `{"op":"place","x":1,"y":2,"block":<u16>,"rotation":0}`,
    /// `{"op":"break","x":1,"y":2}`,
    /// `{"op":"configure","x":1,"y":2,"content":<name>}` (or `"number"`/
    /// `"bool"`/`"text"`),
    /// `{"op":"rotate","x":1,"y":2,"direction":false}`,
    /// `{"op":"delete_plans","positions":[...]}`.
    #[func]
    pub fn submit_player_command_json(&mut self, cmd_json: GString) -> bool {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&cmd_json.to_string()) else {
            log::warn!("submit_player_command_json: invalid JSON");
            return false;
        };
        let args = JsonArgs(&value);
        let op = args.str("op").unwrap_or("");
        let command = match op {
            "place" => {
                let (Some(x), Some(y)) = (args.i16("x"), args.i16("y")) else {
                    return false;
                };
                SimCommand::Place {
                    x,
                    y,
                    block: args.u16_or("block", 0),
                    rotation: args.rotation("rotation", 0),
                    team: args.u8_or("team", 0),
                    player: Some(0),
                }
            }
            "break" => {
                let (Some(x), Some(y)) = (args.i16("x"), args.i16("y")) else {
                    return false;
                };
                SimCommand::Break {
                    x,
                    y,
                    player: Some(0),
                }
            }
            "configure" => {
                use mind_core::determinism::ConfigValue;
                let (Some(x), Some(y)) = (args.i16("x"), args.i16("y")) else {
                    return false;
                };
                let config = if let Some(name) = args.str("content") {
                    ConfigValue::Content(name.to_string())
                } else if let Some(number) = args.f64("number") {
                    // `to_sim_config` (`command_emit.rs:268-277`): integral in
                    // i32 range -> Int, otherwise Float.
                    if number.fract() == 0.0
                        && number >= f64::from(i32::MIN)
                        && number <= f64::from(i32::MAX)
                    {
                        ConfigValue::Int(number as i32)
                    } else {
                        ConfigValue::Float(number as f32)
                    }
                } else if let Some(flag) = args.bool("bool") {
                    ConfigValue::Bool(flag)
                } else if let Some(text) = args.str("text") {
                    ConfigValue::Text(text.to_string())
                } else {
                    log::warn!("submit_player_command_json: configure needs a value");
                    return false;
                };
                SimCommand::Configure {
                    x,
                    y,
                    value: config,
                }
            }
            "rotate" => {
                let (Some(x), Some(y)) = (args.i16("x"), args.i16("y")) else {
                    return false;
                };
                SimCommand::Rotate {
                    x,
                    y,
                    direction: args.bool_or("direction", false),
                }
            }
            "delete_plans" => {
                let positions: smallvec::SmallVec<[i32; 32]> = args.i32_list("positions");
                SimCommand::DeletePlans { positions }
            }
            other => {
                log::warn!("submit_player_command_json: unsupported op `{other}`");
                return false;
            }
        };
        self.submit_player_command(command);
        true
    }
}
