// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Fonts.logic` bitmap-font port: metrics, charset and the icon-tag mapping.
//!
//! Java draws `print`/`printchar` text on a `LogicDisplay` through
//! `Fonts.logic`, a 16 px FreeType font generated at startup into an Arc
//! `BitmapFontData` (`ui/Fonts.java:88-96`). Two consumers read that data:
//!
//! - `LExecutor.DrawI` lays the text out with `data.spaceXadvance` and
//!   `data.lineHeight` (`logic/LExecutor.java:949-995`);
//! - `LogicDisplay.processCommands` places one glyph with
//!   `glyph.xoffset`/`glyph.yoffset` and `data.capHeight + data.ascent`
//!   (`world/blocks/logic/LogicDisplay.java:227-235`).
//!
//! This module ports the pure parts: the libGDX FreeType metric generation
//! (`extensions/freetype/src/arc/freetype/FreeTypeFontGenerator.java:339-383`)
//! over the shipped `assets/fonts/logic.ttf`, the generated glyph charset, and
//! the `Fonts.unicodeToName`/`Fonts.getLargeIcon` mapping used by the world
//! processor tag (`world/blocks/logic/LogicBlock.java:715-724`).
//! Rasterising the glyph bitmaps themselves stays on the view side
//! (`mind-gdext::display_context`) because it draws the same FreeType glyphs
//! through Godot's `FontFile`.

use std::sync::LazyLock;

use crate::assets::icons::{IconCodes, Iconc};

/// `Fonts.logic`'s generated pixel size (`Fonts.java:90`).
pub const LOGIC_FONT_SIZE: u32 = 16;

/// The generated glyph set (`Fonts.java:92`), minus the space glyph the
/// generator always synthesises (`FreeTypeFontGenerator.java:508-517`).
///
/// `'\0'` is the `.notdef` box; `€` is the one non-ASCII drawable glyph.
pub const GLYPH_CHARS: &str = "\0ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz1234567890\"!`?'.,;:()[]{}<>|/@\\^$€-%+=#_&~*";

/// `logic.ttf` as shipped (metric source; the view side rasterises the same
/// file through Godot's `FontFile`).
const LOGIC_TTF: &[u8] = include_bytes!("../../../../../assets/fonts/logic.ttf");

/// Arc `BitmapFontData` metrics the logic paths read (`Font.java` `FontData`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitmapFontMetrics {
    /// `FontData.ascent`: the FreeType ascender **minus** `capHeight`
    /// (`FreeTypeFontGenerator.java:377-383`).
    pub ascent: i32,
    /// `FontData.descent` (negative).
    pub descent: i32,
    /// `FontData.capHeight`: the rendered height of the first available
    /// `capChars` entry (`M` here).
    pub cap_height: i32,
    /// `FontData.xHeight`: the rendered height of the first available
    /// `xChars` entry (`x` here).
    pub x_height: i32,
    /// `FontData.lineHeight` (ascender - descender + line gap).
    pub line_height: i32,
    /// `FontData.spaceXadvance`. The logic font is monospaced: the layout uses
    /// this advance for every character (`LExecutor.java:953`).
    pub space_x_advance: i32,
    /// Whether a `missingGlyph` was registered (`FontData.missingGlyph`): the
    /// `'\0'` glyph's bitmap is non-empty, so `hasGlyph` is true for every
    /// character (`FreeTypeFontGenerator.java:434-441`).
    pub missing_glyph: bool,
}

/// The committed metric values (`Fonts.logic`, 16 px). Used if the embedded
/// font ever fails to parse; [`logic_font_metrics`] derives the same numbers
/// from `logic.ttf` and the unit tests pin them.
pub const COMMITTED_METRICS: BitmapFontMetrics = BitmapFontMetrics {
    ascent: 3,
    descent: -3,
    cap_height: 7,
    x_height: 6,
    line_height: 13,
    space_x_advance: 7,
    missing_glyph: true,
};

/// The `Fonts.logic` metrics, generated once from the shipped font.
pub fn logic_font_metrics() -> &'static BitmapFontMetrics {
    static METRICS: LazyLock<BitmapFontMetrics> =
        LazyLock::new(|| font_metrics(LOGIC_TTF, LOGIC_FONT_SIZE).unwrap_or(COMMITTED_METRICS));
    &METRICS
}

/// Whether `Fonts.logic` has a drawable glyph for `c`
/// (`FontData.getGlyph(c) != null`): the generated set plus the synthesised
/// space glyph.
///
/// Characters outside the set come back null (the generator is not
/// incremental, so `FreeTypeFontData.getGlyph` never creates them on demand),
/// and `LogicDisplay.processCommands` skips them (`LogicDisplay.java:228-229`).
pub fn has_glyph(c: char) -> bool {
    c == ' ' || GLYPH_CHARS.contains(c)
}

/// `FontData.hasGlyph(c)` (`arc/graphics/g2d/Font.java:844-847`) for
/// `Fonts.logic`.
///
/// Arc returns `true` for every character when a `missingGlyph` is installed,
/// and `'\0'` is part of the generated set with a non-empty `.notdef` glyph,
/// so the `LExecutor` text layout emits a `commandPrint` for every character —
/// including ones the display then skips when drawing.
pub fn layout_has_glyph(_c: char) -> bool {
    logic_font_metrics().missing_glyph
}

/// `Fonts.unicodeToName` (`Fonts.java:121-123`): icon-tag code -> icon name.
///
/// The content-icon table (`icons.properties`, registered through
/// `registerIcon`, `Fonts.java:135-143`) maps the code to its atlas region
/// name; the generated icon-font table (`Iconc.codeToName`, the capitalised
/// css name) is the fallback.
pub fn unicode_to_name(icons: &Iconc, codes: &IconCodes, code: u32) -> Option<String> {
    if let Some(name) = icons.name(code)
        && let Some(region) = icons.texture(name)
        && !region.is_empty()
    {
        return Some(region.to_owned());
    }
    codes
        .glyphs
        .iter()
        .find(|glyph| glyph.code == code)
        .map(|glyph| capitalise(&glyph.css))
}

/// What `Fonts.getLargeIcon(name)` resolves to (`Fonts.java:125-134`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LargeIcon<'a> {
    /// A fontgen icon name: draw the `iconLarge` 48 px glyph
    /// (`Fonts.java:129-132`).
    Glyph(char),
    /// Any other name: `Core.atlas.find(name)` (`Fonts.java:133`).
    Atlas(&'a str),
}

/// `Fonts.getLargeIcon(name)` (`Fonts.java:125-134`) as a pure mapping.
///
/// Arc looks the name up in `Iconc.codes` with `'\uF8D4'` as the default; that
/// code is not a registered icon glyph, so the icon-large glyph lookup fails
/// and the atlas region wins. Fontgen names (capitalised css names) resolve to
/// their icon-font glyph.
pub fn large_icon<'a>(codes: &IconCodes, name: &'a str) -> LargeIcon<'a> {
    codes
        .glyphs
        .iter()
        .find(|glyph| capitalise(&glyph.css) == name)
        .and_then(|glyph| char::from_u32(glyph.code))
        .map_or(LargeIcon::Atlas(name), LargeIcon::Glyph)
}

/// Arc `Strings.capitalize` (`Character.toUpperCase(s.charAt(0)) + rest`).
fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Ports `FreeTypeFontGenerator.generateData`'s metric block
/// (`FreeTypeFontGenerator.java:339-383`) for one font at `pixel_size`.
///
/// Returns `None` when a required table or character is missing. The font is
/// parsed as a plain TrueType file (`head`/`hhea`/`maxp`/`hmtx`/`cmap`/
/// `loca`/`glyf`); no glyph is rasterised.
pub fn font_metrics(ttf: &[u8], pixel_size: u32) -> Option<BitmapFontMetrics> {
    let head = table(ttf, *b"head")?;
    let hhea = table(ttf, *b"hhea")?;
    let maxp = table(ttf, *b"maxp")?;
    let upem = u16_at(ttf, head + 18)?;
    if upem == 0 {
        return None;
    }
    let font = FontView {
        data: ttf,
        upem,
        long_loca: i16_at(ttf, head + 50)? == 1,
        num_glyphs: u16_at(ttf, maxp + 4)?,
        num_h_metrics: u16_at(ttf, hhea + 34)?,
        loca: table(ttf, *b"loca")?,
        glyf: table(ttf, *b"glyf")?,
        hmtx: table(ttf, *b"hmtx")?,
    };
    let ascender = i32::from(i16_at(ttf, hhea + 4)?);
    let descender = i32::from(i16_at(ttf, hhea + 6)?);
    let line_gap = i32::from(i16_at(ttf, hhea + 8)?);

    let raw_ascender = to_int(pixel_26_6(ascender, upem, pixel_size));
    let descent = to_int(pixel_26_6(descender, upem, pixel_size));
    let line_height = to_int(pixel_26_6(
        ascender - descender + line_gap,
        upem,
        pixel_size,
    ));
    // `FontData.xChars`/`capChars` (`arc/graphics/g2d/Font.java:541-543`): the
    // rendered height of the first character of each list that exists.
    let cap_height = first_rendered_height(&font, "MNBCDEFKAGHIJLOPQRSTUVWXYZ", pixel_size)?;
    let x_height = first_rendered_height(&font, "xeaonsrcumvwz", pixel_size)?;
    let space_x_advance = {
        let glyph = font.char_glyph(' ')?;
        to_int(pixel_26_6(
            i32::from(font.advance(glyph)?),
            upem,
            pixel_size,
        ))
    };
    let missing_glyph = font.rendered_height(0, pixel_size).is_some_and(|px| px > 0);

    Some(BitmapFontMetrics {
        ascent: raw_ascender - cap_height,
        descent,
        cap_height,
        x_height,
        line_height,
        space_x_advance,
        missing_glyph,
    })
}

/// The first character in `chars` that has a non-empty glyph, rendered at
/// `pixel_size` (`data.xHeight`/`data.capHeight` loops).
fn first_rendered_height(font: &FontView<'_>, chars: &str, pixel_size: u32) -> Option<i32> {
    for c in chars.chars() {
        if let Some(height) = font
            .char_glyph(c)
            .and_then(|glyph| font.rendered_height(glyph, pixel_size))
            && height > 0
        {
            return Some(height);
        }
    }
    None
}

/// The tables-backed font cursor used while generating metrics.
struct FontView<'a> {
    data: &'a [u8],
    upem: u16,
    long_loca: bool,
    num_glyphs: u16,
    num_h_metrics: u16,
    loca: usize,
    glyf: usize,
    hmtx: usize,
}

impl FontView<'_> {
    /// The rendered pixel height of `glyph` (FreeType's
    /// `getMetrics().getHeight()`).
    fn rendered_height(&self, glyph: u16, pixel_size: u32) -> Option<i32> {
        let height = self.glyph_height(glyph)?;
        Some(to_int(pixel_26_6(height, self.upem, pixel_size)))
    }

    /// `hmtx` advance width of `glyph` (fonts share the last advance past
    /// `numberOfHMetrics`).
    fn advance(&self, glyph: u16) -> Option<u16> {
        let metrics = self.num_h_metrics.max(1);
        let index = glyph.min(metrics - 1);
        u16_at(self.data, self.hmtx + 4 * usize::from(index))
    }

    /// `cmap` format-4 lookup (Windows Unicode BMP preferred).
    fn char_glyph(&self, c: char) -> Option<u16> {
        let cmap = table(self.data, *b"cmap")?;
        let count = u16_at(self.data, cmap + 2)?;
        let mut fallback = None;
        for index in 0..count {
            let record = cmap + 4 + 8 * usize::from(index);
            let platform = u16_at(self.data, record)?;
            let encoding = u16_at(self.data, record + 2)?;
            let offset = u32_at(self.data, record + 4)? as usize;
            let subtable = cmap.checked_add(offset)?;
            if u16_at(self.data, subtable)? != 4 {
                continue;
            }
            if platform == 3 && encoding == 1 {
                return cmap_format_4(self.data, subtable, c);
            }
            fallback.get_or_insert(subtable);
        }
        fallback.and_then(|subtable| cmap_format_4(self.data, subtable, c))
    }

    /// `glyf` bounding-box height in font units (`None` for empty glyphs).
    fn glyph_height(&self, glyph: u16) -> Option<i32> {
        if glyph >= self.num_glyphs {
            return None;
        }
        let index = usize::from(glyph);
        let start = self.loca_offset(index)?;
        if self.loca_offset(index + 1)? == start {
            return None;
        }
        let origin = self.glyf.checked_add(start)?;
        if i16_at(self.data, origin)? == 0 {
            return None;
        }
        let y_min = i32::from(i16_at(self.data, origin + 4)?);
        let y_max = i32::from(i16_at(self.data, origin + 6)?);
        Some(y_max - y_min)
    }

    fn loca_offset(&self, glyph: usize) -> Option<usize> {
        if self.long_loca {
            Some(u32_at(self.data, self.loca.checked_add(glyph * 4)?)? as usize)
        } else {
            Some(u16_at(self.data, self.loca.checked_add(glyph * 2)?)? as usize * 2)
        }
    }
}

/// Offset of `tag`'s table data.
fn table(data: &[u8], tag: [u8; 4]) -> Option<usize> {
    let count = u16_at(data, 4)?;
    for index in 0..count {
        let record = 12 + 16 * usize::from(index);
        if data.get(record..record + 4)? == tag.as_slice() {
            return Some(u32_at(data, record + 8)? as usize);
        }
    }
    None
}

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn i16_at(data: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_be_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

/// `cmap` format 4 segment lookup.
fn cmap_format_4(data: &[u8], subtable: usize, c: char) -> Option<u16> {
    let c = u32::from(c);
    if c > 0xffff {
        return None;
    }
    let c = c as u16;
    let seg_x2 = u16_at(data, subtable + 6)?;
    let end_codes = subtable + 14;
    let start_codes = end_codes + usize::from(seg_x2) + 2;
    let deltas = start_codes + usize::from(seg_x2);
    let range_offsets = deltas + usize::from(seg_x2);
    for index in 0..u32::from(seg_x2) / 2 {
        let stride = index as usize * 2;
        let end = u16_at(data, end_codes + stride)?;
        if c > end {
            continue;
        }
        let start = u16_at(data, start_codes + stride)?;
        if c < start {
            return None;
        }
        let delta = i16_at(data, deltas + stride)?;
        let range_offset = u16_at(data, range_offsets + stride)?;
        if range_offset == 0 {
            return Some(c.wrapping_add_signed(delta));
        }
        let offset =
            range_offsets + stride + usize::from(range_offset) + 2 * usize::from(c - start);
        let glyph = u16_at(data, offset)?;
        if glyph == 0 {
            return Some(0);
        }
        return Some(glyph.wrapping_add_signed(delta));
    }
    None
}

/// FreeType 26.6 fixed-point rounding:
/// `FT_MulFix(units, FT_DivFix(ppem << 6, upem))`.
fn pixel_26_6(units: i32, upem: u16, pixel_size: u32) -> i32 {
    let size_26_6 = i64::from(pixel_size) << 6;
    let upem = i64::from(upem);
    let scale = ((size_26_6 << 16) + upem / 2) / upem;
    ((i64::from(units) * scale + 0x8000) >> 16) as i32
}

/// Arc `FreeType.toInt`: round a 26.6 value **up** to whole pixels
/// (`extensions/freetype/src/arc/freetype/FreeType.java:123-125`).
fn to_int(value: i32) -> i32 {
    (value.saturating_add(63) & !63) >> 6
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICONS: &str = "\
        57344=router|block-router-ui
        57345=conveyor|block-conveyor-ui
        57346=untextured|
";

    fn iconc() -> Iconc {
        Iconc::from_properties(ICONS)
    }

    fn icon_codes() -> IconCodes {
        IconCodes {
            format: 1,
            glyphs: vec![
                crate::assets::icons::IconGlyph {
                    name: "Add".to_owned(),
                    code: 0xe7f3,
                    css: "add".to_owned(),
                },
                crate::assets::icons::IconGlyph {
                    name: "ArrowNote".to_owned(),
                    code: 0xe844,
                    css: "arrow-note".to_owned(),
                },
            ],
        }
    }

    /// `Fonts.logic` is generated from the shipped `logic.ttf`; the metric
    /// block (`FreeTypeFontGenerator.java:339-383`) yields the committed
    /// values.
    #[test]
    fn logic_font_metrics_match_the_shipped_font() {
        let metrics = font_metrics(LOGIC_TTF, LOGIC_FONT_SIZE).expect("logic.ttf parses");
        assert_eq!(metrics, COMMITTED_METRICS);
        assert_eq!(logic_font_metrics(), &COMMITTED_METRICS);
        // `spaceXadvance`/`lineHeight` are what `LExecutor.java:953-955`
        // casts to ints for the print layout.
        assert_eq!(metrics.space_x_advance, 7);
        assert_eq!(metrics.line_height, 13);
        // FreeType ascender 10 (608/64 -> toInt) minus `capHeight` 7 (`M`
        // renders 7 px): `FontData.ascent`.
        assert_eq!(metrics.ascent, 3);
        assert_eq!(metrics.cap_height, 7);
        assert_eq!(metrics.descent, -3);
        assert_eq!(metrics.x_height, 6);
        // `'\0'` maps to the non-empty `.notdef` box, so Arc installs a
        // `missingGlyph` and `hasGlyph` is always true.
        assert!(metrics.missing_glyph);
        assert!(layout_has_glyph('é'));
        assert!(layout_has_glyph('\u{1F600}'));
    }

    /// The generated set is `Fonts.java:92` plus the synthesised space.
    #[test]
    fn glyph_charset_matches_java() {
        assert_eq!(GLYPH_CHARS.chars().count(), 96);
        assert!(GLYPH_CHARS.starts_with('\0'));
        for c in "AZaz09\"!`?'.,;:()[]{}<>|/@\\^$€-%+=#_&~*".chars() {
            assert!(has_glyph(c), "generated glyph {c:?}");
        }
        // `Fonts.logic`'s `characters` has no space; the generator adds one.
        assert!(has_glyph(' '));
        assert!(!has_glyph('é'));
        assert!(!has_glyph('\u{1F600}'));
        assert!(!has_glyph('\n'));
    }

    /// `Fonts.unicodeToName` (`Fonts.java:121-123`): content codes resolve to
    /// their atlas region, fontgen codes to the capitalised css name.
    #[test]
    fn unicode_to_name_matches_java() {
        let icons = iconc();
        let codes = icon_codes();
        assert_eq!(
            unicode_to_name(&icons, &codes, 57344).as_deref(),
            Some("block-router-ui")
        );
        // The icon table is checked before the fontgen fallback.
        assert_eq!(
            unicode_to_name(&icons, &codes, 0xe7f3).as_deref(),
            Some("Add")
        );
        assert_eq!(
            unicode_to_name(&icons, &codes, 0xe844).as_deref(),
            Some("Arrow-note")
        );
        assert_eq!(unicode_to_name(&icons, &codes, 0), None);
        assert_eq!(unicode_to_name(&icons, &codes, 0xffff), None);
    }

    /// `Fonts.getLargeIcon` (`Fonts.java:125-134`) resolves fontgen names to
    /// `iconLarge` glyphs and everything else to `Core.atlas.find`.
    #[test]
    fn large_icon_matches_java() {
        let codes = icon_codes();
        assert_eq!(large_icon(&codes, "Add"), LargeIcon::Glyph('\u{e7f3}'));
        // `Iconc.codes` is keyed by the capitalised css name, so the fontello
        // name is not a fontgen key.
        assert_eq!(
            large_icon(&codes, "Arrow-note"),
            LargeIcon::Glyph('\u{e844}')
        );
        assert_eq!(
            large_icon(&codes, "ArrowNote"),
            LargeIcon::Atlas("ArrowNote")
        );
        // Content region names are not fontgen names.
        assert_eq!(
            large_icon(&codes, "block-router-ui"),
            LargeIcon::Atlas("block-router-ui")
        );
    }

    /// Malformed input degrades to `None`, never panics.
    #[test]
    fn malformed_fonts_are_rejected() {
        assert!(font_metrics(&[], LOGIC_FONT_SIZE).is_none());
        assert!(font_metrics(&[0, 1, 0, 0, 0, 0], LOGIC_FONT_SIZE).is_none());
        // Truncated real font: no tables resolve.
        assert!(font_metrics(&LOGIC_TTF[..64], LOGIC_FONT_SIZE).is_none());
    }
}
