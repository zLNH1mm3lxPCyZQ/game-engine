use std::collections::HashMap;

use gfx::glam::{Vec2, Vec4};
use gfx::{GpuContext, Texture};

use crate::{Sprite, SpriteRenderer, SpriteTextureId};

const ATLAS_SIZE: u32 = 1024;
/// Empty pixels between glyphs, so neighbors never bleed into each other.
const PADDING: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("invalid font data: {0}")]
    Invalid(&'static str),
}

/// How a piece of text looks.
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    /// Height in pixels.
    pub size: f32,
    pub color: Vec4,
    pub layer: i32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            size: 24.0,
            color: Vec4::ONE,
            layer: 10,
        }
    }
}

/// A glyph's place in the atlas and how to position it.
#[derive(Clone, Copy, Debug)]
struct Glyph {
    uv_min: Vec2,
    uv_max: Vec2,
    /// Bitmap size in pixels. Zero for whitespace.
    size: Vec2,
    /// Bitmap's bottom-left corner relative to the pen position on the baseline.
    offset: Vec2,
}

/// Places rectangles in rows, left to right, top to bottom.
struct ShelfPacker {
    x: u32,
    y: u32,
    row_height: u32,
}

impl ShelfPacker {
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if self.x + width + PADDING > ATLAS_SIZE {
            // Start a new row below the current one.
            self.x = 0;
            self.y += self.row_height + PADDING;
            self.row_height = 0;
        }
        if self.y + height + PADDING > ATLAS_SIZE {
            return None;
        }
        let position = (self.x + PADDING, self.y + PADDING);
        self.x += width + PADDING;
        self.row_height = self.row_height.max(height + PADDING);
        Some(position)
    }
}

/// A font with its own glyph atlas, drawn through a `SpriteRenderer`.
pub struct Font {
    font: fontdue::Font,
    atlas: SpriteTextureId,
    packer: ShelfPacker,
    glyphs: HashMap<(char, u32), Glyph>,
    atlas_full_warned: bool,
}

impl Font {
    /// Load a TrueType/OpenType font. Its atlas is registered in `sprites`,
    /// so text from this font must be drawn with that same sprite renderer.
    pub fn new(
        gpu: &GpuContext,
        sprites: &mut SpriteRenderer,
        bytes: &[u8],
    ) -> Result<Self, FontError> {
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
            .map_err(FontError::Invalid)?;

        let empty = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
        let texture = Texture::from_rgba8(gpu, ATLAS_SIZE, ATLAS_SIZE, &empty, Default::default());
        let atlas = sprites.add_texture(gpu, &texture, &gpu.samplers.nearest);

        Ok(Self {
            font,
            atlas,
            packer: ShelfPacker {
                x: 0,
                y: 0,
                row_height: 0,
            },
            glyphs: HashMap::new(),
            atlas_full_warned: false,
        })
    }

    /// Width and height of a block of text, in pixels.
    pub fn measure(&self, text: &str, size: f32) -> Vec2 {
        self.layout(text, size).1
    }

    /// Draw text with its top-left corner at `top_left` (in pixel-view coordinates, y up).
    pub fn draw(
        &mut self,
        gpu: &GpuContext,
        sprites: &mut SpriteRenderer,
        text: &str,
        top_left: Vec2,
        style: &TextStyle,
    ) {
        let px = style.size.round().max(1.0) as u32;
        let origin = top_left.round();
        let (pens, _) = self.layout(text, px as f32);

        for (c, pen) in pens {
            let Some(glyph) = self.glyph(gpu, sprites, c, px) else {
                continue;
            };
            if glyph.size.x == 0.0 || glyph.size.y == 0.0 {
                continue; // whitespace
            }
            let bottom_left = (origin + pen + glyph.offset).round();
            sprites.draw(
                self.atlas,
                &Sprite {
                    position: bottom_left + glyph.size * 0.5,
                    size: glyph.size,
                    color: style.color,
                    uv_min: glyph.uv_min,
                    uv_max: glyph.uv_max,
                    layer: style.layer,
                    ..Default::default()
                },
            );
        }
    }

    /// Pen positions (on the baseline, relative to the block's top-left) and the block's size.
    fn layout(&self, text: &str, size: f32) -> (Vec<(char, Vec2)>, Vec2) {
        let (ascent, descent, line_height) = match self.font.horizontal_line_metrics(size) {
            Some(m) => (m.ascent, m.descent, m.new_line_size),
            None => (size * 0.8, -size * 0.2, size * 1.2),
        };

        let mut pens = Vec::with_capacity(text.len());
        let mut pen = Vec2::new(0.0, -ascent);
        let mut width: f32 = 0.0;
        let mut lines = 1;
        let mut previous = None;

        for c in text.chars() {
            if c == '\n' {
                width = width.max(pen.x);
                pen.x = 0.0;
                pen.y -= line_height;
                lines += 1;
                previous = None;
                continue;
            }
            if let Some(p) = previous {
                pen.x += self.font.horizontal_kern(p, c, size).unwrap_or(0.0);
            }
            pens.push((c, pen));
            pen.x += self.font.metrics(c, size).advance_width;
            previous = Some(c);
        }
        width = width.max(pen.x);

        let height = ascent - descent + (lines - 1) as f32 * line_height;
        (pens, Vec2::new(width, height))
    }

    /// Get a glyph from the cache, rasterizing it into the atlas the first time.
    fn glyph(
        &mut self,
        gpu: &GpuContext,
        sprites: &SpriteRenderer,
        c: char,
        px: u32,
    ) -> Option<Glyph> {
        if let Some(glyph) = self.glyphs.get(&(c, px)) {
            return Some(*glyph);
        }

        let (metrics, coverage) = self.font.rasterize(c, px as f32);
        let (w, h) = (metrics.width as u32, metrics.height as u32);
        let mut glyph = Glyph {
            uv_min: Vec2::ZERO,
            uv_max: Vec2::ZERO,
            size: Vec2::new(w as f32, h as f32),
            offset: Vec2::new(metrics.xmin as f32, metrics.ymin as f32),
        };

        if w > 0 && h > 0 {
            let Some((x, y)) = self.packer.allocate(w, h) else {
                if !self.atlas_full_warned {
                    tracing::warn!("glyph atlas is full; some characters will be missing");
                    self.atlas_full_warned = true;
                }
                return None;
            };

            // White pixels, coverage in alpha: the sprite color tints it.
            let rgba: Vec<u8> = coverage.iter().flat_map(|&a| [255, 255, 255, a]).collect();
            sprites
                .texture(self.atlas)
                .write_rgba8(gpu, x, y, w, h, &rgba);

            let atlas = ATLAS_SIZE as f32;
            glyph.uv_min = Vec2::new(x as f32, y as f32) / atlas;
            glyph.uv_max = Vec2::new((x + w) as f32, (y + h) as f32) / atlas;
        }

        self.glyphs.insert((c, px), glyph);
        Some(glyph)
    }
}
