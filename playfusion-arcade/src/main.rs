use gilrs::{Axis, Button, Gilrs};
use macroquad::audio::{load_sound_from_bytes, play_sound, PlaySoundParams, Sound};
use macroquad::prelude::*;
use std::f32::consts::TAU;

const GAMES: [(&str, &str); 6] = [
    ("NEON RIFT", "Dodge, blast, and survive the rift"),
    ("FUSION PONG", "First paddle to seven wins"),
    ("LASER BREAKER", "Clear the reactor brick field"),
    ("GRID SNAKE", "Collect energy without crossing your trail"),
    ("ASTRO BLASTER", "Defend the arcade from the swarm"),
    ("TURBO TUNNEL", "Switch lanes and outrun the grid"),
];

fn window_conf() -> Conf {
    Conf {
        window_title: "PlayFusion Arcade".to_string(),
        window_width: 1280,
        window_height: 720,
        window_resizable: true,
        fullscreen: false,
        ..Default::default()
    }
}

#[derive(Clone, Copy, Default)]
struct FrameInput {
    x: f32,
    y: f32,
    select: bool,
    select_pressed: bool,
    back_pressed: bool,
    up_pressed: bool,
    down_pressed: bool,
    left_pressed: bool,
    right_pressed: bool,
}

#[derive(Default)]
struct Controls {
    previous_select: bool,
    previous_back: bool,
    previous_up: bool,
    previous_down: bool,
    previous_left: bool,
    previous_right: bool,
}

#[derive(Default)]
struct SoundEvents {
    move_cursor: bool,
    select: bool,
    shoot: bool,
    hit: bool,
    score: bool,
    damage: bool,
}

struct SoundBank {
    music: Option<Sound>,
    move_cursor: Option<Sound>,
    select: Option<Sound>,
    shoot: Option<Sound>,
    hit: Option<Sound>,
    score: Option<Sound>,
    damage: Option<Sound>,
    game_over: Option<Sound>,
}

impl SoundBank {
    async fn load() -> Self {
        Self {
            music: load_sound_from_bytes(&music_wav()).await.ok(),
            move_cursor: load_sound_from_bytes(&tone_wav(520.0, 0.055, 0.28, 0)).await.ok(),
            select: load_sound_from_bytes(&tone_wav(760.0, 0.12, 0.35, 1)).await.ok(),
            shoot: load_sound_from_bytes(&tone_wav(980.0, 0.07, 0.24, 2)).await.ok(),
            hit: load_sound_from_bytes(&tone_wav(235.0, 0.09, 0.34, 3)).await.ok(),
            score: load_sound_from_bytes(&chord_wav(&[660.0, 880.0, 1100.0], 0.22)).await.ok(),
            damage: load_sound_from_bytes(&tone_wav(92.0, 0.28, 0.46, 3)).await.ok(),
            game_over: load_sound_from_bytes(&melody_wav(&[330.0, 247.0, 196.0, 147.0], 0.18)).await.ok(),
        }
    }

    fn start_music(&self) {
        if let Some(sound) = &self.music {
            play_sound(sound, PlaySoundParams { looped: true, volume: 0.24 });
        }
    }

    fn play_one(sound: &Option<Sound>, volume: f32) {
        if let Some(sound) = sound {
            play_sound(sound, PlaySoundParams { looped: false, volume });
        }
    }

    fn play_events(&self, events: &SoundEvents) {
        if events.move_cursor { Self::play_one(&self.move_cursor, 0.32); }
        if events.select { Self::play_one(&self.select, 0.42); }
        if events.shoot { Self::play_one(&self.shoot, 0.26); }
        if events.hit { Self::play_one(&self.hit, 0.34); }
        if events.score { Self::play_one(&self.score, 0.40); }
        if events.damage { Self::play_one(&self.damage, 0.48); }
    }
}

fn wav_from_samples(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_size = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

fn tone_wav(frequency: f32, duration: f32, volume: f32, waveform: u8) -> Vec<u8> {
    const RATE: u32 = 22_050;
    let count = (duration * RATE as f32) as usize;
    let mut seed = 0x1234_5678u32;
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let t = index as f32 / RATE as f32;
        let envelope = (1.0 - t / duration).max(0.0).powf(1.35);
        let phase = (t * frequency).fract();
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = ((seed >> 16) as f32 / 32768.0) - 1.0;
        let wave = match waveform {
            1 => (TAU * phase).sin(),
            2 => 1.0 - 2.0 * phase,
            3 => 0.58 * if phase < 0.5 { 1.0 } else { -1.0 } + 0.42 * noise,
            _ => if phase < 0.5 { 1.0 } else { -1.0 },
        };
        samples.push((wave * envelope * volume * i16::MAX as f32) as i16);
    }
    wav_from_samples(&samples, RATE)
}

fn chord_wav(notes: &[f32], duration: f32) -> Vec<u8> {
    const RATE: u32 = 22_050;
    let count = (duration * RATE as f32) as usize;
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let t = index as f32 / RATE as f32;
        let envelope = (1.0 - t / duration).max(0.0);
        let mixed = notes.iter().map(|frequency| {
            if (t * frequency).fract() < 0.5 { 1.0 } else { -1.0 }
        }).sum::<f32>() / notes.len().max(1) as f32;
        samples.push((mixed * envelope * 0.30 * i16::MAX as f32) as i16);
    }
    wav_from_samples(&samples, RATE)
}

fn melody_wav(notes: &[f32], note_duration: f32) -> Vec<u8> {
    const RATE: u32 = 22_050;
    let total_duration = notes.len() as f32 * note_duration;
    let count = (total_duration * RATE as f32) as usize;
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let t = index as f32 / RATE as f32;
        let note_index = ((t / note_duration) as usize).min(notes.len().saturating_sub(1));
        let local = (t % note_duration) / note_duration;
        let phase = (t * notes[note_index]).fract();
        let wave = if phase < 0.5 { 1.0 } else { -1.0 };
        samples.push((wave * (1.0 - local) * 0.30 * i16::MAX as f32) as i16);
    }
    wav_from_samples(&samples, RATE)
}

fn music_wav() -> Vec<u8> {
    const RATE: u32 = 22_050;
    const STEP: f32 = 0.16;
    const STEPS: usize = 48;
    let melody = [
        329.63, 392.00, 493.88, 659.25, 392.00, 493.88, 587.33, 783.99,
        293.66, 369.99, 440.00, 587.33, 369.99, 440.00, 554.37, 739.99,
    ];
    let bass = [82.41, 82.41, 98.00, 98.00, 73.42, 73.42, 92.50, 92.50];
    let count = (STEP * STEPS as f32 * RATE as f32) as usize;
    let mut seed = 0x9e37_79b9u32;
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let t = index as f32 / RATE as f32;
        let step = (t / STEP) as usize;
        let local = (t % STEP) / STEP;
        let lead_phase = (t * melody[step % melody.len()]).fract();
        let bass_phase = (t * bass[(step / 2) % bass.len()]).fract();
        let lead = if lead_phase < 0.5 { 1.0 } else { -1.0 };
        let bass_wave = 1.0 - 2.0 * bass_phase;
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = ((seed >> 16) as f32 / 32768.0) - 1.0;
        let kick = if step % 4 == 0 { noise * (1.0 - local).powf(5.0) } else { 0.0 };
        let sample = lead * (1.0 - local * 0.55) * 0.18 + bass_wave * 0.13 + kick * 0.10;
        samples.push((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
    }
    wav_from_samples(&samples, RATE)
}

impl Controls {
    fn poll(&mut self, gilrs: &mut Option<Gilrs>) -> FrameInput {
        if let Some(gilrs) = gilrs {
            while gilrs.next_event().is_some() {}
        }

        let mut x: f32 = if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
            -1.0
        } else if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
            1.0
        } else {
            0.0
        };
        let mut y: f32 = if is_key_down(KeyCode::Up) || is_key_down(KeyCode::W) {
            -1.0
        } else if is_key_down(KeyCode::Down) || is_key_down(KeyCode::S) {
            1.0
        } else {
            0.0
        };
        let mut select = is_key_down(KeyCode::Enter) || is_key_down(KeyCode::Space);
        let mut back = is_key_down(KeyCode::Escape) || is_key_down(KeyCode::Backspace);
        let mut up = y < -0.45;
        let mut down = y > 0.45;
        let mut left = x < -0.45;
        let mut right = x > 0.45;

        if let Some(gilrs) = gilrs {
            if let Some((_, gamepad)) = gilrs.gamepads().next() {
                let stick_x = gamepad.value(Axis::LeftStickX);
                let stick_y = -gamepad.value(Axis::LeftStickY);
                if stick_x.abs() > x.abs() {
                    x = if stick_x.abs() > 0.18 { stick_x } else { 0.0 };
                }
                if stick_y.abs() > y.abs() {
                    y = if stick_y.abs() > 0.18 { stick_y } else { 0.0 };
                }
                select |= gamepad.is_pressed(Button::South);
                back |= gamepad.is_pressed(Button::East) || gamepad.is_pressed(Button::Start);
                // This SHANWAN/Xbox-compatible pad reports its vertical hat in reverse.
                // Keep the analog stick conventional and correct only the physical D-pad.
                up |= gamepad.is_pressed(Button::DPadDown) || y < -0.52;
                down |= gamepad.is_pressed(Button::DPadUp) || y > 0.52;
                left |= gamepad.is_pressed(Button::DPadLeft) || x < -0.52;
                right |= gamepad.is_pressed(Button::DPadRight) || x > 0.52;
            }
        }

        let frame = FrameInput {
            x,
            y,
            select,
            select_pressed: select && !self.previous_select,
            back_pressed: back && !self.previous_back,
            up_pressed: up && !self.previous_up,
            down_pressed: down && !self.previous_down,
            left_pressed: left && !self.previous_left,
            right_pressed: right && !self.previous_right,
        };
        self.previous_select = select;
        self.previous_back = back;
        self.previous_up = up;
        self.previous_down = down;
        self.previous_left = left;
        self.previous_right = right;
        frame
    }
}

#[derive(Clone, Copy)]
enum GameKind {
    NeonRift,
    FusionPong,
    LaserBreaker,
    GridSnake,
    AstroBlaster,
    TurboTunnel,
}

impl GameKind {
    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::NeonRift,
            1 => Self::FusionPong,
            2 => Self::LaserBreaker,
            3 => Self::GridSnake,
            4 => Self::AstroBlaster,
            _ => Self::TurboTunnel,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::NeonRift => 0,
            Self::FusionPong => 1,
            Self::LaserBreaker => 2,
            Self::GridSnake => 3,
            Self::AstroBlaster => 4,
            Self::TurboTunnel => 5,
        }
    }
}

struct NeonRift {
    player: Vec2,
    bullets: Vec<Vec2>,
    enemies: Vec<Vec2>,
    spawn: f32,
    fire: f32,
    score: f32,
    lives: i32,
}

impl NeonRift {
    fn new() -> Self {
        Self {
            player: vec2(0.16, 0.5),
            bullets: Vec::new(),
            enemies: Vec::new(),
            spawn: 0.4,
            fire: 0.0,
            score: 0.0,
            lives: 3,
        }
    }
}

struct FusionPong {
    player_y: f32,
    cpu_y: f32,
    ball: Vec2,
    velocity: Vec2,
    player_score: i32,
    cpu_score: i32,
}

impl FusionPong {
    fn new() -> Self {
        Self {
            player_y: 0.5,
            cpu_y: 0.5,
            ball: vec2(0.5, 0.5),
            velocity: vec2(0.48, 0.29),
            player_score: 0,
            cpu_score: 0,
        }
    }
}

struct LaserBreaker {
    paddle: f32,
    ball: Vec2,
    velocity: Vec2,
    bricks: Vec<bool>,
    lives: i32,
    score: i32,
}

impl LaserBreaker {
    fn new() -> Self {
        Self {
            paddle: 0.5,
            ball: vec2(0.5, 0.72),
            velocity: vec2(0.35, -0.44),
            bricks: vec![true; 40],
            lives: 3,
            score: 0,
        }
    }
}

struct GridSnake {
    body: Vec<IVec2>,
    direction: IVec2,
    food: IVec2,
    timer: f32,
    score: i32,
    dead: bool,
}

impl GridSnake {
    fn new() -> Self {
        Self {
            body: vec![ivec2(10, 7), ivec2(9, 7), ivec2(8, 7)],
            direction: ivec2(1, 0),
            food: ivec2(16, 5),
            timer: 0.0,
            score: 0,
            dead: false,
        }
    }
}

struct AstroBlaster {
    player_x: f32,
    enemies: Vec<Vec2>,
    bullets: Vec<Vec2>,
    enemy_bullets: Vec<Vec2>,
    enemy_direction: f32,
    fire: f32,
    enemy_fire: f32,
    score: i32,
    lives: i32,
}

impl AstroBlaster {
    fn new() -> Self {
        let mut enemies = Vec::new();
        for row in 0..4 {
            for column in 0..8 {
                enemies.push(vec2(0.20 + column as f32 * 0.085, 0.16 + row as f32 * 0.075));
            }
        }
        Self {
            player_x: 0.5,
            enemies,
            bullets: Vec::new(),
            enemy_bullets: Vec::new(),
            enemy_direction: 0.075,
            fire: 0.0,
            enemy_fire: 0.8,
            score: 0,
            lives: 3,
        }
    }
}

struct TurboTunnel {
    lane: i32,
    obstacles: Vec<(i32, f32)>,
    spawn: f32,
    score: f32,
    lives: i32,
}

impl TurboTunnel {
    fn new() -> Self {
        Self {
            lane: 1,
            obstacles: Vec::new(),
            spawn: 0.8,
            score: 0.0,
            lives: 3,
        }
    }
}

enum ActiveGame {
    NeonRift(NeonRift),
    FusionPong(FusionPong),
    LaserBreaker(LaserBreaker),
    GridSnake(GridSnake),
    AstroBlaster(AstroBlaster),
    TurboTunnel(TurboTunnel),
}

impl ActiveGame {
    fn new(kind: GameKind) -> Self {
        match kind {
            GameKind::NeonRift => Self::NeonRift(NeonRift::new()),
            GameKind::FusionPong => Self::FusionPong(FusionPong::new()),
            GameKind::LaserBreaker => Self::LaserBreaker(LaserBreaker::new()),
            GameKind::GridSnake => Self::GridSnake(GridSnake::new()),
            GameKind::AstroBlaster => Self::AstroBlaster(AstroBlaster::new()),
            GameKind::TurboTunnel => Self::TurboTunnel(TurboTunnel::new()),
        }
    }

    fn kind(&self) -> GameKind {
        match self {
            Self::NeonRift(_) => GameKind::NeonRift,
            Self::FusionPong(_) => GameKind::FusionPong,
            Self::LaserBreaker(_) => GameKind::LaserBreaker,
            Self::GridSnake(_) => GameKind::GridSnake,
            Self::AstroBlaster(_) => GameKind::AstroBlaster,
            Self::TurboTunnel(_) => GameKind::TurboTunnel,
        }
    }
}

enum AppMode {
    Menu,
    Playing(ActiveGame),
}

fn brand_color(phase: f32) -> Color {
    let palette = [
        Color::new(0.05, 0.88, 1.0, 1.0),
        Color::new(0.08, 0.46, 1.0, 1.0),
        Color::new(0.55, 0.12, 1.0, 1.0),
        Color::new(1.0, 0.10, 0.70, 1.0),
        Color::new(1.0, 0.34, 0.12, 1.0),
        Color::new(1.0, 0.10, 0.70, 1.0),
    ];
    let position = phase.rem_euclid(1.0) * palette.len() as f32;
    let index = position.floor() as usize % palette.len();
    let next = (index + 1) % palette.len();
    let amount = position.fract();
    Color::new(
        palette[index].r + (palette[next].r - palette[index].r) * amount,
        palette[index].g + (palette[next].g - palette[index].g) * amount,
        palette[index].b + (palette[next].b - palette[index].b) * amount,
        1.0,
    )
}

fn cabinet_screen() -> Rect {
    Rect::new(
        screen_width() * 0.145,
        screen_height() * 0.145,
        screen_width() * 0.71,
        screen_height() * 0.59,
    )
}

fn to_screen(screen: Rect, point: Vec2) -> Vec2 {
    vec2(screen.x + point.x * screen.w, screen.y + point.y * screen.h)
}

fn draw_centered(text: &str, y: f32, size: f32, color: Color) {
    let dimensions = measure_text(text, None, size as u16, 1.0);
    draw_text(text, (screen_width() - dimensions.width) * 0.5, y, size, color);
}

fn draw_arcade_shell(screen: Rect, title: &str) {
    clear_background(Color::new(0.008, 0.006, 0.025, 1.0));
    let time = get_time() as f32;
    for row in 0..18 {
        let y = screen_height() * 0.56 + row as f32 * screen_height() * 0.034;
        let spread = row as f32 * screen_width() * 0.04;
        let color = brand_color(row as f32 / 18.0 + time * 0.015);
        draw_line(screen_width() * 0.5 - spread, y, screen_width() * 0.5 + spread, y, 1.0, Color::new(color.r, color.g, color.b, 0.16));
    }

    draw_rectangle(
        screen.x - screen_width() * 0.055,
        screen.y - screen_height() * 0.105,
        screen.w + screen_width() * 0.11,
        screen.h + screen_height() * 0.235,
        Color::new(0.018, 0.02, 0.05, 0.98),
    );
    for layer in 0..4 {
        let inset = layer as f32 * 3.0;
        let color = brand_color(time * 0.035 + layer as f32 * 0.18);
        draw_rectangle_lines(
            screen.x - 16.0 + inset,
            screen.y - 16.0 + inset,
            screen.w + 32.0 - inset * 2.0,
            screen.h + 32.0 - inset * 2.0,
            2.0,
            Color::new(color.r, color.g, color.b, 0.82 - layer as f32 * 0.12),
        );
    }
    draw_rectangle(screen.x, screen.y, screen.w, screen.h, Color::new(0.002, 0.008, 0.024, 1.0));

    let title_size = (screen_height() * 0.045).clamp(22.0, 44.0);
    let title_dims = measure_text(title, None, title_size as u16, 1.0);
    draw_text(
        title,
        (screen_width() - title_dims.width) * 0.5,
        screen.y - screen_height() * 0.045,
        title_size,
        brand_color(time * 0.025),
    );
}

fn draw_crt_overlay(screen: Rect) {
    // Strong enough to remain visible after Gamescope scales the 720p cabinet.
    let mut y = screen.y + 1.0;
    while y < screen.y + screen.h {
        draw_rectangle(screen.x, y, screen.w, 2.0, Color::new(0.0, 0.0, 0.0, 0.28));
        y += 5.0;
    }
    let mut x = screen.x + 1.0;
    while x < screen.x + screen.w {
        draw_rectangle(x, screen.y, 1.0, screen.h, Color::new(0.10, 0.45, 1.0, 0.035));
        x += 4.0;
    }
    let sweep = screen.y + ((get_time() as f32 * 74.0).rem_euclid(screen.h + 90.0)) - 45.0;
    draw_rectangle(screen.x, sweep, screen.w, 28.0, Color::new(0.20, 0.72, 1.0, 0.035));
    draw_rectangle(screen.x, screen.y, screen.w, screen.h * 0.13, Color::new(0.42, 0.72, 1.0, 0.025));
    draw_rectangle_lines(screen.x, screen.y, screen.w, screen.h, 5.0, Color::new(0.08, 0.75, 1.0, 0.36));
    draw_rectangle(screen.x, screen.y, screen.w, 12.0, Color::new(0.0, 0.0, 0.0, 0.22));
    draw_rectangle(screen.x, screen.y + screen.h - 12.0, screen.w, 12.0, Color::new(0.0, 0.0, 0.0, 0.28));
    draw_rectangle(screen.x, screen.y, 12.0, screen.h, Color::new(0.0, 0.0, 0.0, 0.22));
    draw_rectangle(screen.x + screen.w - 12.0, screen.y, 12.0, screen.h, Color::new(0.0, 0.0, 0.0, 0.28));

    let panel_y = screen.y + screen.h + screen_height() * 0.045;
    draw_circle(screen_width() * 0.39, panel_y, 13.0, Color::new(1.0, 0.12, 0.62, 1.0));
    draw_circle(screen_width() * 0.61, panel_y, 11.0, Color::new(0.05, 0.75, 1.0, 1.0));
    draw_centered(
        "A / ENTER: START & FIRE     B / ESC: BACK",
        screen_height() * 0.91,
        (screen_height() * 0.025).clamp(14.0, 24.0),
        Color::new(0.72, 0.84, 1.0, 1.0),
    );
}

fn draw_ship(position: Vec2, scale: f32, color: Color) {
    draw_triangle(
        vec2(position.x + scale, position.y),
        vec2(position.x - scale * 0.75, position.y - scale * 0.58),
        vec2(position.x - scale * 0.75, position.y + scale * 0.58),
        color,
    );
    draw_circle(position.x - scale * 0.55, position.y, scale * 0.22, WHITE);
}

fn update_neon_rift(game: &mut NeonRift, input: FrameInput, dt: f32, sounds: &mut SoundEvents) -> bool {
    game.player += vec2(input.x, input.y) * dt * 0.48;
    game.player.x = game.player.x.clamp(0.08, 0.55);
    game.player.y = game.player.y.clamp(0.08, 0.92);
    game.fire -= dt;
    if input.select && game.fire <= 0.0 {
        game.bullets.push(game.player + vec2(0.055, 0.0));
        game.fire = 0.15;
        sounds.shoot = true;
    }
    game.spawn -= dt;
    if game.spawn <= 0.0 {
        game.enemies.push(vec2(1.05, macroquad::rand::gen_range(0.08, 0.92)));
        game.spawn = macroquad::rand::gen_range(0.24, 0.62);
    }
    for bullet in &mut game.bullets {
        bullet.x += dt * 0.95;
    }
    for enemy in &mut game.enemies {
        enemy.x -= dt * (0.30 + game.score * 0.00025).min(0.66);
    }

    let mut removed_bullets = Vec::new();
    let mut removed_enemies = Vec::new();
    for (bullet_index, bullet) in game.bullets.iter().enumerate() {
        for (enemy_index, enemy) in game.enemies.iter().enumerate() {
            if bullet.distance(*enemy) < 0.045 {
                removed_bullets.push(bullet_index);
                removed_enemies.push(enemy_index);
                game.score += 75.0;
                sounds.hit = true;
            }
        }
    }
    removed_bullets.sort_unstable();
    removed_bullets.dedup();
    removed_enemies.sort_unstable();
    removed_enemies.dedup();
    game.bullets = game
        .bullets
        .drain(..)
        .enumerate()
        .filter_map(|(index, value)| (!removed_bullets.contains(&index) && value.x < 1.1).then_some(value))
        .collect();
    game.enemies = game
        .enemies
        .drain(..)
        .enumerate()
        .filter_map(|(index, value)| (!removed_enemies.contains(&index)).then_some(value))
        .collect();

    let mut hit = false;
    game.enemies.retain(|enemy| {
        let collision = enemy.distance(game.player) < 0.052;
        hit |= collision;
        !collision && enemy.x >= -0.08
    });
    if hit {
        game.lives -= 1;
        sounds.damage = true;
    }
    game.score += dt * 12.0;
    game.lives <= 0
}

fn draw_neon_rift(game: &NeonRift, screen: Rect) {
    for star in 0..80 {
        let x = ((star * 73) as f32 + get_time() as f32 * (20 + star % 5 * 9) as f32).rem_euclid(screen.w);
        let y = ((star * 47) as f32).rem_euclid(screen.h);
        draw_circle(screen.x + screen.w - x, screen.y + y, 1.2, Color::new(0.4, 0.8, 1.0, 0.7));
    }
    draw_ship(to_screen(screen, game.player), screen.h * 0.035, brand_color(get_time() as f32 * 0.04));
    for bullet in &game.bullets {
        let point = to_screen(screen, *bullet);
        draw_line(point.x - 16.0, point.y, point.x + 8.0, point.y, 4.0, Color::new(0.2, 0.9, 1.0, 1.0));
    }
    for enemy in &game.enemies {
        let point = to_screen(screen, *enemy);
        draw_poly(point.x, point.y, 6, screen.h * 0.03, get_time() as f32 * 70.0, Color::new(1.0, 0.1, 0.65, 1.0));
    }
    draw_text(&format!("SCORE {:06}", game.score as i32), screen.x + 18.0, screen.y + 32.0, 26.0, WHITE);
    draw_text(&format!("SHIELDS {}", game.lives), screen.x + screen.w - 165.0, screen.y + 32.0, 24.0, Color::new(1.0, 0.35, 0.16, 1.0));
}

fn update_pong(game: &mut FusionPong, input: FrameInput, dt: f32, sounds: &mut SoundEvents) -> bool {
    game.player_y = (game.player_y + input.y * dt * 0.75).clamp(0.12, 0.88);
    game.cpu_y += (game.ball.y - game.cpu_y).clamp(-dt * 0.52, dt * 0.52);
    game.ball += game.velocity * dt;
    if game.ball.y < 0.04 || game.ball.y > 0.96 {
        game.velocity.y *= -1.0;
        game.ball.y = game.ball.y.clamp(0.04, 0.96);
        sounds.hit = true;
    }
    if game.ball.x < 0.10 && game.velocity.x < 0.0 && (game.ball.y - game.player_y).abs() < 0.16 {
        game.velocity.x = game.velocity.x.abs() * 1.04;
        game.velocity.y += (game.ball.y - game.player_y) * 0.9;
        sounds.hit = true;
    }
    if game.ball.x > 0.90 && game.velocity.x > 0.0 && (game.ball.y - game.cpu_y).abs() < 0.16 {
        game.velocity.x = -game.velocity.x.abs() * 1.04;
        game.velocity.y += (game.ball.y - game.cpu_y) * 0.8;
        sounds.hit = true;
    }
    if game.ball.x < -0.03 || game.ball.x > 1.03 {
        if game.ball.x < 0.0 {
            game.cpu_score += 1;
        } else {
            game.player_score += 1;
        }
        sounds.score = true;
        game.ball = vec2(0.5, 0.5);
        game.velocity = vec2(if game.player_score <= game.cpu_score { -0.48 } else { 0.48 }, macroquad::rand::gen_range(-0.34, 0.34));
    }
    game.player_score >= 7 || game.cpu_score >= 7
}

fn draw_pong(game: &FusionPong, screen: Rect) {
    let center_x = screen.x + screen.w * 0.5;
    let mut y = screen.y + 12.0;
    while y < screen.y + screen.h {
        draw_rectangle(center_x - 2.0, y, 4.0, 14.0, Color::new(0.3, 0.55, 1.0, 0.5));
        y += 27.0;
    }
    let paddle_h = screen.h * 0.24;
    draw_rectangle(screen.x + screen.w * 0.07, screen.y + game.player_y * screen.h - paddle_h * 0.5, 12.0, paddle_h, Color::new(1.0, 0.1, 0.7, 1.0));
    draw_rectangle(screen.x + screen.w * 0.91, screen.y + game.cpu_y * screen.h - paddle_h * 0.5, 12.0, paddle_h, Color::new(0.05, 0.85, 1.0, 1.0));
    let ball = to_screen(screen, game.ball);
    draw_circle(ball.x, ball.y, screen.h * 0.025, WHITE);
    draw_text(&game.player_score.to_string(), center_x - 80.0, screen.y + 58.0, 52.0, Color::new(1.0, 0.1, 0.7, 1.0));
    draw_text(&game.cpu_score.to_string(), center_x + 45.0, screen.y + 58.0, 52.0, Color::new(0.05, 0.85, 1.0, 1.0));
}

fn update_breaker(game: &mut LaserBreaker, input: FrameInput, dt: f32, sounds: &mut SoundEvents) -> bool {
    game.paddle = (game.paddle + input.x * dt * 0.72).clamp(0.13, 0.87);
    game.ball += game.velocity * dt;
    if game.ball.x < 0.025 || game.ball.x > 0.975 {
        game.velocity.x *= -1.0;
        sounds.hit = true;
    }
    if game.ball.y < 0.04 {
        game.velocity.y = game.velocity.y.abs();
        sounds.hit = true;
    }
    if game.ball.y > 0.86 && game.ball.y < 0.92 && (game.ball.x - game.paddle).abs() < 0.14 && game.velocity.y > 0.0 {
        game.velocity.y = -game.velocity.y.abs();
        game.velocity.x += (game.ball.x - game.paddle) * 1.1;
        sounds.hit = true;
    }
    for index in 0..game.bricks.len() {
        if !game.bricks[index] {
            continue;
        }
        let column = index % 8;
        let row = index / 8;
        let bx = 0.08 + column as f32 * 0.115;
        let by = 0.10 + row as f32 * 0.075;
        if game.ball.x >= bx && game.ball.x <= bx + 0.095 && game.ball.y >= by && game.ball.y <= by + 0.052 {
            game.bricks[index] = false;
            game.velocity.y *= -1.0;
            game.score += 100;
            sounds.score = true;
            break;
        }
    }
    if game.ball.y > 1.03 {
        game.lives -= 1;
        sounds.damage = true;
        game.ball = vec2(game.paddle, 0.72);
        game.velocity = vec2(0.34, -0.44);
    }
    game.lives <= 0 || game.bricks.iter().all(|brick| !*brick)
}

fn draw_breaker(game: &LaserBreaker, screen: Rect) {
    for (index, active) in game.bricks.iter().enumerate() {
        if !active {
            continue;
        }
        let column = index % 8;
        let row = index / 8;
        let x = screen.x + screen.w * (0.08 + column as f32 * 0.115);
        let y = screen.y + screen.h * (0.10 + row as f32 * 0.075);
        let color = brand_color(row as f32 / 5.0 + column as f32 * 0.03);
        draw_rectangle(x, y, screen.w * 0.095, screen.h * 0.052, color);
    }
    draw_rectangle(screen.x + screen.w * (game.paddle - 0.12), screen.y + screen.h * 0.89, screen.w * 0.24, 12.0, Color::new(0.08, 0.86, 1.0, 1.0));
    let ball = to_screen(screen, game.ball);
    draw_circle(ball.x, ball.y, screen.h * 0.022, WHITE);
    draw_text(&format!("SCORE {:05}  LIVES {}", game.score, game.lives), screen.x + 15.0, screen.y + screen.h - 16.0, 22.0, WHITE);
}

fn update_snake(game: &mut GridSnake, input: FrameInput, dt: f32, sounds: &mut SoundEvents) -> bool {
    let requested = if input.up_pressed {
        ivec2(0, -1)
    } else if input.down_pressed {
        ivec2(0, 1)
    } else if input.left_pressed {
        ivec2(-1, 0)
    } else if input.right_pressed {
        ivec2(1, 0)
    } else {
        game.direction
    };
    if requested != -game.direction {
        game.direction = requested;
    }
    game.timer += dt;
    if game.timer >= (0.13 - game.score as f32 * 0.002).max(0.055) {
        game.timer = 0.0;
        let head = game.body[0] + game.direction;
        if head.x < 0 || head.y < 0 || head.x >= 22 || head.y >= 14 || game.body.contains(&head) {
            game.dead = true;
            sounds.damage = true;
            return true;
        }
        game.body.insert(0, head);
        if head == game.food {
            game.score += 1;
            sounds.score = true;
            loop {
                let candidate = ivec2(macroquad::rand::gen_range(0, 22), macroquad::rand::gen_range(0, 14));
                if !game.body.contains(&candidate) {
                    game.food = candidate;
                    break;
                }
            }
        } else {
            game.body.pop();
        }
    }
    game.dead
}

fn draw_snake(game: &GridSnake, screen: Rect) {
    let cell_w = screen.w / 22.0;
    let cell_h = screen.h / 14.0;
    for x in 0..=22 {
        draw_line(screen.x + x as f32 * cell_w, screen.y, screen.x + x as f32 * cell_w, screen.y + screen.h, 1.0, Color::new(0.12, 0.4, 0.75, 0.22));
    }
    for y in 0..=14 {
        draw_line(screen.x, screen.y + y as f32 * cell_h, screen.x + screen.w, screen.y + y as f32 * cell_h, 1.0, Color::new(0.7, 0.1, 0.9, 0.18));
    }
    for (index, segment) in game.body.iter().enumerate() {
        let color = brand_color(index as f32 / game.body.len().max(1) as f32 * 0.45);
        draw_rectangle(screen.x + segment.x as f32 * cell_w + 2.0, screen.y + segment.y as f32 * cell_h + 2.0, cell_w - 4.0, cell_h - 4.0, color);
    }
    draw_circle(screen.x + (game.food.x as f32 + 0.5) * cell_w, screen.y + (game.food.y as f32 + 0.5) * cell_h, cell_h * 0.28, Color::new(1.0, 0.35, 0.12, 1.0));
    draw_text(&format!("ENERGY {}", game.score), screen.x + 12.0, screen.y + 28.0, 24.0, WHITE);
}

fn update_astro(game: &mut AstroBlaster, input: FrameInput, dt: f32, sounds: &mut SoundEvents) -> bool {
    game.player_x = (game.player_x + input.x * dt * 0.72).clamp(0.06, 0.94);
    game.fire -= dt;
    if input.select && game.fire <= 0.0 {
        game.bullets.push(vec2(game.player_x, 0.86));
        game.fire = 0.18;
        sounds.shoot = true;
    }
    let mut edge = false;
    for enemy in &mut game.enemies {
        enemy.x += game.enemy_direction * dt;
        edge |= enemy.x < 0.07 || enemy.x > 0.93;
    }
    if edge {
        game.enemy_direction *= -1.0;
        for enemy in &mut game.enemies {
            enemy.y += 0.035;
        }
    }
    for bullet in &mut game.bullets {
        bullet.y -= dt * 0.78;
    }
    for bullet in &mut game.enemy_bullets {
        bullet.y += dt * 0.48;
    }
    game.enemy_fire -= dt;
    if game.enemy_fire <= 0.0 && !game.enemies.is_empty() {
        let index = macroquad::rand::gen_range(0, game.enemies.len());
        game.enemy_bullets.push(game.enemies[index]);
        game.enemy_fire = macroquad::rand::gen_range(0.38, 0.92);
    }

    let mut hit_enemies = Vec::new();
    let mut hit_bullets = Vec::new();
    for (bullet_index, bullet) in game.bullets.iter().enumerate() {
        for (enemy_index, enemy) in game.enemies.iter().enumerate() {
            if bullet.distance(*enemy) < 0.038 {
                hit_bullets.push(bullet_index);
                hit_enemies.push(enemy_index);
                game.score += 50;
            }
        }
    }
    hit_enemies.sort_unstable();
    hit_enemies.dedup();
    if !hit_enemies.is_empty() {
        sounds.score = true;
    }
    hit_bullets.sort_unstable();
    hit_bullets.dedup();
    game.enemies = game.enemies.drain(..).enumerate().filter_map(|(i, value)| (!hit_enemies.contains(&i)).then_some(value)).collect();
    game.bullets = game.bullets.drain(..).enumerate().filter_map(|(i, value)| (!hit_bullets.contains(&i) && value.y > -0.05).then_some(value)).collect();
    let mut player_hit = false;
    game.enemy_bullets.retain(|bullet| {
        let hit = bullet.y > 0.84 && bullet.y < 0.94 && (bullet.x - game.player_x).abs() < 0.055;
        player_hit |= hit;
        !hit && bullet.y < 1.05
    });
    if player_hit {
        game.lives -= 1;
        sounds.damage = true;
    }
    game.lives <= 0 || game.enemies.iter().any(|enemy| enemy.y > 0.80) || game.enemies.is_empty()
}

fn draw_astro(game: &AstroBlaster, screen: Rect) {
    for enemy in &game.enemies {
        let point = to_screen(screen, *enemy);
        draw_poly(point.x, point.y, 6, screen.h * 0.025, 0.0, Color::new(1.0, 0.12, 0.66, 1.0));
        draw_circle(point.x, point.y, screen.h * 0.008, WHITE);
    }
    for bullet in &game.bullets {
        let point = to_screen(screen, *bullet);
        draw_rectangle(point.x - 2.0, point.y - 10.0, 4.0, 16.0, Color::new(0.08, 0.9, 1.0, 1.0));
    }
    for bullet in &game.enemy_bullets {
        let point = to_screen(screen, *bullet);
        draw_circle(point.x, point.y, 4.0, Color::new(1.0, 0.35, 0.12, 1.0));
    }
    draw_ship(to_screen(screen, vec2(game.player_x, 0.91)), screen.h * 0.035, Color::new(0.08, 0.9, 1.0, 1.0));
    draw_text(&format!("SCORE {:05}  SHIPS {}", game.score, game.lives), screen.x + 14.0, screen.y + 30.0, 23.0, WHITE);
}

fn update_tunnel(game: &mut TurboTunnel, input: FrameInput, dt: f32, sounds: &mut SoundEvents) -> bool {
    if input.left_pressed {
        game.lane = (game.lane - 1).max(0);
        sounds.move_cursor = true;
    }
    if input.right_pressed {
        game.lane = (game.lane + 1).min(2);
        sounds.move_cursor = true;
    }
    game.spawn -= dt;
    if game.spawn <= 0.0 {
        game.obstacles.push((macroquad::rand::gen_range(0, 3), 0.02));
        game.spawn = (0.82 - game.score * 0.0008).max(0.28);
    }
    let speed = (0.46 + game.score * 0.0007).min(0.95);
    let mut hit = false;
    for obstacle in &mut game.obstacles {
        obstacle.1 += dt * speed;
        if obstacle.0 == game.lane && obstacle.1 > 0.78 && obstacle.1 < 0.96 {
            hit = true;
            obstacle.1 = 1.2;
        }
    }
    game.obstacles.retain(|obstacle| obstacle.1 < 1.05);
    if hit {
        game.lives -= 1;
        sounds.damage = true;
    }
    game.score += dt * 30.0;
    game.lives <= 0
}

fn draw_tunnel(game: &TurboTunnel, screen: Rect) {
    let center = vec2(screen.x + screen.w * 0.5, screen.y + screen.h * 0.18);
    for line in -7..=7 {
        let bottom_x = screen.x + screen.w * (0.5 + line as f32 * 0.10);
        draw_line(center.x, center.y, bottom_x, screen.y + screen.h, 2.0, brand_color((line + 7) as f32 / 14.0));
    }
    for row in 0..10 {
        let t = ((row as f32 / 10.0 + get_time() as f32 * 0.35).fract()).powf(2.1);
        let width = screen.w * t;
        let y = center.y + (screen.y + screen.h - center.y) * t;
        draw_line(center.x - width * 0.5, y, center.x + width * 0.5, y, 2.0, Color::new(0.35, 0.2, 1.0, 0.65));
    }
    for (lane, depth) in &game.obstacles {
        let lane_x = screen.x + screen.w * (0.37 + *lane as f32 * 0.13);
        let y = screen.y + screen.h * (0.18 + depth * 0.76);
        let size = 8.0 + depth * screen.h * 0.08;
        draw_poly(lane_x, y, 4, size, 45.0, Color::new(1.0, 0.12, 0.65, 1.0));
    }
    let player_x = screen.x + screen.w * (0.37 + game.lane as f32 * 0.13);
    draw_ship(vec2(player_x, screen.y + screen.h * 0.89), screen.h * 0.045, Color::new(0.05, 0.88, 1.0, 1.0));
    draw_text(&format!("DISTANCE {:06}  ARMOR {}", game.score as i32, game.lives), screen.x + 14.0, screen.y + 30.0, 23.0, WHITE);
}

fn game_over(active: &ActiveGame) -> bool {
    match active {
        ActiveGame::NeonRift(game) => game.lives <= 0,
        ActiveGame::FusionPong(game) => game.player_score >= 7 || game.cpu_score >= 7,
        ActiveGame::LaserBreaker(game) => game.lives <= 0 || game.bricks.iter().all(|brick| !*brick),
        ActiveGame::GridSnake(game) => game.dead,
        ActiveGame::AstroBlaster(game) => game.lives <= 0 || game.enemies.is_empty() || game.enemies.iter().any(|enemy| enemy.y > 0.80),
        ActiveGame::TurboTunnel(game) => game.lives <= 0,
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut gilrs = Gilrs::new().ok();
    let mut controls = Controls::default();
    let sounds = SoundBank::load().await;
    sounds.start_music();
    let mut mode = AppMode::Menu;
    let mut selection = 0usize;
    let mut game_over_announced = false;

    loop {
        let input = controls.poll(&mut gilrs);
        let dt = get_frame_time().min(0.05);
        let screen = cabinet_screen();
        let mut sound_events = SoundEvents::default();

        match &mut mode {
            AppMode::Menu => {
                if input.up_pressed {
                    selection = if selection == 0 { GAMES.len() - 1 } else { selection - 1 };
                    sound_events.move_cursor = true;
                }
                if input.down_pressed {
                    selection = (selection + 1) % GAMES.len();
                    sound_events.move_cursor = true;
                }
                if input.select_pressed {
                    sound_events.select = true;
                    game_over_announced = false;
                    mode = AppMode::Playing(ActiveGame::new(GameKind::from_index(selection)));
                }
                if input.back_pressed {
                    break;
                }

                draw_arcade_shell(screen, "PLAYFUSION ARCADE");
                draw_centered("SELECT A CABINET", screen.y + 45.0, 30.0, WHITE);
                for (index, (name, description)) in GAMES.iter().enumerate() {
                    let y = screen.y + 92.0 + index as f32 * (screen.h - 120.0) / GAMES.len() as f32;
                    if index == selection {
                        let pulse = 0.78 + 0.22 * (get_time() as f32 * 5.0).sin();
                        let color = brand_color(index as f32 * 0.13 + get_time() as f32 * 0.025);
                        draw_rectangle(screen.x + 65.0, y - 27.0, screen.w - 130.0, 48.0, Color::new(color.r, color.g, color.b, 0.15 * pulse));
                        draw_rectangle_lines(screen.x + 65.0, y - 27.0, screen.w - 130.0, 48.0, 3.0, color);
                    }
                    draw_text(name, screen.x + 88.0, y, 27.0, if index == selection { WHITE } else { Color::new(0.42, 0.65, 0.88, 1.0) });
                    let description_size = measure_text(description, None, 17, 1.0);
                    draw_text(description, screen.x + screen.w - description_size.width - 88.0, y, 17.0, Color::new(0.66, 0.72, 0.90, 1.0));
                }
                draw_crt_overlay(screen);
            }
            AppMode::Playing(active) => {
                if input.back_pressed {
                    selection = active.kind().index();
                    mode = AppMode::Menu;
                    game_over_announced = false;
                    sound_events.select = true;
                    sounds.play_events(&sound_events);
                    next_frame().await;
                    continue;
                }

                let title = GAMES[active.kind().index()].0;
                draw_arcade_shell(screen, title);
                let already_ended = game_over(active);
                let ended = match active {
                    ActiveGame::NeonRift(game) => {
                        let ended = already_ended || update_neon_rift(game, input, dt, &mut sound_events);
                        draw_neon_rift(game, screen);
                        ended
                    }
                    ActiveGame::FusionPong(game) => {
                        let ended = already_ended || update_pong(game, input, dt, &mut sound_events);
                        draw_pong(game, screen);
                        ended
                    }
                    ActiveGame::LaserBreaker(game) => {
                        let ended = already_ended || update_breaker(game, input, dt, &mut sound_events);
                        draw_breaker(game, screen);
                        ended
                    }
                    ActiveGame::GridSnake(game) => {
                        let ended = already_ended || update_snake(game, input, dt, &mut sound_events);
                        draw_snake(game, screen);
                        ended
                    }
                    ActiveGame::AstroBlaster(game) => {
                        let ended = already_ended || update_astro(game, input, dt, &mut sound_events);
                        draw_astro(game, screen);
                        ended
                    }
                    ActiveGame::TurboTunnel(game) => {
                        let ended = already_ended || update_tunnel(game, input, dt, &mut sound_events);
                        draw_tunnel(game, screen);
                        ended
                    }
                };
                draw_crt_overlay(screen);

                if ended || game_over(active) {
                    if !game_over_announced {
                        SoundBank::play_one(&sounds.game_over, 0.55);
                        game_over_announced = true;
                    }
                    draw_rectangle(screen.x, screen.y, screen.w, screen.h, Color::new(0.0, 0.0, 0.02, 0.78));
                    draw_centered("GAME OVER", screen.y + screen.h * 0.46, 56.0, Color::new(1.0, 0.12, 0.66, 1.0));
                    draw_centered("A: RETRY     B: ARCADE MENU", screen.y + screen.h * 0.59, 25.0, WHITE);
                    if input.select_pressed {
                        let kind = active.kind();
                        *active = ActiveGame::new(kind);
                        game_over_announced = false;
                        sound_events.select = true;
                    }
                }
            }
        }
        sounds.play_events(&sound_events);
        next_frame().await;
    }
}
