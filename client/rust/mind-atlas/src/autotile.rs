// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/ImageTileGenerator.java` (itself a Java port of
// https://github.com/GglLfr/tile-gen — embedded 12x4 layout image).

//! 4x4-tile autotile image -> 47 slices (`ImageTileGenerator`, plan 03 M2).
//!
//! The embedded layout PNG (384x128, sha256-pinned in the tests) maps every
//! output slice pixel back to one of the 16 source cells.

use std::collections::HashMap;

use crate::error::{AtlasError, Result};
use crate::pixmaps::Pixmap;
use crate::png_io;

const LAYOUT_WIDTH: usize = 384;
const LAYOUT_HEIGHT: usize = 128;

const LAYOUT_BASE64: &str = concat!(
    "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",
);

/// Decodes the embedded layout image (lazy; callers cache via `OnceLock`).
pub fn layout() -> Result<Pixmap> {
    let bytes = decode_base64(LAYOUT_BASE64)?;
    png_io::read_png(&bytes)
}

/// The raw (encoded) layout bytes, for the sha256 pin test.
pub fn layout_encoded() -> Result<Vec<u8>> {
    decode_base64(LAYOUT_BASE64)
}

/// Generates the 47 autotile slices for `image` (must be square, sides
/// divisible by 4). Returns each slice in order `0..47`.
pub fn generate_slices(image: &Pixmap, name: &str) -> Result<Vec<Pixmap>> {
    let (width, height) = (image.width, image.height);
    if width % 4 != 0 || height % 4 != 0 {
        return Err(AtlasError::Invalid(format!(
            "Image dimensions are not divisible by 4: {width}x{height}"
        )));
    }
    if width != height {
        return Err(AtlasError::Invalid(format!(
            "Image is not square: {width}x{height}"
        )));
    }
    let layout = layout()?;
    let cell_size = width / 4;

    // color -> packed source cell offset.
    let mut color_to_position: HashMap<u32, (usize, usize)> = HashMap::new();
    for x in 0..4usize {
        for y in 0..4usize {
            color_to_position.insert(
                layout.get(
                    (x * LAYOUT_WIDTH / 12) as i32,
                    (y * LAYOUT_HEIGHT / 4) as i32,
                ),
                (x * width / 4, y * height / 4),
            );
        }
    }

    let out_width = width / 4 * 12;
    let mut out = Pixmap::new(out_width, height);
    for cx in 0..12usize {
        for cy in 0..4usize {
            for rx in 0..cell_size {
                for ry in 0..cell_size {
                    let key = layout.get(
                        ((cx * cell_size + rx) * LAYOUT_WIDTH / (width * 3)) as i32,
                        ((cy * cell_size + ry) * LAYOUT_HEIGHT / height) as i32,
                    );
                    if let Some(&(sx, sy)) = color_to_position.get(&key) {
                        let value = image.get_raw(sx + rx, sy + ry);
                        out.set_raw(cx * cell_size + rx, cy * cell_size + ry, value);
                    }
                }
            }
        }
    }

    let mut slices = Vec::with_capacity(47);
    for i in 0..47usize {
        let cx = i % 12;
        let cy = i / 12;
        slices.push(out.crop(
            (cx * cell_size) as i32,
            (cy * cell_size) as i32,
            cell_size,
            cell_size,
        ));
    }
    let _ = name;
    Ok(slices)
}

fn decode_base64(text: &str) -> Result<Vec<u8>> {
    let mut table = [0xffu8; 256];
    for (i, byte) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        table[*byte as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        if byte == b'=' || byte.is_ascii_whitespace() {
            continue;
        }
        let value = table[byte as usize];
        if value == 0xff {
            return Err(AtlasError::Invalid(format!(
                "base64: invalid byte {byte:#x}"
            )));
        }
        acc = (acc << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixmaps::rgba8888;

    #[test]
    fn layout_hash_and_shape() {
        let encoded = layout_encoded().unwrap();
        let hash = crate::manifest::sha256_hex(&encoded);
        assert_eq!(
            hash, "e8fcd718800e21db53107c16bbf160b8decd062ad8c6a7f5c334cc6d665c2eba",
            "embedded layout drifted"
        );
        let layout = layout().unwrap();
        assert_eq!((layout.width, layout.height), (LAYOUT_WIDTH, LAYOUT_HEIGHT));
    }

    #[test]
    fn slice_layout_and_hash() {
        // 4x4 cells of 8px, each cell a unique solid color.
        let mut image = Pixmap::new(32, 32);
        let mut cell_colors = std::collections::BTreeSet::new();
        for cy in 0..4 {
            for cx in 0..4 {
                let color = rgba8888((cx * 60) as u32, (cy * 60) as u32, 200, 255);
                cell_colors.insert(color);
                for y in 0..8 {
                    for x in 0..8 {
                        image.set_raw(cx * 8 + x, cy * 8 + y, color);
                    }
                }
            }
        }
        let slices = generate_slices(&image, "test").unwrap();
        assert_eq!(slices.len(), 47);
        for (i, slice) in slices.iter().enumerate() {
            assert_eq!((slice.width, slice.height), (8, 8), "slice {i}");
        }
        // Every output pixel comes from one of the 16 source cells (the
        // stencil combines cell pieces within a slice), and the mapping
        // actually permutes cells (more than one color appears overall).
        let mut used = std::collections::BTreeSet::new();
        for slice in &slices {
            for y in 0..8 {
                for x in 0..8 {
                    let value = slice.get_raw(x, y);
                    assert!(
                        cell_colors.contains(&value),
                        "pixel {value:#x} not from any source cell"
                    );
                    used.insert(value);
                }
            }
        }
        assert!(used.len() > 2, "layout did not permute cells: {used:?}");
        // Deterministic across runs.
        let again = generate_slices(&image, "test").unwrap();
        assert_eq!(slices, again);
    }

    #[test]
    fn rejects_non_square_and_indivisible() {
        let image = Pixmap::new(10, 12);
        assert!(generate_slices(&image, "x").is_err());
        let image = Pixmap::new(10, 10);
        assert!(generate_slices(&image, "x").is_err());
    }
}
