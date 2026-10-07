//! PBR lighting: roughness and metalness, the sun, point lights, HDR.
//! Run with: cargo run -p examples --example lighting

#[allow(dead_code)]
mod common;

use common::OrbitCamera;
use gfx::glam::{Vec3, Vec4};
use render::{Material, MaterialId, MeshId, MeshRenderer, PointLight, ToneMapper};
use runtime::{Context, Game};

struct LightingExample {
    meshes: MeshRenderer,
    tonemapper: ToneMapper,
    camera: OrbitCamera,
    sphere: MeshId,
    quad: MeshId,
    floor: MaterialId,
    /// Top row: non-metals, bottom row: metals. Rough to the right.
    sphere_materials: Vec<(MaterialId, Vec3)>,
}

impl Game for LightingExample {
    fn config() -> runtime::Config {
        common::config("lighting")
    }

    fn init(ctx: &mut Context) -> anyhow::Result<Self> {
        let gpu = &ctx.gpu;
        let mut meshes = MeshRenderer::with_format(gpu, ToneMapper::HDR_FORMAT);

        let sphere = meshes.add_mesh(gfx::Mesh::sphere(gpu, 48, 24));
        let quad = meshes.add_mesh(gfx::Mesh::quad(gpu));
        let floor = meshes.add_material(
            gpu,
            Material {
                base_color: Vec4::new(0.5, 0.5, 0.5, 1.0),
                roughness: 0.9,
                ..Default::default()
            },
        );

        let mut sphere_materials = Vec::new();
        for (row, metallic) in [(0, 0.0), (1, 1.0)] {
            for i in 0..5 {
                let material = meshes.add_material(
                    gpu,
                    Material {
                        base_color: Vec4::new(0.9, 0.6, 0.3, 1.0),
                        roughness: 0.1 + i as f32 * 0.2,
                        metallic,
                        ..Default::default()
                    },
                );
                let position = Vec3::new((i as f32 - 2.0) * 1.2, 0.0, 0.5 + row as f32 * 1.3);
                sphere_materials.push((material, position));
            }
        }

        let (w, h) = gpu.size();
        Ok(Self {
            meshes,
            tonemapper: ToneMapper::new(gpu, w, h, gpu.surface_format()),
            camera: OrbitCamera::new(Vec3::new(0.0, 0.0, 1.0), 7.0),
            sphere,
            quad,
            floor,
            sphere_materials,
        })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        common::standard_keys(ctx);
        self.camera.update(ctx, dt);
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut gfx::Frame) {
        let gpu = &ctx.gpu;
        let (w, h) = gpu.size();

        // Spheres and floor
        for &(material, position) in &self.sphere_materials {
            self.meshes.draw(
                self.sphere,
                material,
                &gfx::Transform::from_translation(position),
            );
        }
        self.meshes.draw(
            self.quad,
            self.floor,
            &gfx::Transform {
                scale: Vec3::splat(14.0),
                ..Default::default()
            },
        );

        // Two colored lights circling the spheres
        let t = ctx.time.elapsed() as f32;
        for (phase, color) in [
            (0.0, Vec3::new(1.0, 0.3, 0.2)),
            (std::f32::consts::PI, Vec3::new(0.2, 0.5, 1.0)),
        ] {
            self.meshes.draw_light(&PointLight {
                position: Vec3::new((t + phase).cos() * 3.0, (t + phase).sin() * 2.0, 1.2),
                color,
                intensity: 10.0,
                range: 6.0,
            });
        }

        // Scene in HDR, with shadows, then tone mapped to the screen.
        self.meshes
            .prepare(gpu, &gfx::View::new(&self.camera.camera(), w, h));
        self.meshes.render_shadows(frame);
        self.tonemapper.resize(gpu, w, h);
        self.tonemapper.prepare(gpu);
        {
            let mut pass = frame.clear_pass_to(self.tonemapper.target(), gfx::Color::BLACK);
            self.meshes.render(&mut pass);
        }
        let mut pass = frame.clear_pass(gfx::Color::BLACK);
        self.tonemapper.draw(&mut pass);
    }
}

fn main() -> anyhow::Result<()> {
    runtime::run::<LightingExample>()
}
