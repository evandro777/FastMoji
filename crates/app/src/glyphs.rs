//! Emoji drawn as images. Slint's software renderer cannot draw the bitmap (CBDT) Noto font, so
//! each sequence is shaped (ZWJ, flags and skin tones are GSUB ligatures), its PNG is read from
//! the font and downscaled. Only what is on screen is ever decoded.
//!
//! Images are made at the exact physical size they are drawn at: Slint scales images bilinearly,
//! which skips source pixels when shrinking by more than 2x and leaves jagged, uneven edges.
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const FONT: &[u8] = include_bytes!("../assets/NotoColorEmoji.ttf");
/// Two generations of this size, so at most 600 images are kept.
const CACHE_LIMIT: usize = 300;

pub struct Glyphs {
    face: ttf_parser::Face<'static>,
    shaper: rustybuzz::Face<'static>,
    scale: Cell<f32>,
    cache: RefCell<Cache>,
}

/// Emoji and physical size in pixels.
type Key = (&'static str, u32);

/// When `recent` is full it becomes `old` and the previous `old` is dropped. Anything still being
/// drawn is found in `old` and promoted, so a refill never re-decodes what is on screen.
#[derive(Default)]
struct Cache {
    recent: HashMap<Key, Image>,
    old: HashMap<Key, Image>,
}

impl Glyphs {
    pub fn new() -> Self {
        Self {
            face: ttf_parser::Face::parse(FONT, 0).expect("bundled emoji font"),
            shaper: rustybuzz::Face::from_slice(FONT, 0).expect("bundled emoji font"),
            scale: Cell::new(1.0),
            cache: RefCell::default(),
        }
    }

    /// Returns true when the window scale factor changed; images made for the old one are dropped.
    pub fn set_scale(&self, scale: f32) -> bool {
        if scale <= 0.0 || scale == self.scale.get() {
            return false;
        }
        self.scale.set(scale);
        *self.cache.borrow_mut() = Cache::default();
        true
    }

    /// `size` is the logical size the image is shown at.
    pub fn image(&self, emoji: &'static str, size: f32) -> Image {
        let key = (emoji, (size * self.scale.get()).round().max(1.0) as u32);
        let mut cache = self.cache.borrow_mut();
        let img = match cache.recent.get(&key).cloned().or_else(|| cache.old.remove(&key)) {
            Some(img) => img,
            None => self.render(emoji, key.1 as usize).unwrap_or_default(),
        };
        if cache.recent.len() >= CACHE_LIMIT && !cache.recent.contains_key(&key) {
            cache.old = std::mem::take(&mut cache.recent);
        }
        cache.recent.insert(key, img.clone());
        img
    }

    fn render(&self, emoji: &str, px: usize) -> Option<Image> {
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

        // Premultiplied, so transparent pixels do not bleed their (meaningless) colour into edges.
        let src: Vec<[f32; 4]> = pixels[..w * h * n]
            .chunks_exact(n)
            .map(|p| {
                let (r, g, b) = if n >= 3 { (p[0], p[1], p[2]) } else { (p[0], p[0], p[0]) };
                let a = if n % 2 == 0 { p[n - 1] } else { 255 } as f32 / 255.0;
                [r as f32 * a, g as f32 * a, b as f32 * a, a]
            })
            .collect();

        // Fit the (136x128, not square) strike into the square without stretching it, and average
        // the source area under each output pixel: a separable box filter with fractional edges.
        let scale = px as f32 / w.max(h) as f32;
        let xs = weights(w, px, scale);
        let ys = weights(h, px, scale);
        let mut rows = vec![[0.0f32; 4]; h * px];
        for y in 0..h {
            for (x, taps) in xs.iter().enumerate() {
                rows[y * px + x] = sum(taps.iter().map(|&(i, k)| (src[y * w + i], k)));
            }
        }
        let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(px as u32, px as u32);
        let dst = out.make_mut_slice();
        for (y, taps) in ys.iter().enumerate() {
            for x in 0..px {
                let [r, g, b, a] = sum(taps.iter().map(|&(i, k)| (rows[i * px + x], k)));
                if a > 0.0 {
                    let c = |v: f32| (v / a).clamp(0.0, 255.0).round() as u8;
                    dst[y * px + x] = Rgba8Pixel {
                        r: c(r),
                        g: c(g),
                        b: c(b),
                        a: (a * 255.0).clamp(0.0, 255.0).round() as u8,
                    };
                }
            }
        }
        Some(Image::from_rgba8(out))
    }
}

/// For each of the `out` pixels, the source pixels under it and their weights. They sum to 1,
/// or less where the pixel reaches past the centred source (transparent there).
fn weights(len: usize, out: usize, scale: f32) -> Vec<Vec<(usize, f32)>> {
    let offset = (out as f32 - len as f32 * scale) / 2.0;
    (0..out)
        .map(|o| {
            let start = ((o as f32 - offset) / scale).max(0.0);
            let end = ((o as f32 + 1.0 - offset) / scale).min(len as f32);
            let mut taps = Vec::new();
            let mut i = start.floor() as usize;
            while (i as f32) < end {
                let covered = end.min(i as f32 + 1.0) - start.max(i as f32);
                if covered > 0.0 {
                    taps.push((i, covered * scale));
                }
                i += 1;
            }
            taps
        })
        .collect()
}

fn sum(taps: impl Iterator<Item = ([f32; 4], f32)>) -> [f32; 4] {
    taps.fold([0.0; 4], |acc, (p, k)| std::array::from_fn(|c| acc[c] + p[c] * k))
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

    /// Every source pixel must be counted exactly once in total, or the image gains or loses weight.
    #[test]
    fn weights_cover_the_source_once() {
        for (len, out) in [(136, 20), (128, 28), (136, 35), (128, 72), (136, 144)] {
            let scale = out as f32 / 136.0;
            let mut seen = vec![0.0f32; len];
            for taps in weights(len, out, scale) {
                assert!(taps.iter().map(|t| t.1).sum::<f32>() <= 1.0 + 1e-4);
                for (i, k) in taps {
                    seen[i] += k / scale;
                }
            }
            assert!(seen.iter().all(|s| (s - 1.0).abs() < 1e-3), "{len}->{out}: {seen:?}");
        }
    }

    #[test]
    fn images_follow_size_and_scale() {
        let glyphs = Glyphs::new();
        let side = |img: Image| (img.size().width, img.size().height);
        assert_eq!(side(glyphs.image("😀", 28.0)), (28, 28));
        assert!(glyphs.set_scale(1.5));
        assert!(!glyphs.set_scale(1.5));
        assert_eq!(side(glyphs.image("😀", 28.0)), (42, 42));
    }
}
