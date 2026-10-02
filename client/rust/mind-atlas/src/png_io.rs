// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic PNG IO (plan 03 §3.9: fixed encoder settings, pinned crate
//! version, no timestamps/ancillary chunks).
//!
//! Decode accepts any PNG color type and expands to RGBA8. Encode writes
//! RGBA8 with default deflate settings and no ancillary chunks, so identical
//! pixels produce identical bytes for a pinned `png` crate version (R2).

use std::io::Cursor;

use crate::error::{AtlasError, Result};
use crate::pixmaps::Pixmap;

/// Decodes a PNG into an RGBA8 pixmap. Any color type is accepted; RGB
/// sources gain an opaque alpha channel (Arc `Pixmap` is always RGBA8888).
pub fn read_png(bytes: &[u8]) -> Result<Pixmap> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer)?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(AtlasError::Png(format!(
            "unsupported png bit depth after expansion: {:?}",
            info.bit_depth
        )));
    }
    buffer.truncate(info.buffer_size());
    let (width, height) = (info.width as usize, info.height as usize);
    match info.color_type {
        png::ColorType::Rgba => Pixmap::from_raw(width, height, buffer),
        png::ColorType::Rgb => {
            let mut rgba = vec![0u8; width * height * 4];
            let (dst_chunks, _) = rgba.as_chunks_mut::<4>();
            let (src_chunks, _) = buffer.as_chunks::<3>();
            for (dst, src) in dst_chunks.iter_mut().zip(src_chunks.iter()) {
                dst[0] = src[0];
                dst[1] = src[1];
                dst[2] = src[2];
                dst[3] = 255;
            }
            Pixmap::from_raw(width, height, rgba)
        }
        other => Err(AtlasError::Png(format!(
            "unsupported png after expansion: {other:?}"
        ))),
    }
}

/// Encodes an RGBA8 pixmap deterministically (no ancillary chunks).
pub fn write_png(pixmap: &Pixmap) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, pixmap.width as u32, pixmap.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&pixmap.pixels)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixmaps::{Pixmap, rgba8888};

    #[test]
    fn png_round_trip_is_byte_deterministic() {
        let mut pix = Pixmap::new(16, 8);
        for y in 0..8 {
            for x in 0..16 {
                pix.set_raw(x, y, rgba8888((x * 16) as u32, (y * 32) as u32, 7, 255));
            }
        }
        let first = write_png(&pix).unwrap();
        let second = write_png(&pix).unwrap();
        assert_eq!(first, second);
        let decoded = read_png(&first).unwrap();
        assert_eq!(decoded, pix);
    }
}
