//! Pong, written as entities, components, resources, systems, and plugins.

use ecs::{App, Stage, With, World};
use gfx::glam::{Vec2, Vec4};
use math::{Rect, Rng};
use render::{Font, Sprite, SpriteRenderer, TextStyle};
use runtime::asset::Assets;
use runtime::{ActionMap, Axis1DBinding, Binding, Config, Exit, Input, KeyCode, Time};

const FIELD: Vec2 = Vec2::new(16.0, 9.0);
const PADDLE_SIZE: Vec2 = Vec2::new(0.3, 1.8);
const PADDLE_X: f32 = 7.3;
const PADDLE_SPEED: f32 = 8.0;
const BALL_SIZE: f32 = 0.3;
const BALL_START_SPEED: f32 = 7.0;
const SERVE_DELAY: f32 = 1.0;
const WIN_SCORE: u32 = 5;
const WIN_MESSAGE_TIME: f32 = 2.0;

const FOREGROUND: Vec4 = Vec4::new(0.92, 0.92, 0.88, 1.0);
const DIM: Vec4 = Vec4::new(0.92, 0.92, 0.88, 0.25);
const FIELD_COLOR: Vec4 = Vec4::new(0.08, 0.09, 0.11, 1.0);

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

struct Position(Vec2);
struct Size(Vec2);
struct Velocity(Vec2);
/// 0 = left, 1 = right.
struct Paddle {
    side: usize,
}
/// Tag: marks the ball.
struct Ball;

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Scores([u32; 2]);

/// Counts down before the ball is launched toward `direction` (-1 left, +1 right).
struct Serve {
    timer: f32,
    direction: f32,
}

/// Which side just won, shown for a moment.
#[derive(Default)]
struct Winner {
    side: Option<usize>,
    timer: f32,
}

struct GameRng(Rng);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Action {
    LeftPaddle,
    RightPaddle,
    Quit,
}

struct Controls(ActionMap<Action>);

/// Everything the render system uses, grouped in one resource.
struct Renderers {
    world: SpriteRenderer,
    hud: SpriteRenderer,
    font: Font,
    camera: gfx::Camera,
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn setup(world: &mut World) -> anyhow::Result<()> {
    // Rendering
    let font_bytes = world.resource::<Assets>().read("font.ttf")?;
    let gpu = world.resource::<gfx::GpuContext>();
    let sprites = SpriteRenderer::new(gpu);
    let mut hud = SpriteRenderer::new(gpu);
    let font = Font::new(gpu, &mut hud, &font_bytes)?;
    world.insert_resource(Renderers {
        world: sprites,
        hud,
        font,
        camera: gfx::Camera::orthographic_2d(Vec2::ZERO, FIELD.y),
    });

    // Controls
    world.insert_resource(Controls(
        ActionMap::new()
            .bind_axis_1d(
                Action::LeftPaddle,
                Axis1DBinding::Keys {
                    negative: KeyCode::KeyS,
                    positive: KeyCode::KeyW,
                },
            )
            .bind_axis_1d(
                Action::RightPaddle,
                Axis1DBinding::Keys {
                    negative: KeyCode::ArrowDown,
                    positive: KeyCode::ArrowUp,
                },
            )
            .bind(Action::Quit, Binding::Key(KeyCode::Escape)),
    ));

    // Game state
    let mut rng = Rng::from_time();
    let first_serve = if rng.bool() { 1.0 } else { -1.0 };
    world.insert_resource(GameRng(rng));
    world.insert_resource(Scores::default());
    world.insert_resource(Winner::default());
    world.insert_resource(Serve {
        timer: SERVE_DELAY,
        direction: first_serve,
    });

    // Entities
    for side in 0..2 {
        let x = if side == 0 { -PADDLE_X } else { PADDLE_X };
        world.spawn((
            Paddle { side },
            Position(Vec2::new(x, 0.0)),
            Size(PADDLE_SIZE),
        ));
    }
    world.spawn((
        Ball,
        Position(Vec2::ZERO),
        Size(Vec2::splat(BALL_SIZE)),
        Velocity(Vec2::ZERO),
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// Gameplay (FixedUpdate)
// ---------------------------------------------------------------------------

fn move_paddles(world: &mut World) {
    let input = world.resource::<Input>();
    let controls = &world.resource::<Controls>().0;
    let directions = [
        controls.axis_1d(input, Action::LeftPaddle),
        controls.axis_1d(input, Action::RightPaddle),
    ];
    let dt = world.resource::<Time>().fixed_delta();
    let limit = (FIELD.y - PADDLE_SIZE.y) * 0.5;

    for (paddle, position) in world.query::<(&Paddle, &mut Position)>() {
        let y = position.0.y + directions[paddle.side] * PADDLE_SPEED * dt;
        position.0.y = y.clamp(-limit, limit);
    }
}

fn serve(world: &mut World) {
    let dt = world.resource::<Time>().fixed_delta();
    let serve = world.resource_mut::<Serve>();
    if serve.timer <= 0.0 {
        return; // not serving
    }
    serve.timer -= dt;
    if serve.timer > 0.0 {
        return; // still waiting
    }

    let direction = serve.direction;
    let angle = world.resource_mut::<GameRng>().0.range_f32(-0.6, 0.6);
    for velocity in world.query_filtered::<&mut Velocity, With<Ball>>() {
        velocity.0 = Vec2::new(direction, angle).normalize() * BALL_START_SPEED;
    }
}

fn move_ball(world: &mut World) {
    let dt = world.resource::<Time>().fixed_delta();
    for (position, velocity) in world.query::<(&mut Position, &Velocity)>() {
        position.0 += velocity.0 * dt;
    }
}

fn collide(world: &mut World) {
    // Collect paddle rectangles first: we can't query paddles while the ball query borrows the world.
    let paddles: Vec<(Vec2, Rect)> = world
        .query_filtered::<(&Position, &Size), With<Paddle>>()
        .map(|(position, size)| (position.0, Rect::from_center_size(position.0, size.0)))
        .collect();
    let wall = (FIELD.y - BALL_SIZE) * 0.5;

    for (position, size, velocity) in
        world.query_filtered::<(&mut Position, &Size, &mut Velocity), With<Ball>>()
    {
        // Top and bottom walls
        if position.0.y.abs() > wall {
            position.0.y = wall.copysign(position.0.y);
            velocity.0.y = -velocity.0.y;
        }

        // Paddles
        let ball = Rect::from_center_size(position.0, size.0);
        for (paddle, rect) in &paddles {
            let moving_toward = (paddle.x - position.0.x).signum() == velocity.0.x.signum();
            if moving_toward && ball.overlaps(rect) {
                // -1 at the paddle's bottom edge, +1 at its top edge.
                let offset = ((position.0.y - paddle.y) / (PADDLE_SIZE.y * 0.5)).clamp(-1.0, 1.0);
                let speed = velocity.0.length() * 1.05;
                velocity.0 = Vec2::new(-velocity.0.x.signum(), offset * 0.75).normalize() * speed;
            }
        }
    }
}

fn score(world: &mut World) {
    let goal = FIELD.x * 0.5;
    let mut scorer = None;
    for (position, velocity) in world.query_filtered::<(&mut Position, &mut Velocity), With<Ball>>()
    {
        if position.0.x.abs() > goal {
            // Past the left edge: the right player scores, and the other way round.
            scorer = Some(if position.0.x < 0.0 { 1 } else { 0 });
            position.0 = Vec2::ZERO;
            velocity.0 = Vec2::ZERO;
        }
    }
    let Some(side) = scorer else { return };

    let won = {
        let scores = &mut world.resource_mut::<Scores>().0;
        scores[side] += 1;
        let won = scores[side] >= WIN_SCORE;
        if won {
            *scores = [0, 0];
        }
        won
    };

    // Serve toward the player who lost the point.
    let direction = if side == 1 { -1.0 } else { 1.0 };
    *world.resource_mut::<Serve>() = Serve {
        timer: SERVE_DELAY,
        direction,
    };

    if won {
        *world.resource_mut::<Winner>() = Winner {
            side: Some(side),
            timer: WIN_MESSAGE_TIME,
        };
    }
}

fn tick_winner(world: &mut World) {
    let dt = world.resource::<Time>().fixed_delta();
    let winner = world.resource_mut::<Winner>();
    if winner.timer > 0.0 {
        winner.timer -= dt;
        if winner.timer <= 0.0 {
            winner.side = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Per frame (Update)
// ---------------------------------------------------------------------------

fn quit(world: &mut World) {
    let input = world.resource::<Input>();
    if world.resource::<Controls>().0.pressed(input, Action::Quit) {
        world.resource_mut::<Exit>().request();
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render(world: &mut World) {
    world.resource_scope::<Renderers, _>(|world, r| {
        let (w, h) = world.resource::<gfx::GpuContext>().size();
        let viewport = gfx::Viewport::fit(w, h, FIELD.x / FIELD.y);
        let (vw, vh) = viewport.size();
        let (vwf, vhf) = (vw as f32, vh as f32);
        let white = r.world.white();

        // Field and dashed center line
        r.world.draw(
            white,
            &Sprite {
                size: FIELD,
                color: FIELD_COLOR,
                layer: -1,
                ..Default::default()
            },
        );
        for i in 0..9 {
            r.world.draw(
                white,
                &Sprite {
                    position: Vec2::new(0.0, -4.0 + i as f32),
                    size: Vec2::new(0.1, 0.5),
                    color: DIM,
                    ..Default::default()
                },
            );
        }

        // Every entity with a position and a size: paddles and ball.
        for (position, size) in world.query::<(&Position, &Size)>() {
            r.world.draw(
                white,
                &Sprite {
                    position: position.0,
                    size: size.0,
                    color: FOREGROUND,
                    layer: 1,
                    ..Default::default()
                },
            );
        }

        // HUD
        let scores = world.resource::<Scores>().0;
        let winner = world.resource::<Winner>().side;
        let gpu = world.resource::<gfx::GpuContext>();
        let quantize = |size: f32| ((size / 4.0).round() * 4.0).max(8.0);

        let score_style = TextStyle {
            size: quantize(vhf * 0.1),
            color: FOREGROUND,
            ..Default::default()
        };
        for side in 0..2 {
            let text = scores[side].to_string();
            let size = r.font.measure(&text, score_style.size);
            let sign = if side == 0 { -1.0 } else { 1.0 };
            let top_left = Vec2::new(
                vwf * 0.5 + sign * vwf * 0.08 - size.x * 0.5,
                vhf - vhf * 0.04,
            );
            r.font.draw(gpu, &mut r.hud, &text, top_left, &score_style);
        }
        if let Some(side) = winner {
            let text = format!("Player {} wins!", side + 1);
            let style = TextStyle {
                size: quantize(vhf * 0.07),
                color: FOREGROUND,
                ..Default::default()
            };
            let size = r.font.measure(&text, style.size);
            let top_left = Vec2::new((vwf - size.x) * 0.5, (vhf + size.y) * 0.5);
            r.font.draw(gpu, &mut r.hud, &text, top_left, &style);
        }

        // Prepare and draw.
        r.world.prepare(gpu, &gfx::View::new(&r.camera, vw, vh));
        r.hud.prepare(gpu, &gfx::View::pixels(vw, vh));

        let frame = world.resource_mut::<gfx::Frame>();
        let mut pass = frame.clear_pass(gfx::Color::BLACK);
        viewport.apply(&mut pass);
        r.world.render(&mut pass);
        r.hud.render(&mut pass);
    });
}

// ---------------------------------------------------------------------------
// Plugins and main
// ---------------------------------------------------------------------------

fn gameplay(app: &mut App) {
    app.add_system(Stage::Startup, setup)
        .add_system(Stage::FixedUpdate, move_paddles)
        .add_system(Stage::FixedUpdate, serve)
        .add_system(Stage::FixedUpdate, move_ball)
        .add_system(Stage::FixedUpdate, collide)
        .add_system(Stage::FixedUpdate, score)
        .add_system(Stage::FixedUpdate, tick_winner)
        .add_system(Stage::Update, quit);
}

fn presentation(app: &mut App) {
    app.add_system(Stage::Render, render);
}

fn main() -> anyhow::Result<()> {
    let mut app = App::new();
    app.insert_resource(Config {
        title: "Pong".into(),
        assets_dir: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").into(),
        ..Default::default()
    })
    .add_plugin(gameplay)
    .add_plugin(presentation);

    runtime::run_app(app)
}
