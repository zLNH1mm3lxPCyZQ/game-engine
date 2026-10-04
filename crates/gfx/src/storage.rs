use std::marker::PhantomData;

use crate::GpuContext;

/// A GPU storage buffer holding an array of `T`, growing as needed.
pub struct StorageBuffer<T> {
    raw: wgpu::Buffer,
    capacity: usize,
    label: &'static str,
    _marker: PhantomData<T>,
}

impl<T: bytemuck::Pod> StorageBuffer<T> {
    pub fn new(gpu: &GpuContext, label: &'static str, capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            raw: Self::create(gpu, label, capacity),
            capacity,
            label,
            _marker: PhantomData,
        }
    }

    fn create(gpu: &GpuContext, label: &'static str, capacity: usize) -> wgpu::Buffer {
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (capacity * std::mem::size_of::<T>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Upload `data`, growing the buffer first if it's too small.
    ///
    /// Returns `true` if the buffer was replaced: any bind group using it must be rebuilt.
    #[must_use = "if the buffer grew, bind groups pointing at it must be rebuilt"]
    pub fn write(&mut self, gpu: &GpuContext, data: &[T]) -> bool {
        let grew = data.len() > self.capacity;
        if grew {
            self.capacity = data.len().next_power_of_two();
            self.raw = Self::create(gpu, self.label, self.capacity);
        }
        if !data.is_empty() {
            gpu.queue
                .write_buffer(&self.raw, 0, bytemuck::cast_slice(data));
        }
        grew
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn binding(&self) -> wgpu::BufferBinding<'_> {
        self.raw.as_entire_buffer_binding()
    }

    pub fn raw(&self) -> &wgpu::Buffer {
        &self.raw
    }
}
