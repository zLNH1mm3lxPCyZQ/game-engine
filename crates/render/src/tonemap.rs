use gfx::glam::Vec4;
use gfx::{GpuContext, PipelineBuilder, RenderTarget, UniformBuffer};

use crate::shader_bindings::tonemap as shader;

/// How HDR values are compressed into the displayable range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToneMap {
    /// Clip at 1. For comparison.
    None,
    /// x / (1 + x): never clips, a bit flat.
    Reinhard,
    /// A filmic curve: soft highlights, richer midtones.
    #[default]
    Aces,
}

impl ToneMap {
    fn id(self) -> f32 {
        match self {
            ToneMap::None => 0.0,
            ToneMap::Reinhard => 1.0,
            ToneMap::Aces => 2.0,
        }
    }

    /// The next operator, for cycling through them.
    pub fn next(self) -> Self {
        match self {
            ToneMap::None => ToneMap::Reinhard,
            ToneMap::Reinhard => ToneMap::Aces,
            ToneMap::Aces => ToneMap::None,
        }
    }
}

/// Owns an HDR render target and maps it to a displayable target.
pub struct ToneMapper {
    pipeline: wgpu::RenderPipeline,
    target: RenderTarget,
    params: UniformBuffer<shader::Params>,
    bind_group: shader::WgpuBindGroup0,
    pub exposure: f32,
    pub operator: ToneMap,
}

impl ToneMapper {
    /// The format scene renderers must target to draw into `target()`.
    pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

    /// `output_format` is the format of whatever `draw` will write into.
    pub fn new(
        gpu: &GpuContext,
        width: u32,
        height: u32,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        let module = shader::create_shader_module_embed_source(&gpu.device);
        let layout = shader::create_pipeline_layout(&gpu.device);
        let pipeline = PipelineBuilder::new(gpu, &module, &layout)
            .label("tonemap")
            .target(output_format)
            .build();

        let target = RenderTarget::new(gpu, width.max(1), height.max(1), Self::HDR_FORMAT);
        let params = UniformBuffer::new(
            gpu,
            &shader::Params::new(Vec4::new(1.0, ToneMap::Aces.id(), 0.0, 0.0)),
        );
        let bind_group = Self::bind_group(gpu, &target, &params);

        Self {
            pipeline,
            target,
            params,
            bind_group,
            exposure: 1.0,
            operator: ToneMap::default(),
        }
    }

    fn bind_group(
        gpu: &GpuContext,
        target: &RenderTarget,
        params: &UniformBuffer<shader::Params>,
    ) -> shader::WgpuBindGroup0 {
        shader::WgpuBindGroup0::from_bindings(
            &gpu.device,
            shader::WgpuBindGroup0Entries::new(shader::WgpuBindGroup0EntriesParams {
                hdr_texture: &target.color.view,
                params: params.binding(),
            }),
        )
    }

    /// Recreate the HDR target if the size changed. Cheap when it didn't.
    pub fn resize(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        let raw = &self.target.color.raw;
        if raw.width() == width && raw.height() == height {
            return;
        }
        self.target = RenderTarget::new(gpu, width, height, Self::HDR_FORMAT);
        self.bind_group = Self::bind_group(gpu, &self.target, &self.params);
    }

    /// Render the scene into this target, with renderers created for `HDR_FORMAT`.
    pub fn target(&self) -> &RenderTarget {
        &self.target
    }

    /// Upload exposure and operator. Call once per frame, before `draw`.
    pub fn prepare(&self, gpu: &GpuContext) {
        self.params.write(
            gpu,
            &shader::Params::new(Vec4::new(self.exposure, self.operator.id(), 0.0, 0.0)),
        );
    }

    /// Draw the tone-mapped image. The pass must target something the same size as `target()`.
    pub fn draw(&self, pass: &mut gfx::Pass) {
        let pass = pass.raw();
        pass.set_pipeline(&self.pipeline);
        self.bind_group.set(pass);
        pass.draw(0..3, 0..1);
    }
}
