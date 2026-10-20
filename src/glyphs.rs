//! Emoji drawn as images. Slint's software renderer cannot draw the bitmap (CBDT) Noto font, so
//! each sequence is shaped (ZWJ, flags and skin tones are GSUB ligatures), its PNG is read from
//! the font and downscaled. Only what is on screen is ever decoded.
use std::{cell::RefCell, collections::HashMap};

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const FONT: &[u8] = include_bytes!("../assets/NotoColorEmoji.ttf");
/// Source strike is 136x128; 72 px stays crisp at 2x scaling for the largest (36 px) setting.
const ICON: usize = 72;
/// Two generations of this size, so at most 600 images are kept.
const CACHE_LIMIT: usize = 300;

pub struct Glyphs {
    face: ttf_parser::Face<'static>,
    shaper: rustybuzz::Face<'static>,
    cache: RefCell<Cache>,
}

/// When `recent` is full it becomes `old` and the previous `old` is dropped. Anything still being
/// drawn is found in `old` and promoted, so a refill never re-decodes what is on screen.
#[derive(Default)]
struct Cache {
    recent: HashMap<&'static str, Image>,
    old: HashMap<&'static str, Image>,
}

impl Glyphs {
    pub fn new() -> Self {
        Self {
            face: ttf_parser::Face::parse(FONT, 0).expect("bundled emoji font"),
            shaper: rustybuzz::Face::from_slice(FONT, 0).expect("bundled emoji font"),
            cache: RefCell::default(),
        }
    }

    pub fn image(&self, emoji: &'static str) -> Image {
        let mut cache = self.cache.borrow_mut();
        let img = match cache.recent.get(emoji).cloned().or_else(|| cache.old.remove(emoji)) {
            Some(img) => img,
            None => self.render(emoji).unwrap_or_default(),
        };
        if cache.recent.len() >= CACHE_LIMIT && !cache.recent.contains_key(emoji) {
            cache.old = std::mem::take(&mut cache.recent);
        }
        cache.recent.insert(emoji, img.clone());
        img
    }

    fn render(&self, emoji: &str) -> Option<Image> {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(emoji);
        let shaped = rustybuzz::shape(&self.shaper, &[], buffer);
        let glyph = ttf_parser::GlyphId(shaped.glyph_infos().first()?.glyph_id as u16);
        let raster = self.face.glyph_raster_image(glyph, 136)?;

        let mut decoder = png::Decoder::new(std::io::Cursor::new(raster.data));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().ok()?;
        let mut pixels = vec![0; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut pixels).ok()?;
        let (w, h, n) = (info.width as usize, info.height as usize, info.color_type.samples());

        // Alpha-weighted box filter down to ICON x ICON.
        let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(ICON as u32, ICON as u32);
        let dst = out.make_mut_slice();
        for y in 0..ICON {
            let (y0, y1) = (y * h / ICON, ((y + 1) * h / ICON).max(y * h / ICON + 1).min(h));
            for x in 0..ICON {
                let (x0, x1) = (x * w / ICON, ((x + 1) * w / ICON).max(x * w / ICON + 1).min(w));
                let (mut r, mut g, mut b, mut a, mut count) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let p = &pixels[(yy * w + xx) * n..];
                        let alpha = if n == 4 { p[3] as u32 } else { 255 };
                        r += p[0] as u32 * alpha;
                        g += p[1] as u32 * alpha;
                        b += p[2] as u32 * alpha;
                        a += alpha;
                        count += 1;
                    }
                }
                if a > 0 && count > 0 {
                    dst[y * ICON + x] = Rgba8Pixel {
                        r: (r / a) as u8,
                        g: (g / a) as u8,
                        b: (b / a) as u8,
                        a: (a / count) as u8,
                    };
                }
            }
        }
        Some(Image::from_rgba8(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emojis::EMOJIS;

    /// A sequence the font cannot join shapes to several glyphs (only the first would be drawn),
    /// and a missing one has no image at all. Catches a CSV/font mismatch after a data refresh.
    #[test]
    fn font_draws_every_emoji_as_one_glyph() {
        let glyphs = Glyphs::new();
        let mut bad = Vec::new();
        for e in EMOJIS.iter() {
            for ch in std::iter::once(e.ch).chain(e.tones.iter().copied().filter(|t| !t.is_empty())) {
                let mut buffer = rustybuzz::UnicodeBuffer::new();
                buffer.push_str(ch);
                let shaped = rustybuzz::shape(&glyphs.shaper, &[], buffer);
                let infos = shaped.glyph_infos();
                let drawn = infos.len() == 1
                    && glyphs
                        .face
                        .glyph_raster_image(ttf_parser::GlyphId(infos[0].glyph_id as u16), 136)
                        .is_some();
                if !drawn {
                    bad.push(format!("{ch} ({})", e.name));
                }
            }
        }
        assert!(bad.is_empty(), "{} emoji do not draw: {bad:?}", bad.len());
    }
}
