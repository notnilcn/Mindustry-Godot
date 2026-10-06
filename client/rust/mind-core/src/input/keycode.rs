// Ported from Arc (https://github.com/Anuken/Arc) — Apache-2.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Arc `KeyCode` display names (plan 15).
//!
//! The keybind dialog renders `KeyCode.getName()`, which resolves through
//! `Input.getKeyName()` to the enum's `value` field (`arc/input/KeyCode.java`).
//! The registry stores arc names (`Binding` persistence ABI); this maps them to
//! the exact display strings of the pinned Arc build (`7445105cd2`, the jar the
//! Java reference was built against). Unknown names fall back to Arc's
//! `Strings.capitalize`, so a newly normalized key still renders readably.

/// Arc `KeyCode.value` for a registry key name (`KeyCode.getName()`).
pub fn display_name(name: &str) -> String {
    let display = match name {
        "controllerA" => "A",
        "controllerB" => "B",
        "controllerX" => "X",
        "controllerY" => "Y",
        "controllerGuide" => "Guide",
        "controllerLBumper" => "L Bumper",
        "controllerRBumper" => "R Bumper",
        "controllerBack" => "Back",
        "controllerStart" => "Start",
        "controllerLStick" => "L Stick",
        "controllerRStick" => "R Stick",
        "controllerdPadUp" => "D-Pad Up",
        "controllerdPadDown" => "D-Pad Down",
        "controllerdPadLeft" => "D-Pad Left",
        "controllerdPadRight" => "D-Pad Right",
        "controllerLTrigger" => "L Trigger",
        "controllerRTrigger" => "R Trigger",
        "controllerLStickYAxis" => "L Stick Y Axis",
        "controllerLStickXAxis" => "L Stick X Axis",
        "controllerRStickYAxis" => "R Stick Y Axis",
        "controllerRStickXAxis" => "R Stick X Axis",
        "mouseLeft" => "Mouse Left",
        "mouseRight" => "Mouse Right",
        "mouseMiddle" => "Mouse Middle",
        "mouseBack" => "Mouse Back",
        "mouseForward" => "Mouse Forward",
        "scroll" => "Scrollwheel",
        "anyKey" => "Any Key",
        "num0" => "0",
        "num1" => "1",
        "num2" => "2",
        "num3" => "3",
        "num4" => "4",
        "num5" => "5",
        "num6" => "6",
        "num7" => "7",
        "num8" => "8",
        "num9" => "9",
        "a" => "A",
        "altLeft" => "L-Alt",
        "altRight" => "R-Alt",
        "apostrophe" => "\'",
        "at" => "@",
        "b" => "B",
        "back" => "Back",
        "backslash" => "\\",
        "c" => "C",
        "call" => "Call",
        "camera" => "Camera",
        "clear" => "Clear",
        "comma" => ",",
        "d" => "D",
        "del" => "Delete",
        "backspace" => "Delete",
        "forwardDel" => "Forward Delete",
        "dpadCenter" => "Center",
        "dpadDown" => "Down",
        "dpadLeft" => "Left",
        "dpadRight" => "Right",
        "dpadUp" => "Up",
        "center" => "Center",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "up" => "Up",
        "e" => "E",
        "endcall" => "End Call",
        "enter" => "Enter",
        "envelope" => "Envelope",
        "equals" => "=",
        "explorer" => "Explorer",
        "f" => "F",
        "focus" => "Focus",
        "g" => "G",
        "backtick" => "`",
        "h" => "H",
        "headsetHook" => "Headset Hook",
        "home" => "Home",
        "i" => "I",
        "j" => "J",
        "k" => "K",
        "l" => "L",
        "leftBracket" => "[",
        "m" => "M",
        "mediaFastForward" => "Fast Forward",
        "mediaNext" => "Next Media",
        "mediaPlayPause" => "Play/Pause",
        "mediaPrevious" => "Prev Media",
        "mediaRewind" => "Rewind",
        "mediaStop" => "Stop Media",
        "menu" => "Menu",
        "minus" => "-",
        "mute" => "Mute",
        "n" => "N",
        "notification" => "Notification",
        "num" => "Num",
        "o" => "O",
        "p" => "P",
        "period" => ".",
        "plus" => "Plus",
        "pound" => "#",
        "power" => "Power",
        "q" => "Q",
        "r" => "R",
        "rightBracket" => "]",
        "s" => "S",
        "search" => "Search",
        "semicolon" => ";",
        "shiftLeft" => "L-Shift",
        "shiftRight" => "R-Shift",
        "slash" => "/",
        "softLeft" => "Soft Left",
        "softRight" => "Soft Right",
        "space" => "Space",
        "star" => "*",
        "sym" => "SYM",
        "t" => "T",
        "tab" => "Tab",
        "u" => "U",
        "unknown" => "Unknown",
        "v" => "V",
        "volumeDown" => "Volume Down",
        "volumeUp" => "Volume Up",
        "w" => "W",
        "x" => "X",
        "y" => "Y",
        "z" => "Z",
        "metaAltLeftOn" => "9",
        "metaAltOn" => "Soft Right",
        "metaAltRightOn" => "D",
        "metaShiftLeftOn" => "Explorer",
        "metaShiftOn" => "Soft Left",
        "metaShiftRightOn" => "null",
        "metaSymOn" => "Back",
        "controlLeft" => "L-Ctrl",
        "controlRight" => "R-Ctrl",
        "escape" => "Escape",
        "end" => "End",
        "insert" => "Insert",
        "pageUp" => "Page Up",
        "pageDown" => "Page Down",
        "pictSymbols" => "PICTSYMBOLS",
        "switchCharset" => "switchCharset",
        "buttonCircle" => "F12",
        "buttonA" => "A Button",
        "buttonB" => "B Button",
        "buttonC" => "C Button",
        "buttonX" => "X Button",
        "buttonY" => "Y Button",
        "buttonZ" => "Z Button",
        "buttonL1" => "L1 Button",
        "buttonR1" => "R1 Button",
        "buttonL2" => "L2 Button",
        "buttonR2" => "R2 Button",
        "buttonThumbL" => "Left Thumb",
        "buttonThumbR" => "Right Thumb",
        "buttonStart" => "Start",
        "buttonSelect" => "Select",
        "buttonMode" => "Button Mode",
        "numpad0" => "Numpad 0",
        "numpad1" => "Numpad 1",
        "numpad2" => "Numpad 2",
        "numpad3" => "Numpad 3",
        "numpad4" => "Numpad 4",
        "numpad5" => "Numpad 5",
        "numpad6" => "Numpad 6",
        "numpad7" => "Numpad 7",
        "numpad8" => "Numpad 8",
        "numpad9" => "Numpad 9",
        "colon" => ":",
        "f1" => "F1",
        "f2" => "F2",
        "f3" => "F3",
        "f4" => "F4",
        "f5" => "F5",
        "f6" => "F6",
        "f7" => "F7",
        "f8" => "F8",
        "f9" => "F9",
        "f10" => "F10",
        "f11" => "F11",
        "f12" => "F12",
        "unset" => "Unset",
        "application" => "Application",
        "asterisk" => "*",
        "capsLock" => "Caps Lock",
        "pause" => "Pause",
        "printScreen" => "Print Screen",
        "scrollLock" => "Scroll Lock",
        _ => return capitalize(name),
    };
    display.to_owned()
}

/// Arc `Strings.capitalize`: `_`/`-` become spaces and the following character
/// is upper-cased (`shift_left` -> "Shift Left").
fn capitalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut capitalize_next = true;
    for ch in name.chars() {
        if ch == '_' || ch == '-' {
            out.push(' ');
            capitalize_next = true;
            continue;
        }
        if capitalize_next {
            out.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_arc_keycode_values() {
        assert_eq!(display_name("a"), "A");
        assert_eq!(display_name("shiftLeft"), "L-Shift");
        assert_eq!(display_name("controlLeft"), "L-Ctrl");
        assert_eq!(display_name("mouseLeft"), "Mouse Left");
        assert_eq!(display_name("space"), "Space");
        assert_eq!(display_name("num1"), "1");
        assert_eq!(display_name("leftBracket"), "[");
        assert_eq!(display_name("scroll"), "Scrollwheel");
    }

    #[test]
    fn falls_back_to_capitalize() {
        assert_eq!(display_name("made_up_key"), "Made Up Key");
        assert_eq!(display_name("weird-name"), "Weird Name");
    }
}
