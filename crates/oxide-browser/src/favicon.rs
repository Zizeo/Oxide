//! Favicon decoding.
//!
//! # Scope
//!
//! Browsers find a site's icon through the page's `<link rel="icon">`, which
//! needs the parsed HTML. Oxide has no DOM for a page hosted by the native
//! `WebView`, so it does what every browser also falls back to: fetch
//! `/favicon.ico` and decode it. That covers the overwhelming majority of
//! sites, and a site that only ships a non-PNG icon simply keeps the scheme
//! glyph the strip already drew.
//!
//! # Formats
//!
//! Only PNG is decoded. Two containers carry it:
//!
//! * a bare PNG file served under the `.ico` name (common on modern sites);
//! * an ICO container whose entries embed PNG data (Vista and later).
//!
//! Classic BMP-in-ICO icons are not decoded: they are 32-bit DIBs without
//! alpha, which look worse than the glyph they would replace.

use egui::ColorImage;

/// Largest edge kept in the decoded texture, in pixels.
///
/// The strip paints icons at 12–16 physical points, so anything above this is
/// downloaded detail the GPU would throw away.
const MAX_EDGE: usize = 32;

/// PNG magic number, as specified in RFC 2083.
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Decodes an icon fetched from a site, or `None` when the bytes are not an
/// icon this module understands.
pub fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let png = if bytes.starts_with(&PNG_MAGIC) {
        bytes
    } else if bytes.starts_with(&[0, 0, 1, 0]) {
        largest_png_entry(bytes)?
    } else {
        return None;
    };

    let image = decode_png(png)?;
    Some(downscale(&image, MAX_EDGE))
}

/// Returns the embedded PNG of the biggest entry in an ICO container.
///
/// Bigger entries are preferred because they downscale better; entries that
/// are not PNG (classic DIB) are skipped.
fn largest_png_entry(bytes: &[u8]) -> Option<&[u8]> {
    let count = read_u16(bytes, 4)? as usize;
    // 6 byte header, then 16 bytes per directory entry.
    if bytes.len() < 6 + count * 16 {
        return None;
    }

    let mut best: Option<(u32, &[u8])> = None;
    for index in 0..count {
        let entry = 6 + index * 16;
        // A stored dimension of 0 means 256.
        let width = match bytes[entry] {
            0 => 256,
            other => other as u32,
        };
        let height = match bytes[entry + 1] {
            0 => 256,
            other => other as u32,
        };
        let size = read_u32(bytes, entry + 8)? as usize;
        let offset = read_u32(bytes, entry + 12)? as usize;
        let end = offset.checked_add(size)?;
        let data = bytes.get(offset..end)?;
        if !data.starts_with(&PNG_MAGIC) {
            continue;
        }
        let area = width * height;
        if best.is_none_or(|(best_area, _)| area > best_area) {
            best = Some((area, data));
        }
    }
    best.map(|(_, data)| data)
}

/// Decodes a PNG into straight RGBA8.
fn decode_png(bytes: &[u8]) -> Option<ColorImage> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;

    let width = info.width as usize;
    let height = info.height as usize;
    let data = &buffer[..info.buffer_size()];
    if width == 0 || height == 0 {
        return None;
    }

    let mut pixels = Vec::with_capacity(width * height);
    match info.color_type {
        png::ColorType::Rgba => {
            if data.len() < width * height * 4 {
                return None;
            }
            for index in 0..width * height {
                let base = index * 4;
                pixels.push(egui::Color32::from_rgba_unmultiplied(
                    data[base],
                    data[base + 1],
                    data[base + 2],
                    data[base + 3],
                ));
            }
        }
        png::ColorType::Rgb => {
            if data.len() < width * height * 3 {
                return None;
            }
            for index in 0..width * height {
                let base = index * 3;
                pixels.push(egui::Color32::from_rgb(
                    data[base],
                    data[base + 1],
                    data[base + 2],
                ));
            }
        }
        // `normalize_to_color8` leaves greyscale alone; widen it so every
        // icon reaches the strip as the same pixel format.
        png::ColorType::Grayscale => {
            if data.len() < width * height {
                return None;
            }
            for &value in &data[..width * height] {
                pixels.push(egui::Color32::from_gray(value));
            }
        }
        png::ColorType::GrayscaleAlpha => {
            if data.len() < width * height * 2 {
                return None;
            }
            for index in 0..width * height {
                let base = index * 2;
                pixels.push(egui::Color32::from_rgba_unmultiplied(
                    data[base],
                    data[base],
                    data[base],
                    data[base + 1],
                ));
            }
        }
        png::ColorType::Indexed => return None,
    }

    Some(ColorImage {
        size: [width, height],
        pixels,
    })
}

/// Box-filters `image` down so no edge exceeds `max_edge`.
fn downscale(image: &ColorImage, max_edge: usize) -> ColorImage {
    let [width, height] = image.size;
    if width <= max_edge && height <= max_edge {
        return image.clone();
    }

    let scale = (width.max(height) as f32) / max_edge as f32;
    let out_width = ((width as f32) / scale).round().max(1.0) as usize;
    let out_height = ((height as f32) / scale).round().max(1.0) as usize;

    // Average each source block instead of point-sampling: icon edges are
    // mostly high-frequency detail and nearest-neighbour turns them to noise.
    let mut pixels = Vec::with_capacity(out_width * out_height);
    for y in 0..out_height {
        let y0 = y * height / out_height;
        let y1 = (((y + 1) * height) / out_height).max(y0 + 1);
        for x in 0..out_width {
            let x0 = x * width / out_width;
            let x1 = (((x + 1) * width) / out_width).max(x0 + 1);

            let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for sy in y0..y1.min(height) {
                for sx in x0..x1.min(width) {
                    let px = image.pixels[sy * width + sx];
                    r += px.r() as u32;
                    g += px.g() as u32;
                    b += px.b() as u32;
                    a += px.a() as u32;
                    n += 1;
                }
            }
            let n = n.max(1);
            pixels.push(egui::Color32::from_rgba_premultiplied(
                (r / n) as u8,
                (g / n) as u8,
                (b / n) as u8,
                (a / n) as u8,
            ));
        }
    }
    ColorImage {
        size: [out_width, out_height],
        pixels,
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes a `width`×`height` RGBA PNG with the given pixel colour.
    fn png_bytes(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            let mut data = Vec::with_capacity((width * height * 4) as usize);
            for _ in 0..width * height {
                data.extend_from_slice(&color);
            }
            writer.write_image_data(&data).unwrap();
        }
        out
    }

    /// Wraps `png` in a single-entry ICO container.
    fn ico_with_png(png: &[u8], width: u8) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&[0, 0, 1, 0, 1, 0]); // header, one entry
        out.push(width); // width (0 would mean 256)
        out.push(width); // height
        out.push(0); // palette colours
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&22u32.to_le_bytes()); // offset: 6 + 16
        out.extend_from_slice(png);
        out
    }

    #[test]
    fn a_plain_png_is_decoded() {
        let image = decode(&png_bytes(16, 16, [10, 20, 30, 255])).expect("png decodes");
        assert_eq!(image.size, [16, 16]);
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(10, 20, 30));
    }

    #[test]
    fn a_png_inside_an_ico_container_is_decoded() {
        let ico = ico_with_png(&png_bytes(32, 32, [200, 100, 50, 255]), 32);
        let image = decode(&ico).expect("ico decodes");
        assert_eq!(image.size, [32, 32]);
        assert_eq!(image.pixels[0].r(), 200);
    }

    #[test]
    fn a_classic_bmp_icon_is_rejected() {
        // Header only: the entry points at BMP data, which is not supported.
        let mut ico = vec![0, 0, 1, 0, 1, 0];
        ico.extend_from_slice(&[16, 16, 0, 0, 1, 0, 32, 0]);
        ico.extend_from_slice(&16u32.to_le_bytes());
        ico.extend_from_slice(&22u32.to_le_bytes());
        ico.extend_from_slice(&[0u8; 16]);
        assert!(decode(&ico).is_none());
    }

    #[test]
    fn garbage_is_rejected_without_panicking() {
        assert!(decode(b"not an icon").is_none());
        assert!(decode(&[]).is_none());
        // Truncated ICO header claiming a huge entry count.
        assert!(decode(&[0, 0, 1, 0, 0xFF, 0xFF]).is_none());
    }

    #[test]
    fn oversized_icons_are_scaled_down() {
        let image = decode(&png_bytes(128, 128, [1, 2, 3, 255])).expect("png decodes");
        assert_eq!(image.size, [32, 32]);
        // The box filter must keep the colour, not smear it.
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(1, 2, 3));
    }
}
