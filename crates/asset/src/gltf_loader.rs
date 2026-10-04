use std::f32::consts::FRAC_PI_2;
use std::path::Path;

use gfx::glam::{Mat4, Quat, Vec3, Vec4};
use gfx::{GpuContext, Mesh, SkinnedVertex, Texture, TextureOptions, Transform, Vertex};

use crate::animation::{Animation, Channel, Interpolation, Keyframes};
use crate::model::{
    AlphaMode, Model, ModelMaterial, ModelNode, ModelPart, Skin, TextureFilter, TextureWrap,
};

pub(crate) fn load(gpu: &GpuContext, path: &Path) -> Result<Model, gltf::Error> {
    let (document, buffers, images) = gltf::import(path)?;

    // Images become sRGB textures (they're only used as base color for now).
    let textures: Vec<Texture> = images
        .iter()
        .map(|image| image_to_texture(gpu, image))
        .collect();

    // Materials, plus a default one at the end for primitives without a material.
    let mut materials: Vec<ModelMaterial> = document
        .materials()
        .map(|material| {
            let pbr = material.pbr_metallic_roughness();
            let texture_info = pbr.base_color_texture();

            let (filter, wrap) = match &texture_info {
                Some(info) => {
                    let sampler = info.texture().sampler();
                    let filter = match sampler.mag_filter() {
                        Some(gltf::texture::MagFilter::Nearest) => TextureFilter::Nearest,
                        _ => TextureFilter::Linear,
                    };
                    let wrap = match sampler.wrap_s() {
                        gltf::texture::WrappingMode::ClampToEdge => TextureWrap::Clamp,
                        _ => TextureWrap::Repeat,
                    };
                    (filter, wrap)
                }
                None => (TextureFilter::Linear, TextureWrap::Repeat),
            };

            ModelMaterial {
                base_color: Vec4::from_array(pbr.base_color_factor()),
                base_color_texture: texture_info
                    .map(|info| textures[info.texture().source().index()].clone()),
                roughness: pbr.roughness_factor(),
                metallic: pbr.metallic_factor(),
                alpha_mode: match material.alpha_mode() {
                    gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                    gltf::material::AlphaMode::Mask => AlphaMode::Mask {
                        cutoff: material.alpha_cutoff().unwrap_or(0.5),
                    },
                    gltf::material::AlphaMode::Blend => AlphaMode::Blend,
                },
                double_sided: material.double_sided(),
                filter,
                wrap,
            }
        })
        .collect();
    let default_material = materials.len();
    materials.push(ModelMaterial::default());

    // Each glTF mesh becomes a list of (gfx mesh, material index, is skinned), one per primitive.
    let meshes: Vec<Vec<(Mesh, usize, bool)>> = document
        .meshes()
        .map(|mesh| {
            mesh.primitives()
                .filter_map(|primitive| {
                    let (gpu_mesh, skinned) = primitive_to_mesh(gpu, &primitive, &buffers)?;
                    let material = primitive.material().index().unwrap_or(default_material);
                    Some((gpu_mesh, material, skinned))
                })
                .collect()
        })
        .collect();

    // Node 0 converts glTF's Y-up to the engine's Z-up: +90° around X maps (x, y, z) to (x, -z, y).
    let mut nodes = vec![ModelNode {
        name: Some("root".into()),
        parent: None,
        transform: Transform {
            rotation: Quat::from_rotation_x(FRAC_PI_2),
            ..Default::default()
        },
    }];
    // glTF node index -> our node index (None for nodes not in the scene).
    let mut node_map = vec![None; document.nodes().count()];

    if let Some(scene) = document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        for node in scene.nodes() {
            add_node(node, 0, &mut nodes, &mut node_map);
        }
    }

    // Skins: joints as our node indices, plus their inverse bind matrices.
    let skins: Vec<Skin> = document
        .skins()
        .map(|skin| {
            let reader = skin.reader(|buffer| Some(&buffers[buffer.index()]));
            let joint_count = skin.joints().count();
            let inverse_bind = match reader.read_inverse_bind_matrices() {
                Some(matrices) => matrices.map(|m| Mat4::from_cols_array_2d(&m)).collect(),
                None => vec![Mat4::IDENTITY; joint_count],
            };
            let joints = skin
                .joints()
                .map(|joint| node_map[joint.index()].unwrap_or(0))
                .collect();
            Skin {
                joints,
                inverse_bind,
            }
        })
        .collect();

    // Parts: every primitive of every mesh node, attached to that node (and its skin, if skinned).
    let mut parts = Vec::new();
    for node in document.nodes() {
        let (Some(our_index), Some(mesh)) = (node_map[node.index()], node.mesh()) else {
            continue;
        };
        let skin = node.skin().map(|s| s.index());
        for (gpu_mesh, material, skinned) in &meshes[mesh.index()] {
            let part_skin = match (skinned, skin) {
                (true, Some(skin)) => Some(skin),
                (true, None) => {
                    tracing::warn!("skinned mesh on a node without a skin; skipping");
                    continue;
                }
                (false, _) => None,
            };
            parts.push(ModelPart {
                mesh: gpu_mesh.clone(),
                material: *material,
                node: our_index,
                skin: part_skin,
            });
        }
    }

    let animations = document
        .animations()
        .map(|animation| load_animation(&animation, &buffers, &node_map))
        .collect();

    Ok(Model {
        nodes,
        parts,
        materials,
        animations,
        skins,
    })
}

/// Depth-first: a node is pushed before its children, so parents always come first.
fn add_node(
    node: gltf::Node,
    parent: usize,
    nodes: &mut Vec<ModelNode>,
    node_map: &mut [Option<usize>],
) {
    let (translation, rotation, scale) = node.transform().decomposed();
    let index = nodes.len();
    nodes.push(ModelNode {
        name: node.name().map(String::from),
        parent: Some(parent),
        transform: Transform {
            translation: Vec3::from_array(translation),
            rotation: Quat::from_array(rotation),
            scale: Vec3::from_array(scale),
        },
    });
    node_map[node.index()] = Some(index);
    for child in node.children() {
        add_node(child, index, nodes, node_map);
    }
}

fn load_animation(
    animation: &gltf::Animation,
    buffers: &[gltf::buffer::Data],
    node_map: &[Option<usize>],
) -> Animation {
    use gltf::animation::util::ReadOutputs;

    let channels: Vec<Channel> = animation
        .channels()
        .filter_map(|channel| {
            let node = node_map[channel.target().node().index()]?;
            let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));
            let times: Vec<f32> = reader.read_inputs()?.collect();
            if times.is_empty() {
                return None;
            }

            // Cubic splines store (in-tangent, value, out-tangent) per key: keep the values only.
            let (interpolation, stride, offset) = match channel.sampler().interpolation() {
                gltf::animation::Interpolation::Step => (Interpolation::Step, 1, 0),
                gltf::animation::Interpolation::Linear => (Interpolation::Linear, 1, 0),
                gltf::animation::Interpolation::CubicSpline => {
                    tracing::debug!("cubic spline animation approximated as linear");
                    (Interpolation::Linear, 3, 1)
                }
            };

            let keyframes = match reader.read_outputs()? {
                ReadOutputs::Translations(values) => Keyframes::Translation(
                    values
                        .skip(offset)
                        .step_by(stride)
                        .map(Vec3::from_array)
                        .collect(),
                ),
                ReadOutputs::Rotations(values) => Keyframes::Rotation(
                    values
                        .into_f32()
                        .skip(offset)
                        .step_by(stride)
                        .map(Quat::from_array)
                        .collect(),
                ),
                ReadOutputs::Scales(values) => Keyframes::Scale(
                    values
                        .skip(offset)
                        .step_by(stride)
                        .map(Vec3::from_array)
                        .collect(),
                ),
                ReadOutputs::MorphTargetWeights(_) => {
                    tracing::warn!("morph target animations are not supported; skipping channel");
                    return None;
                }
            };

            Some(Channel {
                node,
                times,
                keyframes,
                interpolation,
            })
        })
        .collect();

    let duration = channels
        .iter()
        .filter_map(|c| c.times.last().copied())
        .fold(0.0, f32::max);

    Animation {
        name: animation.name().map(String::from),
        duration,
        channels,
    }
}

/// Convert one primitive to a GPU mesh. Returns whether it has skinning data (joints and weights).
fn primitive_to_mesh(
    gpu: &GpuContext,
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
) -> Option<(Mesh, bool)> {
    if primitive.mode() != gltf::mesh::Mode::Triangles {
        tracing::warn!(mode = ?primitive.mode(), "skipping non-triangle primitive");
        return None;
    }

    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

    let positions: Vec<[f32; 3]> = match reader.read_positions() {
        Some(positions) => positions.collect(),
        None => {
            tracing::warn!("skipping primitive without positions");
            return None;
        }
    };
    let count = positions.len();

    let normals: Vec<[f32; 3]> = match reader.read_normals() {
        Some(normals) => normals.collect(),
        None => {
            tracing::warn!("primitive has no normals; lighting will look flat");
            vec![[0.0, 1.0, 0.0]; count]
        }
    };
    let uvs: Vec<[f32; 2]> = match reader.read_tex_coords(0) {
        Some(uvs) => uvs.into_f32().collect(),
        None => vec![[0.0, 0.0]; count],
    };
    let indices: Vec<u32> = match reader.read_indices() {
        Some(indices) => indices.into_u32().collect(),
        None => (0..count as u32).collect(),
    };

    let joints: Option<Vec<[u16; 4]>> = reader.read_joints(0).map(|j| j.into_u16().collect());
    let weights: Option<Vec<[f32; 4]>> = reader.read_weights(0).map(|w| w.into_f32().collect());

    if let (Some(joints), Some(weights)) = (joints, weights) {
        let vertices: Vec<SkinnedVertex> = (0..count)
            .map(|i| SkinnedVertex {
                position: positions[i],
                normal: normals[i],
                uv: uvs[i],
                joints: joints[i],
                weights: weights[i],
            })
            .collect();
        return Some((Mesh::new(gpu, &vertices, &indices), true));
    }

    let vertices: Vec<Vertex> = positions
        .iter()
        .zip(&normals)
        .zip(&uvs)
        .map(|((&position, &normal), &uv)| Vertex {
            position,
            normal,
            uv,
        })
        .collect();
    Some((Mesh::new(gpu, &vertices, &indices), false))
}

/// Convert a decoded glTF image to an RGBA8 sRGB texture.
fn image_to_texture(gpu: &GpuContext, image: &gltf::image::Data) -> Texture {
    use gltf::image::Format;

    let pixels: Vec<u8> = match image.format {
        Format::R8G8B8A8 => image.pixels.clone(),
        Format::R8G8B8 => image
            .pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        Format::R8G8 => image
            .pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        Format::R8 => image.pixels.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        other => {
            tracing::warn!(format = ?other, "unsupported glTF image format; using white");
            vec![255; (image.width * image.height * 4) as usize]
        }
    };

    Texture::from_rgba8(
        gpu,
        image.width,
        image.height,
        &pixels,
        TextureOptions::default(),
    )
}
