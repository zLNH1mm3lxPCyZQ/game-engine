use crate::{GpuContext, VertexFormat};

pub struct PipelineBuilder<'a> {
    gpu: &'a GpuContext,
    shader: &'a wgpu::ShaderModule,
    layout: &'a wgpu::PipelineLayout,
    label: Option<&'a str>,
    vs_entry: &'a str,
    fs_entry: Option<&'a str>,
    targets: Vec<wgpu::TextureFormat>,
    blend: Option<wgpu::BlendState>,
    primitive: wgpu::PrimitiveState,
    vertex_buffers: Vec<wgpu::VertexBufferLayout<'a>>,
    depth: Option<wgpu::DepthStencilState>,
}

impl<'a> PipelineBuilder<'a> {
    pub fn new(
        gpu: &'a GpuContext,
        shader: &'a wgpu::ShaderModule,
        layout: &'a wgpu::PipelineLayout,
    ) -> Self {
        Self {
            gpu,
            shader,
            layout,
            label: None,
            vs_entry: "vs_main",
            fs_entry: Some("fs_main"),
            targets: Vec::new(),
            blend: None,
            primitive: wgpu::PrimitiveState::default(),
            vertex_buffers: Vec::new(),
            depth: Some(depth_state(false, wgpu::CompareFunction::Always)),
        }
    }
    /// Push depth values away from the viewer: `constant` in depth units, plus
    /// `slope_scale` times the surface's slope. Call after `depth_test`.
    /// Used for shadow maps, to prevent surfaces from shadowing themselves.
    pub fn depth_bias(mut self, constant: i32, slope_scale: f32) -> Self {
        if let Some(depth) = &mut self.depth {
            depth.bias = wgpu::DepthBiasState {
                constant,
                slope_scale,
                clamp: 0.0,
            };
        }
        self
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    pub fn entry_points(mut self, vs: &'a str, fs: Option<&'a str>) -> Self {
        self.vs_entry = vs;
        self.fs_entry = fs;
        self
    }

    /// Add a color target. If none are added, the surface format is used.
    pub fn target(mut self, format: wgpu::TextureFormat) -> Self {
        self.targets.push(format);
        self
    }

    pub fn blend(mut self, blend: wgpu::BlendState) -> Self {
        self.blend = Some(blend);
        self
    }

    pub fn cull_back(mut self) -> Self {
        self.primitive.cull_mode = Some(wgpu::Face::Back);
        self
    }

    /// Add a vertex buffer using one of the engine's vertex formats.
    /// Call multiple times for multiple buffers (slot 0, 1, ...).
    pub fn vertex<V: VertexFormat>(mut self) -> Self {
        self.vertex_buffers.push(V::LAYOUT);
        self
    }

    /// Escape hatch: add custom vertex buffer layouts directly.
    pub fn vertex_buffers(mut self, layouts: &[wgpu::VertexBufferLayout<'a>]) -> Self {
        self.vertex_buffers.extend_from_slice(layouts);
        self
    }

    /// Test against depth and write to it. Use for opaque 3D geometry.
    pub fn depth_test(mut self) -> Self {
        self.depth = Some(depth_state(true, wgpu::CompareFunction::Less));
        self
    }

    /// Test against depth without writing. Use for transparent geometry.
    pub fn depth_read_only(mut self) -> Self {
        self.depth = Some(depth_state(false, wgpu::CompareFunction::Less));
        self
    }

    /// No depth state at all, for custom passes without a depth attachment.
    pub fn no_depth(mut self) -> Self {
        self.depth = None;
        self
    }

    pub fn build(self) -> wgpu::RenderPipeline {
        let formats = if self.targets.is_empty() {
            vec![self.gpu.surface_format()]
        } else {
            self.targets
        };
        let targets: Vec<_> = formats
            .into_iter()
            .map(|format| {
                Some(wgpu::ColorTargetState {
                    format,
                    blend: self.blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })
            })
            .collect();
        let buffers: Vec<_> = self.vertex_buffers.into_iter().map(Some).collect();

        self.gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: self.label,
                layout: Some(self.layout),
                vertex: wgpu::VertexState {
                    module: self.shader,
                    entry_point: Some(self.vs_entry),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                fragment: self.fs_entry.map(|entry| wgpu::FragmentState {
                    module: self.shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &targets,
                }),
                primitive: self.primitive,
                depth_stencil: self.depth,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
    }
}

fn depth_state(write: bool, compare: wgpu::CompareFunction) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: crate::DEPTH_FORMAT,
        depth_write_enabled: Some(write),
        depth_compare: Some(compare),
        stencil: Default::default(),
        bias: Default::default(),
    }
}
