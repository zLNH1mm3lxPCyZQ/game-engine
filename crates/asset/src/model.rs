use gfx::Transform;
use gfx::glam::{Mat4, Vec4};

use crate::animation::Animation;

/// How a texture is sampled between texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextureFilter {
    /// Smooth: photos, painted textures.
    #[default]
    Linear,
    /// Hard pixels: pixel art, low-resolution textures.
    Nearest,
}

/// What happens to UVs outside 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextureWrap {
    /// Tile the texture (glTF's default).
    #[default]
    Repeat,
    /// Stretch the edge pixels.
    Clamp,
}

/// A model loaded from a file, independent of any renderer.
#[derive(Clone)]
pub struct Model {
    /// The node hierarchy, ordered so every parent comes before its children.
    /// Node 0 is a root that converts the file's Y-up axes to the engine's Z-up.
    pub nodes: Vec<ModelNode>,
    pub parts: Vec<ModelPart>,
    pub materials: Vec<ModelMaterial>,
    pub animations: Vec<Animation>,
    pub skins: Vec<Skin>,
}

#[derive(Clone, Debug)]
pub struct ModelNode {
    pub name: Option<String>,
    pub parent: Option<usize>,
    /// Rest transform, relative to the parent.
    pub transform: Transform,
}

/// One drawable piece: a mesh with one material, attached to a node.
#[derive(Clone)]
pub struct ModelPart {
    pub mesh: gfx::Mesh,
    /// Index into `Model::materials`.
    pub material: usize,
    /// Index into `Model::nodes`.
    pub node: usize,
    /// Index into `Model::skins` if this part is skinned. Skinned parts ignore their node's transform.
    pub skin: Option<usize>,
}

/// How a material's alpha is used. Mirrors glTF.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum AlphaMode {
    /// Alpha is ignored.
    #[default]
    Opaque,
    /// Pixels with alpha below `cutoff` are discarded; the rest are opaque.
    Mask { cutoff: f32 },
    /// Alpha blends with what's behind. Drawn after opaque objects, back to front.
    Blend,
}

/// How a part looks, described with plain data.
#[derive(Clone)]
pub struct ModelMaterial {
    pub base_color: Vec4,
    pub base_color_texture: Option<gfx::Texture>,
    pub roughness: f32,
    pub metallic: f32,
    pub alpha_mode: AlphaMode,
    pub double_sided: bool,
    pub filter: TextureFilter,
    pub wrap: TextureWrap,
}

impl Default for ModelMaterial {
    fn default() -> Self {
        Self {
            base_color: Vec4::ONE,
            base_color_texture: None,
            roughness: 0.5,
            metallic: 0.0,
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            filter: TextureFilter::Linear,
            wrap: TextureWrap::Repeat,
        }
    }
}

/// The state of a model's hierarchy at one moment.
#[derive(Clone, Debug)]
pub struct Pose {
    /// Each node's transform relative to its parent.
    pub locals: Vec<Transform>,
    /// Each node's transform relative to the model's origin. Updated by `update_worlds`.
    pub worlds: Vec<Mat4>,
}

impl Model {
    pub fn animation_index(&self, name: &str) -> Option<usize> {
        self.animations
            .iter()
            .position(|a| a.name.as_deref() == Some(name))
    }
    /// The pose where every node is at its rest transform.
    pub fn rest_pose(&self) -> Pose {
        let mut pose = Pose {
            locals: self.nodes.iter().map(|n| n.transform).collect(),
            worlds: vec![Mat4::IDENTITY; self.nodes.len()],
        };
        pose.update_worlds(self);
        pose
    }

    pub fn node_index(&self, name: &str) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.name.as_deref() == Some(name))
    }

    pub fn animation(&self, name: &str) -> Option<&Animation> {
        self.animations
            .iter()
            .find(|a| a.name.as_deref() == Some(name))
    }
}

impl Pose {
    pub fn blend(&mut self, other: &Pose, weight: f32) {
        for (local, target) in self.locals.iter_mut().zip(&other.locals) {
            local.translation = local.translation.lerp(target.translation, weight);
            local.rotation = local.rotation.slerp(target.rotation, weight);
            local.scale = local.scale.lerp(target.scale, weight);
        }
    }
    /// Put every node back at its rest transform.
    pub fn reset(&mut self, model: &Model) {
        for (local, node) in self.locals.iter_mut().zip(&model.nodes) {
            *local = node.transform;
        }
    }

    /// Recompute world transforms from local ones.
    /// Parents come before children, so a single forward pass is enough.
    pub fn update_worlds(&mut self, model: &Model) {
        for (i, node) in model.nodes.iter().enumerate() {
            let local = self.locals[i].matrix();
            self.worlds[i] = match node.parent {
                Some(parent) => self.worlds[parent] * local,
                None => local,
            };
        }
    }
}

/// Joints that deform a skinned mesh.
#[derive(Clone, Debug)]
pub struct Skin {
    /// Index into `Model::nodes` for each joint, in the order vertices refer to them.
    pub joints: Vec<usize>,
    /// For each joint: from the mesh's space into the joint's space, in the bind pose.
    pub inverse_bind: Vec<Mat4>,
}

impl Skin {
    /// Append this skin's joint matrices for the given node world transforms.
    pub fn joint_matrices(&self, worlds: &[Mat4], out: &mut Vec<Mat4>) {
        out.extend(
            self.joints
                .iter()
                .zip(&self.inverse_bind)
                .map(|(&joint, inverse_bind)| worlds[joint] * *inverse_bind),
        );
    }
}
