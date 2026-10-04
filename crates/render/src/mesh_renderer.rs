use asset::{AlphaMode, TextureFilter, TextureWrap};
use gfx::glam::{Mat4, UVec4, Vec3, Vec4};
use gfx::{
    GpuContext, Mesh, PipelineBuilder, SkinnedVertex, StorageBuffer, Texture, Transform,
    UniformBuffer, Vertex, VertexFormat, View,
};

use crate::lighting::{Lighting, PointLight, ShadowSettings};
use crate::material::{Material, MaterialId};
use crate::shader_bindings::mesh as shader;
use crate::shader_bindings::shadow as shadow_shader;

/// Must match `MAX_POINT_LIGHTS` in mesh.wgsl.
pub const MAX_POINT_LIGHTS: usize = 16;
/// Size of the sun's shadow map, in texels per side.
pub const SHADOW_RESOLUTION: u32 = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(u32);

/// Which pipeline variant a material needs (skinning is decided per draw).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PipelineKey {
    blend: bool,
    double_sided: bool,
}

/// Index into `MeshRenderer::pipelines`: skinned * 4 + blend * 2 + double_sided.
fn pipeline_index(key: PipelineKey, skinned: bool) -> usize {
    (skinned as usize) * 4 + (key.blend as usize) * 2 + key.double_sided as usize
}

struct DrawCommand {
    mesh: MeshId,
    material: MaterialId,
    model: Mat4,
    skinned: bool,
    joint_offset: u32,
}

/// A draw that has been uploaded and is ready to record.
#[derive(Clone, Copy)]
struct PreparedDraw {
    mesh: MeshId,
    material: MaterialId,
    pipeline: usize,
}

impl PreparedDraw {
    fn skinned(&self) -> bool {
        self.pipeline & 4 != 0
    }

    fn blended(&self) -> bool {
        self.pipeline & 2 != 0
    }
}

/// A material's GPU side: its parameters buffer, bind group, and pipeline variant.
struct GpuMaterial {
    _params: UniformBuffer<shader::MaterialParams>,
    bind_group: shader::WgpuBindGroup2,
    key: PipelineKey,
}

struct RegisteredPart {
    mesh: MeshId,
    material: MaterialId,
    node: usize,
    skin: Option<usize>,
}

/// A model registered in this renderer: its parts, skins, and rest pose.
struct RegisteredModel {
    parts: Vec<RegisteredPart>,
    skins: Vec<asset::Skin>,
    /// World transform of each node at rest.
    rest: Vec<Mat4>,
}

fn objects_bind_group(
    gpu: &GpuContext,
    objects: &StorageBuffer<shader::Object>,
    joints: &StorageBuffer<Mat4>,
) -> shader::WgpuBindGroup1 {
    shader::WgpuBindGroup1::from_bindings(
        &gpu.device,
        shader::WgpuBindGroup1Entries::new(shader::WgpuBindGroup1EntriesParams {
            objects: objects.binding(),
            joints: joints.binding(),
        }),
    )
}

/// The shadow pass reads the same buffers through its own bind group type.
fn shadow_objects_bind_group(
    gpu: &GpuContext,
    objects: &StorageBuffer<shader::Object>,
    joints: &StorageBuffer<Mat4>,
) -> shadow_shader::WgpuBindGroup1 {
    shadow_shader::WgpuBindGroup1::from_bindings(
        &gpu.device,
        shadow_shader::WgpuBindGroup1Entries::new(shadow_shader::WgpuBindGroup1EntriesParams {
            objects: objects.binding(),
            joints: joints.binding(),
        }),
    )
}

/// Pack scene lighting and this frame's point lights into the shader's layout.
fn lights_uniform(lighting: &Lighting, points: &[PointLight]) -> shader::Lights {
    let mut packed = [shader::PointLight::new(Vec4::ZERO, Vec4::ZERO); MAX_POINT_LIGHTS];
    for (slot, light) in packed.iter_mut().zip(points) {
        *slot = shader::PointLight::new(
            light.position.extend(light.range),
            (light.color * light.intensity).extend(0.0),
        );
    }
    let count = points.len().min(MAX_POINT_LIGHTS) as u32;

    shader::Lights::new(
        (-lighting.sun.direction.normalize()).extend(0.0),
        (lighting.sun.color * lighting.sun.intensity).extend(0.0),
        lighting.sky_color.extend(0.0),
        lighting.ground_color.extend(0.0),
        UVec4::new(count, 0, 0, 0),
        packed,
    )
}

/// The sun's orthographic camera over the shadowed region, snapped to whole texels.
fn light_view_proj(sun_direction: Vec3, settings: &ShadowSettings) -> Mat4 {
    let direction = sun_direction.normalize();
    let radius = settings.radius;
    // look_at breaks down if `up` is parallel to the view direction.
    let up = if direction.z.abs() > 0.99 {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let eye = settings.center - direction * radius * 2.0;

    let view = gfx::glam::camera::rh::view::look_at_mat4(eye, settings.center, up);
    let projection = gfx::glam::camera::rh::proj::directx::orthographic(
        -radius,
        radius,
        -radius,
        radius,
        0.0,
        radius * 4.0,
    );
    let view_proj = projection * view;

    // Snap so the texel grid stays fixed in the world: no shimmering when the center moves.
    let texel = 2.0 / SHADOW_RESOLUTION as f32; // NDC units per texel
    let origin = view_proj.project_point3(Vec3::ZERO).truncate();
    let snapped = (origin / texel).round() * texel;
    Mat4::from_translation((snapped - origin).extend(0.0)) * view_proj
}

fn shadow_uniform(lighting: &Lighting, settings: &ShadowSettings) -> shader::ShadowParams {
    let texel_world = 2.0 * settings.radius / SHADOW_RESOLUTION as f32;
    shader::ShadowParams::new(
        light_view_proj(lighting.sun.direction, settings),
        Vec4::new(
            0.0005,                                   // depth bias
            if settings.enabled { 1.0 } else { 0.0 }, // enabled
            1.0 / SHADOW_RESOLUTION as f32,           // texel size in UV
            texel_world * 1.5,                        // normal offset, in world units
        ),
    )
}

/// All eight main pipeline variants, indexed by `pipeline_index`.
fn build_pipelines(gpu: &GpuContext, format: wgpu::TextureFormat) -> [wgpu::RenderPipeline; 8] {
    let module = shader::create_shader_module_embed_source(&gpu.device);
    let layout = shader::create_pipeline_layout(&gpu.device);

    let build = |skinned: bool, blend: bool, double_sided: bool| {
        let mut builder = PipelineBuilder::new(gpu, &module, &layout)
            .label("meshes")
            .target(format);
        builder = if skinned {
            builder
                .entry_points("vs_skinned", Some("fs_main"))
                .vertex::<SkinnedVertex>()
        } else {
            builder.vertex::<Vertex>()
        };
        if !double_sided {
            builder = builder.cull_back();
        }
        builder = if blend {
            builder
                .blend(wgpu::BlendState::ALPHA_BLENDING)
                .depth_read_only()
        } else {
            builder.depth_test()
        };
        builder.build()
    };

    std::array::from_fn(|i| build(i & 4 != 0, i & 2 != 0, i & 1 != 0))
}

/// Depth-only pipelines for the shadow pass: [static, skinned].
fn build_shadow_pipelines(gpu: &GpuContext) -> [wgpu::RenderPipeline; 2] {
    let module = shadow_shader::create_shader_module_embed_source(&gpu.device);
    let layout = shadow_shader::create_pipeline_layout(&gpu.device);

    let build = |skinned: bool| {
        let builder = PipelineBuilder::new(gpu, &module, &layout).label("shadows");
        let builder = if skinned {
            builder
                .entry_points("vs_skinned", None)
                .vertex::<SkinnedVertex>()
        } else {
            builder.entry_points("vs_main", None).vertex::<Vertex>()
        };
        builder.depth_test().depth_bias(2, 2.0).build()
    };

    [build(false), build(true)]
}

/// Draws 3D meshes with PBR materials, lights, skinning, and sun shadows.
pub struct MeshRenderer {
    pipelines: [wgpu::RenderPipeline; 8],
    camera_ubo: UniformBuffer<shader::Camera>,
    lights_ubo: UniformBuffer<shader::Lights>,
    view_bind_group: shader::WgpuBindGroup0,
    objects: StorageBuffer<shader::Object>,
    joints: StorageBuffer<Mat4>,
    objects_bind_group: shader::WgpuBindGroup1,

    shadow_pipelines: [wgpu::RenderPipeline; 2],
    shadow_map: Texture,
    shadow_params: UniformBuffer<shader::ShadowParams>,
    /// Group 3 of the main pass: shadow map, comparison sampler, params.
    shadow_bind_group: shader::WgpuBindGroup3,
    /// Group 0 of the shadow pass: the sun's camera.
    shadow_view_bind_group: shadow_shader::WgpuBindGroup0,
    shadow_objects_bind_group: shadow_shader::WgpuBindGroup1,
    shadow_settings: ShadowSettings,

    meshes: Vec<Mesh>,
    textures: Vec<Texture>,
    materials: Vec<GpuMaterial>,
    models: Vec<RegisteredModel>,
    white_texture: TextureId,
    default_material: MaterialId,
    lighting: Lighting,
    point_lights: Vec<PointLight>,
    too_many_lights_warned: bool,
    draws: Vec<DrawCommand>,
    /// This frame's joint matrices for all skinned draws.
    joint_data: Vec<Mat4>,
    prepared: Vec<PreparedDraw>,
}

impl MeshRenderer {
    /// A renderer drawing into targets of the surface format.
    pub fn new(gpu: &GpuContext) -> Self {
        Self::with_format(gpu, gpu.surface_format())
    }

    /// A renderer drawing into targets of the given format (for example HDR).
    pub fn with_format(gpu: &GpuContext, format: wgpu::TextureFormat) -> Self {
        let lighting = Lighting::default();
        let shadow_settings = ShadowSettings::default();

        let camera_ubo = UniformBuffer::new(gpu, &shader::Camera::new(Mat4::IDENTITY, Vec4::ZERO));
        let lights_ubo = UniformBuffer::new(gpu, &lights_uniform(&lighting, &[]));
        let view_bind_group = shader::WgpuBindGroup0::from_bindings(
            &gpu.device,
            shader::WgpuBindGroup0Entries::new(shader::WgpuBindGroup0EntriesParams {
                camera: camera_ubo.binding(),
                lights: lights_ubo.binding(),
            }),
        );

        let objects = StorageBuffer::new(gpu, "mesh objects", 64);
        let joints = StorageBuffer::new(gpu, "joint matrices", 64);
        let objects_bind_group = objects_bind_group(gpu, &objects, &joints);
        let shadow_objects_bind_group = shadow_objects_bind_group(gpu, &objects, &joints);

        // Shadow map and its sampling setup.
        let shadow_map = Texture::depth(gpu, SHADOW_RESOLUTION, SHADOW_RESOLUTION);
        let shadow_sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow comparison"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_params = UniformBuffer::new(gpu, &shadow_uniform(&lighting, &shadow_settings));
        let shadow_bind_group = shader::WgpuBindGroup3::from_bindings(
            &gpu.device,
            shader::WgpuBindGroup3Entries::new(shader::WgpuBindGroup3EntriesParams {
                shadow: shadow_params.binding(),
                shadow_map: &shadow_map.view,
                shadow_sampler: &shadow_sampler,
            }),
        );
        let shadow_view_bind_group = shadow_shader::WgpuBindGroup0::from_bindings(
            &gpu.device,
            shadow_shader::WgpuBindGroup0Entries::new(shadow_shader::WgpuBindGroup0EntriesParams {
                shadow: shadow_params.binding(),
            }),
        );

        let mut renderer = Self {
            pipelines: build_pipelines(gpu, format),
            camera_ubo,
            lights_ubo,
            view_bind_group,
            objects,
            joints,
            objects_bind_group,
            shadow_pipelines: build_shadow_pipelines(gpu),
            shadow_map,
            shadow_params,
            shadow_bind_group,
            shadow_view_bind_group,
            shadow_objects_bind_group,
            shadow_settings,
            meshes: Vec::new(),
            textures: Vec::new(),
            materials: Vec::new(),
            models: Vec::new(),
            white_texture: TextureId(0),
            default_material: MaterialId(0),
            lighting,
            point_lights: Vec::new(),
            too_many_lights_warned: false,
            draws: Vec::new(),
            joint_data: Vec::new(),
            prepared: Vec::new(),
        };

        // Built-in resources: always at index 0.
        let white = Texture::from_rgba8(gpu, 1, 1, &[255, 255, 255, 255], Default::default());
        renderer.white_texture = renderer.add_texture(&white);
        renderer.default_material = renderer.add_material(gpu, Material::default());

        renderer
    }

    // ---------------------------------------------------------------------
    // Resources
    // ---------------------------------------------------------------------

    pub fn add_mesh(&mut self, mesh: Mesh) -> MeshId {
        self.meshes.push(mesh);
        MeshId(self.meshes.len() as u32 - 1)
    }

    /// Replace a mesh's contents. Its `MeshId` stays valid.
    pub fn update_mesh<V: VertexFormat>(
        &mut self,
        gpu: &GpuContext,
        mesh: MeshId,
        vertices: &[V],
        indices: &[u32],
    ) {
        self.meshes[mesh.0 as usize].update(gpu, vertices, indices);
    }

    pub fn add_texture(&mut self, texture: &Texture) -> TextureId {
        self.textures.push(texture.clone());
        TextureId(self.textures.len() as u32 - 1)
    }

    pub fn add_material(&mut self, gpu: &GpuContext, material: Material) -> MaterialId {
        let texture_id = material.base_color_texture.unwrap_or(self.white_texture);
        let texture = &self.textures[texture_id.0 as usize];

        let alpha_cutoff = match material.alpha_mode {
            AlphaMode::Mask { cutoff } => cutoff,
            AlphaMode::Opaque | AlphaMode::Blend => 0.0,
        };
        let params = UniformBuffer::new(
            gpu,
            &shader::MaterialParams::new(
                material.base_color,
                material.roughness,
                material.metallic,
                alpha_cutoff,
            ),
        );
        let bind_group = shader::WgpuBindGroup2::from_bindings(
            &gpu.device,
            shader::WgpuBindGroup2Entries::new(shader::WgpuBindGroup2EntriesParams {
                material: params.binding(),
                base_color_texture: &texture.view,
                base_color_sampler: gpu.samplers.get(
                    material.filter == TextureFilter::Nearest,
                    material.wrap == TextureWrap::Repeat,
                ),
            }),
        );
        let key = PipelineKey {
            blend: material.alpha_mode == AlphaMode::Blend,
            double_sided: material.double_sided,
        };

        self.materials.push(GpuMaterial {
            _params: params,
            bind_group,
            key,
        });
        MaterialId(self.materials.len() as u32 - 1)
    }

    /// Register a loaded model: its meshes, textures, materials, and skins.
    pub fn add_model(&mut self, gpu: &GpuContext, model: &asset::Model) -> ModelId {
        let materials: Vec<MaterialId> = model
            .materials
            .iter()
            .map(|m| {
                let texture = m.base_color_texture.as_ref().map(|t| self.add_texture(t));
                self.add_material(
                    gpu,
                    Material {
                        base_color: m.base_color,
                        base_color_texture: texture,
                        roughness: m.roughness,
                        metallic: m.metallic,
                        alpha_mode: m.alpha_mode,
                        double_sided: m.double_sided,
                        filter: m.filter,
                        wrap: m.wrap,
                    },
                )
            })
            .collect();

        let parts = model
            .parts
            .iter()
            .map(|part| RegisteredPart {
                mesh: self.add_mesh(part.mesh.clone()),
                material: materials[part.material],
                node: part.node,
                skin: part.skin,
            })
            .collect();

        self.models.push(RegisteredModel {
            parts,
            skins: model.skins.clone(),
            rest: model.rest_pose().worlds,
        });
        ModelId(self.models.len() as u32 - 1)
    }

    /// A plain white material, always available.
    pub fn default_material(&self) -> MaterialId {
        self.default_material
    }

    /// Set the sun and ambient light. Persists until changed.
    pub fn set_lighting(&mut self, lighting: Lighting) {
        self.lighting = lighting;
    }

    pub fn lighting(&self) -> &Lighting {
        &self.lighting
    }

    /// Set where sun shadows are computed. Persists until changed.
    pub fn set_shadows(&mut self, settings: ShadowSettings) {
        self.shadow_settings = settings;
    }

    pub fn shadows(&self) -> &ShadowSettings {
        &self.shadow_settings
    }

    // ---------------------------------------------------------------------
    // Frame
    // ---------------------------------------------------------------------

    /// Queue a mesh to be drawn this frame.
    pub fn draw(&mut self, mesh: MeshId, material: MaterialId, transform: &Transform) {
        self.draws.push(DrawCommand {
            mesh,
            material,
            model: transform.matrix(),
            skinned: false,
            joint_offset: 0,
        });
    }

    /// Queue every part of a model in its rest pose, placed by `transform`.
    pub fn draw_model(&mut self, model: ModelId, transform: &Transform) {
        self.queue_model(model, transform, None);
    }

    /// Queue every part of a model in a given pose, placed by `transform`.
    /// The pose must come from the same model (`asset::Model::rest_pose`).
    pub fn draw_model_posed(&mut self, model: ModelId, transform: &Transform, pose: &asset::Pose) {
        self.queue_model(model, transform, Some(&pose.worlds));
    }

    fn queue_model(&mut self, model: ModelId, transform: &Transform, posed: Option<&[Mat4]>) {
        let root = transform.matrix();
        let registered = &self.models[model.0 as usize];
        let worlds = posed.unwrap_or(&registered.rest);

        // Each skin's joint matrices, computed at most once for this draw.
        let mut skin_offsets: Vec<Option<u32>> = vec![None; registered.skins.len()];

        for part in &registered.parts {
            let command = match part.skin {
                Some(skin) => {
                    let offset = match skin_offsets[skin] {
                        Some(offset) => offset,
                        None => {
                            let offset = self.joint_data.len() as u32;
                            registered.skins[skin].joint_matrices(worlds, &mut self.joint_data);
                            skin_offsets[skin] = Some(offset);
                            offset
                        }
                    };
                    // Skinned parts ignore their node's transform: the joints place them.
                    DrawCommand {
                        mesh: part.mesh,
                        material: part.material,
                        model: root,
                        skinned: true,
                        joint_offset: offset,
                    }
                }
                None => DrawCommand {
                    mesh: part.mesh,
                    material: part.material,
                    model: root * worlds[part.node],
                    skinned: false,
                    joint_offset: 0,
                },
            };
            self.draws.push(command);
        }
    }

    /// Add a point light for this frame.
    pub fn draw_light(&mut self, light: &PointLight) {
        if self.point_lights.len() < MAX_POINT_LIGHTS {
            self.point_lights.push(*light);
        } else if !self.too_many_lights_warned {
            tracing::warn!("more than {MAX_POINT_LIGHTS} point lights; extra lights are ignored");
            self.too_many_lights_warned = true;
        }
    }

    /// Upload everything queued this frame. Call once per frame, before any pass.
    pub fn prepare(&mut self, gpu: &GpuContext, view: &View) {
        self.prepared.clear();

        // Per-view data: camera, lights, shadow camera.
        self.camera_ubo.write(
            gpu,
            &shader::Camera::new(view.view_proj(), view.position.extend(1.0)),
        );
        self.lights_ubo
            .write(gpu, &lights_uniform(&self.lighting, &self.point_lights));
        self.shadow_params
            .write(gpu, &shadow_uniform(&self.lighting, &self.shadow_settings));
        self.point_lights.clear();

        if self.draws.is_empty() {
            self.joint_data.clear();
            return;
        }

        // Opaque and masked first, blended after.
        let materials = &self.materials;
        let key_of = |d: &DrawCommand| materials[d.material.0 as usize].key;
        let (mut opaque, mut blended): (Vec<_>, Vec<_>) =
            self.draws.drain(..).partition(|d| !key_of(d).blend);

        // Opaque: group by pipeline, material, mesh, to minimize state changes.
        opaque.sort_by_key(|d| (pipeline_index(key_of(d), d.skinned), d.material, d.mesh));

        // Blended: farthest first, so nearer surfaces blend over farther ones.
        let eye = view.position;
        let distance = |d: &DrawCommand| (d.model.w_axis.truncate() - eye).length_squared();
        blended.sort_by(|a, b| distance(b).total_cmp(&distance(a)));

        self.draws.extend(opaque);
        self.draws.extend(blended);

        // Upload objects (in final order) and joint matrices.
        let objects: Vec<shader::Object> = self
            .draws
            .iter()
            .map(|d| {
                shader::Object::new(
                    d.model,
                    d.model.inverse().transpose(),
                    UVec4::new(d.joint_offset, 0, 0, 0),
                )
            })
            .collect();
        // Both writes must happen, so don't combine them with `||` (it would skip the second).
        let objects_grew = self.objects.write(gpu, &objects);
        let joints_grew = self.joints.write(gpu, &self.joint_data);
        if objects_grew || joints_grew {
            self.objects_bind_group = objects_bind_group(gpu, &self.objects, &self.joints);
            self.shadow_objects_bind_group =
                shadow_objects_bind_group(gpu, &self.objects, &self.joints);
        }
        self.joint_data.clear();

        // Keep only what recording needs.
        let materials = &self.materials;
        self.prepared
            .extend(self.draws.drain(..).map(|d| PreparedDraw {
                mesh: d.mesh,
                material: d.material,
                pipeline: pipeline_index(materials[d.material.0 as usize].key, d.skinned),
            }));
    }

    /// Render the sun's shadow map. Call after `prepare`, before passes that use `render`.
    pub fn render_shadows(&self, frame: &mut gfx::Frame) {
        if !self.shadow_settings.enabled {
            return;
        }
        let mut pass = frame.depth_pass(&self.shadow_map);
        if self.prepared.is_empty() {
            return; // the map is still cleared: nothing casts shadows
        }

        self.shadow_view_bind_group.set(&mut pass);
        self.shadow_objects_bind_group.set(&mut pass);

        let mut current_pipeline = None;
        for (i, draw) in self.prepared.iter().enumerate() {
            if draw.blended() {
                continue; // transparent objects don't cast shadows
            }
            let pipeline = draw.skinned() as usize;
            if current_pipeline != Some(pipeline) {
                pass.set_pipeline(&self.shadow_pipelines[pipeline]);
                current_pipeline = Some(pipeline);
            }
            let i = i as u32;
            self.meshes[draw.mesh.0 as usize].draw_instanced(&mut pass, i..i + 1);
        }
    }

    /// Record the prepared draws. Can be called in any number of passes.
    pub fn render(&self, pass: &mut wgpu::RenderPass) {
        if self.prepared.is_empty() {
            return;
        }

        // Bind groups stay bound across pipeline changes, since all variants share one layout.
        self.view_bind_group.set(pass);
        self.objects_bind_group.set(pass);
        self.shadow_bind_group.set(pass);

        let mut current_pipeline = None;
        let mut current_material = None;
        for (i, draw) in self.prepared.iter().enumerate() {
            if current_pipeline != Some(draw.pipeline) {
                pass.set_pipeline(&self.pipelines[draw.pipeline]);
                current_pipeline = Some(draw.pipeline);
            }
            if current_material != Some(draw.material) {
                self.materials[draw.material.0 as usize]
                    .bind_group
                    .set(pass);
                current_material = Some(draw.material);
            }
            let i = i as u32;
            self.meshes[draw.mesh.0 as usize].draw_instanced(pass, i..i + 1);
        }
    }
}
