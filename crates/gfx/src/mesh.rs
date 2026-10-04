//! Meshes and vertex formats.
//!
//! # Vertex location table
//!
//! Every vertex format and every mesh shader follows this table. A format may
//! use any subset, but an attribute always lives at the same location.
//!
//! | Loc | Attribute | Shader type | Purpose                         |
//! |-----|-----------|-------------|---------------------------------|
//! | 0   | position  | vec3<f32>   | everything                      |
//! | 1   | normal    | vec3<f32>   | lighting                        |
//! | 2   | uv0       | vec2<f32>   | textures                        |
//! | 3   | tangent   | vec4<f32>   | normal mapping (w = handedness) |
//! | 4   | joints    | vec4<u32>   | skinning: bone indices          |
//! | 5   | weights   | vec4<f32>   | skinning: bone weights          |
//! | 6   | color     | vec4<f32>   | vertex colors                   |
//! | 7   | uv1       | vec2<f32>   | lightmaps, detail textures      |
//!
//! Mirrors glTF: POSITION, NORMAL, TEXCOORD_0, TANGENT, JOINTS_0, WEIGHTS_0,
//! COLOR_0, TEXCOORD_1.

use crate::GpuContext;

/// A type that can be stored in a vertex buffer.
pub trait VertexFormat: bytemuck::Pod {
    const LAYOUT: wgpu::VertexBufferLayout<'static>;
}

// ---------------------------------------------------------------------------
// Vertex: the default format for static meshes.
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

const _: () = assert!(std::mem::size_of::<Vertex>() == 32);

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x3, // position
        1 => Float32x3, // normal
        2 => Float32x2, // uv0
    ];
}

impl VertexFormat for Vertex {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &Self::ATTRIBUTES,
    };
}

// ---------------------------------------------------------------------------
// SkinnedVertex: animated meshes with up to 4 bones per vertex.
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<SkinnedVertex>() == 56);

impl SkinnedVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3, // position
        1 => Float32x3, // normal
        2 => Float32x2, // uv0
        4 => Uint16x4,  // joints
        5 => Float32x4, // weights
    ];
}

impl VertexFormat for SkinnedVertex {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &Self::ATTRIBUTES,
    };
}

// ---------------------------------------------------------------------------
// Mesh
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Mesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl Mesh {
    /// A 1x1 quad in the XY plane, centered at the origin, facing +Z.
    pub fn quad(gpu: &GpuContext) -> Self {
        let n = [0.0, 0.0, 1.0];
        let vertices = [
            Vertex {
                position: [-0.5, -0.5, 0.0],
                normal: n,
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, 0.0],
                normal: n,
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, 0.0],
                normal: n,
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5, 0.0],
                normal: n,
                uv: [0.0, 0.0],
            },
        ];
        let indices = [0, 1, 2, 0, 2, 3];
        Self::new(gpu, &vertices, &indices)
    }

    /// A sphere of diameter 1 centered at the origin, poles on the Z axis.
    pub fn sphere(gpu: &GpuContext, segments: u32, rings: u32) -> Self {
        use glam::Vec3;
        use std::f32::consts::{PI, TAU};

        let mut vertices = Vec::with_capacity(((segments + 1) * (rings + 1)) as usize);
        for ring in 0..=rings {
            let v = ring as f32 / rings as f32;
            let theta = v * PI; // 0 at the top pole, PI at the bottom
            for segment in 0..=segments {
                let u = segment as f32 / segments as f32;
                let phi = u * TAU; // around the Z axis
                let normal = Vec3::new(
                    theta.sin() * phi.cos(),
                    theta.sin() * phi.sin(),
                    theta.cos(),
                );
                vertices.push(Vertex {
                    position: (normal * 0.5).into(),
                    normal: normal.into(),
                    uv: [u, v],
                });
            }
        }

        let mut indices = Vec::with_capacity((segments * rings * 6) as usize);
        let row = segments + 1;
        for ring in 0..rings {
            for segment in 0..segments {
                let a = ring * row + segment; // this ring
                let b = a + row; // the ring below
                indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }

        Self::new(gpu, &vertices, &indices)
    }

    /// A 1x1x1 cube centered at the origin, with per-face normals and UVs.
    pub fn cube(gpu: &GpuContext) -> Self {
        use glam::Vec3;

        // Each face: outward normal and the face's "up" direction.
        let faces = [
            (Vec3::X, Vec3::Z),
            (Vec3::NEG_X, Vec3::Z),
            (Vec3::Y, Vec3::Z),
            (Vec3::NEG_Y, Vec3::Z),
            (Vec3::Z, Vec3::Y),
            (Vec3::NEG_Z, Vec3::NEG_Y),
        ];
        // Corner offsets along (right, up), with matching UVs.
        let corners = [
            (-1.0, -1.0, [0.0, 1.0]),
            (1.0, -1.0, [1.0, 1.0]),
            (1.0, 1.0, [1.0, 0.0]),
            (-1.0, 1.0, [0.0, 0.0]),
        ];

        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);
        for (normal, up) in faces {
            let right = up.cross(normal);
            let base = vertices.len() as u32;
            for (sx, sy, uv) in corners {
                let p = (normal + right * sx + up * sy) * 0.5;
                vertices.push(Vertex {
                    position: p.into(),
                    normal: normal.into(),
                    uv,
                });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        Self::new(gpu, &vertices, &indices)
    }

    pub fn new<V: VertexFormat>(gpu: &GpuContext, vertices: &[V], indices: &[u32]) -> Self {
        let vertex_bytes: &[u8] = bytemuck::cast_slice(vertices);
        let index_bytes: &[u8] = bytemuck::cast_slice(indices);

        let vertex_buffer = Self::create_buffer(
            gpu,
            "vertices",
            wgpu::BufferUsages::VERTEX,
            vertex_bytes.len(),
        );
        let index_buffer =
            Self::create_buffer(gpu, "indices", wgpu::BufferUsages::INDEX, index_bytes.len());
        if !vertex_bytes.is_empty() {
            gpu.queue.write_buffer(&vertex_buffer, 0, vertex_bytes);
        }
        if !index_bytes.is_empty() {
            gpu.queue.write_buffer(&index_buffer, 0, index_bytes);
        }

        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
        }
    }

    /// Replace the mesh's contents, reusing its buffers when the data fits.
    ///
    /// Clones share buffers: an in-place update affects them all, but a growing
    /// update only affects this copy. Don't share meshes you intend to update.
    pub fn update<V: VertexFormat>(&mut self, gpu: &GpuContext, vertices: &[V], indices: &[u32]) {
        let vertex_bytes: &[u8] = bytemuck::cast_slice(vertices);
        let index_bytes: &[u8] = bytemuck::cast_slice(indices);

        if vertex_bytes.len() as u64 > self.vertex_buffer.size() {
            let size = vertex_bytes.len().next_power_of_two();
            self.vertex_buffer =
                Self::create_buffer(gpu, "vertices", wgpu::BufferUsages::VERTEX, size);
        }
        if index_bytes.len() as u64 > self.index_buffer.size() {
            let size = index_bytes.len().next_power_of_two();
            self.index_buffer =
                Self::create_buffer(gpu, "indices", wgpu::BufferUsages::INDEX, size);
        }

        if !vertex_bytes.is_empty() {
            gpu.queue.write_buffer(&self.vertex_buffer, 0, vertex_bytes);
        }
        if !index_bytes.is_empty() {
            gpu.queue.write_buffer(&self.index_buffer, 0, index_bytes);
        }
        self.index_count = indices.len() as u32;
    }

    /// A buffer of at least `size` bytes that can be written to later.
    fn create_buffer(
        gpu: &GpuContext,
        label: &str,
        usage: wgpu::BufferUsages,
        size: usize,
    ) -> wgpu::Buffer {
        // Buffer sizes must be non-zero multiples of 4 bytes.
        let size = (size.max(4) as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.draw_instanced(pass, 0..1);
    }

    pub fn draw_instanced(&self, pass: &mut wgpu::RenderPass, instances: std::ops::Range<u32>) {
        if self.index_count == 0 {
            return;
        }
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, instances);
    }
}
