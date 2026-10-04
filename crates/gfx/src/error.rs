#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to create surface: {0}")]
    CreateSurface(#[from] wgpu::CreateSurfaceError),

    #[error("no compatible GPU adapter found: {0}")]
    RequestAdapter(#[from] wgpu::RequestAdapterError),

    #[error("failed to create GPU device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),

    #[error("the window surface is not supported by the GPU adapter")]
    UnsupportedSurface,
}
