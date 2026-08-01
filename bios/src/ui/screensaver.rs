use macroquad::prelude::*;
use std::{fs, path::PathBuf};

const MAZE_SIZE: usize = 21;
const MOVE_SPEED: f32 = 1.15;
const TURN_SPEED: f32 = 2.4;
const FOV: f32 = 1.16;

pub struct MazeScreensaver {
    maze: [[bool; MAZE_SIZE]; MAZE_SIZE],
    position: Vec2,
    current_cell: IVec2,
    target_cell: IVec2,
    heading: IVec2,
    angle: f32,
    target_angle: f32,
    covers: Vec<Texture2D>,
    cover_paths: Vec<PathBuf>,
}

impl MazeScreensaver {
    pub fn new() -> Self {
        let mut state = Self {
            maze: [[true; MAZE_SIZE]; MAZE_SIZE],
            position: vec2(1.5, 1.5),
            current_cell: ivec2(1, 1),
            target_cell: ivec2(2, 1),
            heading: ivec2(1, 0),
            angle: 0.0,
            target_angle: 0.0,
            covers: Vec::new(),
            cover_paths: Vec::new(),
        };
        state.regenerate();
        state
    }

    pub async fn load_internal_game_covers(&mut self) {
        let mut cover_paths = Vec::new();
        for root in super::internal_games::internal_library_roots() {
            for game_folder in super::internal_games::library_game_folders(&root) {
                if let Ok(files) = fs::read_dir(game_folder) {
                    let mut candidates = files
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| {
                            let extension = path
                                .extension()
                                .and_then(|value| value.to_str())
                                .unwrap_or_default()
                                .to_ascii_lowercase();
                            let stem = path
                                .file_stem()
                                .and_then(|value| value.to_str())
                                .unwrap_or_default()
                                .to_ascii_lowercase();
                            stem.contains("cover")
                                && matches!(
                                    extension.as_str(),
                                    "png" | "jpg" | "jpeg" | "webp" | "avif"
                                )
                        })
                        .collect::<Vec<_>>();
                    candidates.sort();
                    if let Some(cover) = candidates.into_iter().next() {
                        cover_paths.push(cover);
                    }
                }
            }
        }
        cover_paths.sort();
        if cover_paths == self.cover_paths {
            return;
        }

        self.covers.clear();
        for path in cover_paths.iter().take(32) {
            // Macroquad's direct loader can panic on JPEG even though the
            // project already supports it elsewhere. Decode through `image`
            // first, then upload a bounded RGBA texture to the GPU.
            let Ok(bytes) = fs::read(path) else {
                continue;
            };
            let Ok(decoded) = image::load_from_memory(&bytes) else {
                eprintln!(
                    "[Screensaver] Skipping unsupported wall poster: {}",
                    path.display()
                );
                continue;
            };
            let poster = decoded.resize(256, 384, image::imageops::FilterType::Triangle);
            let rgba = poster.to_rgba8();
            let texture =
                Texture2D::from_rgba8(rgba.width() as u16, rgba.height() as u16, rgba.as_raw());
            texture.set_filter(FilterMode::Nearest);
            self.covers.push(texture);
        }
        self.cover_paths = cover_paths;
        println!(
            "[Screensaver] Loaded {} internal-game wall posters",
            self.covers.len()
        );
    }

    pub fn regenerate(&mut self) {
        self.maze = generate_maze();
        self.position = vec2(1.5, 1.5);
        self.current_cell = ivec2(1, 1);
        self.heading = ivec2(1, 0);
        self.angle = 0.0;
        self.target_angle = 0.0;
        self.choose_next_cell();
    }

    pub fn update(&mut self, frame_time: f32) {
        let angle_delta = wrap_angle(self.target_angle - self.angle);
        if angle_delta.abs() > 0.015 {
            let turn = (TURN_SPEED * frame_time).min(angle_delta.abs());
            self.angle = wrap_angle(self.angle + turn * angle_delta.signum());
            return;
        }
        self.angle = self.target_angle;

        let target = vec2(
            self.target_cell.x as f32 + 0.5,
            self.target_cell.y as f32 + 0.5,
        );
        let offset = target - self.position;
        let distance = offset.length();
        let movement = MOVE_SPEED * frame_time;
        if distance <= movement {
            self.position = target;
            self.current_cell = self.target_cell;
            self.choose_next_cell();
        } else if distance > 0.0 {
            self.position += offset / distance * movement;
        }
    }

    pub fn draw(&self) {
        self.draw_style(false);
    }

    pub fn draw_retro(&self) {
        self.draw_style(true);
    }

    fn draw_style(&self, retro: bool) {
        let width = screen_width();
        let height = screen_height();
        let horizon = height * 0.5;

        clear_background(Color::from_rgba(2, 4, 8, 255));
        draw_floor_and_ceiling(self.position, self.angle, width, height, horizon, retro);

        // Deliberately chunky columns preserve the software-rendered Windows
        // 9x appearance while still scaling cleanly to modern displays.
        let strip_width = (width / 320.0).max(2.0);
        let mut screen_x = 0.0;
        while screen_x < width {
            let camera_x = (2.0 * screen_x / width) - 1.0;
            let ray_angle = self.angle + camera_x * (FOV * 0.5);
            let ray = vec2(ray_angle.cos(), ray_angle.sin());
            let hit = cast_ray(&self.maze, self.position, ray);
            let corrected_distance = (hit.distance * (ray_angle - self.angle).cos()).max(0.08);
            let wall_height = (height / corrected_distance).min(height * 2.4);
            let wall_top = horizon - wall_height * 0.5;

            let distance_shade = (1.0 / (1.0 + corrected_distance * 0.13)).clamp(0.22, 1.0);
            let side_shade = if hit.vertical { 0.84 } else { 1.0 };
            let brightness = distance_shade * side_shade;

            // Sample a small procedural brick texture. Each course is offset
            // by half a brick, matching the familiar Windows maze walls.
            const TEXTURE_ROWS: usize = 32;
            let sample_height = wall_height / TEXTURE_ROWS as f32;
            for sample in 0..TEXTURE_ROWS {
                let course = sample / 4;
                let course_y = sample % 4;
                let stagger = if course % 2 == 0 { 0.0 } else { 0.5 };
                let brick_u = (hit.wall_offset * 4.0 + stagger).fract();
                let mortar = course_y == 0 || brick_u < 0.055;
                let brick_variation =
                    0.88 + (((course * 13 + (brick_u * 17.0) as usize) % 5) as f32 * 0.035);
                let color = if retro {
                    let grid = course_y == 0 || brick_u < 0.045;
                    if grid {
                        let mut neon = super::playfusion_neon_color(
                            hit.wall_offset * 0.42
                                + sample as f32 / TEXTURE_ROWS as f32 * 0.32
                                + get_time() as f32 * 0.025,
                        );
                        neon.r *= brightness;
                        neon.g *= brightness;
                        neon.b *= brightness;
                        neon
                    } else {
                        Color::new(
                            0.025 * brightness,
                            0.035 * brightness,
                            (0.12 + 0.035 * brick_variation) * brightness,
                            1.0,
                        )
                    }
                } else if mortar {
                    Color::new(0.52 * brightness, 0.50 * brightness, 0.46 * brightness, 1.0)
                } else {
                    Color::new(
                        0.69 * brightness * brick_variation,
                        0.22 * brightness * brick_variation,
                        0.13 * brightness * brick_variation,
                        1.0,
                    )
                };
                draw_rectangle(
                    screen_x,
                    wall_top + sample as f32 * sample_height,
                    strip_width + 0.5,
                    sample_height + 0.5,
                    color,
                );
            }

            // Selected maze wall faces become framed cover-art posters. The
            // source strip follows wall UV coordinates so the artwork remains
            // attached to the wall in perspective.
            if let Some(cover) = self.poster_for_hit(&hit) {
                const POSTER_LEFT: f32 = 0.14;
                const POSTER_RIGHT: f32 = 0.86;
                if hit.wall_offset >= POSTER_LEFT && hit.wall_offset <= POSTER_RIGHT {
                    let poster_u =
                        1.0 - (hit.wall_offset - POSTER_LEFT) / (POSTER_RIGHT - POSTER_LEFT);
                    let source_width = (cover.width() / 180.0).max(1.0);
                    let source_x = (poster_u * (cover.width() - source_width).max(0.0)).floor();
                    let poster_top = wall_top + wall_height * 0.10;
                    let poster_height = wall_height * 0.80;
                    let poster_tint = (brightness * 1.28).clamp(0.30, 1.0);

                    // Dark frame behind the cover.
                    draw_rectangle(
                        screen_x,
                        poster_top - (wall_height * 0.025),
                        strip_width + 0.5,
                        poster_height + wall_height * 0.05,
                        Color::new(0.08 * brightness, 0.06 * brightness, 0.04 * brightness, 1.0),
                    );
                    draw_texture_ex(
                        cover,
                        screen_x,
                        poster_top,
                        Color::new(poster_tint, poster_tint, poster_tint, 1.0),
                        DrawTextureParams {
                            dest_size: Some(vec2(strip_width + 0.5, poster_height)),
                            source: Some(Rect::new(source_x, 0.0, source_width, cover.height())),
                            ..Default::default()
                        },
                    );
                }
            }

            screen_x += strip_width;
        }

        // A faint scanline treatment keeps the recreation stylistically close
        // to the low-resolution software-rendered original.
        let mut y = 0.0;
        while y < height {
            draw_rectangle(0.0, y, width, 1.0, Color::new(0.0, 0.0, 0.0, 0.08));
            y += 4.0;
        }
    }

    fn choose_next_cell(&mut self) {
        let mut candidates = Vec::with_capacity(4);
        for direction in [ivec2(1, 0), ivec2(-1, 0), ivec2(0, 1), ivec2(0, -1)] {
            let cell = self.current_cell + direction;
            if is_open(&self.maze, cell) && direction != -self.heading {
                candidates.push(direction);
            }
        }

        if candidates.is_empty() {
            candidates.push(-self.heading);
        }

        let straight = candidates
            .iter()
            .position(|direction| *direction == self.heading);
        let selected = if straight.is_some() && macroquad::rand::gen_range(0.0, 1.0) < 0.68 {
            self.heading
        } else {
            let index = macroquad::rand::gen_range(0, candidates.len() as i32) as usize;
            candidates[index]
        };

        self.heading = selected;
        self.target_cell = self.current_cell + selected;
        self.target_angle = match (selected.x, selected.y) {
            (1, 0) => 0.0,
            (-1, 0) => std::f32::consts::PI,
            (0, 1) => std::f32::consts::FRAC_PI_2,
            (0, -1) => -std::f32::consts::FRAC_PI_2,
            _ => self.angle,
        };
    }

    fn poster_for_hit(&self, hit: &RayHit) -> Option<&Texture2D> {
        if self.covers.is_empty() {
            return None;
        }
        let face = if hit.vertical { 17_i64 } else { 31_i64 };
        let hash = (hit.map_x as i64 * 73_856_093) ^ (hit.map_y as i64 * 19_349_663) ^ face;
        if hash.rem_euclid(4) != 0 {
            return None;
        }
        let index = hash.rem_euclid(self.covers.len() as i64) as usize;
        self.covers.get(index)
    }
}

#[derive(Clone, Copy)]
pub enum PongSound {
    Paddle,
    Wall,
}

pub struct PongScreensaver {
    ball: Vec2,
    ball_velocity: Vec2,
    trail: Vec<Vec2>,
    left_paddle_y: f32,
    right_paddle_y: f32,
    left_score: u32,
    right_score: u32,
    serve_delay: f32,
    ai_decision_timer: f32,
    left_error: f32,
    right_error: f32,
}

impl PongScreensaver {
    const FIELD_WIDTH: f32 = 640.0;
    const FIELD_HEIGHT: f32 = 360.0;
    const FIELD_TOP: f32 = 28.0;
    const FIELD_BOTTOM: f32 = 344.0;
    const PADDLE_WIDTH: f32 = 10.0;
    const PADDLE_HEIGHT: f32 = 68.0;
    const LEFT_PADDLE_X: f32 = 28.0;
    const RIGHT_PADDLE_X: f32 = 602.0;
    const BALL_RADIUS: f32 = 6.0;

    pub fn new() -> Self {
        let mut state = Self {
            ball: vec2(Self::FIELD_WIDTH * 0.5, Self::FIELD_HEIGHT * 0.5),
            ball_velocity: vec2(255.0, 95.0),
            trail: Vec::new(),
            left_paddle_y: (Self::FIELD_HEIGHT - Self::PADDLE_HEIGHT) * 0.5,
            right_paddle_y: (Self::FIELD_HEIGHT - Self::PADDLE_HEIGHT) * 0.5,
            left_score: 0,
            right_score: 0,
            serve_delay: 0.45,
            ai_decision_timer: 0.0,
            left_error: 0.0,
            right_error: 0.0,
        };
        state.reset_ball(if macroquad::rand::gen_range(0, 2) == 0 {
            -1.0
        } else {
            1.0
        });
        state
    }

    pub fn reset_match(&mut self) {
        self.left_score = 0;
        self.right_score = 0;
        self.left_paddle_y = (Self::FIELD_HEIGHT - Self::PADDLE_HEIGHT) * 0.5;
        self.right_paddle_y = self.left_paddle_y;
        self.reset_ball(if macroquad::rand::gen_range(0, 2) == 0 {
            -1.0
        } else {
            1.0
        });
    }

    pub fn update(&mut self, frame_time: f32) -> Option<PongSound> {
        let dt = frame_time.min(0.04);
        self.update_ai(dt);

        if self.serve_delay > 0.0 {
            self.serve_delay = (self.serve_delay - dt).max(0.0);
            return None;
        }

        self.trail.push(self.ball);
        if self.trail.len() > 12 {
            self.trail.remove(0);
        }

        self.ball += self.ball_velocity * dt;
        let mut sound = None;

        if self.ball.y - Self::BALL_RADIUS <= Self::FIELD_TOP {
            self.ball.y = Self::FIELD_TOP + Self::BALL_RADIUS;
            self.ball_velocity.y = self.ball_velocity.y.abs();
            sound = Some(PongSound::Wall);
        } else if self.ball.y + Self::BALL_RADIUS >= Self::FIELD_BOTTOM {
            self.ball.y = Self::FIELD_BOTTOM - Self::BALL_RADIUS;
            self.ball_velocity.y = -self.ball_velocity.y.abs();
            sound = Some(PongSound::Wall);
        }

        let left_hit = self.ball_velocity.x < 0.0
            && self.ball.x - Self::BALL_RADIUS <= Self::LEFT_PADDLE_X + Self::PADDLE_WIDTH
            && self.ball.x >= Self::LEFT_PADDLE_X
            && self.ball.y + Self::BALL_RADIUS >= self.left_paddle_y
            && self.ball.y - Self::BALL_RADIUS <= self.left_paddle_y + Self::PADDLE_HEIGHT;
        if left_hit {
            self.ball.x = Self::LEFT_PADDLE_X + Self::PADDLE_WIDTH + Self::BALL_RADIUS;
            self.bounce_from_paddle(self.left_paddle_y, 1.0);
            sound = Some(PongSound::Paddle);
        }

        let right_hit = self.ball_velocity.x > 0.0
            && self.ball.x + Self::BALL_RADIUS >= Self::RIGHT_PADDLE_X
            && self.ball.x <= Self::RIGHT_PADDLE_X + Self::PADDLE_WIDTH
            && self.ball.y + Self::BALL_RADIUS >= self.right_paddle_y
            && self.ball.y - Self::BALL_RADIUS <= self.right_paddle_y + Self::PADDLE_HEIGHT;
        if right_hit {
            self.ball.x = Self::RIGHT_PADDLE_X - Self::BALL_RADIUS;
            self.bounce_from_paddle(self.right_paddle_y, -1.0);
            sound = Some(PongSound::Paddle);
        }

        if self.ball.x < -24.0 {
            self.right_score = self.right_score.saturating_add(1);
            self.reset_ball(-1.0);
        } else if self.ball.x > Self::FIELD_WIDTH + 24.0 {
            self.left_score = self.left_score.saturating_add(1);
            self.reset_ball(1.0);
        }

        sound
    }

    pub fn draw(&self) {
        let scale_x = screen_width() / Self::FIELD_WIDTH;
        let scale_y = screen_height() / Self::FIELD_HEIGHT;
        let scale = scale_x.min(scale_y);
        let x = |value: f32| value * scale_x;
        let y = |value: f32| value * scale_y;

        clear_background(Color::from_rgba(2, 4, 10, 255));

        // Subtle arcade glow around the playfield.
        draw_rectangle(
            0.0,
            y(Self::FIELD_TOP - 3.0),
            screen_width(),
            y(3.0),
            Color::new(0.15, 0.85, 1.0, 0.22),
        );
        draw_rectangle(
            0.0,
            y(Self::FIELD_BOTTOM),
            screen_width(),
            y(3.0),
            Color::new(1.0, 0.20, 0.68, 0.22),
        );

        let dash_height = 11.0;
        let mut dash_y = Self::FIELD_TOP + 7.0;
        while dash_y < Self::FIELD_BOTTOM - dash_height {
            draw_rectangle(
                x(Self::FIELD_WIDTH * 0.5 - 1.5),
                y(dash_y),
                x(3.0),
                y(dash_height),
                Color::new(0.80, 0.84, 0.92, 0.62),
            );
            dash_y += 20.0;
        }

        for (index, position) in self.trail.iter().enumerate() {
            let strength = (index + 1) as f32 / self.trail.len().max(1) as f32;
            draw_circle(
                x(position.x),
                y(position.y),
                Self::BALL_RADIUS * scale * (0.45 + strength * 0.45),
                Color::new(0.55, 0.90, 1.0, strength * 0.22),
            );
        }

        self.draw_paddle(
            Self::LEFT_PADDLE_X,
            self.left_paddle_y,
            Color::new(0.18, 0.90, 1.0, 1.0),
            scale_x,
            scale_y,
        );
        self.draw_paddle(
            Self::RIGHT_PADDLE_X,
            self.right_paddle_y,
            Color::new(1.0, 0.24, 0.72, 1.0),
            scale_x,
            scale_y,
        );

        draw_circle(
            x(self.ball.x),
            y(self.ball.y),
            (Self::BALL_RADIUS + 5.0) * scale,
            Color::new(0.55, 0.88, 1.0, 0.12),
        );
        draw_circle(
            x(self.ball.x),
            y(self.ball.y),
            Self::BALL_RADIUS * scale,
            Color::new(0.96, 0.98, 1.0, 1.0),
        );

        let score_size = (46.0 * scale).max(20.0);
        let left_score = self.left_score.to_string();
        let right_score = self.right_score.to_string();
        let left_measure = measure_text(&left_score, None, score_size as u16, 1.0);
        let right_measure = measure_text(&right_score, None, score_size as u16, 1.0);
        draw_text(
            &left_score,
            x(264.0) - left_measure.width * 0.5,
            y(69.0),
            score_size,
            Color::new(0.18, 0.90, 1.0, 0.92),
        );
        draw_text(
            &right_score,
            x(376.0) - right_measure.width * 0.5,
            y(69.0),
            score_size,
            Color::new(1.0, 0.24, 0.72, 0.92),
        );

        let label = "PLAYFUSION  AI PONG";
        let label_size = (14.0 * scale).max(10.0);
        let label_measure = measure_text(label, None, label_size as u16, 1.0);
        draw_text(
            label,
            (screen_width() - label_measure.width) * 0.5,
            y(20.0),
            label_size,
            Color::new(0.74, 0.78, 0.90, 0.70),
        );

        let mut scanline_y = 0.0;
        while scanline_y < screen_height() {
            draw_rectangle(
                0.0,
                scanline_y,
                screen_width(),
                1.0,
                Color::new(0.0, 0.0, 0.0, 0.10),
            );
            scanline_y += (4.0 * scale).max(3.0);
        }
    }

    fn update_ai(&mut self, dt: f32) {
        self.ai_decision_timer -= dt;
        if self.ai_decision_timer <= 0.0 {
            self.ai_decision_timer = macroquad::rand::gen_range(0.16, 0.34);
            self.left_error = macroquad::rand::gen_range(-30.0, 30.0);
            self.right_error = macroquad::rand::gen_range(-30.0, 30.0);
        }

        let center = Self::FIELD_HEIGHT * 0.5;
        let left_target = if self.ball_velocity.x < 0.0 {
            self.ball.y + self.left_error
        } else {
            center + self.left_error * 0.35
        };
        let right_target = if self.ball_velocity.x > 0.0 {
            self.ball.y + self.right_error
        } else {
            center + self.right_error * 0.35
        };

        self.left_paddle_y = move_paddle(
            self.left_paddle_y,
            left_target - Self::PADDLE_HEIGHT * 0.5,
            172.0,
            dt,
        );
        self.right_paddle_y = move_paddle(
            self.right_paddle_y,
            right_target - Self::PADDLE_HEIGHT * 0.5,
            178.0,
            dt,
        );

        let min_y = Self::FIELD_TOP;
        let max_y = Self::FIELD_BOTTOM - Self::PADDLE_HEIGHT;
        self.left_paddle_y = self.left_paddle_y.clamp(min_y, max_y);
        self.right_paddle_y = self.right_paddle_y.clamp(min_y, max_y);
    }

    fn bounce_from_paddle(&mut self, paddle_y: f32, direction: f32) {
        let contact = ((self.ball.y - (paddle_y + Self::PADDLE_HEIGHT * 0.5))
            / (Self::PADDLE_HEIGHT * 0.5))
            .clamp(-1.0, 1.0);
        let next_speed = (self.ball_velocity.length() * 1.035).clamp(260.0, 455.0);
        let angle = contact * 0.82;
        self.ball_velocity = vec2(
            direction * next_speed * angle.cos(),
            next_speed * angle.sin(),
        );
    }

    fn reset_ball(&mut self, direction: f32) {
        self.ball = vec2(Self::FIELD_WIDTH * 0.5, Self::FIELD_HEIGHT * 0.5);
        let vertical = macroquad::rand::gen_range(-125.0, 125.0);
        self.ball_velocity = vec2(direction * 255.0, vertical);
        self.trail.clear();
        self.serve_delay = 0.55;
    }

    fn draw_paddle(&self, paddle_x: f32, paddle_y: f32, color: Color, scale_x: f32, scale_y: f32) {
        draw_rectangle(
            (paddle_x - 5.0) * scale_x,
            (paddle_y - 5.0) * scale_y,
            (Self::PADDLE_WIDTH + 10.0) * scale_x,
            (Self::PADDLE_HEIGHT + 10.0) * scale_y,
            Color::new(color.r, color.g, color.b, 0.12),
        );
        draw_rectangle(
            paddle_x * scale_x,
            paddle_y * scale_y,
            Self::PADDLE_WIDTH * scale_x,
            Self::PADDLE_HEIGHT * scale_y,
            color,
        );
        draw_rectangle(
            (paddle_x + 2.0) * scale_x,
            (paddle_y + 2.0) * scale_y,
            2.0 * scale_x,
            (Self::PADDLE_HEIGHT - 4.0) * scale_y,
            Color::new(1.0, 1.0, 1.0, 0.56),
        );
    }
}

fn move_paddle(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    let delta = target - current;
    if delta.abs() <= speed * dt {
        target
    } else {
        current + delta.signum() * speed * dt
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ShowcaseKind {
    Hyperspace,
    DigitalRain,
    PlasmaGrid,
    NeonSwarm,
    PrismTunnel,
}

pub struct ShowcaseScreensaver {
    kind: ShowcaseKind,
    time: f32,
}

impl ShowcaseScreensaver {
    pub fn new() -> Self {
        Self {
            kind: ShowcaseKind::Hyperspace,
            time: 0.0,
        }
    }

    pub fn set_kind(&mut self, kind: ShowcaseKind) {
        self.kind = kind;
        self.time = 0.0;
    }

    pub fn update(&mut self, frame_time: f32) {
        self.time += frame_time.min(0.05);
    }

    pub fn draw(&self) {
        match self.kind {
            ShowcaseKind::Hyperspace => self.draw_hyperspace(),
            ShowcaseKind::DigitalRain => self.draw_digital_rain(),
            ShowcaseKind::PlasmaGrid => self.draw_plasma_grid(),
            ShowcaseKind::NeonSwarm => self.draw_neon_swarm(),
            ShowcaseKind::PrismTunnel => self.draw_prism_tunnel(),
        }
    }

    fn draw_hyperspace(&self) {
        let width = screen_width();
        let height = screen_height();
        let center = vec2(
            width * 0.5 + (self.time * 0.43).sin() * width * 0.055,
            height * 0.5 + (self.time * 0.31).cos() * height * 0.045,
        );
        let max_radius = (width * width + height * height).sqrt() * 0.62;
        clear_background(Color::from_rgba(1, 2, 10, 255));

        for index in 0..210 {
            let seed = index as f32;
            let angle = hash01(seed * 3.17) * std::f32::consts::TAU;
            let speed = 0.13 + hash01(seed * 7.31) * 0.28;
            let depth = (hash01(seed * 11.9) + self.time * speed).fract();
            let previous_depth = (depth - 0.035 - speed * 0.018).max(0.0);
            let radius = depth.powf(2.15) * max_radius;
            let previous_radius = previous_depth.powf(2.15) * max_radius;
            let stretch = 0.72 + hash01(seed * 2.03) * 0.56;
            let point = center + vec2(angle.cos(), angle.sin() * stretch) * radius;
            let previous = center + vec2(angle.cos(), angle.sin() * stretch) * previous_radius;
            let color = hsv_color((seed * 0.071 + self.time * 0.045).fract(), 0.66, 1.0);
            let alpha = (0.12 + depth * 0.88).min(1.0);
            draw_line(
                previous.x,
                previous.y,
                point.x,
                point.y,
                0.8 + depth * 3.8,
                Color::new(color.r, color.g, color.b, alpha),
            );
            if depth > 0.56 {
                draw_circle(
                    point.x,
                    point.y,
                    0.7 + depth * 1.8,
                    Color::new(1.0, 1.0, 1.0, alpha),
                );
            }
        }

        for ring in 0..8 {
            let phase = ((self.time * 0.33 + ring as f32 / 8.0).fract()).powf(2.0);
            let radius = 15.0 + phase * max_radius * 0.72;
            let color = hsv_color((ring as f32 / 8.0 + self.time * 0.06).fract(), 0.75, 1.0);
            draw_poly_lines(
                center.x,
                center.y,
                8,
                radius,
                self.time * 7.0,
                1.0 + phase * 1.8,
                Color::new(color.r, color.g, color.b, (1.0 - phase) * 0.33),
            );
        }

        draw_centered_label("HYPERSPACE", height - 17.0, 13.0, 0.42);
        draw_scanlines();
    }

    fn draw_digital_rain(&self) {
        let width = screen_width();
        let height = screen_height();
        clear_background(Color::from_rgba(0, 5, 5, 255));

        let column_width = (width / 43.0).max(12.0);
        let columns = (width / column_width).ceil() as usize;
        let glyph_size = (column_width * 0.82).clamp(10.0, 22.0);
        const GLYPHS: &[u8] = b"01ABCDEF<>[]{}#*+KAZETA";

        for column in 0..columns {
            let seed = column as f32;
            let speed = 45.0 + hash01(seed * 8.71) * 105.0;
            let length = 7 + (hash01(seed * 5.19) * 13.0) as usize;
            let track = height + length as f32 * glyph_size * 1.15;
            let head = (self.time * speed + hash01(seed * 19.3) * track) % track;
            let x = column as f32 * column_width + column_width * 0.12;

            for offset in 0..length {
                let y = head - offset as f32 * glyph_size * 1.12;
                if y < -glyph_size || y > height + glyph_size {
                    continue;
                }
                let glyph_seed =
                    column * 31 + offset * 17 + (self.time * (7.0 + seed % 4.0)) as usize;
                let glyph = GLYPHS[glyph_seed % GLYPHS.len()] as char;
                let alpha = 1.0 - offset as f32 / length as f32;
                let color = if offset == 0 {
                    Color::new(0.84, 1.0, 1.0, 0.98)
                } else {
                    let hue = (0.37 + seed * 0.012 + self.time * 0.018).fract();
                    let rgb = hsv_color(hue, 0.85, 0.92);
                    Color::new(rgb.r, rgb.g, rgb.b, alpha * 0.82)
                };
                draw_text(&glyph.to_string(), x, y, glyph_size, color);
            }
        }

        let title = "PLAYFUSION";
        let title_size = (height * 0.10).clamp(24.0, 54.0);
        let measured = measure_text(title, None, title_size as u16, 1.0);
        draw_rectangle(
            (width - measured.width) * 0.5 - 16.0,
            height * 0.46 - title_size,
            measured.width + 32.0,
            title_size + 24.0,
            Color::new(0.0, 0.02, 0.03, 0.72),
        );
        draw_text(
            title,
            (width - measured.width) * 0.5,
            height * 0.46,
            title_size,
            Color::new(0.72, 1.0, 0.92, 0.90),
        );
        draw_centered_label("DIGITAL RAIN", height - 17.0, 13.0, 0.42);
        draw_scanlines();
    }

    fn draw_plasma_grid(&self) {
        let width = screen_width();
        let height = screen_height();
        clear_background(Color::from_rgba(3, 1, 13, 255));

        let cells_x = 40;
        let cells_y = 23;
        let cell_w = width / cells_x as f32 + 0.5;
        let cell_h = height / cells_y as f32 + 0.5;
        for grid_y in 0..cells_y {
            for grid_x in 0..cells_x {
                let nx = grid_x as f32 / cells_x as f32 * 5.5;
                let ny = grid_y as f32 / cells_y as f32 * 4.0;
                let value = (nx * 1.7 + self.time * 1.2).sin()
                    + (ny * 2.1 - self.time * 1.55).cos()
                    + ((nx + ny) * 1.25 + self.time * 0.8).sin()
                    + ((nx * nx + ny * ny).sqrt() * 2.0 - self.time * 1.8).cos();
                let hue = (value * 0.075 + self.time * 0.035 + 0.64).fract().abs();
                let brightness = 0.42 + ((value + 4.0) / 8.0) * 0.58;
                let color = hsv_color(hue, 0.90, brightness);
                draw_rectangle(
                    grid_x as f32 * cell_w,
                    grid_y as f32 * cell_h,
                    cell_w,
                    cell_h,
                    color,
                );
            }
        }

        let center = vec2(
            width * 0.5 + (self.time * 0.7).sin() * width * 0.14,
            height * 0.5 + (self.time * 0.9).cos() * height * 0.13,
        );
        for ring in (1..12).rev() {
            let pulse = (self.time * 42.0 + ring as f32 * 13.0).sin() * 2.5;
            let radius = ring as f32 * height * 0.038 + pulse;
            let color = hsv_color((ring as f32 * 0.075 + self.time * 0.08).fract(), 0.72, 1.0);
            draw_circle_lines(
                center.x,
                center.y,
                radius,
                1.3,
                Color::new(color.r, color.g, color.b, 0.28),
            );
        }

        draw_centered_label("PLASMA GRID", height - 17.0, 13.0, 0.55);
        draw_scanlines();
    }

    fn draw_neon_swarm(&self) {
        let width = screen_width();
        let height = screen_height();
        clear_background(Color::from_rgba(1, 2, 9, 255));

        let count = 110;
        for index in 0..count {
            let seed = index as f32;
            let phase = hash01(seed * 13.7) * std::f32::consts::TAU;
            let speed = 0.38 + hash01(seed * 4.91) * 1.35;
            let orbit_x = 0.20 + hash01(seed * 8.33) * 0.29;
            let orbit_y = 0.18 + hash01(seed * 2.77) * 0.27;
            let time = self.time * speed;
            let point = vec2(
                width * 0.5
                    + (time + phase).sin() * width * orbit_x
                    + (time * 0.37 + phase * 2.0).cos() * width * 0.07,
                height * 0.5
                    + (time * 1.21 + phase).cos() * height * orbit_y
                    + (time * 0.51 + phase * 1.7).sin() * height * 0.08,
            );
            let previous_time = time - 0.045;
            let previous = vec2(
                width * 0.5
                    + (previous_time + phase).sin() * width * orbit_x
                    + (previous_time * 0.37 + phase * 2.0).cos() * width * 0.07,
                height * 0.5
                    + (previous_time * 1.21 + phase).cos() * height * orbit_y
                    + (previous_time * 0.51 + phase * 1.7).sin() * height * 0.08,
            );
            let color = hsv_color((seed / count as f32 + self.time * 0.035).fract(), 0.82, 1.0);
            draw_line(
                previous.x,
                previous.y,
                point.x,
                point.y,
                1.0 + hash01(seed * 6.1) * 2.0,
                Color::new(color.r, color.g, color.b, 0.66),
            );
            draw_circle(
                point.x,
                point.y,
                2.0 + hash01(seed * 3.2) * 3.5,
                Color::new(color.r, color.g, color.b, 0.82),
            );
            if index % 7 == 0 {
                draw_circle(
                    point.x,
                    point.y,
                    9.0,
                    Color::new(color.r, color.g, color.b, 0.10),
                );
            }
        }

        for ring in 0..5 {
            let radius = 28.0 + ring as f32 * 17.0 + (self.time * 3.0).sin() * 4.0;
            let color = hsv_color((ring as f32 * 0.16 + self.time * 0.05).fract(), 0.76, 1.0);
            draw_circle_lines(
                width * 0.5,
                height * 0.5,
                radius,
                1.0,
                Color::new(color.r, color.g, color.b, 0.28),
            );
        }

        draw_centered_label("NEON SWARM", height - 17.0, 13.0, 0.42);
        draw_scanlines();
    }

    fn draw_prism_tunnel(&self) {
        let width = screen_width();
        let height = screen_height();
        clear_background(Color::from_rgba(1, 1, 8, 255));
        let center = vec2(
            width * 0.5 + (self.time * 0.63).sin() * width * 0.09,
            height * 0.5 + (self.time * 0.47).cos() * height * 0.07,
        );
        let max_radius = width.max(height) * 0.86;

        for index in (0..28).rev() {
            let depth = (index as f32 / 28.0 + self.time * 0.21).fract();
            let radius = 12.0 + depth.powf(2.0) * max_radius;
            let sides = 5 + index % 4;
            let rotation = self.time * (12.0 + (index % 3) as f32 * 4.0) + index as f32 * 8.0;
            let color = hsv_color((depth * 0.72 + self.time * 0.055).fract(), 0.82, 1.0);
            let alpha = (1.0 - depth * 0.55).clamp(0.18, 0.90);
            draw_poly_lines(
                center.x,
                center.y,
                sides as u8,
                radius,
                rotation,
                1.0 + depth * 3.2,
                Color::new(color.r, color.g, color.b, alpha),
            );
        }

        for ray in 0..18 {
            let angle = ray as f32 / 18.0 * std::f32::consts::TAU + self.time * 0.08;
            let endpoint = center + vec2(angle.cos(), angle.sin()) * max_radius;
            let color = hsv_color((ray as f32 / 18.0 + self.time * 0.04).fract(), 0.78, 1.0);
            draw_line(
                center.x,
                center.y,
                endpoint.x,
                endpoint.y,
                1.0,
                Color::new(color.r, color.g, color.b, 0.17),
            );
        }

        draw_circle(
            center.x,
            center.y,
            8.0 + (self.time * 4.0).sin().abs() * 5.0,
            Color::new(1.0, 1.0, 1.0, 0.88),
        );
        draw_centered_label("PRISM TUNNEL", height - 17.0, 13.0, 0.42);
        draw_scanlines();
    }
}

fn hash01(value: f32) -> f32 {
    ((value * 12.9898).sin() * 43_758.547).fract().abs()
}

fn hsv_color(hue: f32, saturation: f32, value: f32) -> Color {
    let hue = hue.fract().abs() * 6.0;
    let sector = hue.floor() as i32;
    let fraction = hue - sector as f32;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * fraction);
    let t = value * (1.0 - saturation * (1.0 - fraction));
    let (red, green, blue) = match sector.rem_euclid(6) {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        _ => (value, p, q),
    };
    Color::new(red, green, blue, 1.0)
}

fn draw_centered_label(label: &str, y: f32, size: f32, alpha: f32) {
    let measured = measure_text(label, None, size as u16, 1.0);
    draw_text(
        label,
        (screen_width() - measured.width) * 0.5,
        y,
        size,
        Color::new(0.88, 0.92, 1.0, alpha),
    );
}

fn draw_scanlines() {
    let mut y = 0.0;
    while y < screen_height() {
        draw_rectangle(
            0.0,
            y,
            screen_width(),
            1.0,
            Color::new(0.0, 0.0, 0.0, 0.075),
        );
        y += 4.0;
    }
}

fn draw_floor_and_ceiling(
    position: Vec2,
    angle: f32,
    width: f32,
    height: f32,
    horizon: f32,
    retro: bool,
) {
    // Floor-casting makes the tiles occupy the same world as the walls, so
    // they shrink naturally toward the horizon instead of appearing as bands.
    let block = (width / 320.0).max(2.0);
    let focal = (width * 0.5) / (FOV * 0.5).tan();
    let mut y = horizon + block;
    while y < height {
        let row_from_center = (y - horizon).max(1.0);
        let row_distance = (height * 0.5 * focal / row_from_center) / width;
        let depth_shade = (1.0 / (1.0 + row_distance * 0.075)).clamp(0.34, 1.0);
        let mut x = 0.0;
        while x < width {
            let camera_x = (2.0 * (x + block * 0.5) / width) - 1.0;
            let ray_angle = angle + camera_x * (FOV * 0.5);
            let corrected_distance = row_distance / (ray_angle - angle).cos().max(0.2);
            let world = position + vec2(ray_angle.cos(), ray_angle.sin()) * corrected_distance;

            // Warm, lightly mottled stone floor tiles like the original.
            let tile_x = world.x.floor() as i32;
            let tile_y = world.y.floor() as i32;
            let checker = (tile_x + tile_y).rem_euclid(2) == 0;
            let floor_color = if retro {
                if checker {
                    Color::new(
                        0.018 * depth_shade,
                        0.035 * depth_shade,
                        0.115 * depth_shade,
                        1.0,
                    )
                } else {
                    Color::new(
                        0.035 * depth_shade,
                        0.018 * depth_shade,
                        0.105 * depth_shade,
                        1.0,
                    )
                }
            } else if checker {
                Color::new(
                    0.54 * depth_shade,
                    0.39 * depth_shade,
                    0.20 * depth_shade,
                    1.0,
                )
            } else {
                Color::new(
                    0.42 * depth_shade,
                    0.29 * depth_shade,
                    0.14 * depth_shade,
                    1.0,
                )
            };

            let grout = world.x.fract().abs() < 0.055 || world.y.fract().abs() < 0.055;
            let floor_color = if grout && retro {
                let mut neon = super::playfusion_neon_color(
                    (world.x + world.y) * 0.045 + get_time() as f32 * 0.018,
                );
                neon.r *= depth_shade * 0.78;
                neon.g *= depth_shade * 0.78;
                neon.b *= depth_shade * 0.78;
                neon
            } else if grout {
                Color::new(
                    0.20 * depth_shade,
                    0.17 * depth_shade,
                    0.13 * depth_shade,
                    1.0,
                )
            } else {
                floor_color
            };
            draw_rectangle(x, y, block + 0.5, block + 0.5, floor_color);

            // The Windows maze used a bright tiled ceiling. Mirror the same
            // world sample above the horizon with cool off-white panels.
            let ceiling_y = horizon - (y - horizon) - block;
            let ceiling_color = if retro && grout {
                let mut neon = super::playfusion_neon_color(
                    (world.x - world.y) * 0.04 + get_time() as f32 * 0.016 + 0.25,
                );
                neon.r *= depth_shade * 0.58;
                neon.g *= depth_shade * 0.58;
                neon.b *= depth_shade * 0.58;
                neon
            } else if retro {
                Color::new(
                    0.012 * depth_shade,
                    0.018 * depth_shade,
                    (0.07 + if checker { 0.025 } else { 0.0 }) * depth_shade,
                    1.0,
                )
            } else if grout {
                Color::new(
                    0.30 * depth_shade,
                    0.32 * depth_shade,
                    0.33 * depth_shade,
                    1.0,
                )
            } else if checker {
                Color::new(
                    0.83 * depth_shade,
                    0.84 * depth_shade,
                    0.78 * depth_shade,
                    1.0,
                )
            } else {
                Color::new(
                    0.68 * depth_shade,
                    0.72 * depth_shade,
                    0.69 * depth_shade,
                    1.0,
                )
            };
            draw_rectangle(
                x,
                ceiling_y.max(0.0),
                block + 0.5,
                block + 0.5,
                ceiling_color,
            );

            x += block;
        }
        y += block;
    }
}

struct RayHit {
    distance: f32,
    wall_offset: f32,
    vertical: bool,
    map_x: i32,
    map_y: i32,
}

fn cast_ray(maze: &[[bool; MAZE_SIZE]; MAZE_SIZE], position: Vec2, ray: Vec2) -> RayHit {
    let mut map_x = position.x.floor() as i32;
    let mut map_y = position.y.floor() as i32;
    let delta_x = if ray.x.abs() < 0.0001 {
        1.0e30
    } else {
        (1.0 / ray.x).abs()
    };
    let delta_y = if ray.y.abs() < 0.0001 {
        1.0e30
    } else {
        (1.0 / ray.y).abs()
    };
    let step_x = if ray.x < 0.0 { -1 } else { 1 };
    let step_y = if ray.y < 0.0 { -1 } else { 1 };
    let mut side_x = if ray.x < 0.0 {
        (position.x - map_x as f32) * delta_x
    } else {
        (map_x as f32 + 1.0 - position.x) * delta_x
    };
    let mut side_y = if ray.y < 0.0 {
        (position.y - map_y as f32) * delta_y
    } else {
        (map_y as f32 + 1.0 - position.y) * delta_y
    };
    let mut vertical = false;

    for _ in 0..(MAZE_SIZE * 3) {
        if side_x < side_y {
            side_x += delta_x;
            map_x += step_x;
            vertical = true;
        } else {
            side_y += delta_y;
            map_y += step_y;
            vertical = false;
        }
        if map_x < 0
            || map_y < 0
            || map_x >= MAZE_SIZE as i32
            || map_y >= MAZE_SIZE as i32
            || maze[map_y as usize][map_x as usize]
        {
            break;
        }
    }

    let distance = if vertical {
        (map_x as f32 - position.x + (1 - step_x) as f32 * 0.5) / ray.x
    } else {
        (map_y as f32 - position.y + (1 - step_y) as f32 * 0.5) / ray.y
    }
    .abs();
    let intersection = if vertical {
        position.y + distance * ray.y
    } else {
        position.x + distance * ray.x
    };
    let mut wall_offset = intersection.fract().abs();
    if (vertical && ray.x > 0.0) || (!vertical && ray.y < 0.0) {
        wall_offset = 1.0 - wall_offset;
    }

    RayHit {
        distance,
        wall_offset,
        vertical,
        map_x,
        map_y,
    }
}

fn generate_maze() -> [[bool; MAZE_SIZE]; MAZE_SIZE] {
    let mut maze = [[true; MAZE_SIZE]; MAZE_SIZE];
    let mut visited = [[false; MAZE_SIZE]; MAZE_SIZE];
    let mut stack = vec![ivec2(1, 1)];
    visited[1][1] = true;
    maze[1][1] = false;

    while let Some(current) = stack.last().copied() {
        let mut neighbors = Vec::with_capacity(4);
        for direction in [ivec2(2, 0), ivec2(-2, 0), ivec2(0, 2), ivec2(0, -2)] {
            let next = current + direction;
            if next.x > 0
                && next.y > 0
                && next.x < (MAZE_SIZE - 1) as i32
                && next.y < (MAZE_SIZE - 1) as i32
                && !visited[next.y as usize][next.x as usize]
            {
                neighbors.push(next);
            }
        }

        if neighbors.is_empty() {
            stack.pop();
            continue;
        }

        let index = macroquad::rand::gen_range(0, neighbors.len() as i32) as usize;
        let next = neighbors[index];
        let between = (current + next) / 2;
        maze[between.y as usize][between.x as usize] = false;
        maze[next.y as usize][next.x as usize] = false;
        visited[next.y as usize][next.x as usize] = true;
        stack.push(next);
    }

    maze
}

fn is_open(maze: &[[bool; MAZE_SIZE]; MAZE_SIZE], cell: IVec2) -> bool {
    cell.x >= 0
        && cell.y >= 0
        && cell.x < MAZE_SIZE as i32
        && cell.y < MAZE_SIZE as i32
        && !maze[cell.y as usize][cell.x as usize]
}

fn wrap_angle(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}
