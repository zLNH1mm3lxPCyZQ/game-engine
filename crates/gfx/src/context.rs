use crate::{RenderTarget, Texture};

pub struct Samplers {
    pub linear: wgpu::Sampler,
    pub nearest: wgpu::Sampler,
    pub linear_repeat: wgpu::Sampler,
    pub nearest_repeat: wgpu::Sampler,
}

impl Samplers {
    fn new(device: &wgpu::Device) -> Self {
        let make = |label, filter, address_mode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                mag_filter: filter,
                min_filter: filter,
                address_mode_u: address_mode,
                address_mode_v: address_mode,
                address_mode_w: address_mode,
                ..Default::default()
            })
        };
        use wgpu::{AddressMode::*, FilterMode::*};
        Self {
            linear: make("linear", Linear, ClampToEdge),
            nearest: make("nearest", Nearest, ClampToEdge),
            linear_repeat: make("linear repeat", Linear, Repeat),
            nearest_repeat: make("nearest repeat", Nearest, Repeat),
        }
    }

    /// The sampler for a filter and wrap combination.
    pub fn get(&self, nearest: bool, repeat: bool) -> &wgpu::Sampler {
        match (nearest, repeat) {
            (false, false) => &self.linear,
            (true, false) => &self.nearest,
            (false, true) => &self.linear_repeat,
            (true, true) => &self.nearest_repeat,
        }
    }
}

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub samplers: Samplers,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    depth: Texture,
}

pub struct Frame {
    pub view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
    pub encoder: wgpu::CommandEncoder,
    surface_texture: wgpu::SurfaceTexture,
}

impl GpuContext {
    #[tracing::instrument(skip_all)]
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
        vsync: bool,
    ) -> Result<Self, crate::Error> {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(target)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;

        let info = adapter.get_info();
        tracing::info!(
            gpu = %info.name,
            backend = ?info.backend,
            driver = %info.driver,
            "selected GPU"
        );

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;

        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or(crate::Error::UnsupportedSurface)?;
        config.present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        surface.configure(&device, &config);

        let depth = Texture::create_depth(&device, config.width, config.height);
        let samplers = Samplers::new(&device);

        Ok(Self {
            device,
            queue,
            samplers,
            surface,
            config,
            depth,
        })
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = Texture::create_depth(&self.device, width, height);
    }

    pub fn begin_frame(&mut self) -> Option<Frame> {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                drop(t);
                self.surface.configure(&self.device, &self.config);
                return None;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return None;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return None;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                tracing::error!("surface validation error");
                return None;
            }
        };
        let view = surface_texture.texture.create_view(&Default::default());
        let encoder = self.device.create_command_encoder(&Default::default());
        Some(Frame {
            view,
            depth_view: self.depth.view.clone(),
            encoder,
            surface_texture,
        })
    }

    pub fn end_frame(&self, frame: Frame) {
        self.queue.submit([frame.encoder.finish()]);
        self.queue.present(frame.surface_texture);
    }
}

impl Frame {
    /// A pass with only a depth attachment (no color), cleared to the far plane.
    /// For shadow maps and depth pre-passes.
    pub fn depth_pass(&mut self, depth: &Texture) -> wgpu::RenderPass<'_> {
        self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        })
    }

    pub fn clear_pass(&mut self, color: wgpu::Color) -> wgpu::RenderPass<'_> {
        begin_pass(
            &mut self.encoder,
            &self.view,
            &self.depth_view,
            wgpu::LoadOp::Clear(color),
        )
    }

    pub fn load_pass(&mut self) -> wgpu::RenderPass<'_> {
        begin_pass(
            &mut self.encoder,
            &self.view,
            &self.depth_view,
            wgpu::LoadOp::Load,
        )
    }

    pub fn clear_pass_to(
        &mut self,
        target: &RenderTarget,
        color: wgpu::Color,
    ) -> wgpu::RenderPass<'_> {
        begin_pass(
            &mut self.encoder,
            &target.color.view,
            &target.depth.view,
            wgpu::LoadOp::Clear(color),
        )
    }

    pub fn load_pass_to(&mut self, target: &RenderTarget) -> wgpu::RenderPass<'_> {
        begin_pass(
            &mut self.encoder,
            &target.color.view,
            &target.depth.view,
            wgpu::LoadOp::Load,
        )
    }
}

fn begin_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'e> {
    // Clearing color also clears depth; loading color keeps depth.
    let depth_load = match load {
        wgpu::LoadOp::Clear(_) => wgpu::LoadOp::Clear(1.0),
        _ => wgpu::LoadOp::Load,
    };

    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: depth_load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    })
}
