// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Binding` / `KeyBind` registry (`core/src/mindustry/input/Binding.java`).
//!
//! Plan 15 §6.1: the binding names/defaults/categories/axis-ness are a parity
//! ABI (settings keys and bundle keys). Values are client-local and live in a
//! [`BindingState`] (never sim state, never checksummed — deviation I3).

use crate::io::SettingsStore;

/// Numeric id of a registered key binding (its index in [`KeyBindTable::all`]).
pub type KeyBindId = u16;

/// A key binding is either a single key or an axis with a negative/positive key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    /// Single key (or mouse button) binding.
    Key,
    /// Axis binding (`negative`/`positive`).
    Axis,
}

/// Bundle category (`category.<name>.name`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// `general`.
    General,
    /// `command`.
    Command,
    /// `blocks`.
    Blocks,
    /// `view`.
    View,
    /// `multiplayer`.
    Multiplayer,
}

impl Category {
    /// Upstream category name (bundle ABI).
    pub const fn name(self) -> &'static str {
        match self {
            Category::General => "general",
            Category::Command => "command",
            Category::Blocks => "blocks",
            Category::View => "view",
            Category::Multiplayer => "multiplayer",
        }
    }

    /// All categories in `Category.values()` order.
    pub const ALL: [Category; 5] = [
        Category::General,
        Category::Command,
        Category::Blocks,
        Category::View,
        Category::Multiplayer,
    ];
}

/// The upstream default for a binding (`KeyCode.unset` → `None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingDefault {
    /// A single key/mouse name, or unset.
    Key(Option<&'static str>),
    /// An axis with optional negative/positive key names.
    Axis {
        /// Negative direction key.
        negative: Option<&'static str>,
        /// Positive direction key.
        positive: Option<&'static str>,
    },
}

/// One registered binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBind {
    /// Settings/bundle name.
    pub name: &'static str,
    /// Bundle category (`None` for the debug-only entries).
    pub category: Option<Category>,
    /// Key or axis.
    pub kind: KeyKind,
    /// Upstream default.
    pub default: BindingDefault,
}

impl KeyBind {
    /// Bundle key for the localized display name (`keybind.<name>.name`).
    pub fn bundle_key(&self) -> String {
        format!("keybind.{}.name", self.name)
    }
}

/// Ids for every binding, in [`KeyBindTable::all`] order (parity ABI).
pub mod ids {
    use super::KeyBindId;

    /// `move_x`.
    pub const MOVE_X: KeyBindId = 0;
    /// `move_y`.
    pub const MOVE_Y: KeyBindId = 1;
    /// `mouse_move`.
    pub const MOUSE_MOVE: KeyBindId = 2;
    /// `pan`.
    pub const PAN: KeyBindId = 3;
    /// `boost`.
    pub const BOOST: KeyBindId = 4;
    /// `respawn`.
    pub const RESPAWN: KeyBindId = 5;
    /// `control`.
    pub const CONTROL: KeyBindId = 6;
    /// `select`.
    pub const SELECT: KeyBindId = 7;
    /// `deselect`.
    pub const DESELECT: KeyBindId = 8;
    /// `break_block`.
    pub const BREAK_BLOCK: KeyBindId = 9;
    /// `pickupCargo`.
    pub const PICKUP_CARGO: KeyBindId = 10;
    /// `dropCargo`.
    pub const DROP_CARGO: KeyBindId = 11;
    /// `clear_building`.
    pub const CLEAR_BUILDING: KeyBindId = 12;
    /// `pause_building`.
    pub const PAUSE_BUILDING: KeyBindId = 13;
    /// `rotate`.
    pub const ROTATE: KeyBindId = 14;
    /// `rotateplaced`.
    pub const ROTATE_PLACED: KeyBindId = 15;
    /// `diagonal_placement`.
    pub const DIAGONAL_PLACEMENT: KeyBindId = 16;
    /// `pick`.
    pub const PICK: KeyBindId = 17;
    /// `ping`.
    pub const PING: KeyBindId = 18;
    /// `rebuild_select`.
    pub const REBUILD_SELECT: KeyBindId = 19;
    /// `schematic_select`.
    pub const SCHEMATIC_SELECT: KeyBindId = 20;
    /// `schematic_flip_x`.
    pub const SCHEMATIC_FLIP_X: KeyBindId = 21;
    /// `schematic_flip_y`.
    pub const SCHEMATIC_FLIP_Y: KeyBindId = 22;
    /// `schematic_menu`.
    pub const SCHEMATIC_MENU: KeyBindId = 23;
    /// `command_mode`.
    pub const COMMAND_MODE: KeyBindId = 24;
    /// `command_queue`.
    pub const COMMAND_QUEUE: KeyBindId = 25;
    /// `create_control_group`.
    pub const CREATE_CONTROL_GROUP: KeyBindId = 26;
    /// `select_all_units`.
    pub const SELECT_ALL_UNITS: KeyBindId = 27;
    /// `select_all_unit_factories`.
    pub const SELECT_ALL_UNIT_FACTORIES: KeyBindId = 28;
    /// `select_all_unit_transport`.
    pub const SELECT_ALL_UNIT_TRANSPORT: KeyBindId = 29;
    /// `select_across_screen`.
    pub const SELECT_ACROSS_SCREEN: KeyBindId = 30;
    /// `cancel_orders`.
    pub const CANCEL_ORDERS: KeyBindId = 31;
    /// `unit_stance_hold_fire`.
    pub const UNIT_STANCE_HOLD_FIRE: KeyBindId = 32;
    /// `unit_stance_pursue_target`.
    pub const UNIT_STANCE_PURSUE_TARGET: KeyBindId = 33;
    /// `unit_stance_patrol`.
    pub const UNIT_STANCE_PATROL: KeyBindId = 34;
    /// `unit_stance_ram`.
    pub const UNIT_STANCE_RAM: KeyBindId = 35;
    /// `unit_stance_boost`.
    pub const UNIT_STANCE_BOOST: KeyBindId = 36;
    /// `unit_stance_hold_position`.
    pub const UNIT_STANCE_HOLD_POSITION: KeyBindId = 37;
    /// `unit_command_move`.
    pub const UNIT_COMMAND_MOVE: KeyBindId = 38;
    /// `unit_command_repair`.
    pub const UNIT_COMMAND_REPAIR: KeyBindId = 39;
    /// `unit_command_rebuild`.
    pub const UNIT_COMMAND_REBUILD: KeyBindId = 40;
    /// `unit_command_assist`.
    pub const UNIT_COMMAND_ASSIST: KeyBindId = 41;
    /// `unit_command_mine`.
    pub const UNIT_COMMAND_MINE: KeyBindId = 42;
    /// `unit_command_enter_payload`.
    pub const UNIT_COMMAND_ENTER_PAYLOAD: KeyBindId = 43;
    /// `unit_command_load_units`.
    pub const UNIT_COMMAND_LOAD_UNITS: KeyBindId = 44;
    /// `unit_command_load_blocks`.
    pub const UNIT_COMMAND_LOAD_BLOCKS: KeyBindId = 45;
    /// `unit_command_unload_payload`.
    pub const UNIT_COMMAND_UNLOAD_PAYLOAD: KeyBindId = 46;
    /// `unit_command_loop_payload`.
    pub const UNIT_COMMAND_LOOP_PAYLOAD: KeyBindId = 47;
    /// `category_prev`.
    pub const CATEGORY_PREV: KeyBindId = 48;
    /// `category_next`.
    pub const CATEGORY_NEXT: KeyBindId = 49;
    /// `block_select_left`.
    pub const BLOCK_SELECT_LEFT: KeyBindId = 50;
    /// `block_select_right`.
    pub const BLOCK_SELECT_RIGHT: KeyBindId = 51;
    /// `block_select_up`.
    pub const BLOCK_SELECT_UP: KeyBindId = 52;
    /// `block_select_down`.
    pub const BLOCK_SELECT_DOWN: KeyBindId = 53;
    /// `block_select_01`.
    pub const BLOCK_SELECT_01: KeyBindId = 54;
    /// `block_select_02`.
    pub const BLOCK_SELECT_02: KeyBindId = 55;
    /// `block_select_03`.
    pub const BLOCK_SELECT_03: KeyBindId = 56;
    /// `block_select_04`.
    pub const BLOCK_SELECT_04: KeyBindId = 57;
    /// `block_select_05`.
    pub const BLOCK_SELECT_05: KeyBindId = 58;
    /// `block_select_06`.
    pub const BLOCK_SELECT_06: KeyBindId = 59;
    /// `block_select_07`.
    pub const BLOCK_SELECT_07: KeyBindId = 60;
    /// `block_select_08`.
    pub const BLOCK_SELECT_08: KeyBindId = 61;
    /// `block_select_09`.
    pub const BLOCK_SELECT_09: KeyBindId = 62;
    /// `block_select_10`.
    pub const BLOCK_SELECT_10: KeyBindId = 63;
    /// `zoom`.
    pub const ZOOM: KeyBindId = 64;
    /// `detach_camera`.
    pub const DETACH_CAMERA: KeyBindId = 65;
    /// `teleport_cursor`.
    pub const TELEPORT_CURSOR: KeyBindId = 66;
    /// `menu`.
    pub const MENU: KeyBindId = 67;
    /// `fullscreen`.
    pub const FULLSCREEN: KeyBindId = 68;
    /// `pause`.
    pub const PAUSE: KeyBindId = 69;
    /// `skip_wave`.
    pub const SKIP_WAVE: KeyBindId = 70;
    /// `minimap`.
    pub const MINIMAP: KeyBindId = 71;
    /// `research`.
    pub const RESEARCH: KeyBindId = 72;
    /// `planet_map`.
    pub const PLANET_MAP: KeyBindId = 73;
    /// `block_info`.
    pub const BLOCK_INFO: KeyBindId = 74;
    /// `toggle_menus`.
    pub const TOGGLE_MENUS: KeyBindId = 75;
    /// `screenshot`.
    pub const SCREENSHOT: KeyBindId = 76;
    /// `toggle_power_lines`.
    pub const TOGGLE_POWER_LINES: KeyBindId = 77;
    /// `toggle_block_status`.
    pub const TOGGLE_BLOCK_STATUS: KeyBindId = 78;
    /// `player_list`.
    pub const PLAYER_LIST: KeyBindId = 79;
    /// `chat`.
    pub const CHAT: KeyBindId = 80;
    /// `chat_history_prev`.
    pub const CHAT_HISTORY_PREV: KeyBindId = 81;
    /// `chat_history_next`.
    pub const CHAT_HISTORY_NEXT: KeyBindId = 82;
    /// `chat_scroll`.
    pub const CHAT_SCROLL: KeyBindId = 83;
    /// `chat_mode`.
    pub const CHAT_MODE: KeyBindId = 84;
    /// `console`.
    pub const CONSOLE: KeyBindId = 85;
    /// `debug_hitboxes`.
    pub const DEBUG_HITBOXES: KeyBindId = 86;
    /// `performance_metrics`.
    pub const PERFORMANCE_METRICS: KeyBindId = 87;
}

#[cfg(test)]
use ids::*;

/// The full upstream registry (append-only; names are settings/bundle keys).
pub const BINDS: &[KeyBind] = &[
    KeyBind {
        name: "move_x",
        category: Some(Category::General),
        kind: KeyKind::Axis,
        default: BindingDefault::Axis {
            negative: Some("a"),
            positive: Some("d"),
        },
    },
    KeyBind {
        name: "move_y",
        category: Some(Category::General),
        kind: KeyKind::Axis,
        default: BindingDefault::Axis {
            negative: Some("s"),
            positive: Some("w"),
        },
    },
    KeyBind {
        name: "mouse_move",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("mouseBack")),
    },
    KeyBind {
        name: "pan",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("mouseForward")),
    },
    KeyBind {
        name: "boost",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("shiftLeft")),
    },
    KeyBind {
        name: "respawn",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("v")),
    },
    KeyBind {
        name: "control",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("controlLeft")),
    },
    KeyBind {
        name: "select",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("mouseLeft")),
    },
    KeyBind {
        name: "deselect",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("mouseRight")),
    },
    KeyBind {
        name: "break_block",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("mouseRight")),
    },
    KeyBind {
        name: "pickupCargo",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("leftBracket")),
    },
    KeyBind {
        name: "dropCargo",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("rightBracket")),
    },
    KeyBind {
        name: "clear_building",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("q")),
    },
    KeyBind {
        name: "pause_building",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("e")),
    },
    KeyBind {
        name: "rotate",
        category: Some(Category::General),
        kind: KeyKind::Axis,
        default: BindingDefault::Axis {
            negative: None,
            positive: Some("scroll"),
        },
    },
    KeyBind {
        name: "rotateplaced",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("r")),
    },
    KeyBind {
        name: "diagonal_placement",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("controlLeft")),
    },
    KeyBind {
        name: "pick",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("mouseMiddle")),
    },
    KeyBind {
        name: "ping",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("p")),
    },
    KeyBind {
        name: "rebuild_select",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("b")),
    },
    KeyBind {
        name: "schematic_select",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("f")),
    },
    KeyBind {
        name: "schematic_flip_x",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("z")),
    },
    KeyBind {
        name: "schematic_flip_y",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("x")),
    },
    KeyBind {
        name: "schematic_menu",
        category: Some(Category::General),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("t")),
    },
    KeyBind {
        name: "command_mode",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("shiftLeft")),
    },
    KeyBind {
        name: "command_queue",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("mouseMiddle")),
    },
    KeyBind {
        name: "create_control_group",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("controlLeft")),
    },
    KeyBind {
        name: "select_all_units",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("g")),
    },
    KeyBind {
        name: "select_all_unit_factories",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("h")),
    },
    KeyBind {
        name: "select_all_unit_transport",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "select_across_screen",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("altLeft")),
    },
    KeyBind {
        name: "cancel_orders",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_stance_hold_fire",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_stance_pursue_target",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_stance_patrol",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_stance_ram",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_stance_boost",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_stance_hold_position",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_command_move",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_command_repair",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_command_rebuild",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_command_assist",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_command_mine",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_command_enter_payload",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_command_load_units",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_command_load_blocks",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_command_unload_payload",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "unit_command_loop_payload",
        category: Some(Category::Command),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "category_prev",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("comma")),
    },
    KeyBind {
        name: "category_next",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("period")),
    },
    KeyBind {
        name: "block_select_left",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("left")),
    },
    KeyBind {
        name: "block_select_right",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("right")),
    },
    KeyBind {
        name: "block_select_up",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("up")),
    },
    KeyBind {
        name: "block_select_down",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("down")),
    },
    KeyBind {
        name: "block_select_01",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("num1")),
    },
    KeyBind {
        name: "block_select_02",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("num2")),
    },
    KeyBind {
        name: "block_select_03",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("num3")),
    },
    KeyBind {
        name: "block_select_04",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("num4")),
    },
    KeyBind {
        name: "block_select_05",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("num5")),
    },
    KeyBind {
        name: "block_select_06",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("num6")),
    },
    KeyBind {
        name: "block_select_07",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("num7")),
    },
    KeyBind {
        name: "block_select_08",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("num8")),
    },
    KeyBind {
        name: "block_select_09",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("num9")),
    },
    KeyBind {
        name: "block_select_10",
        category: Some(Category::Blocks),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("num0")),
    },
    KeyBind {
        name: "zoom",
        category: Some(Category::View),
        kind: KeyKind::Axis,
        default: BindingDefault::Axis {
            negative: None,
            positive: Some("scroll"),
        },
    },
    KeyBind {
        name: "detach_camera",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "teleport_cursor",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "menu",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("escape")),
    },
    KeyBind {
        name: "fullscreen",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("f11")),
    },
    KeyBind {
        name: "pause",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("space")),
    },
    KeyBind {
        name: "skip_wave",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "minimap",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("m")),
    },
    KeyBind {
        name: "research",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("j")),
    },
    KeyBind {
        name: "planet_map",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("n")),
    },
    KeyBind {
        name: "block_info",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("f1")),
    },
    KeyBind {
        name: "toggle_menus",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("c")),
    },
    KeyBind {
        name: "screenshot",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("f12")),
    },
    KeyBind {
        name: "toggle_power_lines",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("f5")),
    },
    KeyBind {
        name: "toggle_block_status",
        category: Some(Category::View),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("f6")),
    },
    KeyBind {
        name: "player_list",
        category: Some(Category::Multiplayer),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("tab")),
    },
    KeyBind {
        name: "chat",
        category: Some(Category::Multiplayer),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("enter")),
    },
    KeyBind {
        name: "chat_history_prev",
        category: Some(Category::Multiplayer),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("up")),
    },
    KeyBind {
        name: "chat_history_next",
        category: Some(Category::Multiplayer),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("down")),
    },
    KeyBind {
        name: "chat_scroll",
        category: Some(Category::Multiplayer),
        kind: KeyKind::Axis,
        default: BindingDefault::Axis {
            negative: None,
            positive: Some("scroll"),
        },
    },
    KeyBind {
        name: "chat_mode",
        category: Some(Category::Multiplayer),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("tab")),
    },
    KeyBind {
        name: "console",
        category: Some(Category::Multiplayer),
        kind: KeyKind::Key,
        default: BindingDefault::Key(Some("f8")),
    },
    KeyBind {
        name: "debug_hitboxes",
        category: None,
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
    KeyBind {
        name: "performance_metrics",
        category: None,
        kind: KeyKind::Key,
        default: BindingDefault::Key(None),
    },
];

/// Static registry access (`Binding.init()` is a no-op in Rust — the table is
/// a `const`; the function exists so call sites mirror upstream).
pub struct KeyBindTable;

impl KeyBindTable {
    /// Mirrors `Binding.init()` (forced class init upstream).
    pub fn init() {}

    /// All binds in registration order.
    pub fn all() -> &'static [KeyBind] {
        BINDS
    }

    /// Number of registered binds.
    pub fn len() -> usize {
        BINDS.len()
    }

    /// Lookup by index (panics on out-of-range; use [`Self::try_get`] otherwise).
    pub fn get(id: KeyBindId) -> &'static KeyBind {
        &BINDS[id as usize]
    }

    /// Bounds-checked lookup.
    pub fn try_get(id: KeyBindId) -> Option<&'static KeyBind> {
        BINDS.get(id as usize)
    }

    /// Lookup by settings/bundle name.
    pub fn by_name(name: &str) -> Option<&'static KeyBind> {
        BINDS.iter().find(|bind| bind.name == name)
    }

    /// Binds in one bundle category.
    pub fn category(category: Category) -> impl Iterator<Item = &'static KeyBind> {
        BINDS
            .iter()
            .filter(move |bind| bind.category == Some(category))
    }
}

/// A bind's current value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BindingValue {
    /// Unbound.
    #[default]
    Unset,
    /// Single key/mouse button name.
    Key(String),
    /// Axis with optional negative/positive names.
    Axis {
        /// Negative direction key.
        negative: Option<String>,
        /// Positive direction key.
        positive: Option<String>,
    },
}

impl BindingValue {
    /// Display name (`Key` value, or axis positive for a single-element axis).
    pub fn display_name(&self) -> Option<&str> {
        match self {
            BindingValue::Unset => None,
            BindingValue::Key(name) => Some(name),
            BindingValue::Axis { positive, .. } => positive.as_deref(),
        }
    }
}

/// Client-local values for every [`KeyBind`] (never sim state; I3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingState {
    values: Vec<BindingValue>,
}

impl Default for BindingState {
    fn default() -> Self {
        Self::new()
    }
}

impl BindingState {
    /// Seeds every bind from its upstream default.
    pub fn new() -> Self {
        Self {
            values: BINDS.iter().map(default_value).collect(),
        }
    }

    /// Number of values.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the state has no values (never true for a full registry).
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Current value for `id`.
    pub fn value(&self, id: KeyBindId) -> &BindingValue {
        &self.values[id as usize]
    }

    /// Mutable value for `id`.
    pub fn value_mut(&mut self, id: KeyBindId) -> &mut BindingValue {
        &mut self.values[id as usize]
    }

    /// Name of the bound key (single-key binds; axis returns positive).
    pub fn name(&self, id: KeyBindId) -> Option<&str> {
        self.values[id as usize].display_name()
    }

    /// Rebind a single-key/axis binding to `name` (mirrors `KeyBind.key`).
    pub fn set_key(&mut self, id: KeyBindId, name: &str) {
        self.values[id as usize] = BindingValue::Key(name.to_owned());
    }

    /// Rebind an axis.
    pub fn set_axis(&mut self, id: KeyBindId, negative: Option<&str>, positive: Option<&str>) {
        self.values[id as usize] = BindingValue::Axis {
            negative: negative.map(str::to_owned),
            positive: positive.map(str::to_owned),
        };
    }

    /// Unbind.
    pub fn clear(&mut self, id: KeyBindId) {
        self.values[id as usize] = BindingValue::Unset;
    }

    /// Resets every bind to its upstream default.
    pub fn reset(&mut self) {
        self.values = BINDS.iter().map(default_value).collect();
    }

    /// The first axis that a key name participates in (negative matches first).
    pub fn axis_of(&self, id: KeyBindId, name: &str) -> Option<f32> {
        match self.values[id as usize] {
            BindingValue::Axis {
                ref negative,
                ref positive,
            } => {
                if negative.as_deref() == Some(name) {
                    Some(-1.0)
                } else if positive.as_deref() == Some(name) {
                    Some(1.0)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Loads overrides from plan-04 settings (missing keys keep defaults).
    pub fn load_from_settings(&mut self, settings: &SettingsStore) {
        for (index, bind) in BINDS.iter().enumerate() {
            let store_key = format!("keybind.{}", bind.name);
            if !settings.has(&store_key) {
                continue;
            }
            let raw = settings.get_string(&store_key, "");
            self.values[index] = match bind.kind {
                KeyKind::Key => {
                    if raw.is_empty() {
                        BindingValue::Unset
                    } else {
                        BindingValue::Key(raw)
                    }
                }
                KeyKind::Axis => match raw.split_once('|') {
                    Some((negative, positive)) => BindingValue::Axis {
                        negative: non_empty(negative),
                        positive: non_empty(positive),
                    },
                    None => BindingValue::Axis {
                        negative: None,
                        positive: non_empty(&raw),
                    },
                },
            };
        }
    }

    /// Persists every value to plan-04 settings (`keybind.<name>`).
    pub fn save_to_settings(&self, settings: &mut SettingsStore) {
        for (index, bind) in BINDS.iter().enumerate() {
            let store_key = format!("keybind.{}", bind.name);
            match &self.values[index] {
                BindingValue::Unset => settings.put_string(&store_key, ""),
                BindingValue::Key(name) => settings.put_string(&store_key, name),
                BindingValue::Axis { negative, positive } => {
                    let value = format!(
                        "{}|{}",
                        negative.as_deref().unwrap_or(""),
                        positive.as_deref().unwrap_or("")
                    );
                    settings.put_string(&store_key, &value);
                }
            }
        }
    }
}

fn non_empty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn default_value(bind: &KeyBind) -> BindingValue {
    match bind.default {
        BindingDefault::Key(Some(name)) => BindingValue::Key(name.to_owned()),
        BindingDefault::Key(None) => BindingValue::Unset,
        BindingDefault::Axis { negative, positive } => BindingValue::Axis {
            negative: negative.map(str::to_owned),
            positive: positive.map(str::to_owned),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_upstream() {
        assert_eq!(KeyBindTable::len(), 88);
        assert!(KeyBindTable::by_name("move_x").is_some());
        assert!(KeyBindTable::by_name("nope").is_none());

        // Axis vs key parity.
        assert_eq!(KeyBindTable::get(MOVE_X).kind, KeyKind::Axis);
        assert_eq!(KeyBindTable::get(MOVE_Y).kind, KeyKind::Axis);
        assert_eq!(KeyBindTable::get(ROTATE).kind, KeyKind::Axis);
        assert_eq!(KeyBindTable::get(ZOOM).kind, KeyKind::Axis);
        assert_eq!(KeyBindTable::get(CHAT_SCROLL).kind, KeyKind::Axis);
        assert_eq!(KeyBindTable::get(SELECT).kind, KeyKind::Key);

        // Exact defaults.
        assert_eq!(
            KeyBindTable::get(MOVE_X).default,
            BindingDefault::Axis {
                negative: Some("a"),
                positive: Some("d")
            }
        );
        assert_eq!(
            KeyBindTable::get(MOVE_Y).default,
            BindingDefault::Axis {
                negative: Some("s"),
                positive: Some("w")
            }
        );
        assert_eq!(
            KeyBindTable::get(SELECT_ALL_UNIT_TRANSPORT).default,
            BindingDefault::Key(None)
        );
        assert_eq!(
            KeyBindTable::get(DEBUG_HITBOXES).default,
            BindingDefault::Key(None)
        );
        assert_eq!(
            KeyBindTable::get(MENU).default,
            BindingDefault::Key(Some("escape"))
        );

        // Categories + bundle keys.
        assert_eq!(KeyBindTable::get(MOVE_X).category, Some(Category::General));
        assert_eq!(
            KeyBindTable::get(COMMAND_MODE).category,
            Some(Category::Command)
        );
        assert_eq!(
            KeyBindTable::get(CATEGORY_PREV).category,
            Some(Category::Blocks)
        );
        assert_eq!(KeyBindTable::get(ZOOM).category, Some(Category::View));
        assert_eq!(
            KeyBindTable::get(PLAYER_LIST).category,
            Some(Category::Multiplayer)
        );
        assert_eq!(KeyBindTable::get(DEBUG_HITBOXES).category, None);
        assert_eq!(
            KeyBindTable::get(MOVE_X).bundle_key(),
            "keybind.move_x.name"
        );

        // Every id constant lines up with its registry name.
        for (id, name, category) in [
            (MOVE_X, "move_x", Some(Category::General)),
            (MOVE_Y, "move_y", Some(Category::General)),
            (COMMAND_MODE, "command_mode", Some(Category::Command)),
            (CATEGORY_PREV, "category_prev", Some(Category::Blocks)),
            (ZOOM, "zoom", Some(Category::View)),
            (PLAYER_LIST, "player_list", Some(Category::Multiplayer)),
            (PERFORMANCE_METRICS, "performance_metrics", None),
        ] {
            let bind = KeyBindTable::get(id);
            assert_eq!(bind.name, name);
            assert_eq!(bind.category, category);
        }

        // Names are unique and dense.
        let mut names: Vec<&str> = KeyBindTable::all().iter().map(|bind| bind.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before);
        assert_eq!(
            Category::ALL
                .iter()
                .map(|c| KeyBindTable::category(*c).count())
                .sum::<usize>()
                + 2,
            88
        );
    }

    #[test]
    fn defaults_seed_state() {
        let state = BindingState::new();
        assert_eq!(state.name(MOVE_X), Some("d"));
        assert_eq!(state.name(MOVE_X).map(str::to_owned), Some("d".to_owned()));
        assert_eq!(state.name(SELECT_ALL_UNIT_TRANSPORT), None);
        assert_eq!(state.axis_of(MOVE_X, "a"), Some(-1.0));
        assert_eq!(state.axis_of(MOVE_X, "d"), Some(1.0));
        assert_eq!(state.axis_of(MOVE_X, "q"), None);
    }

    #[test]
    fn rebind_round_trip_through_settings() {
        use crate::io::SettingsStore;

        let mut state = BindingState::new();
        state.set_key(SELECT, "q");
        state.set_axis(MOVE_X, Some("left"), Some("right"));
        state.clear(RESPAWN);

        let mut settings = SettingsStore::new();
        state.save_to_settings(&mut settings);
        assert_eq!(settings.get_string("keybind.select", ""), "q");
        assert_eq!(settings.get_string("keybind.move_x", ""), "left|right");
        assert_eq!(settings.get_string("keybind.respawn", ""), "");

        let mut loaded = BindingState::new();
        loaded.load_from_settings(&settings);
        assert_eq!(loaded, state);
        // Defaults survive for unmentioned binds.
        assert_eq!(loaded.name(BOOST), Some("shiftLeft"));
    }
}
