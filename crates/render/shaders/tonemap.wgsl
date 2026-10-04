// Maps an HDR image to displayable 0..1 colors.

struct Params {
    settings: vec4<f32>, // x = exposure, y = operator (0 none, 1 Reinhard, 2 ACES)
}

@group(0) @binding(0) var hdr_texture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0); // one oversized triangle
}

// Fitted ACES curve (Krzysztof Narkowicz).
fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    // Same size in and out: read the exact pixel, no sampler needed.
    let hdr = textureLoad(hdr_texture, vec2<i32>(position.xy), 0).rgb * params.settings.x;

    var mapped: vec3<f32>;
    switch u32(params.settings.y) {
        case 0u: { mapped = clamp(hdr, vec3<f32>(0.0), vec3<f32>(1.0)); }
        case 1u: { mapped = hdr / (1.0 + hdr); }
        default: { mapped = aces(hdr); }
    }
    // The output target is sRGB: the GPU encodes these linear values on write.
    return vec4<f32>(mapped, 1.0);
}