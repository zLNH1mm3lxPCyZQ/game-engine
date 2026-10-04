// Engine mesh shader: PBR lighting (metallic-roughness), Z-up, optional skinning.
// Vertex inputs follow the location table in gfx::mesh.
// Bind groups: 0 = per-view (camera, lights), 1 = objects and joints, 2 = material.

const PI: f32 = 3.14159265;
const MAX_POINT_LIGHTS: u32 = 16u;

struct Camera {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
}

struct PointLight {
    position_range: vec4<f32>,  // xyz = position, w = range
    color_intensity: vec4<f32>, // rgb = color * intensity
}

struct Lights {
    sun_direction: vec4<f32>,   // xyz = direction toward the sun
    sun_color: vec4<f32>,       // rgb = color * intensity
    sky_color: vec4<f32>,
    ground_color: vec4<f32>,
    counts: vec4<u32>,          // x = number of point lights
    points: array<PointLight, 16>,
}

struct Object {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,        // inverse transpose of model
    joint_offset: vec4<u32>,    // x = first joint matrix in `joints` (skinned draws only)
}

struct MaterialParams {
    base_color: vec4<f32>,
    roughness: f32,
    metallic: f32,
    alpha_cutoff: f32,
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> lights: Lights;
@group(1) @binding(0) var<storage, read> objects: array<Object>;
@group(1) @binding(1) var<storage, read> joints: array<mat4x4<f32>>;
@group(2) @binding(0) var<uniform> material: MaterialParams;
@group(2) @binding(1) var base_color_texture: texture_2d<f32>;
@group(2) @binding(2) var base_color_sampler: sampler;

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

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct ShadowParams {
    light_view_proj: mat4x4<f32>,
    settings: vec4<f32>, // x = depth bias, y = enabled (0/1), z = texel size in UV, w = normal offset
}

@group(3) @binding(0) var<uniform> shadow: ShadowParams;
@group(3) @binding(1) var shadow_map: texture_depth_2d;
@group(3) @binding(2) var shadow_sampler: sampler_comparison;

// Shared by both vertex entry points: model space to world and clip space.
fn finish(object: Object, position: vec4<f32>, normal: vec3<f32>, uv: vec2<f32>) -> VsOut {
    let world = object.model * position;
    var out: VsOut;
    out.pos = camera.view_proj * world;
    out.world_pos = world.xyz;
    out.normal = (object.normal * vec4<f32>(normal, 0.0)).xyz;
    out.uv = uv;
    return out;
}

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance: u32) -> VsOut {
    return finish(objects[instance], vec4<f32>(in.position, 1.0), in.normal, in.uv);
}

@vertex
fn vs_skinned(in: SkinnedVertexInput, @builtin(instance_index) instance: u32) -> VsOut {
    let object = objects[instance];
    let base = object.joint_offset.x;

    // Blend up to four joint matrices by their weights.
    let skin = joints[base + in.joint_indices.x] * in.joint_weights.x
             + joints[base + in.joint_indices.y] * in.joint_weights.y
             + joints[base + in.joint_indices.z] * in.joint_weights.z
             + joints[base + in.joint_indices.w] * in.joint_weights.w;

    let position = skin * vec4<f32>(in.position, 1.0);
    let normal = (skin * vec4<f32>(in.normal, 0.0)).xyz;
    return finish(object, position, normal, in.uv);
}

// --- PBR building blocks -------------------------------------------------

// How many microfacets point along the half vector (GGX). Controls highlight size.
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

// How much microfacets shadow each other (Smith with Schlick-GGX).
fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = r * r / 8.0;
    let gv = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let gl = n_dot_l / (n_dot_l * (1.0 - k) + k);
    return gv * gl;
}

// How much light reflects instead of entering the surface (Fresnel, Schlick's approximation).
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
}

// Light arriving from direction `l` with brightness `radiance`, seen from direction `v`.
fn shade(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, radiance: vec3<f32>,
         albedo: vec3<f32>, roughness: f32, metallic: f32) -> vec3<f32> {
    let n_dot_l = max(dot(n, l), 0.0);
    if n_dot_l <= 0.0 {
        return vec3<f32>(0.0);
    }
    let h = normalize(v + l);
    let n_dot_v = max(dot(n, v), 1e-4);
    let n_dot_h = max(dot(n, h), 0.0);
    let h_dot_v = max(dot(h, v), 0.0);

    let f0 = mix(vec3<f32>(0.04), albedo, metallic);
    let f = fresnel_schlick(h_dot_v, f0);
    let specular = distribution_ggx(n_dot_h, roughness) * geometry_smith(n_dot_v, n_dot_l, roughness) * f
        / (4.0 * n_dot_v * n_dot_l + 1e-4);
    let diffuse = (vec3<f32>(1.0) - f) * (1.0 - metallic) * albedo / PI;

    return (diffuse + specular) * radiance * n_dot_l;
}

// 1 = fully lit by the sun, 0 = fully shadowed.
fn sun_visibility(world_pos: vec3<f32>, n: vec3<f32>) -> f32 {
    if shadow.settings.y < 0.5 {
        return 1.0;
    }

    // Sample from a point nudged along the normal, to avoid self-shadowing.
    let offset_pos = world_pos + n * shadow.settings.w;
    let clip = shadow.light_view_proj * vec4<f32>(offset_pos, 1.0);
    let ndc = clip.xyz / clip.w;
    // NDC y is up, texture v is down.
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);

    // Outside the shadow region: lit.
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z > 1.0 {
        return 1.0;
    }

    // 3x3 PCF: average nine hardware-filtered comparisons for soft edges.
    let depth = ndc.z - shadow.settings.x;
    let texel = shadow.settings.z;
    var lit = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            lit += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, depth);
        }
    }
    return lit / 9.0;
}

// --- Fragment ------------------------------------------------------------

@fragment
fn fs_main(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let base = textureSample(base_color_texture, base_color_sampler, in.uv) * material.base_color;
    if base.a < material.alpha_cutoff {
        discard;
    }
    let albedo = base.rgb;
    let roughness = clamp(material.roughness, 0.04, 1.0);
    let metallic = clamp(material.metallic, 0.0, 1.0);

    // Back faces of double-sided surfaces face the other way.
    let n = normalize(select(-in.normal, in.normal, front_facing));
    let v = normalize(camera.position.xyz - in.world_pos);

    // Sun
    // Sun, shadowed.
var color = shade(n, v, lights.sun_direction.xyz, lights.sun_color.rgb, albedo, roughness, metallic)
          * sun_visibility(in.world_pos, n);

    // Point lights
    for (var i = 0u; i < min(lights.counts.x, MAX_POINT_LIGHTS); i++) {
        let light = lights.points[i];
        let to_light = light.position_range.xyz - in.world_pos;
        let distance = length(to_light);
        let range = light.position_range.w;
        // Inverse-square falloff, smoothly reaching zero at `range`.
        let window = pow(clamp(1.0 - pow(distance / range, 4.0), 0.0, 1.0), 2.0);
        let attenuation = window / (distance * distance + 1.0);
        let l = to_light / max(distance, 1e-4);
        color += shade(n, v, l, light.color_intensity.rgb * attenuation, albedo, roughness, metallic);
    }

    // Hemisphere ambient: sky from above, ground from below (Z-up).
    let hemisphere = mix(lights.ground_color.rgb, lights.sky_color.rgb, n.z * 0.5 + 0.5);
    let f0 = mix(vec3<f32>(0.04), albedo, metallic);
    color += hemisphere * (albedo * (1.0 - metallic) + f0 * metallic);

    return vec4<f32>(color, base.a);
}