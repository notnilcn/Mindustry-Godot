// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/Menus.java (`@Remote` payloads),
//         core/src/mindustry/ui/builder/MenuBuilder.java/MenuResult.java.

//! Plan-14 UI relay payload codec (plan 14 §3.6/§6.3).
//!
//! `Menus.java` `@Remote` calls become plan-21 relay records. The transport
//! (STDB `match_ui_event` for host → client; `CommandKind` rows for client →
//! host) is plan 21's; this module owns the bytes. Every payload starts with a
//! `format: 1` header so a future version bump is explicit rather than a guess.
//!
//! `MenuBuilderShow`/`Update` embed a `ui_node` tree (§6.1); the rest are small
//! typed records. Nothing here touches Godot or the network.

use crate::ui::builder::menu_result::{MenuResult, MenuResultError};
use crate::ui::builder::ui_node::{Cursor, UiNode};
use crate::ui::builder::ui_key::UiKey;

/// Header byte for every plan-14 relay payload.
pub const UI_RELAY_FORMAT: u8 = 1;

/// Payload codec errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayError {
    /// Unknown format header byte.
    BadFormat(u8),
    /// Unknown value tag byte.
    BadTag(u8),
    /// Unexpected end of input.
    Eof,
    /// Invalid UTF-8 in a string payload.
    Utf8,
    /// Bytes left after decoding.
    TrailingBytes(usize),
    /// The embedded `ui_node` tree failed to decode.
    Node(crate::ui::builder::ui_node::WireError),
    /// A `MenuResult` sub-payload failed to decode.
    Result(MenuResultError),
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelayError::BadFormat(value) => write!(f, "unknown ui relay format {value}"),
            RelayError::BadTag(value) => write!(f, "unknown ui relay tag {value}"),
            RelayError::Eof => write!(f, "unexpected end of ui relay input"),
            RelayError::Utf8 => write!(f, "invalid utf-8 in ui relay string"),
            RelayError::TrailingBytes(count) => write!(f, "{count} trailing bytes"),
            RelayError::Node(error) => write!(f, "embedded ui node: {error}"),
            RelayError::Result(error) => write!(f, "embedded menu result: {error}"),
        }
    }
}

impl std::error::Error for RelayError {}

impl From<crate::ui::builder::ui_node::WireError> for RelayError {
    fn from(error: crate::ui::builder::ui_node::WireError) -> Self {
        RelayError::Node(error)
    }
}

impl From<MenuResultError> for RelayError {
    fn from(error: MenuResultError) -> Self {
        RelayError::Result(error)
    }
}

/// `menu_builder_show` payload (§6.3).
#[derive(Debug, Clone, PartialEq)]
pub struct MenuBuilderShow {
    /// Menu callback id.
    pub id: i32,
    /// Token echoed back in the result.
    pub token: i64,
    /// Title; `None` removes the orange title bar.
    pub title: Option<String>,
    /// Hide after a click listener fires.
    pub hide_on_click: bool,
    /// Hide an existing menu with the same id.
    pub hide_existing: bool,
    /// Fill the screen.
    pub fill_screen: bool,
    /// Menu body.
    pub ui: UiNode,
}

/// `menu_builder_update` payload (§6.3).
#[derive(Debug, Clone, PartialEq)]
pub struct MenuBuilderUpdate {
    /// Menu id to update.
    pub id: i32,
    /// Element id of the table to replace.
    pub table_id: String,
    /// Replacement body.
    pub ui: UiNode,
}

/// `menu_choose` payload (client → server).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuChoose {
    /// Menu callback id.
    pub menu_id: i32,
    /// Chosen option (`-1` = cancelled).
    pub option: i32,
}

/// `menu_builder_choose` payload (client → server).
#[derive(Debug, Clone, PartialEq)]
pub struct MenuBuilderChoose {
    /// Menu callback id.
    pub menu_id: i32,
    /// Captured result.
    pub result: MenuResult,
}

/// `text_input` payload (server → client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInput {
    /// Callback id.
    pub id: i32,
    /// Dialog title.
    pub title: String,
    /// Prompt body.
    pub message: String,
    /// Max length (upstream clamps to 1000).
    pub len: i32,
    /// Default text.
    pub def: String,
    /// Digits-only filter.
    pub numeric: bool,
    /// Allow an empty submission.
    pub allow_empty: bool,
}

/// `text_input_result` payload (client → server).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInputResult {
    /// Callback id.
    pub id: i32,
    /// Submitted text (`None` = cancelled).
    pub text: Option<String>,
}

/// `info_popup` payload (server → client).
#[derive(Debug, Clone, PartialEq)]
pub struct InfoPopup {
    /// Message (`None` removes the popup with `id`).
    pub message: Option<String>,
    /// Replacement id.
    pub id: Option<String>,
    /// Duration in seconds.
    pub duration: f32,
    /// Alignment bits.
    pub align: i32,
    /// Top inset.
    pub top: i32,
    /// Left inset.
    pub left: i32,
    /// Bottom inset.
    pub bottom: i32,
    /// Right inset.
    pub right: i32,
}

/// `label` payload (server → client).
#[derive(Debug, Clone, PartialEq)]
pub struct WorldLabel {
    /// Message (`None` removes the label).
    pub message: Option<String>,
    /// Label id.
    pub id: i32,
    /// Duration in seconds.
    pub duration: f32,
    /// World x.
    pub world_x: f32,
    /// World y.
    pub world_y: f32,
    /// `WorldLabel` flags (`background`/`outline`/`autoscale`).
    pub flags: i32,
}

/// `warning_toast` payload (server → client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarningToast {
    /// Icon unicode codepoint.
    pub icon: i32,
    /// Toast text.
    pub text: String,
}

/// `ping_marker` payload (server → client).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingMarker {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
}

/// Encodes `menu_builder_show`.
pub fn encode_menu_builder_show(value: &MenuBuilderShow) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_i32(&mut out, value.id);
    put_i64(&mut out, value.token);
    let mut flags = 0u8;
    if value.hide_on_click {
        flags |= 1;
    }
    if value.hide_existing {
        flags |= 1 << 1;
    }
    if value.fill_screen {
        flags |= 1 << 2;
    }
    if value.title.is_some() {
        flags |= 1 << 3;
    }
    out.push(flags);
    if let Some(title) = &value.title {
        put_str(&mut out, title);
    }
    value.ui.encode_into(&mut out);
    out
}

/// Decodes `menu_builder_show`.
pub fn decode_menu_builder_show(data: &[u8]) -> Result<MenuBuilderShow, RelayError> {
    let mut reader = Reader::new(data)?;
    let flags = reader.u8()?;
    let id = reader.i32()?;
    let token = reader.i64()?;
    let title = if flags & (1 << 3) != 0 {
        Some(reader.str()?)
    } else {
        None
    };
    let ui = reader.node()?;
    reader.finish()?;
    Ok(MenuBuilderShow {
        id,
        token,
        title,
        hide_on_click: flags & 1 != 0,
        hide_existing: flags & (1 << 1) != 0,
        fill_screen: flags & (1 << 2) != 0,
        ui,
    })
}

/// Encodes `menu_builder_update`.
pub fn encode_menu_builder_update(value: &MenuBuilderUpdate) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_i32(&mut out, value.id);
    put_str(&mut out, &value.table_id);
    value.ui.encode_into(&mut out);
    out
}

/// Decodes `menu_builder_update`.
pub fn decode_menu_builder_update(data: &[u8]) -> Result<MenuBuilderUpdate, RelayError> {
    let mut reader = Reader::new(data)?;
    let id = reader.i32()?;
    let table_id = reader.str()?;
    let ui = reader.node()?;
    reader.finish()?;
    Ok(MenuBuilderUpdate { id, table_id, ui })
}

/// Encodes `menu_builder_hide`.
pub fn encode_menu_builder_hide(id: i32) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_i32(&mut out, id);
    out
}

/// Decodes `menu_builder_hide`.
pub fn decode_menu_builder_hide(data: &[u8]) -> Result<i32, RelayError> {
    let mut reader = Reader::new(data)?;
    let id = reader.i32()?;
    reader.finish()?;
    Ok(id)
}

/// Encodes `menu_choose`.
pub fn encode_menu_choose(value: MenuChoose) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_i32(&mut out, value.menu_id);
    put_i32(&mut out, value.option);
    out
}

/// Decodes `menu_choose`.
pub fn decode_menu_choose(data: &[u8]) -> Result<MenuChoose, RelayError> {
    let mut reader = Reader::new(data)?;
    let menu_id = reader.i32()?;
    let option = reader.i32()?;
    reader.finish()?;
    Ok(MenuChoose { menu_id, option })
}

/// Encodes `menu_builder_choose`.
pub fn encode_menu_builder_choose(value: &MenuBuilderChoose) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_i32(&mut out, value.menu_id);
    out.extend_from_slice(&value.result.encode());
    out
}

/// Decodes `menu_builder_choose`.
pub fn decode_menu_builder_choose(data: &[u8]) -> Result<MenuBuilderChoose, RelayError> {
    let mut reader = Reader::new(data)?;
    let menu_id = reader.i32()?;
    let rest = &data[reader.pos..];
    let result = MenuResult::decode(rest)?;
    // `MenuResult::decode` consumes `rest`, so require it to be the tail.
    Ok(MenuBuilderChoose { menu_id, result })
}

/// Encodes `text_input`.
pub fn encode_text_input(value: &TextInput) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_i32(&mut out, value.id);
    put_str(&mut out, &value.title);
    put_str(&mut out, &value.message);
    put_i32(&mut out, value.len);
    put_str(&mut out, &value.def);
    let mut flags = 0u8;
    if value.numeric {
        flags |= 1;
    }
    if value.allow_empty {
        flags |= 1 << 1;
    }
    out.push(flags);
    out
}

/// Decodes `text_input`.
pub fn decode_text_input(data: &[u8]) -> Result<TextInput, RelayError> {
    let mut reader = Reader::new(data)?;
    let id = reader.i32()?;
    let title = reader.str()?;
    let message = reader.str()?;
    let len = reader.i32()?;
    let def = reader.str()?;
    let flags = reader.u8()?;
    reader.finish()?;
    Ok(TextInput {
        id,
        title,
        message,
        len,
        def,
        numeric: flags & 1 != 0,
        allow_empty: flags & (1 << 1) != 0,
    })
}

/// Encodes `text_input_result`.
pub fn encode_text_input_result(value: &TextInputResult) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_i32(&mut out, value.id);
    put_opt_str(&mut out, value.text.as_deref());
    out
}

/// Decodes `text_input_result`.
pub fn decode_text_input_result(data: &[u8]) -> Result<TextInputResult, RelayError> {
    let mut reader = Reader::new(data)?;
    let id = reader.i32()?;
    let text = reader.opt_str()?;
    reader.finish()?;
    Ok(TextInputResult { id, text })
}

/// Encodes `info_popup`.
pub fn encode_info_popup(value: &InfoPopup) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_opt_str(&mut out, value.message.as_deref());
    put_opt_str(&mut out, value.id.as_deref());
    put_f32(&mut out, value.duration);
    put_i32(&mut out, value.align);
    put_i32(&mut out, value.top);
    put_i32(&mut out, value.left);
    put_i32(&mut out, value.bottom);
    put_i32(&mut out, value.right);
    out
}

/// Decodes `info_popup`.
pub fn decode_info_popup(data: &[u8]) -> Result<InfoPopup, RelayError> {
    let mut reader = Reader::new(data)?;
    let info = InfoPopup {
        message: reader.opt_str()?,
        id: reader.opt_str()?,
        duration: reader.f32()?,
        align: reader.i32()?,
        top: reader.i32()?,
        left: reader.i32()?,
        bottom: reader.i32()?,
        right: reader.i32()?,
    };
    reader.finish()?;
    Ok(info)
}

/// Encodes `label`.
pub fn encode_label(value: &WorldLabel) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_opt_str(&mut out, value.message.as_deref());
    put_i32(&mut out, value.id);
    put_f32(&mut out, value.duration);
    put_f32(&mut out, value.world_x);
    put_f32(&mut out, value.world_y);
    put_i32(&mut out, value.flags);
    out
}

/// Decodes `label`.
pub fn decode_label(data: &[u8]) -> Result<WorldLabel, RelayError> {
    let mut reader = Reader::new(data)?;
    let label = WorldLabel {
        message: reader.opt_str()?,
        id: reader.i32()?,
        duration: reader.f32()?,
        world_x: reader.f32()?,
        world_y: reader.f32()?,
        flags: reader.i32()?,
    };
    reader.finish()?;
    Ok(label)
}

/// Encodes `warning_toast`.
pub fn encode_warning_toast(value: &WarningToast) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_i32(&mut out, value.icon);
    put_str(&mut out, &value.text);
    out
}

/// Decodes `warning_toast`.
pub fn decode_warning_toast(data: &[u8]) -> Result<WarningToast, RelayError> {
    let mut reader = Reader::new(data)?;
    let icon = reader.i32()?;
    let text = reader.str()?;
    reader.finish()?;
    Ok(WarningToast { icon, text })
}

/// Encodes `ping_marker`.
pub fn encode_ping_marker(value: PingMarker) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_f32(&mut out, value.x);
    put_f32(&mut out, value.y);
    out
}

/// Decodes `ping_marker`.
pub fn decode_ping_marker(data: &[u8]) -> Result<PingMarker, RelayError> {
    let mut reader = Reader::new(data)?;
    let ping = PingMarker {
        x: reader.f32()?,
        y: reader.f32()?,
    };
    reader.finish()?;
    Ok(ping)
}

/// Encodes a message-only payload (`hud_text`/`announce`/`info_toast`/…).
pub fn encode_message(message: &str) -> Vec<u8> {
    let mut out = vec![UI_RELAY_FORMAT];
    put_str(&mut out, message);
    out
}

/// Encodes an empty marker payload (`hide_hud_text`).
pub fn encode_empty() -> Vec<u8> {
    vec![UI_RELAY_FORMAT]
}

/// Decodes a message-only payload.
pub fn decode_message(data: &[u8]) -> Result<String, RelayError> {
    let mut reader = Reader::new(data)?;
    let message = reader.str()?;
    reader.finish()?;
    Ok(message)
}

/// Decodes an empty marker payload.
pub fn decode_empty(data: &[u8]) -> Result<(), RelayError> {
    let reader = Reader::new(data)?;
    reader.finish()
}

/// Decodes the `ui_node` body of a `menu_builder_show`/`update` (used by
/// `menu_host` after the id fields are read).
pub fn decode_ui_node_tail(data: &[u8]) -> Result<UiNode, RelayError> {
    let mut cursor = Cursor::new(data);
    let node = UiNode::decode_from(&mut cursor)?;
    if cursor.pos != data.len() {
        return Err(RelayError::TrailingBytes(data.len() - cursor.pos));
    }
    Ok(node)
}

fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn put_opt_str(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            out.push(1);
            put_str(out, value);
        }
        None => out.push(0),
    }
}

/// Byte reader shared by the relay decoders.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Verifies the `format: 1` header and returns a positioned reader.
    fn new(data: &'a [u8]) -> Result<Self, RelayError> {
        let format = *data.first().ok_or(RelayError::Eof)?;
        if format != UI_RELAY_FORMAT {
            return Err(RelayError::BadFormat(format));
        }
        Ok(Self { data, pos: 1 })
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], RelayError> {
        if self.pos + len > self.data.len() {
            return Err(RelayError::Eof);
        }
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, RelayError> {
        Ok(self.take(1)?[0])
    }

    fn i32(&mut self) -> Result<i32, RelayError> {
        let bytes = self.take(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn i64(&mut self) -> Result<i64, RelayError> {
        let bytes = self.take(8)?;
        Ok(i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn f32(&mut self) -> Result<f32, RelayError> {
        let bytes = self.take(4)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn str(&mut self) -> Result<String, RelayError> {
        let len = u16::from_le_bytes({
            let bytes = self.take(2)?;
            [bytes[0], bytes[1]]
        }) as usize;
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| RelayError::Utf8)
    }

    fn opt_str(&mut self) -> Result<Option<String>, RelayError> {
        match self.u8()? {
            0 => Ok(None),
            _ => Ok(Some(self.str()?)),
        }
    }

    fn node(&mut self) -> Result<UiNode, RelayError> {
        let mut cursor = Cursor {
            data: self.data,
            pos: self.pos,
        };
        let node = UiNode::decode_from(&mut cursor)?;
        self.pos = cursor.pos;
        Ok(node)
    }

    fn finish(self) -> Result<(), RelayError> {
        if self.pos != self.data.len() {
            return Err(RelayError::TrailingBytes(self.data.len() - self.pos));
        }
        Ok(())
    }
}

/// A minimal `ui_node` golden tree used by tests and the `ui menu-tree` corpus.
pub fn fixture_tree() -> UiNode {
    UiNode::new(UiKey::Table)
        .str(UiKey::Background, "grayPanel")
        .child(
            UiNode::new(UiKey::Label)
                .str(UiKey::Text, "Title")
                .str(UiKey::Id, "title"),
        )
        .child(
            UiNode::new(UiKey::Slider)
                .str(UiKey::Id, "volume")
                .f32(UiKey::Min, 0.0)
                .f32(UiKey::Max, 1.0)
                .f32(UiKey::DefaultValue, 0.5),
        )
        .child(
            UiNode::new(UiKey::Button)
                .str(UiKey::Text, "Buy")
                .str(UiKey::Clicked, "buy"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::builder::menu_result::{MenuResult, MenuValue};

    #[test]
    fn menu_builder_show_roundtrip() {
        let value = MenuBuilderShow {
            id: 3,
            token: 99,
            title: Some("Shop".to_owned()),
            hide_on_click: true,
            hide_existing: false,
            fill_screen: true,
            ui: fixture_tree(),
        };
        let bytes = encode_menu_builder_show(&value);
        assert_eq!(bytes[0], UI_RELAY_FORMAT);
        assert_eq!(decode_menu_builder_show(&bytes).unwrap(), value);

        let no_title = MenuBuilderShow {
            title: None,
            ..value
        };
        assert_eq!(
            decode_menu_builder_show(&encode_menu_builder_show(&no_title)).unwrap(),
            no_title
        );
    }

    #[test]
    fn menu_builder_update_and_hide_roundtrip() {
        let update = MenuBuilderUpdate {
            id: 7,
            table_id: "body".to_owned(),
            ui: fixture_tree(),
        };
        assert_eq!(
            decode_menu_builder_update(&encode_menu_builder_update(&update)).unwrap(),
            update
        );
        assert_eq!(decode_menu_builder_hide(&encode_menu_builder_hide(5)).unwrap(), 5);
    }

    #[test]
    fn client_result_roundtrip() {
        let mut result = MenuResult::from_result("buy");
        result.token = 12;
        result.insert("volume", MenuValue::F32(0.25));
        let choose = MenuBuilderChoose {
            menu_id: 4,
            result,
        };
        let bytes = encode_menu_builder_choose(&choose);
        assert_eq!(decode_menu_builder_choose(&bytes).unwrap(), choose);

        let menu = MenuChoose {
            menu_id: 4,
            option: -1,
        };
        assert_eq!(decode_menu_choose(&encode_menu_choose(menu)).unwrap(), menu);

        let input = TextInputResult {
            id: 2,
            text: Some("hello".to_owned()),
        };
        assert_eq!(
            decode_text_input_result(&encode_text_input_result(&input)).unwrap(),
            input
        );
        let cancelled = TextInputResult { id: 2, text: None };
        assert_eq!(
            decode_text_input_result(&encode_text_input_result(&cancelled)).unwrap(),
            cancelled
        );
    }

    #[test]
    fn text_input_and_overlays_roundtrip() {
        let input = TextInput {
            id: 1,
            title: "Name".to_owned(),
            message: "Enter".to_owned(),
            len: 32,
            def: "abc".to_owned(),
            numeric: true,
            allow_empty: false,
        };
        assert_eq!(decode_text_input(&encode_text_input(&input)).unwrap(), input);

        let popup = InfoPopup {
            message: Some("hi".to_owned()),
            id: Some("p1".to_owned()),
            duration: 2.5,
            align: 1,
            top: 4,
            left: 5,
            bottom: 6,
            right: 7,
        };
        assert_eq!(decode_info_popup(&encode_info_popup(&popup)).unwrap(), popup);

        let label = WorldLabel {
            message: Some("marker".to_owned()),
            id: 9,
            duration: 3.0,
            world_x: 12.0,
            world_y: -4.5,
            flags: 7,
        };
        assert_eq!(decode_label(&encode_label(&label)).unwrap(), label);

        let toast = WarningToast {
            icon: 63743,
            text: "careful".to_owned(),
        };
        assert_eq!(
            decode_warning_toast(&encode_warning_toast(&toast)).unwrap(),
            toast
        );

        let ping = PingMarker { x: 1.0, y: 2.0 };
        assert_eq!(decode_ping_marker(&encode_ping_marker(ping)).unwrap(), ping);

        assert_eq!(decode_message(&encode_message("hud")).unwrap(), "hud");
        assert_eq!(decode_empty(&encode_empty()).unwrap(), ());
    }

    #[test]
    fn relay_format_and_truncation_errors() {
        assert_eq!(decode_empty(&[]), Err(RelayError::Eof));
        assert_eq!(decode_empty(&[2]), Err(RelayError::BadFormat(2)));
        // Valid header but trailing bytes / truncated fields.
        assert_eq!(
            decode_menu_builder_hide(&[UI_RELAY_FORMAT, 0, 0]),
            Err(RelayError::Eof)
        );
        let bytes = encode_menu_builder_hide(1);
        let mut padded = bytes.clone();
        padded.push(0);
        assert!(matches!(
            decode_menu_builder_hide(&padded),
            Err(RelayError::TrailingBytes(1))
        ));
    }
}
