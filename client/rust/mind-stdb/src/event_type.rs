// SPDX-License-Identifier: GPL-3.0-only

//! The complete upstream `EventType` taxonomy and its relay payload codec
//! (GAPS #37 / PAR-0548).
//!
//! Ported from Mindustry `core/src/mindustry/game/EventType.java`, which
//! declares 89 event classes (plus the 41-member `Trigger` enum, ported in
//! `mind-core::sim::events`). Java fires these classes in-process on the
//! server; a SpacetimeDB module has no such bus, so the relay publishes one
//! `match_event` row per event (`server/spacetimedb/src/relay/events.rs`) and
//! `my_match_events` exposes the caller's match. This module names every class
//! so a row can carry a typed payload:
//!
//! - [`EventType`] has one variant per upstream class, named exactly like the
//!   Java class (including the suffix-less `PlayerJoin`/`PlayerConnect`/
//!   `PlayerLeave`/`PlayerConnectionConfirmed`).
//! - Payloads are the **wire projection** of the Java fields: live objects are
//!   raw ids or packed positions (`player`/`unit`/`build`/`tile`/`bullet`),
//!   content is a name (the relay's content convention), and client-only
//!   objects that have no representation across the relay (`MultiPacker`,
//!   `Schematic`, `Rules`, `Seq<DataAsset>`, `Host`) are reduced to their
//!   identifying string (or dropped where they carry nothing).
//! - [`EventType::encode`]/[`EventType::decode`] are the JSON envelope written
//!   into `MatchEvent.detail`: `{"type":"UnitCreateEvent", ...fields}`.
//!   `ServerEvent::typed` (see [`crate::events`]) decodes it; a plain-text
//!   detail (Java-class-less module kinds, older rows) decodes to `None` and
//!   the bus falls back to the [`crate::events::event_type_name`] mapping.
//!
//! The `ALL_EVENT_TYPE_NAMES` table is the subscribe-side allowlist: every
//! name the relay mapping can produce must appear here, and a test keeps the
//! table and the enum in lockstep.

use serde::{Deserialize, Serialize};

/// JSON envelope key holding the upstream class name.
pub const EVENT_TYPE_TAG: &str = "type";

/// Every upstream `EventType` class name, in `EventType.java` declaration
/// order (89 entries; `Trigger` is an enum, not an event class).
pub const ALL_EVENT_TYPE_NAMES: [&str; 89] = [
    "WinEvent",
    "LoseEvent",
    "ResizeEvent",
    "MapMakeEvent",
    "MapPublishEvent",
    "SaveWriteEvent",
    "ClientCreateEvent",
    "ServerLoadEvent",
    "DisposeEvent",
    "PlayEvent",
    "ResetEvent",
    "HostEvent",
    "WaveEvent",
    "TurnEvent",
    "LineConfirmEvent",
    "BlockInfoEvent",
    "ContentInitEvent",
    "AtlasPackEvent",
    "ModContentLoadEvent",
    "ClientLoadEvent",
    "MusicRegisterEvent",
    "FileTreeInitEvent",
    "WorldLoadEvent",
    "WorldLoadBeginEvent",
    "WorldLoadEndEvent",
    "DataPatchLoadEvent",
    "TextureStreamEvent",
    "SaveLoadEvent",
    "SectorLoseEvent",
    "SectorInvasionEvent",
    "LaunchItemEvent",
    "SectorLaunchEvent",
    "SectorLaunchLoadoutEvent",
    "SchematicCreateEvent",
    "ClientPreConnectEvent",
    "ClientServerConnectEvent",
    "MenuOptionChooseEvent",
    "MenuBuilderOptionChooseEvent",
    "TextInputEvent",
    "PlayerChatEvent",
    "ClientChatEvent",
    "SectorCaptureEvent",
    "WithdrawEvent",
    "DepositEvent",
    "ConfigEvent",
    "TapEvent",
    "PickupEvent",
    "PayloadDropEvent",
    "UnitControlEvent",
    "BuildingCommandEvent",
    "GameOverEvent",
    "BuildDamageEvent",
    "BulletCreateEvent",
    "TilePreChangeEvent",
    "TileChangeEvent",
    "TileFloorChangeEvent",
    "TileOverlayChangeEvent",
    "BuildTeamChangeEvent",
    "CoreChangeEvent",
    "StateChangeEvent",
    "UnlockEvent",
    "ResearchEvent",
    "RulesLoadEvent",
    "BlockBuildBeginEvent",
    "BlockBuildEndEvent",
    "BuildRotateEvent",
    "BuildSelectEvent",
    "BlockDestroyEvent",
    "GeneratorPressureExplodeEvent",
    "BuildingBulletDestroyEvent",
    "UnitDestroyEvent",
    "UnitBulletDestroyEvent",
    "UnitDamageEvent",
    "UnitDrownEvent",
    "UnitCreateEvent",
    "UnitSpawnEvent",
    "UnitUnloadEvent",
    "UnitChangeEvent",
    "ConnectionEvent",
    "ConnectPacketEvent",
    "PlayerConnectionConfirmed",
    "PlayerJoin",
    "PlayerConnect",
    "PlayerLeave",
    "PlayerBanEvent",
    "PlayerUnbanEvent",
    "PlayerIpBanEvent",
    "PlayerIpUnbanEvent",
    "AdminRequestEvent",
];

/// Decode failure for [`EventType::decode`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EventDecodeError {
    /// The envelope is not valid JSON or does not match any class.
    #[error("invalid event type envelope: {0}")]
    Json(String),
    /// The envelope names a class this port does not know.
    #[error("unknown event class: {0}")]
    UnknownClass(String),
}

/// One upstream `EventType` class instance. Variant names are the Java class
/// names; field names follow the Java fields (camelCase on the wire).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum EventType {
    /// `WinEvent`.
    #[serde(rename = "WinEvent")]
    Win,
    /// `LoseEvent`.
    #[serde(rename = "LoseEvent")]
    Lose,
    /// `ResizeEvent`.
    #[serde(rename = "ResizeEvent")]
    Resize,
    /// `MapMakeEvent`.
    #[serde(rename = "MapMakeEvent")]
    MapMake,
    /// `MapPublishEvent`.
    #[serde(rename = "MapPublishEvent")]
    MapPublish,
    /// `SaveWriteEvent`.
    #[serde(rename = "SaveWriteEvent")]
    SaveWrite,
    /// `ClientCreateEvent`.
    #[serde(rename = "ClientCreateEvent")]
    ClientCreate,
    /// `ServerLoadEvent`.
    #[serde(rename = "ServerLoadEvent")]
    ServerLoad,
    /// `DisposeEvent`.
    #[serde(rename = "DisposeEvent")]
    Dispose,
    /// `PlayEvent`.
    #[serde(rename = "PlayEvent")]
    Play,
    /// `ResetEvent`.
    #[serde(rename = "ResetEvent")]
    Reset,
    /// `HostEvent`.
    #[serde(rename = "HostEvent")]
    Host,
    /// `WaveEvent`.
    #[serde(rename = "WaveEvent")]
    Wave,
    /// `TurnEvent`.
    #[serde(rename = "TurnEvent")]
    Turn,
    /// `LineConfirmEvent`.
    #[serde(rename = "LineConfirmEvent")]
    LineConfirm,
    /// `BlockInfoEvent`.
    #[serde(rename = "BlockInfoEvent")]
    BlockInfo,
    /// `ContentInitEvent`.
    #[serde(rename = "ContentInitEvent")]
    ContentInit,
    /// `AtlasPackEvent` (the `MultiPacker` has no wire form).
    #[serde(rename = "AtlasPackEvent")]
    AtlasPack,
    /// `ModContentLoadEvent`.
    #[serde(rename = "ModContentLoadEvent")]
    ModContentLoad,
    /// `ClientLoadEvent`.
    #[serde(rename = "ClientLoadEvent")]
    ClientLoad,
    /// `MusicRegisterEvent`.
    #[serde(rename = "MusicRegisterEvent")]
    MusicRegister,
    /// `FileTreeInitEvent`.
    #[serde(rename = "FileTreeInitEvent")]
    FileTreeInit,
    /// `WorldLoadEvent`.
    #[serde(rename = "WorldLoadEvent")]
    WorldLoad,
    /// `WorldLoadBeginEvent`.
    #[serde(rename = "WorldLoadBeginEvent")]
    WorldLoadBegin,
    /// `WorldLoadEndEvent`.
    #[serde(rename = "WorldLoadEndEvent")]
    WorldLoadEnd,
    /// `DataPatchLoadEvent` (`assets` reduced to names).
    #[serde(rename = "DataPatchLoadEvent")]
    DataPatchLoad {
        /// Loaded custom asset names.
        assets: Vec<String>,
    },
    /// `TextureStreamEvent`.
    #[serde(rename = "TextureStreamEvent")]
    TextureStream {
        /// Texture name.
        name: String,
    },
    /// `SaveLoadEvent`.
    #[serde(rename = "SaveLoadEvent")]
    SaveLoad {
        /// Whether a map (not a save) was loaded.
        is_map: bool,
    },
    /// `SectorLoseEvent`.
    #[serde(rename = "SectorLoseEvent")]
    SectorLose {
        /// Sector id (`Sector.id`).
        sector: String,
    },
    /// `SectorInvasionEvent`.
    #[serde(rename = "SectorInvasionEvent")]
    SectorInvasion {
        /// Sector id.
        sector: String,
    },
    /// `LaunchItemEvent` (`ItemStack` reduced to item name + amount).
    #[serde(rename = "LaunchItemEvent")]
    LaunchItem {
        /// Item content name.
        item: Option<String>,
        /// Stack amount.
        amount: i32,
    },
    /// `SectorLaunchEvent`.
    #[serde(rename = "SectorLaunchEvent")]
    SectorLaunch {
        /// Sector id.
        sector: String,
    },
    /// `SectorLaunchLoadoutEvent` (`Schematic` reduced to its name).
    #[serde(rename = "SectorLaunchLoadoutEvent")]
    SectorLaunchLoadout {
        /// Destination sector id.
        sector: String,
        /// Source sector id.
        from: String,
        /// Loadout schematic name.
        loadout: String,
    },
    /// `SchematicCreateEvent` (`Schematic` reduced to its name).
    #[serde(rename = "SchematicCreateEvent")]
    SchematicCreate {
        /// Schematic name.
        schematic: String,
    },
    /// `ClientPreConnectEvent` (`Host` reduced to its address).
    #[serde(rename = "ClientPreConnectEvent")]
    ClientPreConnect {
        /// Host address.
        host: String,
    },
    /// `ClientServerConnectEvent`.
    #[serde(rename = "ClientServerConnectEvent")]
    ClientServerConnect {
        /// Server IP.
        ip: String,
        /// Server port.
        port: i32,
    },
    /// `MenuOptionChooseEvent`.
    #[serde(rename = "MenuOptionChooseEvent")]
    MenuOptionChoose {
        /// Choosing player id.
        player: Option<i32>,
        /// Menu id.
        menu_id: i32,
        /// Chosen option index.
        option: i32,
    },
    /// `MenuBuilderOptionChooseEvent`.
    #[serde(rename = "MenuBuilderOptionChooseEvent")]
    MenuBuilderOptionChoose {
        /// Choosing player id.
        player: Option<i32>,
        /// Menu id.
        menu_id: i32,
        /// Result payload.
        result: crate::typeio::MenuResult,
    },
    /// `TextInputEvent`.
    #[serde(rename = "TextInputEvent")]
    TextInput {
        /// Typing player id.
        player: Option<i32>,
        /// Text input id.
        text_input_id: i32,
        /// Submitted text (`null` = cancelled).
        text: Option<String>,
    },
    /// `PlayerChatEvent`.
    #[serde(rename = "PlayerChatEvent")]
    PlayerChat {
        /// Sending player id.
        player: Option<i32>,
        /// Message text.
        message: String,
    },
    /// `ClientChatEvent` (client-side only upstream).
    #[serde(rename = "ClientChatEvent")]
    ClientChat {
        /// Message text.
        message: String,
    },
    /// `SectorCaptureEvent`.
    #[serde(rename = "SectorCaptureEvent")]
    SectorCapture {
        /// Captured sector id.
        sector: String,
        /// Whether this was the initial capture.
        initial_capture: bool,
    },
    /// `WithdrawEvent`.
    #[serde(rename = "WithdrawEvent")]
    Withdraw {
        /// Building packed position.
        tile: Option<i32>,
        /// Player id.
        player: Option<i32>,
        /// Item content name.
        item: Option<String>,
        /// Amount withdrawn.
        amount: i32,
    },
    /// `DepositEvent`.
    #[serde(rename = "DepositEvent")]
    Deposit {
        /// Building packed position.
        tile: Option<i32>,
        /// Player id.
        player: Option<i32>,
        /// Item content name.
        item: Option<String>,
        /// Amount deposited.
        amount: i32,
    },
    /// `ConfigEvent`.
    #[serde(rename = "ConfigEvent")]
    Config {
        /// Building packed position.
        tile: Option<i32>,
        /// Configuring player id (`null` for non-player sources).
        player: Option<i32>,
        /// Configuration value (`TypeIO` object projection).
        value: Option<serde_json::Value>,
    },
    /// `TapEvent`.
    #[serde(rename = "TapEvent")]
    Tap {
        /// Tapping player id.
        player: Option<i32>,
        /// Tapped tile packed position.
        tile: Option<i32>,
    },
    /// `PickupEvent`.
    #[serde(rename = "PickupEvent")]
    Pickup {
        /// Carrier unit id.
        carrier: Option<i32>,
        /// Picked-up unit id, when a unit.
        unit: Option<i32>,
        /// Picked-up building position, when a building.
        build: Option<i32>,
    },
    /// `PayloadDropEvent`.
    #[serde(rename = "PayloadDropEvent")]
    PayloadDrop {
        /// Carrier unit id.
        carrier: Option<i32>,
        /// Dropped unit id, when a unit.
        unit: Option<i32>,
        /// Dropped building position, when a building.
        build: Option<i32>,
    },
    /// `UnitControlEvent`.
    #[serde(rename = "UnitControlEvent")]
    UnitControl {
        /// Controlling player id.
        player: Option<i32>,
        /// Controlled unit id (`null` = leaving control).
        unit: Option<i32>,
    },
    /// `BuildingCommandEvent`.
    #[serde(rename = "BuildingCommandEvent")]
    BuildingCommand {
        /// Commanding player id.
        player: Option<i32>,
        /// Building packed position.
        building: Option<i32>,
        /// Command target position `[x, y]`.
        position: [f32; 2],
    },
    /// `GameOverEvent`.
    #[serde(rename = "GameOverEvent")]
    GameOver {
        /// Winning team id.
        winner: u8,
    },
    /// `BuildDamageEvent`.
    #[serde(rename = "BuildDamageEvent")]
    BuildDamage {
        /// Damaged building packed position.
        build: Option<i32>,
        /// Source bullet id.
        source: Option<i32>,
    },
    /// `BulletCreateEvent`.
    #[serde(rename = "BulletCreateEvent")]
    BulletCreate {
        /// Created bullet id.
        bullet: Option<i32>,
    },
    /// `TilePreChangeEvent`.
    #[serde(rename = "TilePreChangeEvent")]
    TilePreChange {
        /// Tile packed position.
        tile: Option<i32>,
    },
    /// `TileChangeEvent`.
    #[serde(rename = "TileChangeEvent")]
    TileChange {
        /// Tile packed position.
        tile: Option<i32>,
    },
    /// `TileFloorChangeEvent`.
    #[serde(rename = "TileFloorChangeEvent")]
    TileFloorChange {
        /// Tile packed position.
        tile: Option<i32>,
        /// Previous floor content name.
        previous: Option<String>,
        /// New floor content name.
        floor: Option<String>,
    },
    /// `TileOverlayChangeEvent`.
    #[serde(rename = "TileOverlayChangeEvent")]
    TileOverlayChange {
        /// Tile packed position.
        tile: Option<i32>,
        /// Previous overlay content name.
        previous: Option<String>,
        /// New overlay content name.
        overlay: Option<String>,
    },
    /// `BuildTeamChangeEvent`.
    #[serde(rename = "BuildTeamChangeEvent")]
    BuildTeamChange {
        /// Previous team id.
        previous: u8,
        /// Building packed position.
        build: Option<i32>,
    },
    /// `CoreChangeEvent`.
    #[serde(rename = "CoreChangeEvent")]
    CoreChange {
        /// Core building packed position.
        core: Option<i32>,
    },
    /// `StateChangeEvent`.
    #[serde(rename = "StateChangeEvent")]
    StateChange {
        /// Previous `GameState.State` name.
        from: String,
        /// New `GameState.State` name.
        to: String,
    },
    /// `UnlockEvent`.
    #[serde(rename = "UnlockEvent")]
    Unlock {
        /// Unlocked content name.
        content: Option<String>,
    },
    /// `ResearchEvent`.
    #[serde(rename = "ResearchEvent")]
    Research {
        /// Researched content name.
        content: Option<String>,
    },
    /// `RulesLoadEvent` (`Rules` JSON is not relayed; the flag is).
    #[serde(rename = "RulesLoadEvent")]
    RulesLoad {
        /// Whether the rules came from a save.
        from_save: bool,
    },
    /// `BlockBuildBeginEvent`.
    #[serde(rename = "BlockBuildBeginEvent")]
    BlockBuildBegin {
        /// Tile packed position.
        tile: Option<i32>,
        /// Builder team id.
        team: u8,
        /// Builder unit id.
        unit: Option<i32>,
        /// Whether the plan is a deconstruction.
        breaking: bool,
    },
    /// `BlockBuildEndEvent`.
    #[serde(rename = "BlockBuildEndEvent")]
    BlockBuildEnd {
        /// Tile packed position.
        tile: Option<i32>,
        /// Builder unit id.
        unit: Option<i32>,
        /// Builder team id.
        team: u8,
        /// Whether the plan was a deconstruction.
        breaking: bool,
        /// Placed block configuration (`TypeIO` object projection).
        config: Option<serde_json::Value>,
    },
    /// `BuildRotateEvent`.
    #[serde(rename = "BuildRotateEvent")]
    BuildRotate {
        /// Rotated building packed position.
        build: Option<i32>,
        /// Rotating unit id.
        unit: Option<i32>,
        /// Previous rotation.
        previous: i32,
    },
    /// `BuildSelectEvent`.
    #[serde(rename = "BuildSelectEvent")]
    BuildSelect {
        /// Tile packed position.
        tile: Option<i32>,
        /// Builder team id.
        team: u8,
        /// Selecting unit id.
        builder: Option<i32>,
        /// Whether the selected plan is a deconstruction.
        breaking: bool,
    },
    /// `BlockDestroyEvent`.
    #[serde(rename = "BlockDestroyEvent")]
    BlockDestroy {
        /// Tile packed position.
        tile: Option<i32>,
    },
    /// `GeneratorPressureExplodeEvent`.
    #[serde(rename = "GeneratorPressureExplodeEvent")]
    GeneratorPressureExplode {
        /// Exploded building packed position.
        build: Option<i32>,
    },
    /// `BuildingBulletDestroyEvent`.
    #[serde(rename = "BuildingBulletDestroyEvent")]
    BuildingBulletDestroy {
        /// Destroyed building packed position.
        build: Option<i32>,
        /// Killing bullet id.
        bullet: Option<i32>,
    },
    /// `UnitDestroyEvent`.
    #[serde(rename = "UnitDestroyEvent")]
    UnitDestroy {
        /// Destroyed unit id.
        unit: Option<i32>,
    },
    /// `UnitBulletDestroyEvent`.
    #[serde(rename = "UnitBulletDestroyEvent")]
    UnitBulletDestroy {
        /// Destroyed unit id.
        unit: Option<i32>,
        /// Killing bullet id.
        bullet: Option<i32>,
    },
    /// `UnitDamageEvent`.
    #[serde(rename = "UnitDamageEvent")]
    UnitDamage {
        /// Damaged unit id.
        unit: Option<i32>,
        /// Damaging bullet id.
        bullet: Option<i32>,
    },
    /// `UnitDrownEvent`.
    #[serde(rename = "UnitDrownEvent")]
    UnitDrown {
        /// Drowned unit id.
        unit: Option<i32>,
    },
    /// `UnitCreateEvent`.
    #[serde(rename = "UnitCreateEvent")]
    UnitCreate {
        /// Created unit id.
        unit: Option<i32>,
        /// Spawner building packed position.
        spawner: Option<i32>,
        /// Spawner unit id (reconstructor/factory unit).
        spawner_unit: Option<i32>,
    },
    /// `UnitSpawnEvent`.
    #[serde(rename = "UnitSpawnEvent")]
    UnitSpawn {
        /// Spawned unit id.
        unit: Option<i32>,
    },
    /// `UnitUnloadEvent`.
    #[serde(rename = "UnitUnloadEvent")]
    UnitUnload {
        /// Unloaded unit id.
        unit: Option<i32>,
    },
    /// `UnitChangeEvent`.
    #[serde(rename = "UnitChangeEvent")]
    UnitChange {
        /// Controlling player id.
        player: Option<i32>,
        /// New unit id.
        unit: Option<i32>,
    },
    /// `ConnectionEvent` (`NetConnection` reduced to its address).
    #[serde(rename = "ConnectionEvent")]
    Connection {
        /// Connection address.
        connection: Option<String>,
    },
    /// `ConnectPacketEvent` (`ConnectPacket` reduced to the player name).
    #[serde(rename = "ConnectPacketEvent")]
    ConnectPacket {
        /// Connection address.
        connection: Option<String>,
        /// Name in the connect packet.
        packet: Option<String>,
    },
    /// `PlayerConnectionConfirmed`.
    #[serde(rename = "PlayerConnectionConfirmed")]
    PlayerConnectionConfirmed {
        /// Player id.
        player: Option<i32>,
    },
    /// `PlayerJoin`.
    #[serde(rename = "PlayerJoin")]
    PlayerJoin {
        /// Player id.
        player: Option<i32>,
    },
    /// `PlayerConnect`.
    #[serde(rename = "PlayerConnect")]
    PlayerConnect {
        /// Player id.
        player: Option<i32>,
    },
    /// `PlayerLeave`.
    #[serde(rename = "PlayerLeave")]
    PlayerLeave {
        /// Player id.
        player: Option<i32>,
    },
    /// `PlayerBanEvent`.
    #[serde(rename = "PlayerBanEvent")]
    PlayerBanEvent {
        /// Banned player id, when online.
        player: Option<i32>,
        /// Banned UUID.
        uuid: String,
    },
    /// `PlayerUnbanEvent`.
    #[serde(rename = "PlayerUnbanEvent")]
    PlayerUnbanEvent {
        /// Unbanned player id, when online.
        player: Option<i32>,
        /// Unbanned UUID.
        uuid: String,
    },
    /// `PlayerIpBanEvent`.
    #[serde(rename = "PlayerIpBanEvent")]
    PlayerIpBanEvent {
        /// Banned IP.
        ip: String,
    },
    /// `PlayerIpUnbanEvent`.
    #[serde(rename = "PlayerIpUnbanEvent")]
    PlayerIpUnbanEvent {
        /// Unbanned IP.
        ip: String,
    },
    /// `AdminRequestEvent`.
    #[serde(rename = "AdminRequestEvent")]
    AdminRequestEvent {
        /// Requesting player id.
        player: Option<i32>,
        /// Target player id.
        other: Option<i32>,
        /// `Packets.AdminAction` name.
        action: String,
    },
}

impl EventType {
    /// Upstream class name (the `type` tag value).
    pub fn class_name(&self) -> &'static str {
        match self {
            Self::Win => "WinEvent",
            Self::Lose => "LoseEvent",
            Self::Resize => "ResizeEvent",
            Self::MapMake => "MapMakeEvent",
            Self::MapPublish => "MapPublishEvent",
            Self::SaveWrite => "SaveWriteEvent",
            Self::ClientCreate => "ClientCreateEvent",
            Self::ServerLoad => "ServerLoadEvent",
            Self::Dispose => "DisposeEvent",
            Self::Play => "PlayEvent",
            Self::Reset => "ResetEvent",
            Self::Host => "HostEvent",
            Self::Wave => "WaveEvent",
            Self::Turn => "TurnEvent",
            Self::LineConfirm => "LineConfirmEvent",
            Self::BlockInfo => "BlockInfoEvent",
            Self::ContentInit => "ContentInitEvent",
            Self::AtlasPack => "AtlasPackEvent",
            Self::ModContentLoad => "ModContentLoadEvent",
            Self::ClientLoad => "ClientLoadEvent",
            Self::MusicRegister => "MusicRegisterEvent",
            Self::FileTreeInit => "FileTreeInitEvent",
            Self::WorldLoad => "WorldLoadEvent",
            Self::WorldLoadBegin => "WorldLoadBeginEvent",
            Self::WorldLoadEnd => "WorldLoadEndEvent",
            Self::DataPatchLoad { .. } => "DataPatchLoadEvent",
            Self::TextureStream { .. } => "TextureStreamEvent",
            Self::SaveLoad { .. } => "SaveLoadEvent",
            Self::SectorLose { .. } => "SectorLoseEvent",
            Self::SectorInvasion { .. } => "SectorInvasionEvent",
            Self::LaunchItem { .. } => "LaunchItemEvent",
            Self::SectorLaunch { .. } => "SectorLaunchEvent",
            Self::SectorLaunchLoadout { .. } => "SectorLaunchLoadoutEvent",
            Self::SchematicCreate { .. } => "SchematicCreateEvent",
            Self::ClientPreConnect { .. } => "ClientPreConnectEvent",
            Self::ClientServerConnect { .. } => "ClientServerConnectEvent",
            Self::MenuOptionChoose { .. } => "MenuOptionChooseEvent",
            Self::MenuBuilderOptionChoose { .. } => "MenuBuilderOptionChooseEvent",
            Self::TextInput { .. } => "TextInputEvent",
            Self::PlayerChat { .. } => "PlayerChatEvent",
            Self::ClientChat { .. } => "ClientChatEvent",
            Self::SectorCapture { .. } => "SectorCaptureEvent",
            Self::Withdraw { .. } => "WithdrawEvent",
            Self::Deposit { .. } => "DepositEvent",
            Self::Config { .. } => "ConfigEvent",
            Self::Tap { .. } => "TapEvent",
            Self::Pickup { .. } => "PickupEvent",
            Self::PayloadDrop { .. } => "PayloadDropEvent",
            Self::UnitControl { .. } => "UnitControlEvent",
            Self::BuildingCommand { .. } => "BuildingCommandEvent",
            Self::GameOver { .. } => "GameOverEvent",
            Self::BuildDamage { .. } => "BuildDamageEvent",
            Self::BulletCreate { .. } => "BulletCreateEvent",
            Self::TilePreChange { .. } => "TilePreChangeEvent",
            Self::TileChange { .. } => "TileChangeEvent",
            Self::TileFloorChange { .. } => "TileFloorChangeEvent",
            Self::TileOverlayChange { .. } => "TileOverlayChangeEvent",
            Self::BuildTeamChange { .. } => "BuildTeamChangeEvent",
            Self::CoreChange { .. } => "CoreChangeEvent",
            Self::StateChange { .. } => "StateChangeEvent",
            Self::Unlock { .. } => "UnlockEvent",
            Self::Research { .. } => "ResearchEvent",
            Self::RulesLoad { .. } => "RulesLoadEvent",
            Self::BlockBuildBegin { .. } => "BlockBuildBeginEvent",
            Self::BlockBuildEnd { .. } => "BlockBuildEndEvent",
            Self::BuildRotate { .. } => "BuildRotateEvent",
            Self::BuildSelect { .. } => "BuildSelectEvent",
            Self::BlockDestroy { .. } => "BlockDestroyEvent",
            Self::GeneratorPressureExplode { .. } => "GeneratorPressureExplodeEvent",
            Self::BuildingBulletDestroy { .. } => "BuildingBulletDestroyEvent",
            Self::UnitDestroy { .. } => "UnitDestroyEvent",
            Self::UnitBulletDestroy { .. } => "UnitBulletDestroyEvent",
            Self::UnitDamage { .. } => "UnitDamageEvent",
            Self::UnitDrown { .. } => "UnitDrownEvent",
            Self::UnitCreate { .. } => "UnitCreateEvent",
            Self::UnitSpawn { .. } => "UnitSpawnEvent",
            Self::UnitUnload { .. } => "UnitUnloadEvent",
            Self::UnitChange { .. } => "UnitChangeEvent",
            Self::Connection { .. } => "ConnectionEvent",
            Self::ConnectPacket { .. } => "ConnectPacketEvent",
            Self::PlayerConnectionConfirmed { .. } => "PlayerConnectionConfirmed",
            Self::PlayerJoin { .. } => "PlayerJoin",
            Self::PlayerConnect { .. } => "PlayerConnect",
            Self::PlayerLeave { .. } => "PlayerLeave",
            Self::PlayerBanEvent { .. } => "PlayerBanEvent",
            Self::PlayerUnbanEvent { .. } => "PlayerUnbanEvent",
            Self::PlayerIpBanEvent { .. } => "PlayerIpBanEvent",
            Self::PlayerIpUnbanEvent { .. } => "PlayerIpUnbanEvent",
            Self::AdminRequestEvent { .. } => "AdminRequestEvent",
        }
    }

    /// Encodes the `MatchEvent.detail` JSON envelope.
    pub fn encode(&self) -> Result<String, EventDecodeError> {
        serde_json::to_string(self).map_err(|error| EventDecodeError::Json(error.to_string()))
    }

    /// Decodes a `MatchEvent.detail` JSON envelope.
    ///
    /// Plain-text details (the module's legacy rows) are not envelopes and
    /// fail with [`EventDecodeError::Json`]; callers use
    /// [`ServerEvent::typed`](crate::events::ServerEvent::typed) to treat that
    /// as "no typed payload".
    pub fn decode(json: &str) -> Result<Self, EventDecodeError> {
        if !json.trim_start().starts_with('{') {
            return Err(EventDecodeError::Json("not a JSON object".to_string()));
        }
        serde_json::from_str(json).map_err(|error| {
            // Surface an unknown class name distinctly: serde reports
            // `unknown variant \`Name\`` for an unrecognized `type` tag.
            let message = error.to_string();
            match message
                .split_once("unknown variant `")
                .and_then(|(_, rest)| rest.split_once('`'))
            {
                Some((name, _)) => EventDecodeError::UnknownClass(name.to_string()),
                None => EventDecodeError::Json(message),
            }
        })
    }

    /// Whether the class name is one of the 89 upstream classes.
    pub fn is_known_class(name: &str) -> bool {
        ALL_EVENT_TYPE_NAMES.contains(&name)
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.class_name())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::typeio::{MenuResult, MenuValue};

    /// One representative instance per class. The list must stay in the same
    /// order as [`ALL_EVENT_TYPE_NAMES`]; the coverage test enforces it.
    fn samples() -> Vec<EventType> {
        vec![
            EventType::Win,
            EventType::Lose,
            EventType::Resize,
            EventType::MapMake,
            EventType::MapPublish,
            EventType::SaveWrite,
            EventType::ClientCreate,
            EventType::ServerLoad,
            EventType::Dispose,
            EventType::Play,
            EventType::Reset,
            EventType::Host,
            EventType::Wave,
            EventType::Turn,
            EventType::LineConfirm,
            EventType::BlockInfo,
            EventType::ContentInit,
            EventType::AtlasPack,
            EventType::ModContentLoad,
            EventType::ClientLoad,
            EventType::MusicRegister,
            EventType::FileTreeInit,
            EventType::WorldLoad,
            EventType::WorldLoadBegin,
            EventType::WorldLoadEnd,
            EventType::DataPatchLoad {
                assets: vec!["a.png".to_string()],
            },
            EventType::TextureStream {
                name: "tex".to_string(),
            },
            EventType::SaveLoad { is_map: true },
            EventType::SectorLose {
                sector: "serpulo-12".to_string(),
            },
            EventType::SectorInvasion {
                sector: "serpulo-12".to_string(),
            },
            EventType::LaunchItem {
                item: Some("copper".to_string()),
                amount: 25,
            },
            EventType::SectorLaunch {
                sector: "serpulo-12".to_string(),
            },
            EventType::SectorLaunchLoadout {
                sector: "serpulo-12".to_string(),
                from: "serpulo-1".to_string(),
                loadout: "loadout".to_string(),
            },
            EventType::SchematicCreate {
                schematic: "spine".to_string(),
            },
            EventType::ClientPreConnect {
                host: "127.0.0.1:6567".to_string(),
            },
            EventType::ClientServerConnect {
                ip: "127.0.0.1".to_string(),
                port: 6567,
            },
            EventType::MenuOptionChoose {
                player: Some(1),
                menu_id: 2,
                option: 3,
            },
            EventType::MenuBuilderOptionChoose {
                player: Some(1),
                menu_id: 2,
                result: MenuResult {
                    token: 9,
                    result: Some("ok".to_string()),
                    values: vec![("field".to_string(), MenuValue::Str("text".to_string()))],
                },
            },
            EventType::TextInput {
                player: Some(1),
                text_input_id: 4,
                text: Some("typed".to_string()),
            },
            EventType::PlayerChat {
                player: Some(1),
                message: "hi".to_string(),
            },
            EventType::ClientChat {
                message: "hi".to_string(),
            },
            EventType::SectorCapture {
                sector: "serpulo-12".to_string(),
                initial_capture: false,
            },
            EventType::Withdraw {
                tile: Some(16),
                player: Some(1),
                item: Some("copper".to_string()),
                amount: 10,
            },
            EventType::Deposit {
                tile: Some(16),
                player: Some(1),
                item: Some("copper".to_string()),
                amount: 10,
            },
            EventType::Config {
                tile: Some(16),
                player: Some(1),
                value: Some(serde_json::json!(true)),
            },
            EventType::Tap {
                player: Some(1),
                tile: Some(16),
            },
            EventType::Pickup {
                carrier: Some(7),
                unit: None,
                build: Some(32),
            },
            EventType::PayloadDrop {
                carrier: Some(7),
                unit: Some(8),
                build: None,
            },
            EventType::UnitControl {
                player: Some(1),
                unit: Some(7),
            },
            EventType::BuildingCommand {
                player: Some(1),
                building: Some(16),
                position: [1.5, -2.5],
            },
            EventType::GameOver { winner: 1 },
            EventType::BuildDamage {
                build: Some(16),
                source: Some(3),
            },
            EventType::BulletCreate { bullet: Some(3) },
            EventType::TilePreChange { tile: Some(16) },
            EventType::TileChange { tile: Some(16) },
            EventType::TileFloorChange {
                tile: Some(16),
                previous: Some("stone".to_string()),
                floor: Some("sand".to_string()),
            },
            EventType::TileOverlayChange {
                tile: Some(16),
                previous: Some("ore-copper".to_string()),
                overlay: None,
            },
            EventType::BuildTeamChange {
                previous: 1,
                build: Some(16),
            },
            EventType::CoreChange { core: Some(16) },
            EventType::StateChange {
                from: "playing".to_string(),
                to: "paused".to_string(),
            },
            EventType::Unlock {
                content: Some("router".to_string()),
            },
            EventType::Research {
                content: Some("router".to_string()),
            },
            EventType::RulesLoad { from_save: true },
            EventType::BlockBuildBegin {
                tile: Some(16),
                team: 1,
                unit: Some(7),
                breaking: false,
            },
            EventType::BlockBuildEnd {
                tile: Some(16),
                unit: Some(7),
                team: 1,
                breaking: false,
                config: Some(serde_json::json!({"x": 1})),
            },
            EventType::BuildRotate {
                build: Some(16),
                unit: Some(7),
                previous: 1,
            },
            EventType::BuildSelect {
                tile: Some(16),
                team: 1,
                builder: Some(7),
                breaking: true,
            },
            EventType::BlockDestroy { tile: Some(16) },
            EventType::GeneratorPressureExplode { build: Some(16) },
            EventType::BuildingBulletDestroy {
                build: Some(16),
                bullet: Some(3),
            },
            EventType::UnitDestroy { unit: Some(7) },
            EventType::UnitBulletDestroy {
                unit: Some(7),
                bullet: Some(3),
            },
            EventType::UnitDamage {
                unit: Some(7),
                bullet: Some(3),
            },
            EventType::UnitDrown { unit: Some(7) },
            EventType::UnitCreate {
                unit: Some(7),
                spawner: Some(16),
                spawner_unit: Some(9),
            },
            EventType::UnitSpawn { unit: Some(7) },
            EventType::UnitUnload { unit: Some(7) },
            EventType::UnitChange {
                player: Some(1),
                unit: Some(7),
            },
            EventType::Connection {
                connection: Some("127.0.0.1:5000".to_string()),
            },
            EventType::ConnectPacket {
                connection: Some("127.0.0.1:5000".to_string()),
                packet: Some("player".to_string()),
            },
            EventType::PlayerConnectionConfirmed { player: Some(1) },
            EventType::PlayerJoin { player: Some(1) },
            EventType::PlayerConnect { player: Some(1) },
            EventType::PlayerLeave { player: Some(1) },
            EventType::PlayerBanEvent {
                player: None,
                uuid: "uuid".to_string(),
            },
            EventType::PlayerUnbanEvent {
                player: Some(1),
                uuid: "uuid".to_string(),
            },
            EventType::PlayerIpBanEvent {
                ip: "10.0.0.1".to_string(),
            },
            EventType::PlayerIpUnbanEvent {
                ip: "10.0.0.1".to_string(),
            },
            EventType::AdminRequestEvent {
                player: Some(1),
                other: Some(2),
                action: "kick".to_string(),
            },
        ]
    }

    #[test]
    fn samples_cover_every_declared_class_exactly_once() {
        let samples = samples();
        assert_eq!(samples.len(), ALL_EVENT_TYPE_NAMES.len());
        for (index, sample) in samples.iter().enumerate() {
            assert_eq!(
                sample.class_name(),
                ALL_EVENT_TYPE_NAMES[index],
                "sample {index} is out of order"
            );
            assert!(EventType::is_known_class(sample.class_name()));
        }
        // Names are unique and every name decodes back to itself.
        let mut names: Vec<&str> = ALL_EVENT_TYPE_NAMES.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ALL_EVENT_TYPE_NAMES.len());
        for sample in &samples {
            assert_eq!(sample.to_string(), sample.class_name());
        }
    }

    #[test]
    fn every_class_round_trips_through_the_json_envelope() {
        for sample in samples() {
            let json = sample.encode().unwrap();
            assert!(
                json.starts_with("{\"type\":\""),
                "envelope missing tag: {json}"
            );
            let decoded = EventType::decode(&json).unwrap();
            assert_eq!(decoded, sample, "round trip failed for {json}");
            assert_eq!(decoded.class_name(), sample.class_name());
        }
    }

    #[test]
    fn json_shape_uses_the_java_class_name_and_camel_case_fields() {
        let json = EventType::UnitCreate {
            unit: Some(7),
            spawner: Some(16),
            spawner_unit: None,
        }
        .encode()
        .unwrap();
        assert_eq!(
            json,
            r#"{"type":"UnitCreateEvent","unit":7,"spawner":16,"spawnerUnit":null}"#
        );
        let plain = EventType::PlayerJoin { player: Some(1) }.encode().unwrap();
        assert_eq!(plain, r#"{"type":"PlayerJoin","player":1}"#);
        let unit = EventType::Wave.encode().unwrap();
        assert_eq!(unit, r#"{"type":"WaveEvent"}"#);
    }

    #[test]
    fn malformed_and_unknown_envelopes_are_rejected() {
        assert!(matches!(
            EventType::decode(""),
            Err(EventDecodeError::Json(_))
        ));
        assert!(matches!(
            EventType::decode("not json"),
            Err(EventDecodeError::Json(_))
        ));
        assert!(matches!(
            EventType::decode("{}"),
            Err(EventDecodeError::Json(_))
        ));
        assert!(matches!(
            EventType::decode("[1,2]"),
            Err(EventDecodeError::Json(_))
        ));
        assert!(matches!(
            EventType::decode(r#"{"type":"NotAnEvent"}"#),
            Err(EventDecodeError::UnknownClass(name)) if name == "NotAnEvent"
        ));
        // A known class with a missing required payload field.
        assert!(matches!(
            EventType::decode(r#"{"type":"GameOverEvent"}"#),
            Err(EventDecodeError::Json(_))
        ));
        // Nullable payload fields may be omitted (`Option` fields default to
        // `None`), which keeps older/newer payload projections compatible.
        assert_eq!(
            EventType::decode(r#"{"type":"UnitCreateEvent"}"#).unwrap(),
            EventType::UnitCreate {
                unit: None,
                spawner: None,
                spawner_unit: None,
            }
        );
        // Wrong payload type.
        assert!(matches!(
            EventType::decode(r#"{"type":"GameOverEvent","winner":"blue"}"#),
            Err(EventDecodeError::Json(_))
        ));
    }
}
