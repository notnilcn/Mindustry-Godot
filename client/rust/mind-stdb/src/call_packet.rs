// SPDX-License-Identifier: GPL-3.0-only

//! Server→client `@Remote` call packets and their dispatch channel
//! (GAPS #35 / PAR-0546).
//!
//! Java's `@Remote` annotation generates one `Call` method and one
//! `*CallPacket` class per overload (`core/build/generated/.../mindustry/gen`,
//! e.g. `InfoMessageCallPacket`, `MenuCallPacket`, `LabelCallPacket3`). The
//! packet body is exactly `write(Writes)`; the player id is the transport
//! envelope, not the body. The port has no arcnet transport, so this module
//! reuses the upstream field layouts inside a SpacetimeDB-relayed framing:
//!
//! ```text
//! [kind: u8][body: Call*CallPacket.write(...)]
//! ```
//!
//! [`CallKind`] is append-only ABI (a new call gets the next ordinal). Bodies
//! are byte-for-byte the `TypeIO` writers upstream uses, so a server adapter
//! only needs to prepend the kind byte. Ported calls (all server→client):
//! `infoMessage`, `menu`/`followUpMenu`/`hideFollowUpMenu`, `menuBuilder`/
//! `menuBuilderUpdate`/`hideMenuBuilder`, `setCameraPosition`, `sound`,
//! `soundAt`, `effect`, `effectReliable`, `playMusic`, `traceInfo`, the three
//! `label` and three `labelReliable` overloads, `removeWorldLabel`, `kick`
//! (string and [`KickReason`] forms).
//!
//! Not ported (owned elsewhere or awaiting another subsystem): the animation
//! snapshot family (`entitySnapshot`, `stateSnapshot`, `blockSnapshot`, ...),
//! the world-mutation calls (`setTile`, `setFloor`, ...), `effect` with an
//! `Object data` payload (`TypeIO.writeObject`), and the remaining logic/UI
//! calls (`setHudText`, `announce`, `infoPopup`, `openURI`, ...) that
//! `UiEventKind` already carries over the relay.
//!
//! Malformed/unknown calls never panic: [`decode`] rejects them with
//! [`CallError`] and [`CallChannel`] records the rejection while continuing to
//! accept later calls (a newer server must not wedge an older client).

use std::collections::VecDeque;

use crate::typeio::{
    self, EffectId, KickReason, Rgba, SoundId, TraceInfo, TypeIoError, TypeIoReader, TypeIoWriter,
    UiNode,
};

/// Maximum decoded calls retained before the oldest is dropped
/// ([`CallChannel`] backpressure: a spamming server cannot grow client memory).
pub const CALL_QUEUE_LIMIT: usize = 256;

/// One server→client `@Remote` call kind. Ordinals are ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum CallKind {
    /// `Call.infoMessage(String)`.
    InfoMessage,
    /// `Call.menu(int, String, String, String[][])`.
    Menu,
    /// `Call.followUpMenu(int, String, String, String[][])`.
    FollowUpMenu,
    /// `Call.hideFollowUpMenu(int)`.
    HideFollowUpMenu,
    /// `Call.menuBuilder(int, long, String, boolean, boolean, boolean, NodeBuilder)`.
    MenuBuilder,
    /// `Call.menuBuilderUpdate(int, String, NodeBuilder)`.
    MenuBuilderUpdate,
    /// `Call.hideMenuBuilder(int)`.
    HideMenuBuilder,
    /// `Call.setCameraPosition(float, float)`.
    SetCameraPosition,
    /// `Call.sound(Sound, float, float, float)`.
    Sound,
    /// `Call.soundAt(Sound, float, float, float, float)`.
    SoundAt,
    /// `Call.effect(Effect, float, float, float, Color)`.
    Effect,
    /// `Call.effectReliable(Effect, float, float, float, Color)`.
    EffectReliable,
    /// `Call.playMusic(String, boolean)`.
    PlayMusic,
    /// `Call.traceInfo(Player, TraceInfo)`.
    TraceInfo,
    /// `Call.label(String, float, float, float)`.
    Label,
    /// `Call.label(String, int, float, float, float)`.
    LabelWithId,
    /// `Call.label(String, int, float, float, float, int)`.
    LabelWithFlags,
    /// `Call.labelReliable(String, float, float, float)`.
    LabelReliable,
    /// `Call.labelReliable(String, int, float, float, float)`.
    LabelReliableWithId,
    /// `Call.labelReliable(String, int, float, float, float, int)`.
    LabelReliableWithFlags,
    /// `Call.removeWorldLabel(int)`.
    RemoveWorldLabel,
    /// `Call.kick(NetConnection, String)`.
    Kick,
    /// `Call.kick(NetConnection, KickReason)`.
    KickReason,
}

impl CallKind {
    /// Every kind, in ABI order.
    pub const ALL: [Self; 23] = [
        Self::InfoMessage,
        Self::Menu,
        Self::FollowUpMenu,
        Self::HideFollowUpMenu,
        Self::MenuBuilder,
        Self::MenuBuilderUpdate,
        Self::HideMenuBuilder,
        Self::SetCameraPosition,
        Self::Sound,
        Self::SoundAt,
        Self::Effect,
        Self::EffectReliable,
        Self::PlayMusic,
        Self::TraceInfo,
        Self::Label,
        Self::LabelWithId,
        Self::LabelWithFlags,
        Self::LabelReliable,
        Self::LabelReliableWithId,
        Self::LabelReliableWithFlags,
        Self::RemoveWorldLabel,
        Self::Kick,
        Self::KickReason,
    ];

    /// Wire ordinal.
    pub fn ordinal(self) -> u8 {
        self as u8
    }

    /// Inverse of [`Self::ordinal`].
    pub fn from_ordinal(ordinal: u8) -> Option<Self> {
        Self::ALL.get(usize::from(ordinal)).copied()
    }

    /// Upstream packet class name (`mindustry.gen.*CallPacket`).
    pub fn packet_name(self) -> &'static str {
        match self {
            Self::InfoMessage => "InfoMessageCallPacket",
            Self::Menu => "MenuCallPacket",
            Self::FollowUpMenu => "FollowUpMenuCallPacket",
            Self::HideFollowUpMenu => "HideFollowUpMenuCallPacket",
            Self::MenuBuilder => "MenuBuilderCallPacket",
            Self::MenuBuilderUpdate => "MenuBuilderUpdateCallPacket",
            Self::HideMenuBuilder => "HideMenuBuilderCallPacket",
            Self::SetCameraPosition => "SetCameraPositionCallPacket",
            Self::Sound => "SoundCallPacket",
            Self::SoundAt => "SoundAtCallPacket",
            Self::Effect => "EffectCallPacket",
            Self::EffectReliable => "EffectReliableCallPacket",
            Self::PlayMusic => "PlayMusicCallPacket",
            Self::TraceInfo => "TraceInfoCallPacket",
            Self::Label => "LabelCallPacket",
            Self::LabelWithId => "LabelCallPacket2",
            Self::LabelWithFlags => "LabelCallPacket3",
            Self::LabelReliable => "LabelReliableCallPacket",
            Self::LabelReliableWithId => "LabelReliableCallPacket2",
            Self::LabelReliableWithFlags => "LabelReliableCallPacket3",
            Self::RemoveWorldLabel => "RemoveWorldLabelCallPacket",
            Self::Kick => "KickCallPacket",
            Self::KickReason => "KickCallPacket2",
        }
    }

    /// Upstream `Call` method name.
    pub fn call_name(self) -> &'static str {
        match self {
            Self::InfoMessage => "infoMessage",
            Self::Menu => "menu",
            Self::FollowUpMenu => "followUpMenu",
            Self::HideFollowUpMenu => "hideFollowUpMenu",
            Self::MenuBuilder => "menuBuilder",
            Self::MenuBuilderUpdate => "menuBuilderUpdate",
            Self::HideMenuBuilder => "hideMenuBuilder",
            Self::SetCameraPosition => "setCameraPosition",
            Self::Sound => "sound",
            Self::SoundAt => "soundAt",
            Self::Effect => "effect",
            Self::EffectReliable => "effectReliable",
            Self::PlayMusic => "playMusic",
            Self::TraceInfo => "traceInfo",
            Self::Label | Self::LabelWithId | Self::LabelWithFlags => "label",
            Self::LabelReliable | Self::LabelReliableWithId | Self::LabelReliableWithFlags => {
                "labelReliable"
            }
            Self::RemoveWorldLabel => "removeWorldLabel",
            Self::Kick | Self::KickReason => "kick",
        }
    }
}

/// One decoded server→client call. Field order is the upstream packet body.
#[derive(Debug, Clone, PartialEq)]
pub enum CallPacket {
    /// `Menus.infoMessage`.
    InfoMessage {
        /// Message text (`null` message shows nothing).
        message: Option<String>,
    },
    /// `Menus.menu`.
    Menu {
        /// Registered menu id.
        menu_id: i32,
        /// Title.
        title: Option<String>,
        /// Message body.
        message: Option<String>,
        /// Rows of button labels (`null` = no button).
        options: Vec<Vec<Option<String>>>,
    },
    /// `Menus.followUpMenu`.
    FollowUpMenu {
        /// Menu id.
        menu_id: i32,
        /// Title.
        title: Option<String>,
        /// Message body.
        message: Option<String>,
        /// Rows of button labels.
        options: Vec<Vec<Option<String>>>,
    },
    /// `Menus.hideFollowUpMenu`.
    HideFollowUpMenu {
        /// Menu id.
        menu_id: i32,
    },
    /// `Menus.menuBuilder`: a server-authored UI tree.
    MenuBuilder {
        /// Menu id.
        id: i32,
        /// Result token echoed by `menuBuilderChoose`.
        token: i64,
        /// Title.
        title: Option<String>,
        /// Close on any click.
        hide_on_click: bool,
        /// Close the previous builder dialog.
        hide_previous: bool,
        /// Fill the screen instead of a floating dialog.
        fill_screen: bool,
        /// Root node.
        builder: UiNode,
    },
    /// `Menus.menuBuilderUpdate`: replace one table inside an open builder.
    MenuBuilderUpdate {
        /// Menu id.
        id: i32,
        /// Table id inside the tree.
        table_id: Option<String>,
        /// Replacement node.
        builder: UiNode,
    },
    /// `Menus.hideMenuBuilder`.
    HideMenuBuilder {
        /// Menu id.
        id: i32,
    },
    /// `NetClient.setCameraPosition`: server-driven camera pan.
    SetCameraPosition {
        /// World x.
        x: f32,
        /// World y.
        y: f32,
    },
    /// `NetClient.sound`: non-positional sound.
    Sound {
        /// Standard sound id, `None` = `Sounds.none`.
        sound: Option<SoundId>,
        /// Volume.
        volume: f32,
        /// Pitch.
        pitch: f32,
        /// Pan (-1..1).
        pan: f32,
    },
    /// `NetClient.soundAt`: positional sound.
    SoundAt {
        /// Standard sound id.
        sound: Option<SoundId>,
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Volume.
        volume: f32,
        /// Pitch.
        pitch: f32,
    },
    /// `NetClient.effect`: fire one visual effect.
    Effect {
        /// Effect content id (`None` = null).
        effect: Option<EffectId>,
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Rotation in degrees.
        rotation: f32,
        /// Tint colour.
        color: Rgba,
    },
    /// `NetClient.effectReliable`.
    EffectReliable {
        /// Effect content id.
        effect: Option<EffectId>,
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Rotation in degrees.
        rotation: f32,
        /// Tint colour.
        color: Rgba,
    },
    /// `NetClient.playMusic`.
    PlayMusic {
        /// Music track name.
        music: Option<String>,
        /// Interrupt the current track.
        interrupt: bool,
    },
    /// `NetClient.traceInfo`: `/trace` result display.
    TraceInfo {
        /// Traced player entity id (`None` = not a live player).
        player: Option<i32>,
        /// Trace payload.
        info: TraceInfo,
    },
    /// `Call.label(String, float, float, float)`.
    Label {
        /// Text.
        message: Option<String>,
        /// Lifetime in seconds (`f32.max` = permanent).
        duration: f32,
        /// World x.
        world_x: f32,
        /// World y.
        world_y: f32,
    },
    /// `Call.label(String, int, float, float, float)`.
    LabelWithId {
        /// Text.
        message: Option<String>,
        /// Label id (replaces an existing label with the same id).
        id: i32,
        /// Lifetime in seconds.
        duration: f32,
        /// World x.
        world_x: f32,
        /// World y.
        world_y: f32,
    },
    /// `Call.label(String, int, float, float, float, int)`.
    LabelWithFlags {
        /// Text.
        message: Option<String>,
        /// Label id.
        id: i32,
        /// Lifetime in seconds.
        duration: f32,
        /// World x.
        world_x: f32,
        /// World y.
        world_y: f32,
        /// `LabelFlags` bit mask.
        flags: i32,
    },
    /// `Call.labelReliable(String, float, float, float)`.
    LabelReliable {
        /// Text.
        message: Option<String>,
        /// Lifetime in seconds.
        duration: f32,
        /// World x.
        world_x: f32,
        /// World y.
        world_y: f32,
    },
    /// `Call.labelReliable(String, int, float, float, float)`.
    LabelReliableWithId {
        /// Text.
        message: Option<String>,
        /// Label id.
        id: i32,
        /// Lifetime in seconds.
        duration: f32,
        /// World x.
        world_x: f32,
        /// World y.
        world_y: f32,
    },
    /// `Call.labelReliable(String, int, float, float, float, int)`.
    LabelReliableWithFlags {
        /// Text.
        message: Option<String>,
        /// Label id.
        id: i32,
        /// Lifetime in seconds.
        duration: f32,
        /// World x.
        world_x: f32,
        /// World y.
        world_y: f32,
        /// `LabelFlags` bit mask.
        flags: i32,
    },
    /// `Call.removeWorldLabel`.
    RemoveWorldLabel {
        /// Label id.
        id: i32,
    },
    /// `Call.kick(NetConnection, String)`: arbitrary kick reason.
    Kick {
        /// Shown reason text.
        reason: Option<String>,
    },
    /// `Call.kick(NetConnection, KickReason)`: localized kick reason.
    KickReason {
        /// Reason enum.
        reason: KickReason,
    },
}

impl CallPacket {
    /// Wire kind of this call.
    pub fn kind(&self) -> CallKind {
        match self {
            Self::InfoMessage { .. } => CallKind::InfoMessage,
            Self::Menu { .. } => CallKind::Menu,
            Self::FollowUpMenu { .. } => CallKind::FollowUpMenu,
            Self::HideFollowUpMenu { .. } => CallKind::HideFollowUpMenu,
            Self::MenuBuilder { .. } => CallKind::MenuBuilder,
            Self::MenuBuilderUpdate { .. } => CallKind::MenuBuilderUpdate,
            Self::HideMenuBuilder { .. } => CallKind::HideMenuBuilder,
            Self::SetCameraPosition { .. } => CallKind::SetCameraPosition,
            Self::Sound { .. } => CallKind::Sound,
            Self::SoundAt { .. } => CallKind::SoundAt,
            Self::Effect { .. } => CallKind::Effect,
            Self::EffectReliable { .. } => CallKind::EffectReliable,
            Self::PlayMusic { .. } => CallKind::PlayMusic,
            Self::TraceInfo { .. } => CallKind::TraceInfo,
            Self::Label { .. } => CallKind::Label,
            Self::LabelWithId { .. } => CallKind::LabelWithId,
            Self::LabelWithFlags { .. } => CallKind::LabelWithFlags,
            Self::LabelReliable { .. } => CallKind::LabelReliable,
            Self::LabelReliableWithId { .. } => CallKind::LabelReliableWithId,
            Self::LabelReliableWithFlags { .. } => CallKind::LabelReliableWithFlags,
            Self::RemoveWorldLabel { .. } => CallKind::RemoveWorldLabel,
            Self::Kick { .. } => CallKind::Kick,
            Self::KickReason { .. } => CallKind::KickReason,
        }
    }

    /// Encodes `[kind][body]`.
    pub fn encode(&self) -> Result<Vec<u8>, CallError> {
        let mut writer = TypeIoWriter::new();
        writer.ub(self.kind().ordinal());
        match self {
            Self::InfoMessage { message } => typeio::write_string(&mut writer, message.as_deref())?,
            Self::Menu {
                menu_id,
                title,
                message,
                options,
            }
            | Self::FollowUpMenu {
                menu_id,
                title,
                message,
                options,
            } => {
                writer.i(*menu_id);
                typeio::write_string(&mut writer, title.as_deref())?;
                typeio::write_string(&mut writer, message.as_deref())?;
                typeio::write_string_array(&mut writer, options)?;
            }
            Self::HideFollowUpMenu { menu_id } => writer.i(*menu_id),
            Self::MenuBuilder {
                id,
                token,
                title,
                hide_on_click,
                hide_previous,
                fill_screen,
                builder,
            } => {
                writer.i(*id);
                writer.l(*token);
                typeio::write_string(&mut writer, title.as_deref())?;
                writer.bool(*hide_on_click);
                writer.bool(*hide_previous);
                writer.bool(*fill_screen);
                typeio::write_ui_builder(&mut writer, builder)?;
            }
            Self::MenuBuilderUpdate {
                id,
                table_id,
                builder,
            } => {
                writer.i(*id);
                typeio::write_string(&mut writer, table_id.as_deref())?;
                typeio::write_ui_builder(&mut writer, builder)?;
            }
            Self::HideMenuBuilder { id } => writer.i(*id),
            Self::SetCameraPosition { x, y } => {
                writer.f(*x);
                writer.f(*y);
            }
            Self::Sound {
                sound,
                volume,
                pitch,
                pan,
            } => {
                typeio::write_sound(&mut writer, *sound);
                writer.f(*volume);
                writer.f(*pitch);
                writer.f(*pan);
            }
            Self::SoundAt {
                sound,
                x,
                y,
                volume,
                pitch,
            } => {
                typeio::write_sound(&mut writer, *sound);
                writer.f(*x);
                writer.f(*y);
                writer.f(*volume);
                writer.f(*pitch);
            }
            Self::Effect {
                effect,
                x,
                y,
                rotation,
                color,
            }
            | Self::EffectReliable {
                effect,
                x,
                y,
                rotation,
                color,
            } => {
                typeio::write_effect(&mut writer, *effect);
                writer.f(*x);
                writer.f(*y);
                writer.f(*rotation);
                typeio::write_color(&mut writer, *color);
            }
            Self::PlayMusic { music, interrupt } => {
                typeio::write_string(&mut writer, music.as_deref())?;
                writer.bool(*interrupt);
            }
            Self::TraceInfo { player, info } => {
                typeio::write_entity(&mut writer, *player);
                typeio::write_trace_info(&mut writer, info)?;
            }
            Self::Label {
                message,
                duration,
                world_x,
                world_y,
            }
            | Self::LabelReliable {
                message,
                duration,
                world_x,
                world_y,
            } => {
                typeio::write_string(&mut writer, message.as_deref())?;
                writer.f(*duration);
                writer.f(*world_x);
                writer.f(*world_y);
            }
            Self::LabelWithId {
                message,
                id,
                duration,
                world_x,
                world_y,
            }
            | Self::LabelReliableWithId {
                message,
                id,
                duration,
                world_x,
                world_y,
            } => {
                typeio::write_string(&mut writer, message.as_deref())?;
                writer.i(*id);
                writer.f(*duration);
                writer.f(*world_x);
                writer.f(*world_y);
            }
            Self::LabelWithFlags {
                message,
                id,
                duration,
                world_x,
                world_y,
                flags,
            }
            | Self::LabelReliableWithFlags {
                message,
                id,
                duration,
                world_x,
                world_y,
                flags,
            } => {
                typeio::write_string(&mut writer, message.as_deref())?;
                writer.i(*id);
                writer.f(*duration);
                writer.f(*world_x);
                writer.f(*world_y);
                writer.i(*flags);
            }
            Self::RemoveWorldLabel { id } => writer.i(*id),
            Self::Kick { reason } => typeio::write_string(&mut writer, reason.as_deref())?,
            Self::KickReason { reason } => typeio::write_kick(&mut writer, *reason),
        }
        Ok(writer.into_bytes())
    }

    /// Stable JSON projection for the `MindNet` debug surface / UI consumer.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind().ordinal(),
            "packet": self.kind().packet_name(),
            "call": self.kind().call_name(),
            "body": self.body_json(),
        })
    }

    fn body_json(&self) -> serde_json::Value {
        match self {
            Self::InfoMessage { message } => serde_json::json!({ "message": message }),
            Self::Menu {
                menu_id,
                title,
                message,
                options,
            }
            | Self::FollowUpMenu {
                menu_id,
                title,
                message,
                options,
            } => serde_json::json!({
                "menuId": menu_id,
                "title": title,
                "message": message,
                "options": options_json(options),
            }),
            Self::HideFollowUpMenu { menu_id } => serde_json::json!({ "menuId": menu_id }),
            Self::MenuBuilder {
                id,
                token,
                title,
                hide_on_click,
                hide_previous,
                fill_screen,
                builder,
            } => serde_json::json!({
                "id": id,
                "token": token,
                "title": title,
                "hideOnClick": hide_on_click,
                "hidePrevious": hide_previous,
                "fillScreen": fill_screen,
                "builder": format!("{builder:?}"),
            }),
            Self::MenuBuilderUpdate {
                id,
                table_id,
                builder,
            } => serde_json::json!({
                "id": id,
                "tableId": table_id,
                "builder": format!("{builder:?}"),
            }),
            Self::HideMenuBuilder { id } => serde_json::json!({ "id": id }),
            Self::SetCameraPosition { x, y } => serde_json::json!({ "x": x, "y": y }),
            Self::Sound {
                sound,
                volume,
                pitch,
                pan,
            } => serde_json::json!({
                "sound": sound.map(|sound| sound.0),
                "volume": volume,
                "pitch": pitch,
                "pan": pan,
            }),
            Self::SoundAt {
                sound,
                x,
                y,
                volume,
                pitch,
            } => serde_json::json!({
                "sound": sound.map(|sound| sound.0),
                "x": x,
                "y": y,
                "volume": volume,
                "pitch": pitch,
            }),
            Self::Effect {
                effect,
                x,
                y,
                rotation,
                color,
            }
            | Self::EffectReliable {
                effect,
                x,
                y,
                rotation,
                color,
            } => serde_json::json!({
                "effect": effect.map(|effect| effect.0),
                "x": x,
                "y": y,
                "rotation": rotation,
                "color": color.to_string(),
            }),
            Self::PlayMusic { music, interrupt } => {
                serde_json::json!({ "music": music, "interrupt": interrupt })
            }
            Self::TraceInfo { player, info } => serde_json::json!({
                "player": player,
                "ip": info.ip.clone(),
                "uuid": info.uuid.clone(),
                "locale": info.locale.clone(),
                "modded": info.modded,
                "mobile": info.mobile,
                "timesJoined": info.times_joined,
                "timesKicked": info.times_kicked,
                "ips": info.ips.clone(),
                "names": info.names.clone(),
            }),
            Self::Label {
                message,
                duration,
                world_x,
                world_y,
            }
            | Self::LabelReliable {
                message,
                duration,
                world_x,
                world_y,
            } => serde_json::json!({
                "message": message,
                "duration": duration,
                "worldX": world_x,
                "worldY": world_y,
            }),
            Self::LabelWithId {
                message,
                id,
                duration,
                world_x,
                world_y,
            }
            | Self::LabelReliableWithId {
                message,
                id,
                duration,
                world_x,
                world_y,
            } => serde_json::json!({
                "message": message,
                "id": id,
                "duration": duration,
                "worldX": world_x,
                "worldY": world_y,
            }),
            Self::LabelWithFlags {
                message,
                id,
                duration,
                world_x,
                world_y,
                flags,
            }
            | Self::LabelReliableWithFlags {
                message,
                id,
                duration,
                world_x,
                world_y,
                flags,
            } => serde_json::json!({
                "message": message,
                "id": id,
                "duration": duration,
                "worldX": world_x,
                "worldY": world_y,
                "flags": flags,
            }),
            Self::RemoveWorldLabel { id } => serde_json::json!({ "id": id }),
            Self::Kick { reason } => serde_json::json!({ "reason": reason }),
            Self::KickReason { reason } => serde_json::json!({
                "reason": reason.name(),
                "bundleKey": reason.bundle_key(),
                "quiet": reason.quiet(),
            }),
        }
    }
}

fn options_json(options: &[Vec<Option<String>>]) -> serde_json::Value {
    serde_json::Value::Array(
        options
            .iter()
            .map(|row| {
                serde_json::Value::Array(
                    row.iter()
                        .map(|cell| match cell {
                            Some(text) => serde_json::Value::String(text.clone()),
                            None => serde_json::Value::Null,
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

/// Decode failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    /// The kind byte has no [`CallKind`].
    #[error("unknown server call kind: {0}")]
    UnknownCall(u8),
    /// The body did not match the kind's layout.
    #[error("malformed call body: {0}")]
    Codec(#[from] TypeIoError),
}

/// Decodes `[kind][body]`; rejects unknown kinds and malformed bodies.
pub fn decode(bytes: &[u8]) -> Result<CallPacket, CallError> {
    let mut reader = TypeIoReader::new(bytes);
    let ordinal = reader.ub().map_err(|_| {
        // A zero-length payload has no kind byte at all.
        CallError::Codec(TypeIoError::UnexpectedEof {
            needed: 1,
            remaining: 0,
        })
    })?;
    let kind = CallKind::from_ordinal(ordinal).ok_or(CallError::UnknownCall(ordinal))?;
    let call = read_body(&mut reader, kind)?;
    reader.expect_end()?;
    Ok(call)
}

fn read_body(reader: &mut TypeIoReader<'_>, kind: CallKind) -> Result<CallPacket, CallError> {
    let call = match kind {
        CallKind::InfoMessage => CallPacket::InfoMessage {
            message: typeio::read_string(reader)?,
        },
        CallKind::Menu | CallKind::FollowUpMenu => {
            let menu_id = reader.i()?;
            let title = typeio::read_string(reader)?;
            let message = typeio::read_string(reader)?;
            let options = typeio::read_string_array(reader)?;
            if kind == CallKind::Menu {
                CallPacket::Menu {
                    menu_id,
                    title,
                    message,
                    options,
                }
            } else {
                CallPacket::FollowUpMenu {
                    menu_id,
                    title,
                    message,
                    options,
                }
            }
        }
        CallKind::HideFollowUpMenu => CallPacket::HideFollowUpMenu {
            menu_id: reader.i()?,
        },
        CallKind::MenuBuilder => CallPacket::MenuBuilder {
            id: reader.i()?,
            token: reader.l()?,
            title: typeio::read_string(reader)?,
            hide_on_click: reader.bool()?,
            hide_previous: reader.bool()?,
            fill_screen: reader.bool()?,
            builder: typeio::read_ui_builder(reader)?,
        },
        CallKind::MenuBuilderUpdate => CallPacket::MenuBuilderUpdate {
            id: reader.i()?,
            table_id: typeio::read_string(reader)?,
            builder: typeio::read_ui_builder(reader)?,
        },
        CallKind::HideMenuBuilder => CallPacket::HideMenuBuilder { id: reader.i()? },
        CallKind::SetCameraPosition => CallPacket::SetCameraPosition {
            x: reader.f()?,
            y: reader.f()?,
        },
        CallKind::Sound => CallPacket::Sound {
            sound: typeio::read_sound(reader)?,
            volume: reader.f()?,
            pitch: reader.f()?,
            pan: reader.f()?,
        },
        CallKind::SoundAt => CallPacket::SoundAt {
            sound: typeio::read_sound(reader)?,
            x: reader.f()?,
            y: reader.f()?,
            volume: reader.f()?,
            pitch: reader.f()?,
        },
        CallKind::Effect | CallKind::EffectReliable => {
            let effect = typeio::read_effect(reader)?;
            let x = reader.f()?;
            let y = reader.f()?;
            let rotation = reader.f()?;
            let color = typeio::read_color(reader)?;
            if kind == CallKind::Effect {
                CallPacket::Effect {
                    effect,
                    x,
                    y,
                    rotation,
                    color,
                }
            } else {
                CallPacket::EffectReliable {
                    effect,
                    x,
                    y,
                    rotation,
                    color,
                }
            }
        }
        CallKind::PlayMusic => CallPacket::PlayMusic {
            music: typeio::read_string(reader)?,
            interrupt: reader.bool()?,
        },
        CallKind::TraceInfo => CallPacket::TraceInfo {
            player: typeio::read_entity(reader)?,
            info: typeio::read_trace_info(reader)?,
        },
        CallKind::Label | CallKind::LabelReliable => {
            let message = typeio::read_string(reader)?;
            let duration = reader.f()?;
            let world_x = reader.f()?;
            let world_y = reader.f()?;
            if kind == CallKind::Label {
                CallPacket::Label {
                    message,
                    duration,
                    world_x,
                    world_y,
                }
            } else {
                CallPacket::LabelReliable {
                    message,
                    duration,
                    world_x,
                    world_y,
                }
            }
        }
        CallKind::LabelWithId | CallKind::LabelReliableWithId => {
            let message = typeio::read_string(reader)?;
            let id = reader.i()?;
            let duration = reader.f()?;
            let world_x = reader.f()?;
            let world_y = reader.f()?;
            if kind == CallKind::LabelWithId {
                CallPacket::LabelWithId {
                    message,
                    id,
                    duration,
                    world_x,
                    world_y,
                }
            } else {
                CallPacket::LabelReliableWithId {
                    message,
                    id,
                    duration,
                    world_x,
                    world_y,
                }
            }
        }
        CallKind::LabelWithFlags | CallKind::LabelReliableWithFlags => {
            let message = typeio::read_string(reader)?;
            let id = reader.i()?;
            let duration = reader.f()?;
            let world_x = reader.f()?;
            let world_y = reader.f()?;
            let flags = reader.i()?;
            if kind == CallKind::LabelWithFlags {
                CallPacket::LabelWithFlags {
                    message,
                    id,
                    duration,
                    world_x,
                    world_y,
                    flags,
                }
            } else {
                CallPacket::LabelReliableWithFlags {
                    message,
                    id,
                    duration,
                    world_x,
                    world_y,
                    flags,
                }
            }
        }
        CallKind::RemoveWorldLabel => CallPacket::RemoveWorldLabel { id: reader.i()? },
        CallKind::Kick => CallPacket::Kick {
            reason: typeio::read_string(reader)?,
        },
        CallKind::KickReason => CallPacket::KickReason {
            reason: typeio::read_kick(reader)?,
        },
    };
    Ok(call)
}

/// Why a call was rejected by [`CallChannel`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallReject {
    /// A kind byte the client does not know (newer server).
    #[error("unknown server call kind: {0}")]
    UnknownCall(u8),
    /// A known kind with a corrupt body.
    #[error("malformed {kind:?} call: {error}")]
    Malformed {
        /// Kind byte, when it was at least readable.
        kind: Option<CallKind>,
        /// Codec failure.
        error: TypeIoError,
    },
    /// The bounded queue dropped the oldest call to make room.
    #[error("call queue overflow: dropped the oldest queued call")]
    Overflow,
}

/// Consumer of drained calls.
pub trait CallHandler {
    /// Applies one call.
    fn handle_call(&mut self, call: CallPacket);

    /// Observes a reject; the channel keeps accepting later calls.
    fn handle_reject(&mut self, _reject: &CallReject) {}
}

/// Ordered, bounded queue of server→client calls with rejection bookkeeping.
///
/// The transport pushes raw `[kind][body]` frames (in arrival/commit order);
/// the game loop drains them at a tick boundary and dispatches through a
/// [`CallHandler`]. Unknown or corrupt frames are recorded and skipped.
#[derive(Debug)]
pub struct CallChannel {
    queue: VecDeque<CallPacket>,
    rejects: Vec<CallReject>,
    accepted: u64,
    rejected: u64,
    dropped: u64,
    limit: usize,
}

impl Default for CallChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl CallChannel {
    /// Empty channel with the default [`CALL_QUEUE_LIMIT`].
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            rejects: Vec::new(),
            accepted: 0,
            rejected: 0,
            dropped: 0,
            limit: CALL_QUEUE_LIMIT,
        }
    }

    /// Empty channel retaining at most `limit` undrained calls.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            ..Self::new()
        }
    }

    /// Decodes and queues one frame. Returns the decoded call's kind on
    /// success; a rejection is recorded and returned to the caller.
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Result<CallKind, CallReject> {
        match decode(bytes) {
            Ok(call) => {
                let kind = call.kind();
                self.push(call);
                Ok(kind)
            }
            Err(CallError::UnknownCall(ordinal)) => {
                let reject = CallReject::UnknownCall(ordinal);
                self.record_reject(reject.clone());
                Err(reject)
            }
            Err(CallError::Codec(error)) => {
                let reject = CallReject::Malformed {
                    kind: CallKind::from_ordinal(bytes.first().copied().unwrap_or(0)),
                    error,
                };
                self.record_reject(reject.clone());
                Err(reject)
            }
        }
    }

    /// Queues one already-decoded call (tests, local host echo).
    pub fn push(&mut self, call: CallPacket) {
        if self.queue.len() >= self.limit {
            self.queue.pop_front();
            self.dropped = self.dropped.saturating_add(1);
            self.record_reject(CallReject::Overflow);
        }
        self.queue.push_back(call);
        self.accepted = self.accepted.saturating_add(1);
    }

    /// Queued calls in arrival order (does not dequeue).
    pub fn queued(&self) -> impl Iterator<Item = &CallPacket> {
        self.queue.iter()
    }

    /// Undrained call count.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether no call is queued.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Drops and returns every queued call in arrival order.
    pub fn drain(&mut self) -> Vec<CallPacket> {
        self.queue.drain(..).collect()
    }

    /// Drains and dispatches every queued call; returns the dispatched count.
    pub fn dispatch(&mut self, handler: &mut dyn CallHandler) -> usize {
        let calls = self.drain();
        let count = calls.len();
        for call in calls {
            handler.handle_call(call);
        }
        count
    }

    /// Recorded rejections, oldest first.
    pub fn rejects(&self) -> &[CallReject] {
        &self.rejects
    }

    /// Accepted (queued) calls since creation.
    pub fn accepted(&self) -> u64 {
        self.accepted
    }

    /// Rejected frames since creation (unknown/malformed/overflow).
    pub fn rejected(&self) -> u64 {
        self.rejected
    }

    /// Calls dropped by the queue cap.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Queue cap.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Drops queued calls and every counter (disconnect/resync).
    pub fn clear(&mut self) {
        self.queue.clear();
        self.rejects.clear();
        self.accepted = 0;
        self.rejected = 0;
        self.dropped = 0;
    }

    fn record_reject(&mut self, reject: CallReject) {
        self.rejected = self.rejected.saturating_add(1);
        self.rejects.push(reject);
        // Reject history is diagnostic; keep a bounded tail like `EventBus`.
        if self.rejects.len() > CALL_QUEUE_LIMIT {
            self.rejects.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::typeio::{MenuValue, UiKey, UiValue};

    fn menu_result_node() -> UiNode {
        UiNode::new(UiKey::Table)
            .with(UiKey::Id, UiValue::Str("root".to_string()))
            .with(
                UiKey::Label,
                UiValue::Node(Box::new(
                    UiNode::new(UiKey::Label).with(UiKey::Text, UiValue::Str("hi".to_string())),
                )),
            )
    }

    fn options() -> Vec<Vec<Option<String>>> {
        vec![
            vec![Some("Yes".to_string()), Some("No".to_string())],
            vec![None],
        ]
    }

    /// One instance of every [`CallKind`] (the coverage test asserts the list
    /// names every kind).
    fn every_call() -> Vec<CallPacket> {
        vec![
            CallPacket::InfoMessage {
                message: Some("hello".to_string()),
            },
            CallPacket::Menu {
                menu_id: 3,
                title: Some("Title".to_string()),
                message: None,
                options: options(),
            },
            CallPacket::FollowUpMenu {
                menu_id: 4,
                title: None,
                message: Some("Follow up".to_string()),
                options: options(),
            },
            CallPacket::HideFollowUpMenu { menu_id: 4 },
            CallPacket::MenuBuilder {
                id: 5,
                token: 0x1234_5678_9abc_def0,
                title: Some("Builder".to_string()),
                hide_on_click: true,
                hide_previous: false,
                fill_screen: true,
                builder: menu_result_node(),
            },
            CallPacket::MenuBuilderUpdate {
                id: 5,
                table_id: Some("root".to_string()),
                builder: menu_result_node(),
            },
            CallPacket::HideMenuBuilder { id: 5 },
            CallPacket::SetCameraPosition { x: 12.5, y: -7.25 },
            CallPacket::Sound {
                sound: Some(SoundId(11)),
                volume: 0.8,
                pitch: 1.2,
                pan: -0.5,
            },
            CallPacket::SoundAt {
                sound: Some(SoundId(12)),
                x: 100.0,
                y: 200.0,
                volume: 0.9,
                pitch: 0.75,
            },
            CallPacket::Effect {
                effect: Some(EffectId(4)),
                x: 1.0,
                y: 2.0,
                rotation: 90.0,
                color: Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 4,
                },
            },
            CallPacket::EffectReliable {
                effect: None,
                x: -1.0,
                y: -2.0,
                rotation: 0.0,
                color: Rgba::default(),
            },
            CallPacket::PlayMusic {
                music: Some("game1".to_string()),
                interrupt: true,
            },
            CallPacket::TraceInfo {
                player: Some(17),
                info: TraceInfo {
                    ip: Some("10.0.0.1".to_string()),
                    uuid: Some("uuid".to_string()),
                    locale: Some("en".to_string()),
                    modded: true,
                    mobile: false,
                    times_joined: 3,
                    times_kicked: 1,
                    ips: vec![Some("10.0.0.1".to_string())],
                    names: vec![Some("player".to_string())],
                },
            },
            CallPacket::Label {
                message: Some("hi".to_string()),
                duration: 5.0,
                world_x: 10.0,
                world_y: 20.0,
            },
            CallPacket::LabelWithId {
                message: Some("hi".to_string()),
                id: 7,
                duration: 5.0,
                world_x: 10.0,
                world_y: 20.0,
            },
            CallPacket::LabelWithFlags {
                message: None,
                id: 7,
                duration: 5.0,
                world_x: 10.0,
                world_y: 20.0,
                flags: 0b101,
            },
            CallPacket::LabelReliable {
                message: Some("hi".to_string()),
                duration: 5.0,
                world_x: 10.0,
                world_y: 20.0,
            },
            CallPacket::LabelReliableWithId {
                message: Some("hi".to_string()),
                id: 8,
                duration: 5.0,
                world_x: 10.0,
                world_y: 20.0,
            },
            CallPacket::LabelReliableWithFlags {
                message: Some("hi".to_string()),
                id: 8,
                duration: 5.0,
                world_x: 10.0,
                world_y: 20.0,
                flags: 1,
            },
            CallPacket::RemoveWorldLabel { id: 8 },
            CallPacket::Kick {
                reason: Some("bye".to_string()),
            },
            CallPacket::KickReason {
                reason: KickReason::Banned,
            },
        ]
    }

    #[test]
    fn every_call_variant_round_trips() {
        let calls = every_call();
        assert_eq!(calls.len(), CallKind::ALL.len());
        let mut kinds: Vec<CallKind> = calls.iter().map(CallPacket::kind).collect();
        kinds.sort();
        assert_eq!(kinds, CallKind::ALL.to_vec());
        for call in calls {
            let bytes = call.encode().unwrap();
            assert_eq!(bytes[0], call.kind().ordinal());
            let decoded = decode(&bytes).unwrap();
            assert_eq!(decoded, call, "round trip failed for {call:?}");
            // The JSON projection names the upstream packet and call.
            let json = call.to_json();
            assert_eq!(json["packet"], call.kind().packet_name());
            assert_eq!(json["call"], call.kind().call_name());
        }
    }

    #[test]
    fn call_kinds_are_append_only_and_named() {
        for (index, kind) in CallKind::ALL.iter().enumerate() {
            assert_eq!(kind.ordinal() as usize, index);
            assert_eq!(CallKind::from_ordinal(kind.ordinal()), Some(*kind));
            assert!(kind.packet_name().contains("CallPacket"));
            assert!(!kind.call_name().is_empty());
        }
        assert_eq!(CallKind::from_ordinal(23), None);
        assert_eq!(CallKind::LabelWithId.packet_name(), "LabelCallPacket2");
        assert_eq!(CallKind::LabelWithFlags.packet_name(), "LabelCallPacket3");
    }

    #[test]
    fn unknown_and_malformed_calls_are_rejected() {
        assert!(matches!(
            decode(&[]),
            Err(CallError::Codec(TypeIoError::UnexpectedEof {
                needed: 1,
                remaining: 0
            }))
        ));
        assert_eq!(decode(&[200]), Err(CallError::UnknownCall(200)));
        assert_eq!(decode(&[23]), Err(CallError::UnknownCall(23)));

        // Truncated body (kind byte present, fields missing).
        let mut bytes = CallPacket::SetCameraPosition { x: 1.0, y: 2.0 }
            .encode()
            .unwrap();
        bytes.truncate(4);
        assert!(matches!(decode(&bytes), Err(CallError::Codec(_))));

        // Trailing bytes after a complete body.
        let mut bytes = CallPacket::HideMenuBuilder { id: 1 }.encode().unwrap();
        bytes.push(0);
        assert!(matches!(
            decode(&bytes),
            Err(CallError::Codec(TypeIoError::TrailingBytes { .. }))
        ));

        // Malformed nested UI tree.
        let mut bytes = CallPacket::MenuBuilderUpdate {
            id: 1,
            table_id: None,
            builder: menu_result_node(),
        }
        .encode()
        .unwrap();
        let last = bytes.len() - 1;
        bytes[last] = 200;
        assert!(matches!(decode(&bytes), Err(CallError::Codec(_))));

        // Kick with an out-of-range reason ordinal.
        let bytes = [CallKind::KickReason.ordinal(), 200];
        assert!(matches!(
            decode(&bytes),
            Err(CallError::Codec(TypeIoError::UnknownTag { .. }))
        ));
    }

    /// Handler used by the dispatch tests.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<CallKind>,
        rejects: Vec<CallReject>,
    }

    impl CallHandler for Recorder {
        fn handle_call(&mut self, call: CallPacket) {
            self.calls.push(call.kind());
        }

        fn handle_reject(&mut self, reject: &CallReject) {
            self.rejects.push(reject.clone());
        }
    }

    #[test]
    fn channel_decodes_dispatches_and_skips_bad_frames() {
        let mut channel = CallChannel::new();
        assert!(channel.is_empty());
        let mut good = Vec::new();
        for kind in [
            CallKind::InfoMessage,
            CallKind::SetCameraPosition,
            CallKind::PlayMusic,
        ] {
            let call = every_call()
                .into_iter()
                .find(|call| call.kind() == kind)
                .unwrap();
            good.push(call);
        }
        for call in &good {
            let bytes = call.encode().unwrap();
            assert_eq!(channel.push_bytes(&bytes), Ok(call.kind()));
        }
        assert_eq!(channel.len(), 3);
        assert_eq!(channel.accepted(), 3);
        assert_eq!(channel.rejected(), 0);

        // Unknown call: recorded, channel keeps working.
        assert_eq!(
            channel.push_bytes(&[250]),
            Err(CallReject::UnknownCall(250))
        );
        assert_eq!(channel.rejected(), 1);
        // Malformed known call: attributed to its kind.
        assert!(matches!(
            channel.push_bytes(&[CallKind::HideMenuBuilder.ordinal(), 0]),
            Err(CallReject::Malformed {
                kind: Some(CallKind::HideMenuBuilder),
                ..
            })
        ));
        assert_eq!(channel.rejected(), 2);
        assert_eq!(channel.len(), 3);

        let mut recorder = Recorder::default();
        assert_eq!(channel.dispatch(&mut recorder), 3);
        assert_eq!(
            recorder.calls,
            vec![
                CallKind::InfoMessage,
                CallKind::SetCameraPosition,
                CallKind::PlayMusic
            ]
        );
        assert!(channel.is_empty());
        assert_eq!(
            channel.rejects(),
            &[
                CallReject::UnknownCall(250),
                CallReject::Malformed {
                    kind: Some(CallKind::HideMenuBuilder),
                    error: TypeIoError::UnexpectedEof {
                        needed: 4,
                        remaining: 1
                    },
                },
            ]
        );
    }

    #[test]
    fn channel_bounds_its_queue_and_records_overflow() {
        let mut channel = CallChannel::with_limit(2);
        for id in 0..3 {
            channel.push(CallPacket::HideMenuBuilder { id });
        }
        assert_eq!(channel.len(), 2);
        assert_eq!(channel.dropped(), 1);
        assert_eq!(channel.rejects(), &[CallReject::Overflow]);
        let drained = channel.drain();
        assert_eq!(
            drained,
            vec![
                CallPacket::HideMenuBuilder { id: 1 },
                CallPacket::HideMenuBuilder { id: 2 }
            ]
        );

        // `clear` resets the queue and every counter.
        channel.push(CallPacket::KickReason {
            reason: KickReason::Vote,
        });
        channel.clear();
        assert!(channel.is_empty());
        assert_eq!(channel.accepted(), 0);
        assert_eq!(channel.rejected(), 0);
        assert_eq!(channel.dropped(), 0);
        assert!(channel.rejects().is_empty());
    }

    #[test]
    fn menu_result_and_ui_builder_payloads_survive_menu_calls() {
        let node = menu_result_node();
        let call = CallPacket::MenuBuilder {
            id: 9,
            token: 1,
            title: Some("t".to_string()),
            hide_on_click: false,
            hide_previous: true,
            fill_screen: false,
            builder: node.clone(),
        };
        let decoded = decode(&call.encode().unwrap()).unwrap();
        let CallPacket::MenuBuilder { builder, .. } = decoded else {
            panic!("expected a menu builder call");
        };
        assert_eq!(builder, node);
        assert_eq!(builder.str_value(UiKey::Id), Some("root"));

        // A menu result payload is the button-click reply, not a call; make
        // sure its value kinds stay distinct through the codec.
        let result = crate::typeio::MenuResult {
            token: 2,
            result: Some("ok".to_string()),
            values: vec![("slider".to_string(), MenuValue::Float(0.5))],
        };
        let mut writer = TypeIoWriter::new();
        crate::typeio::write_menu_result(&mut writer, &result).unwrap();
        let mut reader = TypeIoReader::new(writer.as_bytes());
        assert_eq!(
            crate::typeio::read_menu_result(&mut reader).unwrap(),
            result
        );
    }
}
