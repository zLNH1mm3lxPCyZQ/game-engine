// Engine sprite shader. Sprites live in the XY plane at z = 0.
// Bind groups: 0 = view, 1 = all sprite instances, 2 = texture.

struct Camera {
    view_proj: mat4x4<f32>,
}

struct SpriteInstance {
    color: vec4<f32>,
    position: vec2<f32>,
    size: vec2<f32>,
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
    rotation: f32,
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<storage, read> sprites: array<SpriteInstance>;
@group(2) @binding(0) var sprite_texture: texture_2d<f32>;
@group(2) @binding(1) var sprite_sampler: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let s = sprites[ii];

    // Two triangles covering the unit square, counter-clockwise.
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let corner = corners[vi];

    // Center, scale, rotate, move.
    let local = (corner - 0.5) * s.size;
    let c = cos(s.rotation);
    let r = sin(s.rotation);
    let rotated = vec2<f32>(local.x * c - local.y * r, local.x * r + local.y * c);

    var out: VsOut;
    out.pos = camera.view_proj * vec4<f32>(s.position + rotated, 0.0, 1.0);
    // Image rows go top to bottom, but y goes up: flip v.
    out.uv = mix(s.uv_min, s.uv_max, vec2<f32>(corner.x, 1.0 - corner.y));
    out.color = s.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(sprite_texture, sprite_sampler, in.uv) * in.color;
}