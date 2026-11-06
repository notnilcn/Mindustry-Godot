// SPDX-License-Identifier: GPL-3.0-only

//! `CallBridge` — server→client `@Remote` call dispatch for `MindNet`
//! (GAPS #35 / PAR-0546).
//!
//! Ported from Mindustry's `Call`/`*CallPacket` handlers
//! (`core/src/mindustry/core/NetClient.java`, `core/src/mindustry/ui/Menus.java`):
//! the wire codec and bounded channel live in `mind-stdb::call_packet`; this
//! module is the thin GDExtension-side dispatch seam. It decodes frames pushed
//! by the transport (or a host adapter), classifies the ones the net layer can
//! apply itself ([`CallEffect::Kick`] leaves the match and shows the localized
//! reason), and hands the rest to their owners through
//! [`CallBridge::drain`]/[`MindNet::server_calls_json`](super::MindNet).
//!
//! Nothing here touches `mind-core` sim state, so a malformed or unknown call
//! can never affect the world: it is recorded and skipped.

use mind_stdb::call_packet::{CallPacket, CallReject};

use mind_stdb::CallChannel;

/// A call the net layer acts on directly; unhandled calls stay in the drained
/// list for the UI/view consumers.
#[derive(Debug, Clone, PartialEq)]
pub enum CallEffect {
    /// This client was kicked: show the localized reason and leave the match.
    Kick {
        /// Bundle key (`server.kicked.<reason>`), without the `@` prefix.
        key: String,
        /// Extra detail (`KickReason.extraText` / the raw kick string).
        detail: String,
        /// Whether the reason is quiet (`KickReason.gameover`).
        quiet: bool,
    },
    /// Server-driven camera pan (`Call.setCameraPosition`), in world units.
    Camera {
        /// World x.
        x: f32,
        /// World y.
        y: f32,
    },
    /// Server message shown without a dialog (`Menus.infoMessage`).
    InfoMessage {
        /// Message text.
        message: String,
    },
    /// Music request (`Call.playMusic`).
    PlayMusic {
        /// Track name.
        music: String,
        /// Interrupt the current track.
        interrupt: bool,
    },
}

/// JSON projection of a classified effect, for the UI/view consumers.
pub fn effect_json(effect: Option<&CallEffect>) -> serde_json::Value {
    match effect {
        None => serde_json::Value::Null,
        Some(CallEffect::Kick { key, detail, quiet }) => serde_json::json!({
            "kick": { "key": key, "detail": detail, "quiet": quiet },
        }),
        Some(CallEffect::Camera { x, y }) => serde_json::json!({
            "camera": { "x": x, "y": y },
        }),
        Some(CallEffect::InfoMessage { message }) => serde_json::json!({
            "infoMessage": message,
        }),
        Some(CallEffect::PlayMusic { music, interrupt }) => serde_json::json!({
            "playMusic": { "music": music, "interrupt": interrupt },
        }),
    }
}

/// Decoded server calls plus the local-effect classification.
#[derive(Debug, Default)]
pub struct CallBridge {
    channel: CallChannel,
}

impl CallBridge {
    /// Empty bridge with the default queue cap.
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes and queues one `[kind][body]` frame; `false` when rejected.
    pub fn push_frame(&mut self, frame: &[u8]) -> bool {
        self.channel.push_bytes(frame).is_ok()
    }

    /// Drops and returns every queued call in arrival order.
    pub fn drain(&mut self) -> Vec<CallPacket> {
        self.channel.drain()
    }

    /// Classifies one call into the effect the net layer applies itself.
    pub fn classify(call: &CallPacket) -> Option<CallEffect> {
        match call {
            CallPacket::Kick { reason } => Some(CallEffect::Kick {
                key: "server.kicked.kick".to_string(),
                detail: reason.clone().unwrap_or_default(),
                quiet: false,
            }),
            CallPacket::KickReason { reason } => Some(CallEffect::Kick {
                key: reason.bundle_key(),
                detail: String::new(),
                quiet: reason.quiet(),
            }),
            CallPacket::SetCameraPosition { x, y } => Some(CallEffect::Camera { x: *x, y: *y }),
            CallPacket::InfoMessage { message } => {
                message.as_ref().map(|message| CallEffect::InfoMessage {
                    message: message.clone(),
                })
            }
            CallPacket::PlayMusic { music, interrupt } => {
                music.as_ref().map(|music| CallEffect::PlayMusic {
                    music: music.clone(),
                    interrupt: *interrupt,
                })
            }
            _ => None,
        }
    }

    /// Stable debug JSON: queue depth, counters and the rejection tail.
    pub fn stats_json(&self) -> serde_json::Value {
        serde_json::json!({
            "pending": self.channel.len(),
            "limit": self.channel.limit(),
            "accepted": self.channel.accepted(),
            "rejected": self.channel.rejected(),
            "dropped": self.channel.dropped(),
            "rejects": self.rejects_json(),
        })
    }

    /// Rejection tail as JSON (`{"unknown": n}` / `{"malformed": "..."}`).
    pub fn rejects_json(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.channel
                .rejects()
                .iter()
                .map(|reject| match reject {
                    CallReject::UnknownCall(kind) => serde_json::json!({
                        "unknownCall": kind,
                    }),
                    CallReject::Malformed { kind, error } => serde_json::json!({
                        "malformed": error.to_string(),
                        "kind": kind.map(|kind| kind.ordinal()),
                    }),
                    CallReject::Overflow => serde_json::json!({ "overflow": true }),
                })
                .collect(),
        )
    }

    /// Drops queued calls, counters and the rejection tail (disconnect/resync).
    pub fn clear(&mut self) {
        self.channel.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use mind_stdb::typeio::{KickReason, Rgba, SoundId};

    #[test]
    fn frames_round_trip_and_rejects_are_classified() {
        let mut bridge = CallBridge::new();
        let sound = CallPacket::Sound {
            sound: Some(SoundId(1)),
            volume: 0.5,
            pitch: 1.0,
            pan: 0.0,
        };
        assert!(bridge.push_frame(&sound.encode().unwrap()));
        assert!(bridge.push_frame(&[250]));
        assert_eq!(bridge.stats_json()["pending"], 1);
        let stats = bridge.stats_json();
        assert_eq!(stats["accepted"], 1);
        assert_eq!(stats["rejected"], 1);
        assert_eq!(stats["rejects"][0]["unknownCall"], 250);
        let drained = bridge.drain();
        assert_eq!(drained, vec![sound]);
    }

    #[test]
    fn every_kind_and_overload_classifies_or_passes_through() {
        assert_eq!(
            CallBridge::classify(&CallPacket::Kick {
                reason: Some("bye".to_string())
            }),
            Some(CallEffect::Kick {
                key: "server.kicked.kick".to_string(),
                detail: "bye".to_string(),
                quiet: false,
            })
        );
        assert_eq!(
            CallBridge::classify(&CallPacket::KickReason {
                reason: KickReason::Gameover
            }),
            Some(CallEffect::Kick {
                key: "server.kicked.gameover".to_string(),
                detail: String::new(),
                quiet: true,
            })
        );
        assert_eq!(
            CallBridge::classify(&CallPacket::SetCameraPosition { x: 1.0, y: 2.0 }),
            Some(CallEffect::Camera { x: 1.0, y: 2.0 })
        );
        assert_eq!(
            CallBridge::classify(&CallPacket::InfoMessage {
                message: Some("hi".to_string())
            }),
            Some(CallEffect::InfoMessage {
                message: "hi".to_string()
            })
        );
        assert_eq!(
            CallBridge::classify(&CallPacket::PlayMusic {
                music: Some("game1".to_string()),
                interrupt: true
            }),
            Some(CallEffect::PlayMusic {
                music: "game1".to_string(),
                interrupt: true
            })
        );
        // Calls the UI/view owners apply are not net effects.
        assert_eq!(
            CallBridge::classify(&CallPacket::Sound {
                sound: Some(SoundId(1)),
                volume: 0.5,
                pitch: 1.0,
                pan: 0.0,
            }),
            None
        );
        assert_eq!(
            CallBridge::classify(&CallPacket::Effect {
                effect: None,
                x: 0.0,
                y: 0.0,
                rotation: 0.0,
                color: Rgba::default(),
            }),
            None
        );
    }
}
