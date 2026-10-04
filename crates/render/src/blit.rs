use gfx::{GpuContext, PipelineBuilder, Texture};

use crate::shader_bindings::blit as shader;

/// Draws a texture over the whole viewport: upscaling, showing render targets, post effects.
pub struct Blit {
    pipeline: wgpu::RenderPipeline,
    bind_group: shader::WgpuBindGroup0,
}

impl Blit {
    /// A blit drawing into targets of the surface format.
    pub fn new(gpu: &GpuContext, source: &Texture, sampler: &wgpu::Sampler) -> Self {
        Self::with_format(gpu, source, sampler, gpu.surface_format())
    }

    /// A blit drawing into targets of the given format.
    pub fn with_format(
        gpu: &GpuContext,
        source: &Texture,
        sampler: &wgpu::Sampler,
        format: wgpu::TextureFormat,
    ) -> Self {
        let module = shader::create_shader_module_embed_source(&gpu.device);
        let layout = shader::create_pipeline_layout(&gpu.device);
        let pipeline = PipelineBuilder::new(gpu, &module, &layout)
            .label("blit")
            .target(format)
            .build();
        let bind_group = shader::WgpuBindGroup0::from_bindings(
            &gpu.device,
            shader::WgpuBindGroup0Entries::new(shader::WgpuBindGroup0EntriesParams {
                source_texture: &source.view,
                source_sampler: sampler,
            }),
        );
        Self {
            pipeline,
            bind_group,
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        self.bind_group.set(pass);
        pass.draw(0..3, 0..1);
    }
}
