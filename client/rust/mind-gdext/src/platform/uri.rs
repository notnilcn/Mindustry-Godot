// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! URI / clipboard seam (plan 22 §3.3/§6.7).
//!
//! The pure `mindustry://` parser lives in `mind-core`; this module adds the
//! Godot calls (`OS.shell_open`, `DisplayServer` clipboard) that back
//! `ClientPlatform::open_uri`/`set_clipboard`/`get_clipboard`.

use godot::classes::DisplayServer;
use godot::classes::Os;
use godot::obj::Singleton;
use godot::prelude::GString;

pub use mind_core::platform::uri::{ConnectUri, SCHEMES, parse};

/// Opens `uri` with the OS handler (`Core.app.openURI` / `Menus.openURI`).
///
/// Returns `true` on success.
pub fn open_uri(uri: &str) -> bool {
    Os::singleton().shell_open(uri) == godot::global::Error::OK
}

/// Sets the system clipboard (`Platform.setClipboard`).
pub fn set_clipboard(text: &str) {
    DisplayServer::singleton().clipboard_set(text);
}

/// Reads the system clipboard (`Platform.getClipboard`); `None` when empty.
pub fn get_clipboard() -> Option<String> {
    let value = DisplayServer::singleton().clipboard_get();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Converts a Godot `GString` URI and opens it.
pub fn open_uri_gstring(uri: &GString) -> bool {
    open_uri(&uri.to_string())
}
