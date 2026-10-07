use wgpu::util::DeviceExt;

use crate::GpuContext;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// How a texture is sampled between texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextureFilter {
    /// Smooth: photos, painted textures.
    #[default]
    Linear,
    /// Hard pixels: pixel art, low-resolution textures.
    Nearest,
}

/// What happens to UVs outside 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextureWrap {
    /// Tile the texture (glTF's default).
    #[default]
    Repeat,
    /// Stretch the edge pixels.
    Clamp,
}

#[derive(Clone)]
pub struct Texture {
    pub raw: wgpu::Texture,
    pub view: wgpu::TextureView,
}

pub struct TextureOptions {
    pub label: Option<&'static str>,
    /// Color images: true. Data textures (normals, depth maps, masks): false.
    pub srgb: bool,
    /// Added on top of the usage flags the constructor already sets.
    pub extra_usage: wgpu::TextureUsages,
}

impl Default for TextureOptions {
    fn default() -> Self {
        Self {
            label: None,
            srgb: true,
            extra_usage: wgpu::TextureUsages::empty(),
        }
    }
}

impl Texture {
    pub fn from_rgba8(
        gpu: &GpuContext,
        width: u32,
        height: u32,
        data: &[u8],
        opts: TextureOptions,
    ) -> Self {
        assert_eq!(
            data.len(),
            (width * height * 4) as usize,
            "data must be width * height * 4 bytes"
        );

        let format = if opts.srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };

        let raw = gpu.device.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: opts.label,
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | opts.extra_usage,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            data,
        );
        let view = raw.create_view(&Default::default());

        Self { raw, view }
    }

    /// Overwrite a rectangular region of an RGBA8 texture.
    pub fn write_rgba8(
        &self,
        gpu: &GpuContext,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        assert_eq!(
            data.len(),
            (width * height * 4) as usize,
            "data must be width * height * 4 bytes"
        );
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.raw,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn depth(gpu: &GpuContext, width: u32, height: u32) -> Self {
        Self::create_depth(&gpu.device, width, height)
    }

    /// Used by GpuContext before a GpuContext exists.
    pub(crate) fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let raw = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = raw.create_view(&Default::default());
        Self { raw, view }
    }

    pub fn render_target(
        gpu: &GpuContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let raw = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = raw.create_view(&Default::default());
        Self { raw, view }
    }
}

/// A color texture plus a matching depth texture, for offscreen passes.
pub struct RenderTarget {
    pub color: Texture,
    pub depth: Texture,
}

impl RenderTarget {
    pub fn new(gpu: &GpuContext, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        Self {
            color: Texture::render_target(gpu, width, height, format),
            depth: Texture::depth(gpu, width, height),
        }
    }
}
