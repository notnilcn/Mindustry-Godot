// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/UiStyleLookup.java.

//! Style-name lookup against `styles_manifest.json` (plan 14 §3.6).
//!
//! Java `UiStyleLookup.get(Class<Style>, name)` returns a scene2d style object.
//! Here the manifest is the source of truth: a name resolves to a
//! [`StyleKind`], and the GDScript theme maps it to a Godot type variation.

use crate::ui::manifest::StylesManifest;

/// Kind of scene2d style a name belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    /// Ninepatch/drawable style.
    Drawable,
    /// Text-button style.
    TextButton,
    /// Button style.
    Button,
    /// Image-button style.
    ImageButton,
    /// Pane style.
    Pane,
    /// Slider style.
    Slider,
    /// Label style.
    Label,
    /// Field style.
    Field,
    /// Check style.
    Check,
    /// Dialog style.
    Dialog,
    /// Tree style.
    Tree,
}

impl StyleKind {
    /// All kinds in manifest order.
    pub const ALL: [StyleKind; 11] = [
        StyleKind::Drawable,
        StyleKind::TextButton,
        StyleKind::Button,
        StyleKind::ImageButton,
        StyleKind::Pane,
        StyleKind::Slider,
        StyleKind::Label,
        StyleKind::Field,
        StyleKind::Check,
        StyleKind::Dialog,
        StyleKind::Tree,
    ];

    /// Stable lowercase kind name (goldens/logs), distinct from the manifest key.
    pub fn name(self) -> &'static str {
        match self {
            StyleKind::Drawable => "drawable",
            StyleKind::TextButton => "text_button",
            StyleKind::Button => "button",
            StyleKind::ImageButton => "image_button",
            StyleKind::Pane => "pane",
            StyleKind::Slider => "slider",
            StyleKind::Label => "label",
            StyleKind::Field => "field",
            StyleKind::Check => "check",
            StyleKind::Dialog => "dialog",
            StyleKind::Tree => "tree",
        }
    }

    /// Manifest group key (`drawables`, `text_buttons`, …).
    pub fn key(self) -> &'static str {
        match self {
            StyleKind::Drawable => "drawables",
            StyleKind::TextButton => "text_buttons",
            StyleKind::Button => "buttons",
            StyleKind::ImageButton => "image_buttons",
            StyleKind::Pane => "panes",
            StyleKind::Slider => "sliders",
            StyleKind::Label => "labels",
            StyleKind::Field => "fields",
            StyleKind::Check => "checks",
            StyleKind::Dialog => "dialogs",
            StyleKind::Tree => "trees",
        }
    }

    /// Names registered for this kind.
    pub fn names(self, manifest: &StylesManifest) -> &[String] {
        match self {
            StyleKind::Drawable => &manifest.drawables,
            StyleKind::TextButton => &manifest.text_buttons,
            StyleKind::Button => &manifest.buttons,
            StyleKind::ImageButton => &manifest.image_buttons,
            StyleKind::Pane => &manifest.panes,
            StyleKind::Slider => &manifest.sliders,
            StyleKind::Label => &manifest.labels,
            StyleKind::Field => &manifest.fields,
            StyleKind::Check => &manifest.checks,
            StyleKind::Dialog => &manifest.dialogs,
            StyleKind::Tree => &manifest.trees,
        }
    }

    /// Finds the kind that owns `name`, if any.
    pub fn of(manifest: &StylesManifest, name: &str) -> Option<StyleKind> {
        StyleKind::ALL
            .into_iter()
            .find(|kind| kind.names(manifest).iter().any(|n| n == name))
    }
}

/// Whether `name` resolves to `kind`.
pub fn lookup(manifest: &StylesManifest, kind: StyleKind, name: &str) -> bool {
    kind.names(manifest).iter().any(|n| n == name)
}

/// Manifest-backed style resolver (`UiStyleLookup.get` equivalent).
///
/// The DSL's `style: "grayt"` and `background: "grayPanel"` are resolved
/// through this, so the Godot theme builder and the headless factory agree on
/// exactly which names exist and what kind each belongs to.
#[derive(Debug, Clone, Copy)]
pub struct StyleLookup<'a> {
    manifest: &'a StylesManifest,
}

impl<'a> StyleLookup<'a> {
    /// Wraps a styles manifest.
    pub fn new(manifest: &'a StylesManifest) -> Self {
        Self { manifest }
    }

    /// Resolves `name` to its owning kind, if any.
    pub fn resolve(&self, name: &str) -> Option<StyleKind> {
        StyleKind::of(self.manifest, name)
    }

    /// Whether `name` resolves to `kind`.
    pub fn matches(&self, kind: StyleKind, name: &str) -> bool {
        lookup(self.manifest, kind, name)
    }

    /// All names registered for `kind`.
    pub fn names(&self, kind: StyleKind) -> &[String] {
        kind.names(self.manifest)
    }

    /// Validates every reference, returning the unresolved names (sorted,
    /// de-duplicated). `Ok(())` means every reference resolves.
    ///
    /// This is the `UiStyleLookup` gate used against the committed dialog
    /// sources (plan §7e): a dialog may only reference a `Styles.*` name the
    /// manifest registers.
    pub fn validate<'b, I>(&self, names: I) -> Result<(), Vec<String>>
    where
        I: IntoIterator<Item = &'b str>,
    {
        let mut unresolved: Vec<String> = names
            .into_iter()
            .filter(|name| self.resolve(name).is_none())
            .map(str::to_owned)
            .collect();
        unresolved.sort();
        unresolved.dedup();
        if unresolved.is_empty() {
            Ok(())
        } else {
            Err(unresolved)
        }
    }
}

/// Style-reference tokens recognized in committed UI sources.
///
/// GDScript dialogs apply a style either through `theme_type_variation = "x"`
/// or through a `MindWidgets` helper whose `style` parameter is a literal
/// (`styled_label(…, "techLabel")`, `image_button(…, "flati")`); `.tscn` files
/// carry the same `theme_type_variation = "x"` property. The scanner extracts
/// exactly those literal references so the manifest can be validated against
/// the real dialog set without a full GDScript parser.
pub const STYLE_ASSIGNMENT_TOKEN: &str = "theme_type_variation";
/// Style-taking label helper.
pub const STYLE_LABEL_CALL: &str = "styled_label";
/// Style-taking image-button helper.
pub const STYLE_IMAGE_BUTTON_CALL: &str = "image_button";

/// Extracts the literal style names referenced by a GDScript/`.tscn` source.
///
/// The result is sorted and de-duplicated. Comments and dynamic style
/// expressions are ignored (`result.style = style` yields nothing).
pub fn scan_style_references(source: &str) -> Vec<String> {
    let mut refs = Vec::new();
    collect_assignment_refs(source, STYLE_ASSIGNMENT_TOKEN, &mut refs);
    collect_call_refs(source, STYLE_LABEL_CALL, &mut refs);
    collect_call_refs(source, STYLE_IMAGE_BUTTON_CALL, &mut refs);
    refs.sort();
    refs.dedup();
    refs
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn find_from(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || start >= haystack.len() {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

fn skip_ascii_space(bytes: &[u8], mut pos: usize) -> usize {
    while matches!(bytes.get(pos), Some(b' ' | b'\t' | b'\r' | b'\n')) {
        pos += 1;
    }
    pos
}

/// Reads a double-quoted string starting at `start` (which must point at the
/// opening quote), honoring backslash escapes.
fn read_quoted(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    let mut out = Vec::new();
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return Some((String::from_utf8(out).ok()?, index + 1)),
            b'\\' => {
                index += 1;
                out.push(*bytes.get(index)?);
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    None
}

/// Collects `theme_type_variation = "style"` references.
fn collect_assignment_refs(source: &str, token: &str, out: &mut Vec<String>) {
    let bytes = source.as_bytes();
    let needle = token.as_bytes();
    let mut search = 0;
    while let Some(position) = find_from(bytes, needle, search) {
        search = position + needle.len();
        if position > 0 && is_ident_byte(bytes[position - 1]) {
            continue;
        }
        let after = skip_ascii_space(bytes, position + needle.len());
        if bytes.get(after) != Some(&b'=') {
            continue;
        }
        let mut quoted = skip_ascii_space(bytes, after + 1);
        // Godot scenes use StringName literals: `theme_type_variation = &"x"`.
        if bytes.get(quoted) == Some(&b'&') {
            quoted = skip_ascii_space(bytes, quoted + 1);
        }
        if let Some((value, _)) = read_quoted(bytes, quoted) {
            out.push(value);
        }
    }
}

/// Collects the last quoted literal inside `token(...)` calls.
fn collect_call_refs(source: &str, token: &str, out: &mut Vec<String>) {
    let bytes = source.as_bytes();
    let needle = token.as_bytes();
    let mut search = 0;
    while let Some(position) = find_from(bytes, needle, search) {
        search = position + needle.len();
        if position > 0 && is_ident_byte(bytes[position - 1]) {
            continue;
        }
        let open = skip_ascii_space(bytes, position + needle.len());
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let mut depth = 0usize;
        let mut index = open;
        let mut literal: Option<String> = None;
        while index < bytes.len() {
            match bytes[index] {
                b'"' => {
                    if let Some((value, next)) = read_quoted(bytes, index) {
                        literal = Some(value);
                        index = next;
                        continue;
                    }
                    break;
                }
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            index += 1;
        }
        if let Some(value) = literal {
            out.push(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::manifest::StylesManifest;

    fn manifest() -> StylesManifest {
        StylesManifest::from_json(
            r#"{"format":1,"text_buttons":["defaultt","grayt"],"labels":["defaultLabel"],"drawables":["black","grayPanel"]}"#,
        )
        .unwrap()
    }

    #[test]
    fn style_lookup_resolves_kinds() {
        let manifest = manifest();
        assert!(lookup(&manifest, StyleKind::TextButton, "grayt"));
        assert!(!lookup(&manifest, StyleKind::TextButton, "black"));
        assert!(lookup(&manifest, StyleKind::Drawable, "black"));
        assert_eq!(
            StyleKind::of(&manifest, "grayt"),
            Some(StyleKind::TextButton)
        );
        assert_eq!(StyleKind::of(&manifest, "missing"), None);
        assert_eq!(StyleKind::TextButton.key(), "text_buttons");
    }

    #[test]
    fn style_lookup_wrapper_resolves_and_matches() {
        let manifest = manifest();
        let lookup = StyleLookup::new(&manifest);
        assert_eq!(lookup.resolve("grayt"), Some(StyleKind::TextButton));
        assert!(lookup.matches(StyleKind::TextButton, "grayt"));
        assert!(!lookup.matches(StyleKind::Drawable, "grayt"));
        assert!(lookup.matches(StyleKind::Drawable, "grayPanel"));
        assert_eq!(lookup.resolve("missing"), None);
        assert_eq!(lookup.names(StyleKind::TextButton), &["defaultt", "grayt"]);
    }

    #[test]
    fn validate_accepts_resolvable_and_reports_unresolved() {
        let manifest = manifest();
        let lookup = StyleLookup::new(&manifest);
        assert!(lookup.validate(["defaultt", "grayt", "black"]).is_ok());
        assert_eq!(
            lookup
                .validate(["grayt", "bogus", "zzz", "bogus"])
                .unwrap_err(),
            vec!["bogus".to_owned(), "zzz".to_owned()]
        );
        assert!(lookup.validate(std::iter::empty()).is_ok());
    }

    #[test]
    fn scan_extracts_literal_style_references() {
        let source = concat!(
            "button.theme_type_variation = \"defaultt\"\n",
            "card.theme_type_variation = &\"flatBordert\"\n",
            "result.theme_type_variation = style\n",
            "label := MindWidgets.styled_label(text, \"techLabel\")\n",
            "icon := MindWidgets.image_button(\"list\", \"emptyi\")\n",
            "## prose theme_type_variation without an assignment\n",
            "default := MindWidgets.image_button(region)\n",
        );
        assert_eq!(
            scan_style_references(source),
            vec![
                "defaultt".to_owned(),
                "emptyi".to_owned(),
                "flatBordert".to_owned(),
                "techLabel".to_owned(),
            ]
        );
        assert!(scan_style_references("no styles here").is_empty());
    }

    fn scan_tree(root: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_tree(&path, out);
                continue;
            }
            if matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("gd" | "tscn")
            ) && let Ok(text) = std::fs::read_to_string(&path)
            {
                out.extend(scan_style_references(&text));
            }
        }
    }

    /// Every literal style name referenced by the committed dialog/fragment
    /// sources (GDScript + `.tscn`) must resolve against the committed styles
    /// manifest (plan §7e: "`UiStyleLookup` resolves every dialog style
    /// reference").
    #[test]
    fn repo_dialog_sources_reference_only_manifest_styles() {
        let client = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let styles_path = client.join("ui/styles_manifest.json");
        let text = std::fs::read_to_string(&styles_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", styles_path.display()));
        let manifest = StylesManifest::from_json(&text).expect("parse styles_manifest.json");
        let lookup = StyleLookup::new(&manifest);

        let mut refs = Vec::new();
        scan_tree(&client.join("ui"), &mut refs);
        scan_tree(&client.join("scenes/ui"), &mut refs);
        refs.sort();
        refs.dedup();
        assert!(
            !refs.is_empty(),
            "expected committed dialog sources to reference styles"
        );
        lookup
            .validate(refs.iter().map(String::as_str))
            .unwrap_or_else(|unresolved| {
                panic!("dialog sources reference unregistered styles: {unresolved:?}")
            });
    }
}
