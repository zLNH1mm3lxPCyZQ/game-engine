use gfx::{Color, glam::{Vec2, Vec4}};
use math::{Rect, Rng};
use render::{Sprite, SpriteRenderer};
use runtime::{ActionMap, Axis1DBinding, Binding, Context, Game, KeyCode};

const FIELD: Vec2 = Vec2::new(16.0, 9.0);
const PADDLE_SIZE: Vec2 = Vec2::new(0.3, 1.8);
const PADDLE_X: f32 = 7.3;
const PADDLE_SPEED: f32 = 8.0;
const BALL_SIZE: f32 = 0.3;
const BALL_START_SPEED: f32 = 7.0;
const SERVE_DELAY: f32 = 1.0;
const WIN_SCORE: u32 = 5;

const FOREGROUND: Vec4 = Vec4::new(0.92, 0.92, 0.88, 1.0);
const DIM: Vec4 = Vec4::new(0.92, 0.92, 0.88, 0.25);
const FIELD_COLOR: Vec4 = Vec4::new(0.08, 0.09, 0.11, 1.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Action {
    LeftPaddle,
    RightPaddle,
    Quit,
}

struct Pong {
    sprites: SpriteRenderer,
    camera: gfx::Camera,
    actions: ActionMap<Action>,
    rng: Rng,

    /// Vertical position of each paddle: [left, right].
    paddles: [f32; 2],
    ball: Vec2,
    velocity: Vec2,
    scores: [u32; 2],
    serve_timer: f32,
    serve_direction: f32,

    hud: SpriteRenderer,
    font: render::Font,
    winner: Option<usize>,
    message_timer: f32,
}

impl Pong {
    /// Put the ball in the center and serve toward `direction` (-1 left, +1 right) after a pause.
    fn reset_ball(&mut self, direction: f32) {
        self.ball = Vec2::ZERO;
        self.velocity = Vec2::ZERO;
        self.serve_timer = SERVE_DELAY;
        self.serve_direction = direction;
    }

    fn paddle_position(&self, side: usize) -> Vec2 {
        let x = if side == 0 { -PADDLE_X } else { PADDLE_X };
        Vec2::new(x, self.paddles[side])
    }

    fn paddle_rect(&self, side: usize) -> Rect {
        Rect::from_center_size(self.paddle_position(side), PADDLE_SIZE)
    }

    fn ball_rect(&self) -> Rect {
        Rect::from_center_size(self.ball, Vec2::splat(BALL_SIZE))
    }
}

impl Game for Pong {
    fn config() -> runtime::Config {
        runtime::Config {
            title: "Pong".into(),
        assets_dir: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").into(),
            ..Default::default()
        }
    }

    fn init(ctx: &mut Context) -> anyhow::Result<Self> {
        let actions = ActionMap::new()
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
            .bind(Action::Quit, Binding::Key(KeyCode::Escape));

        let mut rng = Rng::from_time();
        let first_serve = if rng.bool() { 1.0 } else { -1.0 };

        let mut hud = SpriteRenderer::new(&ctx.gpu);
        let font = render::Font::new(&ctx.gpu, &mut hud, &ctx.assets.read("font.ttf")?)?;

        let mut pong = Self {
            sprites: SpriteRenderer::new(&ctx.gpu),
            hud,
            font,
            winner: None,
            message_timer: 0.0,
            camera: gfx::Camera::orthographic_2d(Vec2::ZERO, FIELD.y),
            actions,
            rng,
            paddles: [0.0, 0.0],
            ball: Vec2::ZERO,
            velocity: Vec2::ZERO,
            scores: [0, 0],
            serve_timer: 0.0,
            serve_direction: 1.0,
        };
        pong.reset_ball(first_serve);
        Ok(pong)
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if self.message_timer > 0.0 {
            self.message_timer -= dt;
            if self.message_timer <= 0.0 {
                self.winner = None;
            }
        }

        let input = &ctx.input;
        let directions = [
            self.actions.axis_1d(input, Action::LeftPaddle),
            self.actions.axis_1d(input, Action::RightPaddle),
        ];
        let quit = self.actions.pressed(input, Action::Quit);

        if quit {
            ctx.quit();
        }

        // Paddles
        let paddle_limit = (FIELD.y - PADDLE_SIZE.y) * 0.5;
        for (y, direction) in self.paddles.iter_mut().zip(directions) {
            *y = (*y + direction * PADDLE_SPEED * dt).clamp(-paddle_limit, paddle_limit);
        }

        // Waiting to serve
        if self.serve_timer > 0.0 {
            self.serve_timer -= dt;
            if self.serve_timer <= 0.0 {
                let angle = self.rng.range_f32(-0.6, 0.6);
                self.velocity =
                    Vec2::new(self.serve_direction, angle).normalize() * BALL_START_SPEED;
            }
            return;
        }

        // Ball movement
        self.ball += self.velocity * dt;

        // Top and bottom walls
        let wall_limit = (FIELD.y - BALL_SIZE) * 0.5;
        if self.ball.y.abs() > wall_limit {
            self.ball.y = wall_limit.copysign(self.ball.y);
            self.velocity.y = -self.velocity.y;
        }

        // Paddles
        for side in 0..2 {
            let paddle = self.paddle_position(side);
            let moving_toward = (paddle.x - self.ball.x).signum() == self.velocity.x.signum();
            if moving_toward && self.ball_rect().overlaps(&self.paddle_rect(side)) {
                // -1 at the paddle's bottom edge, +1 at its top edge.
                let offset = ((self.ball.y - paddle.y) / (PADDLE_SIZE.y * 0.5)).clamp(-1.0, 1.0);
                let speed = self.velocity.length() * 1.05;
                let direction = Vec2::new(-self.velocity.x.signum(), offset * 0.75);
                self.velocity = direction.normalize() * speed;
            }
        }

        // Scoring
        let goal = FIELD.x * 0.5;
        if self.ball.x < -goal {
            self.scores[1] += 1;
            self.reset_ball(-1.0);
        } else if self.ball.x > goal {
            self.scores[0] += 1;
            self.reset_ball(1.0);
        }

        if let Some(side) = self.scores.iter().position(|&s| s >= WIN_SCORE) {
            self.winner = Some(side);
            self.message_timer = 2.0;
            self.scores = [0, 0];
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut gfx::Frame) {
        let gpu = &ctx.gpu;
        let (w, h) = gpu.size();
        let viewport = gfx::Viewport::fit(w, h, FIELD.x / FIELD.y);
        let (vw, vh) = viewport.size();
        let (vwf, vhf) = (vw as f32, vh as f32);
        let white = self.sprites.white();

        // Field background
        self.sprites.draw(
            white,
            &Sprite {
                size: FIELD,
                color: FIELD_COLOR,
                layer: -1,
                ..Default::default()
            },
        );

        // Dashed center line
        for i in 0..9 {
            self.sprites.draw(
                white,
                &Sprite {
                    position: Vec2::new(0.0, -4.0 + i as f32),
                    size: Vec2::new(0.1, 0.5),
                    color: DIM,
                    ..Default::default()
                },
            );
        }

        // Paddles and ball
        for side in 0..2 {
            self.sprites.draw(
                white,
                &Sprite {
                    position: self.paddle_position(side),
                    size: PADDLE_SIZE,
                    color: FOREGROUND,
                    layer: 1,
                    ..Default::default()
                },
            );
        }
        self.sprites.draw(
            white,
            &Sprite {
                position: self.ball,
                size: Vec2::splat(BALL_SIZE),
                color: FOREGROUND,
                layer: 1,
                ..Default::default()
            },
        );

        // HUD: sizes scale with the viewport, quantized so the glyph cache stays small.
        let quantize = |size: f32| ((size / 4.0).round() * 4.0).max(8.0);
        let score_style = render::TextStyle {
            size: quantize(vhf * 0.1),
            color: FOREGROUND,
            ..Default::default()
        };
        for side in 0..2 {
            let text = self.scores[side].to_string();
            let size = self.font.measure(&text, score_style.size);
            let sign = if side == 0 { -1.0 } else { 1.0 };
            let top_left = Vec2::new(
                vwf * 0.5 + sign * vwf * 0.08 - size.x * 0.5,
                vhf - vhf * 0.04,
            );
            self.font
                .draw(gpu, &mut self.hud, &text, top_left, &score_style);
        }

        if let Some(side) = self.winner {
            let text = format!("Player {} wins!", side + 1);
            let style = render::TextStyle {
                size: quantize(vhf * 0.07),
                color: FOREGROUND,
                ..Default::default()
            };
            let size = self.font.measure(&text, style.size);
            let top_left = Vec2::new((vwf - size.x) * 0.5, (vhf + size.y) * 0.5);
            self.font.draw(gpu, &mut self.hud, &text, top_left, &style);
        }

        self.sprites
            .prepare(gpu, &gfx::View::new(&self.camera, vw, vh));
        self.hud.prepare(gpu, &gfx::View::pixels(vw, vh));

        let mut pass = frame.clear_pass(Color::BLACK);
        viewport.apply(&mut pass);
        self.sprites.render(&mut pass);
        self.hud.render(&mut pass);
    }
}

fn main() -> anyhow::Result<()> {
    runtime::run::<Pong>()
}
