//! # Coordinate system
//! Right-handed, Z-up, matching Blender:
//! - +X: right (east)
//! - +Y: forward (north)
//! - +Z: up
//!
//! 2D content lives in the XY plane, viewed from +Z looking down.

mod camera;
mod context;
mod error;
mod mesh;
mod pipeline;
mod storage;
mod texture;
mod transform;
mod uniform;
mod viewport;

pub use camera::{Camera, Projection, View};
pub use context::{Frame, GpuContext, Samplers};
pub use error::Error;
pub use mesh::{Mesh, SkinnedVertex, Vertex, VertexFormat};
pub use pipeline::PipelineBuilder;
pub use storage::StorageBuffer;
pub use texture::{DEPTH_FORMAT, RenderTarget, Texture, TextureOptions};
pub use transform::Transform;
pub use uniform::UniformBuffer;

pub use glam;

pub use viewport::Viewport;
