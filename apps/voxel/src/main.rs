use gfx::{Color, Vertex};
use gfx::glam::{IVec3, Vec2, Vec3, Vec4};
use render::{Material, MaterialId, MeshId, MeshRenderer, Sprite, SpriteRenderer};
use runtime::{
    ActionMap, Axis1DBinding, AxisBinding, Binding, Context, Game, KeyCode, MouseButton,
};

const CHUNK_SIZE: IVec3 = IVec3::new(32, 32, 16);
const REACH: f32 = 8.0;
const MOVE_SPEED: f32 = 8.0;
const LOOK_SPEED: f32 = 0.003;

// ---------------------------------------------------------------------------
// Blocks and the chunk
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Block {
    Air,
    Grass,
    Dirt,
    Stone,
}

impl Block {
    /// Which of the chunk's meshes this block's faces go into.
    fn mesh_index(self) -> Option<usize> {
        match self {
            Block::Air => None,
            Block::Grass => Some(0),
            Block::Dirt => Some(1),
            Block::Stone => Some(2),
        }
    }
}

/// Each face: outward normal and the face's "up" direction (Z-up world).
const FACES: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::Z),
    (Vec3::NEG_X, Vec3::Z),
    (Vec3::Y, Vec3::Z),
    (Vec3::NEG_Y, Vec3::Z),
    (Vec3::Z, Vec3::Y),
    (Vec3::NEG_Z, Vec3::NEG_Y),
];

/// Corner offsets along (right, up), with matching UVs. Counter-clockwise from outside.
const CORNERS: [(f32, f32, [f32; 2]); 4] = [
    (-1.0, -1.0, [0.0, 1.0]),
    (1.0, -1.0, [1.0, 1.0]),
    (1.0, 1.0, [1.0, 0.0]),
    (-1.0, 1.0, [0.0, 0.0]),
];

type MeshData = (Vec<Vertex>, Vec<u32>);

struct Chunk {
    blocks: Vec<Block>,
}

impl Chunk {
    /// Rolling hills: stone at the bottom, a few layers of dirt, grass on top.
    fn generate() -> Self {
        let count = (CHUNK_SIZE.x * CHUNK_SIZE.y * CHUNK_SIZE.z) as usize;
        let mut chunk = Self {
            blocks: vec![Block::Air; count],
        };

        for y in 0..CHUNK_SIZE.y {
            for x in 0..CHUNK_SIZE.x {
                let (fx, fy) = (x as f32, y as f32);
                let height = 6.0
                    + 2.5 * (fx * 0.25).sin()
                    + 2.0 * (fy * 0.2).cos()
                    + 1.5 * ((fx + fy) * 0.15).sin();
                let height = (height as i32).clamp(1, CHUNK_SIZE.z);
                for z in 0..height {
                    let block = if z == height - 1 {
                        Block::Grass
                    } else if z >= height - 3 {
                        Block::Dirt
                    } else {
                        Block::Stone
                    };
                    chunk.set(IVec3::new(x, y, z), block);
                }
            }
        }
        chunk
    }

    fn index(position: IVec3) -> Option<usize> {
        if position.cmplt(IVec3::ZERO).any() || position.cmpge(CHUNK_SIZE).any() {
            return None;
        }
        Some(
            (position.x + position.y * CHUNK_SIZE.x + position.z * CHUNK_SIZE.x * CHUNK_SIZE.y)
                as usize,
        )
    }

    /// Outside the chunk counts as air.
    fn get(&self, position: IVec3) -> Block {
        Self::index(position).map_or(Block::Air, |i| self.blocks[i])
    }

    fn set(&mut self, position: IVec3, block: Block) {
        if let Some(i) = Self::index(position) {
            self.blocks[i] = block;
        }
    }

    /// One mesh per block type, containing only faces that touch air.
    fn build_meshes(&self) -> [MeshData; 3] {
        let mut meshes: [MeshData; 3] = Default::default();

        for z in 0..CHUNK_SIZE.z {
            for y in 0..CHUNK_SIZE.y {
                for x in 0..CHUNK_SIZE.x {
                    let position = IVec3::new(x, y, z);
                    let Some(mesh) = self.get(position).mesh_index() else {
                        continue;
                    };

                    for (normal, up) in FACES {
                        if self.get(position + normal.as_ivec3()) != Block::Air {
                            continue; // hidden behind a neighbor
                        }
                        let (vertices, indices) = &mut meshes[mesh];
                        let base = vertices.len() as u32;
                        let right = up.cross(normal);
                        let face_center = position.as_vec3() + Vec3::splat(0.5) + normal * 0.5;
                        for (sx, sy, uv) in CORNERS {
                            vertices.push(Vertex {
                                position: (face_center + (right * sx + up * sy) * 0.5).into(),
                                normal: normal.into(),
                                uv,
                            });
                        }
                        indices.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    }
                }
            }
        }
        meshes
    }

    /// Walk the grid along a ray (voxel DDA). Returns the first solid block hit,
    /// and the empty cell just before it (where a new block would go).
    fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<(IVec3, IVec3)> {
        let dir = direction.normalize();
        let mut cell = origin.floor().as_ivec3();
        let mut previous = cell;

        let step = IVec3::new(
            dir.x.signum() as i32,
            dir.y.signum() as i32,
            dir.z.signum() as i32,
        );
        // How far along the ray one whole cell is, on each axis.
        let t_delta = Vec3::new(1.0 / dir.x.abs(), 1.0 / dir.y.abs(), 1.0 / dir.z.abs());
        // How far along the ray the first cell boundary is, on each axis.
        let first_boundary = |c: i32, o: f32, d: f32| {
            if d > 0.0 {
                (c as f32 + 1.0 - o) / d
            } else if d < 0.0 {
                (o - c as f32) / -d
            } else {
                f32::INFINITY
            }
        };
        let mut t_max = Vec3::new(
            first_boundary(cell.x, origin.x, dir.x),
            first_boundary(cell.y, origin.y, dir.y),
            first_boundary(cell.z, origin.z, dir.z),
        );

        let mut t = 0.0;
        while t <= max_distance {
            if self.get(cell) != Block::Air {
                return Some((cell, previous));
            }
            previous = cell;
            // Step into whichever neighboring cell the ray reaches first.
            if t_max.x < t_max.y && t_max.x < t_max.z {
                cell.x += step.x;
                t = t_max.x;
                t_max.x += t_delta.x;
            } else if t_max.y < t_max.z {
                cell.y += step.y;
                t = t_max.y;
                t_max.y += t_delta.y;
            } else {
                cell.z += step.z;
                t = t_max.z;
                t_max.z += t_delta.z;
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The game
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Action {
    Move,
    Vertical,
    Break,
    Place,
    ReleaseMouse,
    ToggleFullscreen,
    Quit,
}

struct Voxel {
    meshes: MeshRenderer,
    hud: SpriteRenderer,
    actions: ActionMap<Action>,

    chunk: Chunk,
    chunk_meshes: [MeshId; 3],
    materials: [MaterialId; 3],
    dirty: bool,

    position: Vec3,
    yaw: f32,
    pitch: f32,
    mouse_locked: bool,
}

impl Voxel {
    fn forward(&self) -> Vec3 {
        Vec3::new(
            self.pitch.cos() * self.yaw.cos(),
            self.pitch.cos() * self.yaw.sin(),
            self.pitch.sin(),
        )
    }
}

impl Game for Voxel {
    fn config() -> runtime::Config {
        runtime::Config {
            title: "Voxel".into(),
            ..Default::default()
        }
    }

    fn init(ctx: &mut Context) -> anyhow::Result<Self> {
        let gpu = &ctx.gpu;
        let mut meshes = MeshRenderer::new(gpu);

        let chunk = Chunk::generate();
        let chunk_meshes = chunk
            .build_meshes()
            .map(|(vertices, indices)| meshes.add_mesh(gfx::Mesh::new(gpu, &vertices, &indices)));

        let colors = [
            Vec4::new(0.35, 0.62, 0.25, 1.0), // grass
            Vec4::new(0.45, 0.32, 0.2, 1.0),  // dirt
            Vec4::new(0.5, 0.5, 0.53, 1.0),   // stone
        ];
        let materials = colors.map(|base_color| {
            meshes.add_material(
                gpu,
                Material {
                    base_color,
                    roughness: 0.9,
                    ..Default::default()
                },
            )
        });

        let actions = ActionMap::new()
            .bind_axis(Action::Move, AxisBinding::WASD)
            .bind_axis_1d(
                Action::Vertical,
                Axis1DBinding::Keys {
                    negative: KeyCode::ShiftLeft,
                    positive: KeyCode::Space,
                },
            )
            .bind(Action::Break, Binding::Mouse(MouseButton::Left))
            .bind(Action::Place, Binding::Mouse(MouseButton::Right))
            .bind(Action::ReleaseMouse, Binding::Key(KeyCode::Escape))
            .bind(Action::ToggleFullscreen, Binding::Key(KeyCode::F11))
            .bind(Action::Quit, Binding::Key(KeyCode::KeyQ));

        Ok(Self {
            meshes,
            hud: SpriteRenderer::new(gpu),
            actions,
            chunk,
            chunk_meshes,
            materials,
            dirty: false,
            position: Vec3::new(16.0, -6.0, 14.0),
            yaw: std::f32::consts::FRAC_PI_2, // facing +Y, toward the chunk
            pitch: -0.4,
            mouse_locked: false,
        })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        let input = &ctx.input;
        let actions = &self.actions;

        // Read everything first, then act.
        let movement = actions.axis(input, Action::Move);
        let vertical = actions.axis_1d(input, Action::Vertical);
        let break_block = actions.pressed(input, Action::Break);
        let place_block = actions.pressed(input, Action::Place);
        let release = actions.pressed(input, Action::ReleaseMouse);
        let toggle_fullscreen = actions.pressed(input, Action::ToggleFullscreen);
        let quit = actions.pressed(input, Action::Quit);
        let mouse = input.mouse_delta();

        if quit {
            ctx.quit();
        }
        if toggle_fullscreen {
            ctx.window.set_fullscreen(!ctx.window.is_fullscreen());
        }
        if release {
            ctx.window.set_cursor_locked(false);
            self.mouse_locked = false;
        }

        // The first click captures the mouse; clicks after that edit the world.
        let clicked = break_block || place_block;
        let edit = clicked && self.mouse_locked;
        if clicked && !self.mouse_locked {
            ctx.window.set_cursor_locked(true);
            self.mouse_locked = true;
        }

        // Mouse look
        if self.mouse_locked {
            self.yaw -= mouse.x * LOOK_SPEED;
            self.pitch = (self.pitch - mouse.y * LOOK_SPEED).clamp(-1.55, 1.55);
        }

        // Fly: WASD relative to where you're facing (ignoring pitch), Space/Shift up and down.
        let flat_forward = Vec3::new(self.yaw.cos(), self.yaw.sin(), 0.0);
        let right = Vec3::new(self.yaw.sin(), -self.yaw.cos(), 0.0);
        let velocity = flat_forward * movement.y + right * movement.x + Vec3::Z * vertical;
        self.position += velocity * MOVE_SPEED * dt;

        // Edit the block under the crosshair.
        if edit
            && let Some((hit, before)) = self.chunk.raycast(self.position, self.forward(), REACH)
        {
            if break_block {
                self.chunk.set(hit, Block::Air);
                self.dirty = true;
            } else if before != self.position.floor().as_ivec3() {
                // Don't place a block inside the camera.
                self.chunk.set(before, Block::Dirt);
                self.dirty = true;
            }
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut gfx::Frame) {
        let gpu = &ctx.gpu;
        let (w, h) = gpu.size();

        // Rebuild the chunk's meshes at most once per frame, however many edits happened.
        if self.dirty {
            for (mesh, (vertices, indices)) in
                self.chunk_meshes.iter().zip(self.chunk.build_meshes())
            {
                self.meshes.update_mesh(gpu, *mesh, &vertices, &indices);
            }
            self.dirty = false;
        }

        // World
        for (mesh, material) in self.chunk_meshes.iter().zip(self.materials) {
            self.meshes
                .draw(*mesh, material, &gfx::Transform::default());
        }
        let camera = gfx::Camera {
            position: self.position,
            target: self.position + self.forward(),
            up: Vec3::Z,
            projection: gfx::Projection::Perspective {
                fov_y: 70f32.to_radians(),
                near: 0.05,
                far: 200.0,
            },
        };
        self.meshes.prepare(gpu, &gfx::View::new(&camera, w, h));

        // Crosshair
        let center = Vec2::new(w as f32, h as f32) * 0.5;
        let white = self.hud.white();
        let color = Vec4::new(1.0, 1.0, 1.0, 0.8);
        self.hud.draw(
            white,
            &Sprite {
                position: center,
                size: Vec2::new(16.0, 2.0),
                color,
                ..Default::default()
            },
        );
        self.hud.draw(
            white,
            &Sprite {
                position: center,
                size: Vec2::new(2.0, 16.0),
                color,
                ..Default::default()
            },
        );
        self.hud.prepare(gpu, &gfx::View::pixels(w, h));

        let sky = Color::rgba(0.45, 0.65, 0.95, 1.0);
        let mut pass = frame.clear_pass(sky);
        self.meshes.render(&mut pass);
        self.hud.render(&mut pass);
    }
}

fn main() -> anyhow::Result<()> {
    runtime::run::<Voxel>()
}
