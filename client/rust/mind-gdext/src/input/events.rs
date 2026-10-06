// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
//
//! Godot `InputEvent` → `mind_core::input::RawEvent` translation (plan 15 §3.1).
//! No game rules: this is pure view/input plumbing.

use godot::classes::{
    InputEvent, InputEventAction, InputEventKey, InputEventMagnifyGesture, InputEventMouseButton,
    InputEventMouseMotion, InputEventPanGesture, InputEventScreenDrag, InputEventScreenTouch, Os,
};
use godot::global::{Key, MouseButton};
use godot::obj::Singleton;
use godot::prelude::*;

use mind_core::input::RawEvent;

/// Translates one event into zero or more [`RawEvent`]s.
pub fn translate(event: &Gd<InputEvent>, out: &mut Vec<RawEvent>) {
    if let Ok(key) = event.clone().try_cast::<InputEventKey>() {
        // Arc key events do not auto-repeat; OS echo events must not reach the
        // binding tap path (`Binding`/`InputHandler` taps fire once per press).
        if key.is_echo() {
            return;
        }
        let code = godot_key_name(key.get_keycode());
        out.push(if key.is_pressed() {
            RawEvent::KeyDown { code }
        } else {
            RawEvent::KeyUp { code }
        });
        return;
    }
    if let Ok(mouse) = event.clone().try_cast::<InputEventMouseButton>() {
        let position = mouse.get_position();
        let index = mouse.get_button_index();
        match index {
            MouseButton::WHEEL_UP
            | MouseButton::WHEEL_DOWN
            | MouseButton::WHEEL_LEFT
            | MouseButton::WHEEL_RIGHT => {
                let mut x = 0.0;
                let mut y = 0.0;
                let amount = mouse.get_factor();
                match index {
                    MouseButton::WHEEL_UP => y = amount,
                    MouseButton::WHEEL_DOWN => y = -amount,
                    MouseButton::WHEEL_LEFT => x = -amount,
                    _ => x = amount,
                }
                out.push(RawEvent::Scroll { x, y });
            }
            other => out.push(RawEvent::MouseButton {
                button: mouse_button_name(other).to_owned(),
                down: mouse.is_pressed(),
                x: position.x,
                y: position.y,
            }),
        }
        return;
    }
    if let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() {
        let position = motion.get_position();
        out.push(RawEvent::MouseMove {
            x: position.x,
            y: position.y,
        });
        return;
    }
    if let Ok(touch) = event.clone().try_cast::<InputEventScreenTouch>() {
        let position = touch.get_position();
        out.push(if touch.is_pressed() {
            RawEvent::TouchDown {
                pointer: touch.get_index(),
                x: position.x,
                y: position.y,
            }
        } else {
            RawEvent::TouchUp {
                pointer: touch.get_index(),
                x: position.x,
                y: position.y,
            }
        });
        return;
    }
    if let Ok(drag) = event.clone().try_cast::<InputEventScreenDrag>() {
        let position = drag.get_position();
        out.push(RawEvent::TouchMove {
            pointer: drag.get_index(),
            x: position.x,
            y: position.y,
        });
        return;
    }
    if let Ok(magnify) = event.clone().try_cast::<InputEventMagnifyGesture>() {
        out.push(RawEvent::Magnify {
            factor: magnify.get_factor(),
        });
        return;
    }
    if let Ok(pan) = event.clone().try_cast::<InputEventPanGesture>() {
        let delta = pan.get_delta();
        out.push(RawEvent::Scroll {
            x: delta.x,
            y: delta.y,
        });
        return;
    }
    if let Ok(action) = event.clone().try_cast::<InputEventAction>() {
        out.push(RawEvent::Action {
            action: action.get_action().to_string(),
            value: if action.is_pressed() { 1.0 } else { 0.0 },
        });
    }
}

fn mouse_button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::LEFT => "left",
        MouseButton::RIGHT => "right",
        MouseButton::MIDDLE => "middle",
        MouseButton::WHEEL_UP | MouseButton::WHEEL_DOWN => "wheel",
        _ => "other",
    }
}

/// Maps a Godot keycode to the upstream `KeyCode` name used by the registry.
pub fn godot_key_name(key: Key) -> String {
    let text = Os::singleton().get_keycode_string(key).to_string();
    normalize_key_name(&text)
}

/// Normalizes a Godot key display string to a `Binding` key name.
pub fn normalize_key_name(text: &str) -> String {
    match text {
        "Shift" => return "shiftLeft".to_owned(),
        "Ctrl" | "Control" => return "controlLeft".to_owned(),
        "Alt" => return "altLeft".to_owned(),
        "Meta" | "Command" => return "metaLeft".to_owned(),
        _ => {}
    }
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "space" | " " => "space".to_owned(),
        "escape" => "escape".to_owned(),
        "tab" => "tab".to_owned(),
        "enter" | "return" => "enter".to_owned(),
        "backspace" => "backspace".to_owned(),
        "left" => "left".to_owned(),
        "right" => "right".to_owned(),
        "up" => "up".to_owned(),
        "down" => "down".to_owned(),
        "comma" | "," => "comma".to_owned(),
        "period" | "." => "period".to_owned(),
        "[" => "leftBracket".to_owned(),
        "]" => "rightBracket".to_owned(),
        other => {
            if let Some(rest) = other.strip_prefix('f')
                && matches!(rest.parse::<u8>(), Ok(1..=12))
            {
                return other.to_owned();
            }
            if other.len() == 1 {
                if other.as_bytes()[0].is_ascii_digit() {
                    return format!("num{other}");
                }
                return other.to_owned();
            }
            other.to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_name_normalization() {
        assert_eq!(normalize_key_name("A"), "a");
        assert_eq!(normalize_key_name("Shift"), "shiftLeft");
        assert_eq!(normalize_key_name("Control"), "controlLeft");
        assert_eq!(normalize_key_name("1"), "num1");
        assert_eq!(normalize_key_name("F12"), "f12");
        assert_eq!(normalize_key_name("Escape"), "escape");
        assert_eq!(normalize_key_name("["), "leftBracket");
        assert_eq!(normalize_key_name(","), "comma");
    }
}
