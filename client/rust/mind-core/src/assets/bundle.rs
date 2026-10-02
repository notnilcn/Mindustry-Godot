// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: Arc `arc.util.io.PropertiesUtils` (`load` state machine),
//         Arc `arc.util.I18NBundle` (candidate-locale parent chain),
//         core/src/mindustry/Vars.java (`loadSettings` bundle block,
//         external bundle, `global.properties` merge),
//         core/src/mindustry/ui/IntFormat.java (cached per-frame formats).

//! Bundle catalog + `IntFormat` (plan 03 §3.3/M7).
//!
//! The Godot-free half of the runtime i18n layer: a `.properties` parser
//! matching Arc `PropertiesUtils` (comments, inline separators, line
//! continuations, escape sequences and `\uXXXX`), the candidate-locale parent
//! chain (`bundle_<lang>_<REGION>` → `bundle_<lang>` → `bundle`), the
//! `global.properties` override, `{0}` formatting and a bounded `IntFormat`
//! cache. No filesystem access: callers feed parsed text.

use indexmap::IndexMap;

/// Parses a `.properties` text into an ordered key→value map.
///
/// Faithful port of the Arc `PropertiesUtils.load` line-oriented state machine
/// (which is itself the Apache Harmony `java.util.Properties` parser).
#[allow(unused_assignments)] // macro-reset assignments are conditional reads
pub fn parse_properties(text: &str) -> IndexMap<String, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut map = IndexMap::new();
    let mut buf = String::new();
    let mut key_length: Option<usize> = None;
    let mut key_done = false;
    let mut first_char = true;
    let mut skipping_comment = false;
    let mut i = 0;
    let mut pending_slash = false;
    let mut unicode_accum = 0u32;
    let mut unicode_count = 0u32;
    let mut unicode_mode = false;

    macro_rules! flush_line {
        () => {
            if !buf.is_empty() || key_length == Some(0) {
                let klen = key_length.unwrap_or(buf.chars().count());
                let key: String = buf.chars().take(klen).collect();
                let value: String = buf.chars().skip(klen).collect();
                map.insert(key, value);
            }
            buf.clear();
            key_length = None;
            key_done = false;
            first_char = true;
            skipping_comment = false;
            pending_slash = false;
        };
    }

    while i < chars.len() {
        let c = chars[i];
        i += 1;

        if unicode_mode {
            if let Some(digit) = c.to_digit(16) {
                unicode_accum = (unicode_accum << 4) + digit;
                unicode_count += 1;
                if unicode_count < 4 {
                    continue;
                }
                unicode_mode = false;
                if let Some(ch) = char::from_u32(unicode_accum) {
                    buf.push(ch);
                }
                if c == '\n' {
                    flush_line!();
                }
                continue;
            }
            // Not enough digits before a non-hex char: emit what we have.
            unicode_mode = false;
            if let Some(ch) = char::from_u32(unicode_accum) {
                buf.push(ch);
            }
        }

        if pending_slash {
            pending_slash = false;
            match c {
                '\r' | '\n' => {
                    // Line continuation: swallow the newline and following
                    // leading whitespace.
                    while i < chars.len() && chars[i].is_whitespace() {
                        if chars[i] == '\n' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    continue;
                }
                'b' => buf.push('\u{0008}'),
                'f' => buf.push('\u{000c}'),
                'n' => buf.push('\n'),
                'r' => buf.push('\r'),
                't' => buf.push('\t'),
                'u' => {
                    unicode_mode = true;
                    unicode_accum = 0;
                    unicode_count = 0;
                }
                other => buf.push(other),
            }
            first_char = false;
            continue;
        }

        if c == '\\' {
            if key_done && key_length.is_none() {
                key_length = Some(buf.chars().count());
            }
            pending_slash = true;
            continue;
        }

        match c {
            '#' | '!' if first_char => {
                // Comment: skip to end of line.
                while i < chars.len() && chars[i] != '\n' && chars[i] != '\r' {
                    i += 1;
                }
                // Consume the newline itself; the loop's flush handles the
                // (empty) line below.
                continue;
            }
            '\n' | '\r' => {
                if skipping_comment {
                    skipping_comment = false;
                    buf.clear();
                    key_length = None;
                    first_char = true;
                    continue;
                }
                flush_line!();
                continue;
            }
            ':' | '=' if key_length.is_none() => {
                key_length = Some(buf.chars().count());
                key_done = false;
                continue;
            }
            _ => {}
        }

        if c.is_whitespace() {
            if buf.is_empty() || Some(buf.chars().count()) == key_length {
                continue;
            }
            if key_length.is_none() {
                key_done = true;
                continue;
            }
        }

        first_char = false;
        if key_done {
            key_length = Some(buf.chars().count());
            key_done = false;
        }
        buf.push(c);
    }

    if pending_slash {
        buf.push('\u{0000}');
    }
    if key_length.is_none() && !buf.is_empty() {
        key_length = Some(buf.chars().count());
    }
    flush_line!();

    map
}

/// The loaded bundle chain (`Core.bundle`).
///
/// Layers are ordered **most specific first**; [`Bundle::get`] returns the
/// first non-empty value. The `global.properties` overlay wins over every
/// layer; an empty value is treated as missing for fallback purposes (Arc
/// `getOrNull` semantics).
#[derive(Debug, Clone, Default)]
pub struct Bundle {
    layers: Vec<IndexMap<String, String>>,
    global: IndexMap<String, String>,
}

impl Bundle {
    /// Empty bundle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a bundle from a most-specific-first layer list.
    pub fn from_layers(layers: Vec<IndexMap<String, String>>) -> Self {
        Self {
            layers,
            global: IndexMap::new(),
        }
    }

    /// `global.properties` merge (`putAll`, overrides every layer).
    pub fn merge_global(&mut self, properties: IndexMap<String, String>) {
        for (key, value) in properties {
            self.global.insert(key, value);
        }
    }

    /// Plan-20 hook: merge/overwrite a locale layer's properties.
    pub fn merge_assets(&mut self, locale: &str, properties: IndexMap<String, String>) {
        let _ = locale;
        // The mod layer is the most specific.
        if self.layers.is_empty() {
            self.layers.push(IndexMap::new());
        }
        for (key, value) in properties {
            self.layers[0].insert(key, value);
        }
    }

    /// Raw value lookup: global overlay, then each layer, skipping empty values.
    pub fn get_or_null(&self, key: &str) -> Option<&str> {
        if let Some(value) = self.global.get(key)
            && !value.is_empty()
        {
            return Some(value);
        }
        for layer in &self.layers {
            if let Some(value) = layer.get(key)
                && !value.is_empty()
            {
                return Some(value);
            }
        }
        None
    }

    /// `Core.bundle.get(key)` — the key itself when missing.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.get_or_null(key).unwrap_or(key)
    }

    /// `{0}`-style substitution (`Arc TextFormatter`).
    pub fn format(&self, key: &str, args: &[&str]) -> String {
        format_template(self.get(key), args)
    }

    /// All keys (global first, then most-specific layer order), de-duplicated.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        let mut seen: indexmap::IndexSet<&str> = indexmap::IndexSet::new();
        for key in self.global.keys() {
            seen.insert(key.as_str());
        }
        for layer in &self.layers {
            for key in layer.keys() {
                seen.insert(key.as_str());
            }
        }
        seen.into_iter()
    }

    /// Number of layers.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

/// `Arc TextFormatter.format`: replaces `{0}`, `{1}`, … in template order.
pub fn format_template(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            let mut j = i + 1;
            let mut index = 0usize;
            let mut digits = 0;
            while j < chars.len() && chars[j].is_ascii_digit() {
                index = index * 10 + (chars[j] as usize - '0' as usize);
                digits += 1;
                j += 1;
            }
            if digits > 0 && j < chars.len() && chars[j] == '}' {
                if let Some(arg) = args.get(index) {
                    out.push_str(arg);
                }
                i = j + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Bounded cache of rendered integer formats (`IntFormat`, plan §6.6).
#[derive(Debug)]
pub struct IntFormat {
    text: String,
    /// LRU-ish bounded map keyed by the rendered argument tuple.
    cache: IndexMap<String, String>,
    capacity: usize,
}

impl IntFormat {
    /// Creates a format for `text` with the default 512-entry bound.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            cache: IndexMap::new(),
            capacity: 512,
        }
    }

    /// Formats one integer (`IntFormat.get(int)`), cached.
    pub fn get(&mut self, bundle: &Bundle, value: i32) -> String {
        self.render(bundle, &[value])
    }

    /// Formats two integers (`IntFormat.get(int, int)`), cached.
    pub fn get2(&mut self, bundle: &Bundle, value1: i32, value2: i32) -> String {
        self.render(bundle, &[value1, value2])
    }

    fn render(&mut self, bundle: &Bundle, values: &[i32]) -> String {
        let key = values
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join("\u{1f}");
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let args: Vec<String> = values.iter().map(i32::to_string).collect();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let rendered = bundle.format(&self.text, &args);
        if self.cache.len() >= self.capacity
            && let Some(first) = self.cache.keys().next().cloned()
        {
            self.cache.shift_remove(&first);
        }
        self.cache.insert(key, rendered.clone());
        rendered
    }
}

/// Builds the candidate-locale bundle file suffixes, most specific first
/// (`pt_BR` → `bundle_pt_BR`, `bundle_pt`, `bundle`).
pub fn locale_chain(locale: &str) -> Vec<String> {
    let mut out = Vec::new();
    if locale.is_empty() || locale == "default" || locale == "router" {
        out.push(String::from("bundle"));
        return out;
    }
    let mut parts: Vec<&str> = locale.split('_').collect();
    loop {
        out.push(format!("bundle_{}", parts.join("_")));
        if parts.len() == 1 {
            break;
        }
        parts.pop();
    }
    out.push(String::from("bundle"));
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_handles_separators_escapes_and_comments() {
        let text = concat!(
            "# a comment\n",
            "! another\n",
            "a=1\n",
            "b: two\n",
            "c three\n",
            "esc\\taped = x\\ny\\u0041\n",
            "continued = one\\\n  two\n",
            "unicodeValue = \\u26a0 warn\n",
        );
        let map = parse_properties(text);
        assert_eq!(map.get("a").unwrap(), "1");
        assert_eq!(map.get("b").unwrap(), "two");
        assert_eq!(map.get("c").unwrap(), "three");
        assert_eq!(map.get("esc\taped").unwrap(), "x\nyA");
        assert_eq!(map.get("continued").unwrap(), "onetwo");
        // A value starting with a `\uXXXX` escape must not bleed into the key.
        assert_eq!(map.get("unicodeValue").unwrap(), "\u{26a0} warn");
    }

    #[test]
    fn locale_chain_and_global_merge() {
        let base = parse_properties("a=base\nb=base-b\n");
        let pt = parse_properties("b=pt-b\nempty=\n");
        let pt_br = parse_properties("a=br-a\n");
        // Most specific first.
        let mut bundle = Bundle::from_layers(vec![pt_br, pt, base]);
        assert_eq!(bundle.get("a"), "br-a");
        assert_eq!(bundle.get("b"), "pt-b");
        // Empty value is treated as missing and walks the parent chain, so the
        // key echoes itself when no parent provides a value.
        assert_eq!(bundle.get("empty"), "empty");
        assert_eq!(bundle.get_or_null("empty"), None);
        // Missing key echoes itself.
        assert_eq!(bundle.get("missing.key"), "missing.key");
        assert_eq!(bundle.get_or_null("missing.key"), None);

        bundle.merge_global(parse_properties("b=global-b\nc=global-c\n"));
        assert_eq!(bundle.get("b"), "global-b");
        assert_eq!(bundle.get("c"), "global-c");
        assert_eq!(bundle.get("a"), "br-a");
    }

    #[test]
    fn format_placeholders() {
        let bundle = Bundle::from_layers(vec![parse_properties(
            "hello=Hello {0}, you have {1} items. {0} again\n",
        )]);
        assert_eq!(
            bundle.format("hello", &["World", "3"]),
            "Hello World, you have 3 items. World again"
        );
        // Missing key formats the raw key.
        assert_eq!(bundle.format("nope", &["x"]), "nope");
    }

    #[test]
    fn locale_chain_suffixes() {
        assert_eq!(
            locale_chain("pt_BR"),
            vec!["bundle_pt_BR", "bundle_pt", "bundle"]
        );
        assert_eq!(locale_chain("en"), vec!["bundle_en", "bundle"]);
        assert_eq!(locale_chain("router"), vec!["bundle"]);
    }

    #[test]
    fn int_format_caches_and_bounds() {
        let bundle = Bundle::from_layers(vec![parse_properties("wave=Wave {0}\n")]);
        let mut format = IntFormat::new("wave");
        assert_eq!(format.get(&bundle, 3), "Wave 3");
        assert_eq!(format.get(&bundle, 3), "Wave 3");
        assert_eq!(format.cache.len(), 1);
        format.capacity = 2;
        format.get(&bundle, 4);
        format.get(&bundle, 5);
        assert!(format.cache.len() <= 2);
    }
}
