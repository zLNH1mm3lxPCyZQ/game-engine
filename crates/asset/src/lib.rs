//! Loading files from disk into engine resources, with caching.

mod animation;
mod gltf_loader;
mod model;

pub use animation::{Animation, Animator, Channel, Interpolation, Keyframes};
pub use model::{
    AlphaMode, Model, ModelMaterial, ModelNode, ModelPart, Pose, Skin, TextureFilter, TextureWrap,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("failed to read {}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to decode image {}", .path.display())]
    Image {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },

    #[error("failed to load glTF {}", .path.display())]
    Gltf {
        path: PathBuf,
        #[source]
        source: gltf::Error,
    },
}

/// Loads assets relative to a root folder, caching GPU resources by path.
pub struct Assets {
    root: PathBuf,
    /// Keyed by (path, is sRGB): the same file loaded as color and as data is two textures.
    textures: HashMap<(PathBuf, bool), gfx::Texture>,
    models: HashMap<PathBuf, Model>,
}

impl Assets {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        tracing::info!(root = %root.display(), "asset folder");
        Self {
            root,
            textures: HashMap::new(),
            models: HashMap::new(),
        }
    }

    /// The full path of an asset.
    pub fn path(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }

    /// Raw file contents. Not cached: use for files you parse yourself (fonts, levels, config).
    pub fn read(&self, relative: impl AsRef<Path>) -> Result<Vec<u8>, AssetError> {
        let path = self.path(relative);
        std::fs::read(&path).map_err(|source| AssetError::Io { path, source })
    }

    /// A color texture (sRGB): photos, sprites, albedo maps. Cached.
    pub fn texture(
        &mut self,
        gpu: &gfx::GpuContext,
        relative: impl AsRef<Path>,
    ) -> Result<gfx::Texture, AssetError> {
        self.load_texture(gpu, relative.as_ref(), true)
    }

    /// A data texture (linear): normal maps, masks, depth maps. Cached.
    pub fn data_texture(
        &mut self,
        gpu: &gfx::GpuContext,
        relative: impl AsRef<Path>,
    ) -> Result<gfx::Texture, AssetError> {
        self.load_texture(gpu, relative.as_ref(), false)
    }

    /// A glTF model (.gltf or .glb). Cached.
    pub fn model(
        &mut self,
        gpu: &gfx::GpuContext,
        relative: impl AsRef<Path>,
    ) -> Result<Model, AssetError> {
        let relative = relative.as_ref();
        if let Some(model) = self.models.get(relative) {
            return Ok(model.clone());
        }

        let path = self.path(relative);
        let model = gltf_loader::load(gpu, &path).map_err(|source| AssetError::Gltf {
            path: path.clone(),
            source,
        })?;
        tracing::debug!(
            path = %path.display(),
            parts = model.parts.len(),
            materials = model.materials.len(),
            animations = model.animations.len(),
            skins = model.skins.len(),
            "loaded model"
        );

        self.models.insert(relative.to_path_buf(), model.clone());
        Ok(model)
    }

    fn load_texture(
        &mut self,
        gpu: &gfx::GpuContext,
        relative: &Path,
        srgb: bool,
    ) -> Result<gfx::Texture, AssetError> {
        let key = (relative.to_path_buf(), srgb);
        if let Some(texture) = self.textures.get(&key) {
            return Ok(texture.clone());
        }

        let bytes = self.read(relative)?;
        let path = self.path(relative);
        let image = image::load_from_memory(&bytes)
            .map_err(|source| AssetError::Image {
                path: path.clone(),
                source,
            })?
            .to_rgba8();

        let texture = gfx::Texture::from_rgba8(
            gpu,
            image.width(),
            image.height(),
            &image,
            gfx::TextureOptions {
                srgb,
                ..Default::default()
            },
        );
        tracing::debug!(path = %path.display(), width = image.width(), height = image.height(), "loaded texture");

        self.textures.insert(key, texture.clone());
        Ok(texture)
    }
}
