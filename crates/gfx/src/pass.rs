/// A render pass being recorded. Created by `Frame`'s pass methods.
pub struct Pass<'a> {
    raw: wgpu::RenderPass<'a>,
}

impl<'a> Pass<'a> {
    pub(crate) fn new(raw: wgpu::RenderPass<'a>) -> Self {
        Self { raw }
    }

    /// Escape hatch for renderer authors: the underlying wgpu pass.
    pub fn raw(&mut self) -> &mut wgpu::RenderPass<'a> {
        &mut self.raw
    }
}
