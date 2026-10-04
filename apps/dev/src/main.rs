use gfx::glam::Vec2;
use gfx::glam::{Quat, Vec3, Vec4};
use render::{
    Blit, Material, MaterialId, MeshId, MeshRenderer, Sprite, SpriteRenderer, SpriteTextureId,
};
use runtime::{ActionMap, AxisBinding, Binding, Context, Game, KeyCode, MouseButton};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Action {
    TogglePixelated,
    ToggleProjection,
    ToggleFullscreen,
    Quit,
    CaptureMouse,
    ReleaseMouse,
    Orbit,

    ToggleWalk,
    ToggleShadows,
    CycleToneMap,
    Exposure,
}

struct Dev {
    low_res: gfx::RenderTarget,
    pixelated: bool,
    image_pass: Blit,
    upscale_pass: Blit,
    meshes: MeshRenderer,

    cube: MeshId,
    objects: Vec<gfx::Transform>,
    materials: [MaterialId; 2],

    actions: ActionMap<Action>,
    camera: gfx::Camera,
    orthographic: bool,
    yaw: f32,
    pitch: f32,
    distance: f32,

    sprites: SpriteRenderer,
    photo_sprite: SpriteTextureId,
    hud_angle: f32,

    font: render::Font,
    model: render::ModelId,
    sphere: MeshId,
    sphere_materials: Vec<MaterialId>,
    glass: MaterialId,
    leaves: MaterialId,
    quad: MeshId,

    animated: runtime::asset::Model,
    animated_id: render::ModelId,
    pose: runtime::asset::Pose,
    animator: runtime::asset::Animator,
    walking: bool,
    highlight: MaterialId,
    hovered: Option<usize>,
    ground_hit: Option<Vec3>,
    tonemapper: render::ToneMapper,

    floor: MaterialId,
}

impl Dev {
    fn draw_scene(&self, pass: &mut wgpu::RenderPass) {
        self.image_pass.draw(pass);
        self.meshes.render(pass);
    }
    /// The world view and the viewport it's shown in, for the current mode.
    fn world_view(&self, w: u32, h: u32) -> (gfx::View, gfx::Viewport) {
        if self.pixelated {
            (
                gfx::View::new(&self.camera, 320, 180),
                gfx::Viewport::fit_integer(w, h, 320, 180),
            )
        } else {
            (
                gfx::View::new(&self.camera, w, h),
                gfx::Viewport::full(w, h),
            )
        }
    }

    fn sphere_center(i: usize) -> Vec3 {
        Vec3::new((i as f32 - 2.0) * 1.2, 1.5, 0.0)
    }
}

impl Game for Dev {
    fn config() -> runtime::Config {
        runtime::Config {
            title: "engine dev".into(),
            vsync: false,

            assets_dir: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").into(),
            ..Default::default()
        }
    }

    fn init(ctx: &mut Context) -> anyhow::Result<Self> {
        let photo = ctx.assets.texture(&ctx.gpu, "test.jpg")?;
        let car = ctx.assets.model(&ctx.gpu, "car.glb")?;
        let gpu = &ctx.gpu;
        let animated = ctx.assets.model(&ctx.gpu, "CesiumMan.glb")?;

        // Background and low-res target
        let low_res = gfx::RenderTarget::new(gpu, 320, 180, gpu.surface_format());
        let image_pass = Blit::new(gpu, &photo, &gpu.samplers.linear);
        let upscale_pass = Blit::new(gpu, &low_res.color, &gpu.samplers.nearest);

        // Mesh renderer and resources
        let hdr = render::ToneMapper::HDR_FORMAT;
        let image_pass = Blit::with_format(gpu, &photo, &gpu.samplers.linear, hdr);
        let upscale_pass = Blit::new(gpu, &low_res.color, &gpu.samplers.nearest);

        let mut renderer = MeshRenderer::with_format(gpu, hdr);
        let (w, h) = gpu.size();
        let tonemapper = render::ToneMapper::new(gpu, w, h, gpu.surface_format());
        let cube = renderer.add_mesh(gfx::Mesh::cube(gpu));
        let photo_mesh = renderer.add_texture(&photo);
        let materials = [
            renderer.add_material(
                gpu,
                Material {
                    base_color_texture: Some(photo_mesh),
                    ..Default::default()
                },
            ),
            renderer.add_material(
                gpu,
                Material {
                    base_color: Vec4::new(0.9, 0.2, 0.2, 1.0),
                    ..Default::default()
                },
            ),
        ];
        let highlight = renderer.add_material(
            gpu,
            Material {
                base_color: Vec4::new(1.0, 0.85, 0.2, 1.0),
                roughness: 0.3,
                ..Default::default()
            },
        );
        let sphere = renderer.add_mesh(gfx::Mesh::sphere(gpu, 48, 24));
        let sphere_materials = (0..5)
            .map(|i| {
                renderer.add_material(
                    gpu,
                    Material {
                        base_color: Vec4::new(0.8, 0.8, 0.8, 1.0),
                        roughness: 0.1 + i as f32 * 0.2,
                        ..Default::default()
                    },
                )
            })
            .collect();
        let glass = renderer.add_material(
            gpu,
            Material {
                base_color: Vec4::new(0.4, 0.7, 1.0, 0.35),
                roughness: 0.05,
                alpha_mode: render::AlphaMode::Blend,
                ..Default::default()
            },
        );
        let size = 64;
        let mut pixels = Vec::with_capacity(size * size * 4);
        for y in 0..size {
            for x in 0..size {
                let cx = (x % 16) as f32 - 7.5;
                let cy = (y % 16) as f32 - 7.5;
                let inside = cx * cx + cy * cy < 36.0;
                pixels.extend_from_slice(if inside {
                    &[70, 150, 60, 255]
                } else {
                    &[0, 0, 0, 0]
                });
            }
        }
        let leaves_texture =
            gfx::Texture::from_rgba8(gpu, size as u32, size as u32, &pixels, Default::default());
        let leaves_texture = renderer.add_texture(&leaves_texture);
        let leaves = renderer.add_material(
            gpu,
            Material {
                base_color_texture: Some(leaves_texture),
                roughness: 0.8,
                alpha_mode: render::AlphaMode::Mask { cutoff: 0.5 },
                double_sided: true,
                filter: render::TextureFilter::Nearest,
                ..Default::default()
            },
        );

        let floor = renderer.add_material(
            gpu,
            Material {
                base_color: Vec4::new(0.55, 0.55, 0.5, 1.0),
                roughness: 0.9,
                ..Default::default()
            },
        );
        renderer.set_shadows(render::ShadowSettings {
            center: Vec3::ZERO,
            radius: 6.0,
            ..Default::default()
        });
        let quad = renderer.add_mesh(gfx::Mesh::quad(gpu));
        let animated_id = renderer.add_model(gpu, &animated);
        let pose = animated.rest_pose();
        let mut animator = runtime::asset::Animator::new();
        animator.play(Some(0), 0.0, true); // start walking immediately
        let mut walking = true;

        let model = renderer.add_model(gpu, &car);
        // Scene
        let objects = vec![
            gfx::Transform::default(),
            gfx::Transform {
                translation: Vec3::new(0.6, 0.2, 0.3),
                scale: Vec3::splat(0.8),
                ..Default::default()
            },
        ];
        let mut sprites = SpriteRenderer::new(gpu);
        let photo_sprite = sprites.add_texture(gpu, &photo, &gpu.samplers.linear);

        // Controls
        let actions = ActionMap::new()
            .bind(Action::TogglePixelated, Binding::Key(KeyCode::Space))
            .bind(Action::ToggleProjection, Binding::Key(KeyCode::KeyP))
            .bind(Action::ToggleFullscreen, Binding::Key(KeyCode::F11))
            .bind(Action::Quit, Binding::Key(KeyCode::KeyQ))
            .bind(Action::CaptureMouse, Binding::Mouse(MouseButton::Left))
            .bind(Action::ReleaseMouse, Binding::Key(KeyCode::Escape))
            .bind_axis(Action::Orbit, AxisBinding::ARROWS)
            .bind_axis(Action::Orbit, AxisBinding::WASD)
            .bind(Action::ToggleWalk, Binding::Key(KeyCode::KeyB))
            .bind(Action::CycleToneMap, Binding::Key(KeyCode::KeyT))
            .bind(Action::ToggleShadows, Binding::Key(KeyCode::KeyH))
            .bind_axis_1d(
                Action::Exposure,
                runtime::Axis1DBinding::Keys {
                    negative: KeyCode::BracketLeft,
                    positive: KeyCode::BracketRight,
                },
            );

        let font = render::Font::new(gpu, &mut sprites, &ctx.assets.read("font.ttf")?)?;
        Ok(Self {
            low_res,
            image_pass,
            upscale_pass,
            pixelated: true,
            meshes: renderer,
            cube,
            objects,
            materials,
            actions,
            camera: gfx::Camera::default(),
            orthographic: false,
            yaw: 0.0,
            pitch: 0.4,
            distance: 3.5,
            sprites,
            photo_sprite,
            hud_angle: 0.0,
            tonemapper,
            font,
            model,
            sphere,
            sphere_materials,
            glass,
            leaves,
            quad,

            animated,
            animated_id,
            pose,
            animator,
            walking,
            highlight,
            hovered: None,
            ground_hit: None,

            floor,
        })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        let input = &ctx.input;
        let actions = &self.actions;
        let toggle_walk = actions.pressed(input, Action::ToggleWalk);

        if toggle_walk {
            self.walking = !self.walking;
            let animation = if self.walking { Some(0) } else { None };
            self.animator.play(animation, 0.4, true);
        }
        self.animator.update(dt);

        // Read everything first, then act.
        let toggle_pixelated = actions.pressed(input, Action::TogglePixelated);
        let toggle_projection = actions.pressed(input, Action::ToggleProjection);
        let toggle_fullscreen = actions.pressed(input, Action::ToggleFullscreen);
        let quit = actions.pressed(input, Action::Quit);
        let capture = actions.pressed(input, Action::CaptureMouse);
        let release = actions.pressed(input, Action::ReleaseMouse);
        let orbit = actions.axis(input, Action::Orbit);
        let mouse = input.mouse_delta();
        let scroll = input.scroll().y;
        let cycle_tonemap = actions.pressed(input, Action::CycleToneMap);
        let exposure = actions.axis_1d(input, Action::Exposure);
        let toggle_shadows = actions.pressed(input, Action::ToggleShadows);

        if toggle_shadows {
            let mut settings = *self.meshes.shadows();
            settings.enabled = !settings.enabled;
            self.meshes.set_shadows(settings);
        }
        if toggle_pixelated {
            self.pixelated = !self.pixelated;
        }
        if toggle_projection {
            self.orthographic = !self.orthographic;
        }
        if toggle_fullscreen {
            ctx.window.set_fullscreen(!ctx.window.is_fullscreen());
        }
        if quit {
            ctx.quit();
        }
        if capture {
            ctx.window.set_cursor_locked(true);
        }
        if release {
            ctx.window.set_cursor_locked(false);
        }
        if cycle_tonemap {
            self.tonemapper.operator = self.tonemapper.operator.next();
            tracing::info!(operator = ?self.tonemapper.operator, "tone mapping");
        }
        // Exposure changes multiplicatively: equal steps feel equal, like camera stops.
        self.tonemapper.exposure =
            (self.tonemapper.exposure * (1.0 + exposure * dt)).clamp(0.05, 20.0);

        // Orbit camera: mouse delta (per tick) plus keys (per second).
        self.yaw -= mouse.x * 0.005 + orbit.x * 2.0 * dt;
        self.pitch = (self.pitch + mouse.y * 0.005 - orbit.y * 2.0 * dt).clamp(-1.5, 1.5);
        self.distance = (self.distance - scroll * 0.3).clamp(1.5, 20.0);

        let offset = Vec3::new(
            self.pitch.cos() * self.yaw.sin(),
            -self.pitch.cos() * self.yaw.cos(),
            self.pitch.sin(),
        ) * self.distance;

        // Orthographic zoom is the visible height; tie it to distance so scrolling works in both modes.
        let projection = if self.orthographic {
            gfx::Projection::Orthographic {
                height: self.distance * 0.8,
                near: 0.1,
                far: 100.0,
            }
        } else {
            gfx::Projection::default()
        };

        self.camera = gfx::Camera {
            position: offset,
            target: Vec3::ZERO,
            up: Vec3::Z,
            projection,
        };

        // Spin the first cube
        let spin = Quat::from_rotation_y(dt) * Quat::from_rotation_x(dt * 0.5);
        let t = &mut self.objects[0];
        t.rotation = (spin * t.rotation).normalize();

        self.hud_angle += dt;

        // Mouse picking: the ray under the cursor, tested against spheres and the ground.
        let (w, h) = ctx.gpu.size();
        let (view, viewport) = self.world_view(w, h);
        let ray = viewport
            .window_to_ndc(ctx.input.mouse_position())
            .map(|ndc| view.ray(ndc));

        self.hovered = ray.and_then(|ray| {
            (0..5)
                .filter_map(|i| {
                    ray.intersect_sphere(Self::sphere_center(i), 0.5)
                        .map(|t| (i, t))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        });
        self.ground_hit = ray.and_then(|ray| {
            ray.intersect_plane(Vec3::new(0.0, 0.0, -0.5), Vec3::Z)
                .map(|t| ray.at(t))
        });
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut gfx::Frame) {
        let gpu = &ctx.gpu;
        let (w, h) = gpu.size();
        let (wf, hf) = (w as f32, h as f32);

        // 1. Say what to draw.
        for (t, material) in self.objects.iter().zip(self.materials) {
            self.meshes.draw(self.cube, material, t);
        }
        self.meshes.draw_model(
            self.model,
            &gfx::Transform::from_translation(Vec3::new(0.0, 0.0, 1.8)),
        );
        // A row of spheres, glossy to matte; the middle one is glass, the hovered one is highlighted.
        for (i, &material) in self.sphere_materials.iter().enumerate() {
            let material = if self.hovered == Some(i) {
                self.highlight
            } else if i == 2 {
                self.glass
            } else {
                material
            };
            self.meshes.draw(
                self.sphere,
                material,
                &gfx::Transform::from_translation(Self::sphere_center(i)),
            );
        }

        // A small marker where the mouse points at the ground.
        if let Some(point) = self.ground_hit {
            self.meshes.draw(
                self.cube,
                self.highlight,
                &gfx::Transform {
                    translation: point,
                    scale: Vec3::splat(0.15),
                    ..Default::default()
                },
            );
        }

        // A cutout panel standing upright (the quad lies flat; rotate it to face -Y).
        self.meshes.draw(
            self.quad,
            self.leaves,
            &gfx::Transform {
                translation: Vec3::new(0.0, -1.5, 0.5),
                rotation: Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                scale: Vec3::splat(2.0),
            },
        );

        self.meshes.draw(
            self.quad,
            self.floor,
            &gfx::Transform {
                translation: Vec3::new(0.0, 0.0, -0.5),
                scale: Vec3::splat(14.0),
                ..Default::default()
            },
        );

        // Two colored lights circling the scene.
        let t = ctx.time.elapsed() as f32;
        self.meshes.draw_light(&render::PointLight {
            position: Vec3::new(t.cos() * 2.5, t.sin() * 2.5, 0.8),
            color: Vec3::new(1.0, 0.3, 0.2),
            intensity: 8.0,
            range: 5.0,
        });
        self.meshes.draw_light(&render::PointLight {
            position: Vec3::new(-t.cos() * 2.5, -t.sin() * 2.5, 0.8),
            color: Vec3::new(0.2, 0.5, 1.0),
            intensity: 8.0,
            range: 5.0,
        });

        let white = self.sprites.white();
        // Translucent bar along the top.
        self.sprites.draw(
            white,
            &Sprite {
                position: Vec2::new(wf * 0.5, hf - 20.0),
                size: Vec2::new(wf, 40.0),
                color: Vec4::new(0.0, 0.0, 0.0, 0.5),
                ..Default::default()
            },
        );
        // Photo thumbnail, bottom-right.
        self.sprites.draw(
            self.photo_sprite,
            &Sprite {
                position: Vec2::new(wf - 116.0, 74.0),
                size: Vec2::new(192.0, 108.0),
                layer: 1,
                ..Default::default()
            },
        );
        // Spinning square, bottom-left.
        self.sprites.draw(
            white,
            &Sprite {
                position: Vec2::new(60.0, 60.0),
                size: Vec2::splat(48.0),
                rotation: self.hud_angle,
                color: Vec4::new(1.0, 0.8, 0.2, 1.0),
                layer: 1,
                ..Default::default()
            },
        );

        let fps = ctx.time.fps();
        let frame_ms = if fps > 0.0 { 1000.0 / fps } else { 0.0 };
        let text = format!("{fps:.0} fps  {frame_ms:.2} ms");
        self.font.draw(
            gpu,
            &mut self.sprites,
            &text,
            Vec2::new(12.0, hf - 10.0),
            &render::TextStyle {
                size: 20.0,
                ..Default::default()
            },
        );

        // Sample the first animation, looping.
        self.animator.apply(&self.animated, &mut self.pose);
        self.meshes.draw_model_posed(
            self.animated_id,
            &gfx::Transform::from_translation(Vec3::new(-3.0, 0.0, 0.5)),
            &self.pose,
        );

        // 2. Prepare every renderer once, each with its own view.
        let (world_view, world_viewport) = self.world_view(w, h);
        self.meshes.prepare(gpu, &world_view);
        self.meshes.render_shadows(frame);
        self.sprites.prepare(gpu, &gfx::View::pixels(w, h));

        // The HDR target matches whatever the scene is shown at.
        let (scene_w, scene_h) = if self.pixelated { (320, 180) } else { (w, h) };
        self.tonemapper.resize(gpu, scene_w, scene_h);
        self.tonemapper.prepare(gpu);

        // 3. Compose passes.
        // Scene, in HDR.
        {
            let mut pass = frame.clear_pass_to(self.tonemapper.target(), wgpu::Color::BLACK);
            self.draw_scene(&mut pass);
        }

        if self.pixelated {
            // Tone map into the low-res target, then upscale it to the screen.
            {
                let mut pass = frame.clear_pass_to(&self.low_res, wgpu::Color::BLACK);
                self.tonemapper.draw(&mut pass);
            }
            let mut pass = frame.clear_pass(wgpu::Color::BLACK);
            world_viewport.apply(&mut pass);
            self.upscale_pass.draw(&mut pass);
            gfx::Viewport::full(w, h).apply(&mut pass);
            self.sprites.render(&mut pass);
        } else {
            // Tone map straight to the screen.
            let mut pass = frame.clear_pass(wgpu::Color::BLACK);
            self.tonemapper.draw(&mut pass);
            self.sprites.render(&mut pass);
        }
    }
}

fn main() -> anyhow::Result<()> {
    runtime::run::<Dev>()
}
