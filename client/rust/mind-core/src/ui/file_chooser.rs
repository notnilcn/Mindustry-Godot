// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/FileChooser.java (plan 14 M7 §3.8).

//! `FileChooser` params + validation (plan 14 M7).
//!
//! Godot-free port of `FileChooser.FileChooserParams`: the extension filter,
//! title resolution (`open`/`save` bundle keys, `@` prefix), default file name
//! (`file.<ext0>`) and `Strings.sanitizeFilename`. The native/fallback scene and
//! the actual file I/O stay in `mind-gdext` (native `DisplayServer`) and plan 04.

use crate::content::BundleView;

/// File-chooser request (1:1 with `FileChooserParams`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChooserParams {
    /// Open (`true`) vs save (`false`).
    pub open: bool,
    /// Multiple selection allowed (`submitMulti`).
    pub allow_multiple: bool,
    /// Resolved dialog title (bundle lookup already applied).
    pub title: String,
    /// Sanitized default file name.
    pub file_name: String,
    /// Accepted file extensions (without the dot).
    pub extensions: Vec<String>,
    /// Single-file result handler id (captured by the host, plan 21/22 seam).
    pub handler: String,
    /// Multi-file result handler id.
    pub multiple_handler: String,
}

/// Validation failures (mirrors the upstream `IllegalArgumentException`s).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChooserError {
    /// No extensions were defined.
    NoExtensions,
    /// `submitMulti` with a save request.
    SaveMultiple,
}

impl std::fmt::Display for FileChooserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileChooserError::NoExtensions => {
                write!(f, "Extension types must be defined.")
            }
            FileChooserError::SaveMultiple => {
                write!(
                    f,
                    "Saving in a file chooser with multiple choices does not make sense."
                )
            }
        }
    }
}

impl std::error::Error for FileChooserError {}

/// `Strings.sanitizeFilename` — replaces every character outside
/// `[A-Za-z0-9-_.]` with `_`.
pub fn sanitize_filename(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// `Fi.extension` (lowercase-free raw extension, no dot).
pub fn extension_of(name: &str) -> Option<&str> {
    let (_, ext) = name.rsplit_once('.')?;
    if ext.is_empty() { None } else { Some(ext) }
}

/// `Fi.extEquals` (case-insensitive extension compare).
pub fn ext_equals(name: &str, extension: &str) -> bool {
    extension_of(name).is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
}

impl FileChooserParams {
    /// Empty params (mirrors `new FileChooserParams()`).
    pub fn new() -> Self {
        Self {
            open: false,
            allow_multiple: false,
            title: String::new(),
            file_name: String::new(),
            extensions: Vec::new(),
            handler: String::new(),
            multiple_handler: String::new(),
        }
    }

    /// `FileChooser.open(...)`.
    pub fn open(extensions: &[&str]) -> Self {
        Self::new().with_open(true).with_extensions(extensions)
    }

    /// `FileChooser.save(...)`.
    pub fn save(extensions: &[&str]) -> Self {
        Self::new().with_open(false).with_extensions(extensions)
    }

    /// Builder: sets `open`.
    pub fn with_open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Builder: sets `extensions`.
    pub fn with_extensions(mut self, extensions: &[&str]) -> Self {
        self.extensions = extensions.iter().map(|ext| (*ext).to_owned()).collect();
        self
    }

    /// Builder: sets the file name (`name`).
    pub fn with_name(mut self, file_name: &str) -> Self {
        self.file_name = file_name.to_owned();
        self
    }

    /// Builder: sets an explicit title (`title`).
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = title.to_owned();
        self
    }

    /// `checkParams`: resolves the title/file name and rejects missing
    /// extensions. Idempotent.
    pub fn check_params(&mut self, bundle: &dyn BundleView) -> Result<(), FileChooserError> {
        if self.extensions.is_empty() {
            return Err(FileChooserError::NoExtensions);
        }
        if self.title.is_empty() {
            let key = if self.open { "open" } else { "save" };
            self.title = bundle.get_or(key, key);
        } else if let Some(stripped) = self.title.strip_prefix('@') {
            self.title = bundle.get_or(stripped, stripped);
        }
        if self.file_name.is_empty() {
            self.file_name = format!("file.{}", self.extensions[0]);
        }
        self.file_name = sanitize_filename(&self.file_name);
        Ok(())
    }

    /// `submit`: validates and records the single-file handler id.
    pub fn submit(
        &mut self,
        bundle: &dyn BundleView,
        handler: &str,
    ) -> Result<(), FileChooserError> {
        self.check_params(bundle)?;
        self.handler = handler.to_owned();
        Ok(())
    }

    /// `submitMulti`: validates, forces `allowMultiple`, records the handler.
    pub fn submit_multi(
        &mut self,
        bundle: &dyn BundleView,
        handler: &str,
    ) -> Result<(), FileChooserError> {
        self.check_params(bundle)?;
        if !self.open {
            return Err(FileChooserError::SaveMultiple);
        }
        self.multiple_handler = handler.to_owned();
        self.allow_multiple = true;
        Ok(())
    }

    /// `Structs.contains(extensions, file::extEquals)` — the fallback list filter.
    pub fn accepts(&self, file_name: &str) -> bool {
        self.extensions
            .iter()
            .any(|extension| ext_equals(file_name, extension))
    }

    /// The save-path rewrite (`nameWithoutExtension + "." + extensions[0]`).
    pub fn save_target_name(&self, file_name: &str) -> String {
        let stem = file_name
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(file_name);
        format!(
            "{stem}.{}",
            self.extensions.first().map(String::as_str).unwrap_or("")
        )
    }
}

impl Default for FileChooserParams {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::MemoryBundle;

    fn bundle() -> MemoryBundle {
        let mut bundle = MemoryBundle::new();
        bundle.insert("open", "Open File");
        bundle.insert("save", "Save File");
        bundle
    }

    #[test]
    fn open_defaults_resolve_title_and_name() {
        let mut params = FileChooserParams::open(&["msch"]);
        params.check_params(&bundle()).unwrap();
        assert_eq!(params.title, "Open File");
        assert_eq!(params.file_name, "file.msch");
        assert!(params.open);
    }

    #[test]
    fn save_defaults_and_at_prefixed_title() {
        let mut params = FileChooserParams::save(&["png", "jpg"]).with_title("@save");
        params.check_params(&bundle()).unwrap();
        assert_eq!(params.title, "Save File");
        assert_eq!(params.file_name, "file.png");
    }

    #[test]
    fn sanitizes_file_name() {
        assert_eq!(sanitize_filename("my map:1*.msav"), "my_map_1_.msav");
        let mut params = FileChooserParams::save(&["msav"]).with_name("my map:1");
        params.check_params(&bundle()).unwrap();
        assert_eq!(params.file_name, "my_map_1");
    }

    #[test]
    fn rejects_missing_extensions() {
        let mut params = FileChooserParams::new();
        assert_eq!(
            params.check_params(&bundle()),
            Err(FileChooserError::NoExtensions)
        );
    }

    #[test]
    fn submit_multi_rejects_save() {
        let mut params = FileChooserParams::save(&["msch"]);
        assert_eq!(
            params.submit_multi(&bundle(), "h"),
            Err(FileChooserError::SaveMultiple)
        );
    }

    #[test]
    fn extension_filter_and_save_target() {
        let params = FileChooserParams::open(&["msch", "msav"]);
        assert!(params.accepts("base.MSCH"));
        assert!(params.accepts("campaign.msav"));
        assert!(!params.accepts("image.png"));

        let save = FileChooserParams::save(&["png"]);
        assert_eq!(save.save_target_name("shot.jpeg"), "shot.png");
        assert_eq!(save.save_target_name("noext"), "noext.png");
    }
}
