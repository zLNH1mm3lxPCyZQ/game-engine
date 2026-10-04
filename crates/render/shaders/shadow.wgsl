// Depth-only pass from the sun's point of view.
// Reads the same objects and joints buffers as the mesh shader.

struct ShadowParams {
    light_view_proj: mat4x4<f32>,
    settings: vec4<f32>,
}

struct Object {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    joint_offset: vec4<u32>,
}

@group(0) @binding(0) var<uniform> shadow: ShadowParams;
@group(1) @binding(0) var<storage, read> objects: array<Object>;
@group(1) @binding(1) var<storage, read> joints: array<mat4x4<f32>>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct SkinnedVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) joint_indices: vec4<u32>,
    @location(5) joint_weights: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance: u32) -> @builtin(position) vec4<f32> {
    let world = objects[instance].model * vec4<f32>(in.position, 1.0);
    return shadow.light_view_proj * world;
}

@vertex
fn vs_skinned(in: SkinnedVertexInput, @builtin(instance_index) instance: u32) -> @builtin(position) vec4<f32> {
    let object = objects[instance];
    let base = object.joint_offset.x;
    let skin = joints[base + in.joint_indices.x] * in.joint_weights.x
             + joints[base + in.joint_indices.y] * in.joint_weights.y
             + joints[base + in.joint_indices.z] * in.joint_weights.z
             + joints[base + in.joint_indices.w] * in.joint_weights.w;
    let world = object.model * skin * vec4<f32>(in.position, 1.0);
    return shadow.light_view_proj * world;
}