use std::ops::Range;

use gfx::glam::{Mat4, Vec2, Vec4};
use gfx::{GpuContext, PipelineBuilder, StorageBuffer, Texture, UniformBuffer, View};

use crate::shader_bindings::sprite as shader;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpriteTextureId(u32);

/// One textured rectangle in the XY plane.
#[derive(Clone, Copy, Debug)]
pub struct Sprite {
    /// Center, in the view's units (world units, or pixels with `View::pixels`).
    pub position: Vec2,
    pub size: Vec2,
    /// Radians, counter-clockwise.
    pub rotation: f32,
    /// Multiplied with the texture. Alpha controls transparency.
    pub color: Vec4,
    /// Region of the texture to show, in 0..1. Default: the whole texture.
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    /// Higher layers draw on top.
    pub layer: i32,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            size: Vec2::ONE,
            rotation: 0.0,
            color: Vec4::ONE,
            uv_min: Vec2::ZERO,
            uv_max: Vec2::ONE,
            layer: 0,
        }
    }
}

struct Queued {
    texture: SpriteTextureId,
    layer: i32,
    instance: shader::SpriteInstance,
}

/// A run of consecutive instances sharing one texture: one draw call.
struct Batch {
    texture: SpriteTextureId,
    instances: Range<u32>,
}

struct SpriteTexture {
    texture: Texture,
    bind_group: shader::WgpuBindGroup2,
}

fn instances_bind_group(
    gpu: &GpuContext,
    instances: &StorageBuffer<shader::SpriteInstance>,
) -> shader::WgpuBindGroup1 {
    shader::WgpuBindGroup1::from_bindings(
        &gpu.device,
        shader::WgpuBindGroup1Entries::new(shader::WgpuBindGroup1EntriesParams {
            sprites: instances.binding(),
        }),
    )
}

pub struct SpriteRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_ubo: UniformBuffer<shader::Camera>,
    camera_bind_group: shader::WgpuBindGroup0,
    textures: Vec<SpriteTexture>,
    white: SpriteTextureId,
    queued: Vec<Queued>,
    batches: Vec<Batch>,
    instances: StorageBuffer<shader::SpriteInstance>,
    instances_bind_group: shader::WgpuBindGroup1,
}

impl SpriteRenderer {
    pub fn new(gpu: &GpuContext) -> Self {
        let module = shader::create_shader_module_embed_source(&gpu.device);
        let layout = shader::create_pipeline_layout(&gpu.device);
        let pipeline = PipelineBuilder::new(gpu, &module, &layout)
            .label("sprites")
            .blend(wgpu::BlendState::ALPHA_BLENDING)
            .build();

        let camera_ubo = UniformBuffer::new(gpu, &shader::Camera::new(Mat4::IDENTITY));
        let camera_bind_group = shader::WgpuBindGroup0::from_bindings(
            &gpu.device,
            shader::WgpuBindGroup0Entries::new(shader::WgpuBindGroup0EntriesParams {
                camera: camera_ubo.binding(),
            }),
        );
        let instances = StorageBuffer::new(gpu, "sprite instances", 256);
        let instances_bind_group = instances_bind_group(gpu, &instances);

        let mut renderer = Self {
            pipeline,
            camera_ubo,
            camera_bind_group,
            instances,
            instances_bind_group,
            textures: Vec::new(),
            white: SpriteTextureId(0),
            queued: Vec::new(),
            batches: Vec::new(),
        };

        let white = Texture::from_rgba8(gpu, 1, 1, &[255, 255, 255, 255], Default::default());
        renderer.white = renderer.add_texture(gpu, &white, &gpu.samplers.nearest);
        renderer
    }

    // ---------------------------------------------------------------------
    // Resources
    // ---------------------------------------------------------------------

    /// Store a texture. Use `gpu.samplers.nearest` for pixel art, `linear` for smooth images.
    pub fn add_texture(
        &mut self,
        gpu: &GpuContext,
        texture: &Texture,
        sampler: &wgpu::Sampler,
    ) -> SpriteTextureId {
        let bind_group = shader::WgpuBindGroup2::from_bindings(
            &gpu.device,
            shader::WgpuBindGroup2Entries::new(shader::WgpuBindGroup2EntriesParams {
                sprite_texture: &texture.view,
                sprite_sampler: sampler,
            }),
        );
        self.textures.push(SpriteTexture {
            texture: texture.clone(),
            bind_group,
        });
        SpriteTextureId(self.textures.len() as u32 - 1)
    }

    /// A 1x1 white texture, for solid colored rectangles.
    pub fn white(&self) -> SpriteTextureId {
        self.white
    }

    /// A texture's size in pixels, for drawing it at its natural size.
    pub fn texture_size(&self, id: SpriteTextureId) -> Vec2 {
        let raw = &self.textures[id.0 as usize].texture.raw;
        Vec2::new(raw.width() as f32, raw.height() as f32)
    }

    // ---------------------------------------------------------------------
    // Frame
    // ---------------------------------------------------------------------

    /// Queue a sprite to be drawn this frame.
    pub fn draw(&mut self, texture: SpriteTextureId, sprite: &Sprite) {
        self.queued.push(Queued {
            texture,
            layer: sprite.layer,
            instance: shader::SpriteInstance::new(
                sprite.color,
                sprite.position,
                sprite.size,
                sprite.uv_min,
                sprite.uv_max,
                sprite.rotation,
            ),
        });
    }

    /// Upload everything queued with `draw`. Call once per frame, before any pass.
    pub fn prepare(&mut self, gpu: &GpuContext, view: &View) {
        self.batches.clear();
        if self.queued.is_empty() {
            return;
        }

        // Stable sort: layer first, then texture so batches form.
        self.queued.sort_by_key(|q| (q.layer, q.texture));

        self.camera_ubo
            .write(gpu, &shader::Camera::new(view.view_proj()));
        let instances: Vec<shader::SpriteInstance> =
            self.queued.iter().map(|q| q.instance).collect();

        if self.instances.write(gpu, &instances) {
            self.instances_bind_group = instances_bind_group(gpu, &self.instances);
        }

        // Split into runs of the same texture.
        let mut start = 0;
        for i in 1..=self.queued.len() {
            let run_ends =
                i == self.queued.len() || self.queued[i].texture != self.queued[start].texture;
            if run_ends {
                self.batches.push(Batch {
                    texture: self.queued[start].texture,
                    instances: start as u32..i as u32,
                });
                start = i;
            }
        }

        self.queued.clear();
    }

    /// Record the prepared sprites. Can be called in any number of passes.
    pub fn render(&self, pass: &mut wgpu::RenderPass) {
        if self.batches.is_empty() {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        self.camera_bind_group.set(pass);
        self.instances_bind_group.set(pass);

        for batch in &self.batches {
            self.textures[batch.texture.0 as usize].bind_group.set(pass);
            pass.draw(0..6, batch.instances.clone());
        }
    }

    /// The texture behind an id, for updating its contents.
    pub fn texture(&self, id: SpriteTextureId) -> &Texture {
        &self.textures[id.0 as usize].texture
    }
}
