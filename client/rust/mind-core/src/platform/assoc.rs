// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Save/schematic/URI association file generation (plan 22 §6.7, P22-9).
//!
//! The game never writes these itself; `tools/associate.sh` runs the
//! opt-in installer. This module owns the exact file bodies (a Linux `.desktop`
//! with MimeType handlers and a Windows `.reg` script registering `.msav`,
//! `.msch` and the dual `mindustry://`/`mindustry-godot://` protocol).

/// Custom MIME type for `.msav` saves.
pub const MIME_SAVE: &str = "application/x-mindustry-save";
/// Custom MIME type for `.msch` schematics.
pub const MIME_SCHEMATIC: &str = "application/x-mindustry-schematic";

/// Windows association class for `.msav`.
pub const WINDOWS_SAVE_CLASS: &str = "MindustryGodot.Save";
/// Windows association class for `.msch`.
pub const WINDOWS_SCHEMATIC_CLASS: &str = "MindustryGodot.Schematic";

/// Builds the Linux `<app>.desktop` body (LF, trailing newline).
pub fn desktop_entry(app_name: &str, exec_path: &str, icon: Option<&str>) -> String {
    let icon_line = icon
        .map(|icon| format!("Icon={icon}\n"))
        .unwrap_or_default();
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={app_name}\n\
         Comment=Saves, schematics and join links for {app_name}\n\
         Exec=\"{exec_path}\" %f\n\
         {icon_line}Terminal=false\n\
         Categories=Game;\n\
         MimeType={MIME_SAVE};{MIME_SCHEMATIC};x-scheme-handler/mindustry;x-scheme-handler/mindustry-godot;\n"
    )
}

/// Escapes a path for a `.reg` value (`\` → `\\`, `"` → `\"`).
pub fn reg_escape(path: &str) -> String {
    path.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Builds the Windows `.reg` installer body registering save/schematic/protocol
/// handlers under `HKCU\Software\Classes`.
pub fn windows_reg(exe_path: &str) -> String {
    let exe = reg_escape(exe_path);
    let mut text = String::from("Windows Registry Editor Version 5.00\n");
    for (extension, class) in [
        (".msav", WINDOWS_SAVE_CLASS),
        (".msch", WINDOWS_SCHEMATIC_CLASS),
    ] {
        text.push_str(&format!(
            "\n[HKEY_CURRENT_USER\\Software\\Classes\\{extension}]\n@=\"{class}\"\n"
        ));
        text.push_str(&format!(
            "\n[HKEY_CURRENT_USER\\Software\\Classes\\{class}\\DefaultIcon]\n@=\"\\\"{exe}\\\",0\"\n"
        ));
        text.push_str(&format!(
            "\n[HKEY_CURRENT_USER\\Software\\Classes\\{class}\\shell\\open\\command]\n@=\"\\\"{exe}\\\" \\\"%1\\\"\"\n"
        ));
    }
    for scheme in ["mindustry", "mindustry-godot"] {
        text.push_str(&format!(
            "\n[HKEY_CURRENT_USER\\Software\\Classes\\{scheme}]\n@=\"URL:Mindustry-Godot Protocol\"\n\"URL Protocol\"=\"\"\n"
        ));
        text.push_str(&format!(
            "\n[HKEY_CURRENT_USER\\Software\\Classes\\{scheme}\\shell\\open\\command]\n@=\"\\\"{exe}\\\" \\\"%1\\\"\"\n"
        ));
    }
    text
}

/// Builds the Windows `.reg` uninstaller body (deletes every registered key).
pub fn windows_unreg() -> String {
    let mut text = String::from("Windows Registry Editor Version 5.00\n");
    for key in [
        "Software\\Classes\\.msav".to_owned(),
        "Software\\Classes\\.msch".to_owned(),
        format!("Software\\Classes\\{WINDOWS_SAVE_CLASS}"),
        format!("Software\\Classes\\{WINDOWS_SCHEMATIC_CLASS}"),
        "Software\\Classes\\mindustry".to_owned(),
        "Software\\Classes\\mindustry-godot".to_owned(),
    ] {
        text.push_str(&format!("\n[-HKEY_CURRENT_USER\\{key}]\n"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_registers_mime_and_schemes() {
        let entry = desktop_entry("Mindustry-Godot", "/opt/mg/mind", Some("/opt/mg/icon.png"));
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("Exec=\"/opt/mg/mind\" %f\n"));
        assert!(entry.contains("Icon=/opt/mg/icon.png\n"));
        assert!(entry.contains(MIME_SAVE));
        assert!(entry.contains(MIME_SCHEMATIC));
        assert!(entry.contains("x-scheme-handler/mindustry-godot;"));
        assert!(entry.ends_with('\n'));
    }

    #[test]
    fn windows_reg_escapes_and_covers_all_handlers() {
        let reg = windows_reg(r"C:\Program Files\MG\mind.exe");
        assert!(reg.contains(r#"[HKEY_CURRENT_USER\Software\Classes\.msav]"#));
        assert!(reg.contains(r#"[HKEY_CURRENT_USER\Software\Classes\.msch]"#));
        assert!(reg.contains(r#"[HKEY_CURRENT_USER\Software\Classes\mindustry-godot]"#));
        assert!(reg.contains(r#""URL Protocol"=""#));
        assert!(reg.contains(r#"\""#));
        assert!(reg.contains("C:\\\\Program Files"));

        let unreg = windows_unreg();
        assert!(unreg.contains(r"[-HKEY_CURRENT_USER\Software\Classes\.msav]"));
        assert!(unreg.contains(r"[-HKEY_CURRENT_USER\Software\Classes\mindustry]"));
    }
}
