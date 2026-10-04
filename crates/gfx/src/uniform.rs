use std::marker::PhantomData;

use wgpu::util::DeviceExt;

use crate::GpuContext;

pub struct UniformBuffer<T> {
    pub raw: wgpu::Buffer,
    _marker: PhantomData<T>,
}

impl<T: bytemuck::Pod> UniformBuffer<T> {
    pub fn new(gpu: &GpuContext, value: &T) -> Self {
        let raw = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(std::any::type_name::<T>()),
                contents: bytemuck::bytes_of(value),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        Self {
            raw,
            _marker: PhantomData,
        }
    }

    pub fn write(&self, gpu: &GpuContext, value: &T) {
        gpu.queue
            .write_buffer(&self.raw, 0, bytemuck::bytes_of(value));
    }

    pub fn binding(&self) -> wgpu::BufferBinding<'_> {
        self.raw.as_entire_buffer_binding()
    }
}
