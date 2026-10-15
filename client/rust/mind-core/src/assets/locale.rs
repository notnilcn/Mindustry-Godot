// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Locale codes and closest-locale selection.
//!
//! Ported from `Vars.java:315-327` (the `assets/locales` list becomes Java
//! `Locale`s: `lang` or `lang_COUNTRY`, case-normalized like `java.util.Locale`)
//! and `LanguageDialog.findClosestLocale` (`LanguageDialog.java:110-128`: an
//! exact OS-locale match wins, then the same language, else `en`). The caller
//! stores the result (`Core.settings.put("locale", ...)`), so this module is
//! pure and Godot-free.

/// Parses a locale code into `(language, country?)`.
///
/// `java.util.Locale` lower-cases the language and upper-cases the country, so
/// `"pt_br"` and `"pt_BR"` compare equal (Java `Locale.equals`).
pub fn parse_locale(code: &str) -> (String, Option<String>) {
    let mut parts = code.split('_');
    let language = parts.next().unwrap_or("").to_ascii_lowercase();
    let country = parts
        .next()
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_uppercase);
    (language, country)
}

/// The language half of a code (`Locale.getLanguage()`).
pub fn language(code: &str) -> String {
    parse_locale(code).0
}

/// `LanguageDialog.findClosestLocale`: exact `(language, country)` match first,
/// then the first locale with the same language, else `"en"`.
///
/// `requested` is the OS locale (`Locale.getDefault()`, e.g. `en_US`, `de_DE`
/// or `ja`); `available` is the `assets/locales` code list (`Vars.java:315`).
pub fn find_closest_locale(requested: &str, available: &[String]) -> String {
    let target = parse_locale(requested);
    // `l.equals(Locale.getDefault())` — language + country + variant.
    if let Some(code) = available.iter().find(|code| parse_locale(code) == target) {
        return code.clone();
    }
    // `l.getLanguage().equals(Locale.getDefault().getLanguage())`.
    if let Some(code) = available
        .iter()
        .find(|code| parse_locale(code).0 == target.0)
    {
        return code.clone();
    }
    // `Core.settings.put("locale", new Locale("en").toString())`.
    String::from("en")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locales() -> Vec<String> {
        ["en", "de", "pt_BR", "pt_PT", "zh_CN", "ja"]
            .iter()
            .map(|code| (*code).to_owned())
            .collect()
    }

    /// `LanguageDialog.findClosestLocale`: exact, then language, then `en`.
    #[test]
    fn resolve_locale_default_picks_closest() {
        let available = locales();
        assert_eq!(find_closest_locale("pt_BR", &available), "pt_BR");
        assert_eq!(find_closest_locale("pt_br", &available), "pt_BR");
        assert_eq!(find_closest_locale("pt_PT", &available), "pt_PT");
        assert_eq!(find_closest_locale("de_DE", &available), "de");
        assert_eq!(find_closest_locale("de_AT", &available), "de");
        assert_eq!(find_closest_locale("ja_JP", &available), "ja");
        // No language match: English (`LanguageDialog.java:127`).
        assert_eq!(find_closest_locale("xx_XX", &available), "en");
        assert_eq!(find_closest_locale("en_US", &available), "en");
    }

    #[test]
    fn parse_locale_normalizes_case() {
        assert_eq!(parse_locale("pt_BR"), ("pt".into(), Some("BR".into())));
        assert_eq!(parse_locale("ja"), ("ja".into(), None));
        assert_eq!(parse_locale("zh_tw"), ("zh".into(), Some("TW".into())));
        assert_eq!(language("zh_TW"), "zh");
    }
}
