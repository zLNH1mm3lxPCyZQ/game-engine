use asset::AlphaMode;
use gfx::glam::Vec4;

use crate::TextureId;
use asset::{TextureFilter, TextureWrap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaterialId(pub(crate) u32);

/// Describes how a surface looks.
#[derive(Clone, Copy, Debug)]
pub struct Material {
    /// Multiplied with the texture.
    pub base_color: Vec4,
    /// If `None`, a white texture is used, so the surface is just `base_color`.
    pub base_color_texture: Option<TextureId>,
    /// 0 = mirror-smooth, 1 = fully matte.
    pub roughness: f32,
    /// 0 = non-metal (plastic, wood, skin), 1 = metal.
    pub metallic: f32,
    pub alpha_mode: AlphaMode,
    /// Render back faces too (leaves, paper, cloth).
    pub double_sided: bool,
    /// How the texture is sampled between texels.
    pub filter: TextureFilter,
    /// What happens to UVs outside 0..1.
    pub wrap: TextureWrap,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: Vec4::ONE,
            base_color_texture: None,
            roughness: 0.5,
            metallic: 0.0,
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            filter: TextureFilter::Linear,
            wrap: TextureWrap::Repeat,
        }
    }
}
