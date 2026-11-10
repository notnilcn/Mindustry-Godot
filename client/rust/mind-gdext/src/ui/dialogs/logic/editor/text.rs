// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `text` support code.
//! Upstream: logic/LStatement.java, logic/LCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// text / bundle helpers (`LStatement` / `LCanvas`)
// ---------------------------------------------------------------------------

/// `LogicDialog` bundle lookup (`_t`).
pub(super) fn bundle(key: &str) -> String {
    kit::t(key).to_string()
}

/// A `GString` from any string-like value (avoids the `From<String>` gap).
pub(super) fn gs(text: impl AsRef<str>) -> GString {
    GString::from(text.as_ref())
}

/// `LStatement.statementKey()`: `typeName().toLowerCase()`.
pub(super) fn statement_key(state: &Rc<RefCell<LogicState>>, statement_name: &str) -> String {
    type_name(state, statement_name).to_lowercase()
}

/// `LStatement.typeName()`: the Java class name without `Statement`
/// (`InvalidStatement` -> `Invalid`, `OperationStatement` -> `Operation`).
pub(super) fn type_name(state: &Rc<RefCell<LogicState>>, statement_name: &str) -> String {
    let variant = dict_string(&meta(state, statement_name), "rust_variant");
    if variant.is_empty() {
        statement_name.to_string()
    } else {
        variant
    }
}

/// Registry metadata for a statement (`statement_meta`, cached).
pub(super) fn meta(state: &Rc<RefCell<LogicState>>, statement_name: &str) -> VarDictionary {
    if let Some(info) = state.borrow().meta_cache.get(statement_name) {
        return info.clone();
    }
    let mut info = VarDictionary::new();
    if let Some(mut logic) = kit::logic()
        && logic.has_method("statement_meta")
        && let Ok(dict) = logic
            .call("statement_meta", &[gs(statement_name).to_variant()])
            .try_to::<VarDictionary>()
    {
        info = dict;
    }
    state
        .borrow_mut()
        .meta_cache
        .insert(statement_name.to_string(), info.clone());
    info
}

/// `Strings.insertSpaces`: a space before every non-leading upper-case letter.
pub(super) fn insert_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    for (index, ch) in text.chars().enumerate() {
        if index > 0 && ch.is_uppercase() {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// Arc colour markup (`[accent]`, `[#rrggbb]`, `[]`) removed for plain-text
/// tooltips; `[[` stays a literal bracket (`_plain_markup`).
pub(super) fn plain_markup(text: &str) -> String {
    let escaped = text.replace("[[", "\u{1}");
    let mut out = String::with_capacity(escaped.len());
    let mut chars = escaped.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '[' {
            out.push(ch);
            continue;
        }
        let mut inner = String::new();
        let mut closed = false;
        for next in chars.by_ref() {
            if next == ']' {
                closed = true;
                break;
            }
            inner.push(next);
        }
        if closed && markup_tag(&inner) {
            continue;
        }
        out.push('[');
        out.push_str(&inner);
        if closed {
            out.push(']');
        }
    }
    out.replace('\u{1}', "[")
}

/// `\[(#[0-9a-fA-F]{3,8}|[a-zA-Z]*)\]`: a colour tag or an alphabetic one.
pub(super) fn markup_tag(inner: &str) -> bool {
    if let Some(hex) = inner.strip_prefix('#') {
        return (3..=8).contains(&hex.len()) && hex.chars().all(|ch| ch.is_ascii_hexdigit());
    }
    inner.chars().all(|ch| ch.is_ascii_alphabetic())
}

/// `LStatement.logicLocalization(): Core.settings.getBool("logiclocalization")`.
pub(super) fn logic_localization() -> bool {
    let Some(mut ui) = kit::ui() else {
        return true;
    };
    if !ui.has_method("settings_get_bool") {
        return true;
    }
    ui.call("settings_get_bool", &[gs("logiclocalization").to_variant()])
        .try_to::<bool>()
        .unwrap_or(true)
}

/// `LStatement.localizedName()` (`LStatement.java:371-374`):
/// `instruction.<statementKey>` with `insertSpaces(typeName)` as the fallback;
/// the raw name when the `logiclocalization` setting is off.
pub(super) fn localized_name(state: &Rc<RefCell<LogicState>>, statement_name: &str) -> String {
    let plain = insert_spaces(&type_name(state, statement_name));
    if !logic_localization() {
        return plain;
    }
    let key = format!("instruction.{}", statement_key(state, statement_name));
    let resolved = bundle(&key);
    if resolved == key { plain } else { resolved }
}

/// `LCanvas.tooltip(cell, key)`: the bundle text when the key exists.
pub(super) fn bundle_tooltip(key: &str) -> String {
    let lkey = key.to_lowercase().replace(' ', "");
    let resolved = bundle(&lkey);
    if resolved == lkey {
        String::new()
    } else {
        plain_markup(&resolved)
    }
}

/// `bundle(desc)`: `name.token.<token>` with the raw token as the fallback.
pub(super) fn field_label_text(token: &str) -> String {
    if token.is_empty() {
        return String::new();
    }
    if !logic_localization() {
        return token.to_string();
    }
    let key = format!("name.token.{token}");
    let resolved = bundle(&key);
    if resolved == key {
        token.to_string()
    } else {
        resolved
    }
}

/// The header title: `localizedName()`, plus `-> <dest>` for a set jump.
pub(super) fn header_text(state: &Rc<RefCell<LogicState>>, row: &StatementRow) -> String {
    let mut text = localized_name(state, &row.name);
    let count = state.borrow().statements.len();
    let dest = row.jump_dest(count);
    if row.name == "jump" && dest >= 0 {
        text.push_str(&format!(" -> {dest}"));
    }
    text
}
