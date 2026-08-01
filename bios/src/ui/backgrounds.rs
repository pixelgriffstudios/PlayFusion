use crate::BackgroundState;
use macroquad::prelude::*;
use std::{fs, path::PathBuf};

pub const PROCEDURAL_BACKGROUNDS: &[&str] = &[
    "Retro Laser Grid",
    "Cartridge Energy Core",
    "Neon Console Interior",
    "Retro Maze",
    "Prismatic Space Tunnel",
    "Console Generations",
    "Lava Lamp Plasma",
    "Digital Circuit Rain",
    "Screensaver Showcase",
    "Vibrant Spectrum",
    "Sunset Pop",
    "Aqua Pulse",
    "Vanilla Black",
    "Vanilla White",
    "Vanilla Blue",
    "Vanilla Gray",
];

pub fn is_procedural_background(name: &str) -> bool {
    PROCEDURAL_BACKGROUNDS.contains(&name)
}

pub fn is_light_background(name: &str) -> bool {
    matches!(
        name,
        "Lava Lamp Plasma"
            | "Vibrant Spectrum"
            | "Sunset Pop"
            | "Aqua Pulse"
            | "Vanilla White"
            | "Vanilla Gray"
    )
}

pub fn draw(name: &str, state: &mut BackgroundState) {
    let dt = get_frame_time().min(0.05);
    state.procedural_time += dt;
    state.energy_pulse = (state.energy_pulse - dt * 1.45).max(0.0);
    match name {
        "Cartridge Energy Core" => draw_cartridge_energy_core(state),
        "Neon Console Interior" => draw_console_interior(state),
        "Retro Maze" => {
            state.menu_maze.update(dt * 0.58);
            state.menu_maze.draw_retro();
            // Keep live-menu text readable over the bright maze grid.
            draw_rectangle(
                0.0,
                0.0,
                screen_width(),
                screen_height(),
                Color::new(0.0, 0.0, 0.035, 0.32),
            );
        }
        "Retro Laser Grid" => draw_laser_grid(state),
        "Prismatic Space Tunnel" => draw_space_tunnel(state),
        "Console Generations" => draw_console_generations(state),
        "Lava Lamp Plasma" => draw_lava_lamp(state),
        "Digital Circuit Rain" => draw_circuit_rain(state),
        "Screensaver Showcase" => draw_screensaver_showcase(state),
        "Vibrant Spectrum" => draw_vibrant_spectrum(state),
        "Sunset Pop" => draw_sunset_pop(state),
        "Aqua Pulse" => draw_aqua_pulse(state),
        "Vanilla Black" => clear_background(Color::from_rgba(7, 9, 14, 255)),
        "Vanilla White" => clear_background(Color::from_rgba(244, 245, 248, 255)),
        "Vanilla Blue" => clear_background(Color::from_rgba(16, 70, 158, 255)),
        "Vanilla Gray" => clear_background(Color::from_rgba(190, 195, 204, 255)),
        _ => clear_background(Color::from_rgba(2, 3, 9, 255)),
    }
    draw_dark_theme_visibility_lift(name, state);
    draw_vignette();
}

fn draw_dark_theme_visibility_lift(name: &str, state: &BackgroundState) {
    let tint = match name {
        "Cartridge Energy Core" => Some(Color::new(0.05, 0.20, 0.62, 0.075)),
        "Neon Console Interior" => Some(Color::new(0.02, 0.38, 0.52, 0.064)),
        "Prismatic Space Tunnel" => Some(Color::new(0.35, 0.08, 0.68, 0.070)),
        "Console Generations" => Some(Color::new(0.18, 0.12, 0.62, 0.060)),
        "Digital Circuit Rain" => Some(Color::new(0.00, 0.45, 0.38, 0.060)),
        _ => None,
    };
    let Some(tint) = tint else {
        return;
    };

    let width = screen_width();
    let height = screen_height();
    draw_rectangle(0.0, 0.0, width, height, tint);

    // Slow opposing color blooms lift the midtones without washing out menu text.
    let time = state.procedural_time;
    let centers = [
        vec2(
            width * (0.22 + (time * 0.055).sin() * 0.07),
            height * (0.30 + (time * 0.041).cos() * 0.08),
        ),
        vec2(
            width * (0.78 + (time * 0.047).cos() * 0.06),
            height * (0.70 + (time * 0.038).sin() * 0.07),
        ),
    ];
    for (index, center) in centers.iter().enumerate() {
        let glow_color = if index == 0 {
            Color::new(0.04, 0.84, 1.0, 0.012)
        } else {
            Color::new(1.0, 0.08, 0.70, 0.010)
        };
        for ring in (1..=8).rev() {
            let fraction = ring as f32 / 8.0;
            draw_circle(
                center.x,
                center.y,
                height * (0.12 + fraction * 0.42),
                Color::new(
                    glow_color.r,
                    glow_color.g,
                    glow_color.b,
                    glow_color.a * (1.0 - fraction * 0.72),
                ),
            );
        }
    }
}

fn refresh_internal_game_covers(state: &mut BackgroundState) {
    if get_time() - state.last_background_cover_scan < 8.0 {
        return;
    }
    state.last_background_cover_scan = get_time();

    let mut paths = Vec::new();
    for root in super::internal_games::internal_library_roots() {
        for folder in super::internal_games::library_game_folders(&root) {
            let mut candidates = Vec::new();
            if let Ok(files) = fs::read_dir(folder) {
                for file in files.flatten() {
                    let path = file.path();
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
                    if stem.contains("cover")
                        && matches!(
                            extension.as_str(),
                            "png" | "jpg" | "jpeg" | "webp" | "avif"
                        )
                    {
                        candidates.push(path);
                    }
                }
            }
            candidates.sort_by_key(|path| cover_priority(path));
            if let Some(path) = candidates.into_iter().next() {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths.truncate(32);
    if paths == state.background_cover_paths {
        return;
    }

    let mut textures = Vec::new();
    let mut loaded_paths = Vec::new();
    for path in paths {
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(decoded) = image::load_from_memory(&bytes) else {
            continue;
        };
        let cover = decoded.resize(192, 288, image::imageops::FilterType::Triangle);
        let rgba = cover.to_rgba8();
        let texture =
            Texture2D::from_rgba8(rgba.width() as u16, rgba.height() as u16, rgba.as_raw());
        texture.set_filter(FilterMode::Linear);
        textures.push(texture);
        loaded_paths.push(path);
    }
    state.background_covers = textures;
    state.background_cover_paths = loaded_paths;
    println!(
        "[Backgrounds] Loaded {} internal-game covers",
        state.background_covers.len()
    );
}

fn cover_priority(path: &PathBuf) -> (u8, String) {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let priority = match name.as_str() {
        "cover.png" => 0,
        "cover.jpg" | "cover.jpeg" => 1,
        _ if name.starts_with(".kazeta-cover") => 3,
        _ => 2,
    };
    (priority, name)
}

fn draw_cartridge_energy_core(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    let pulse = state.energy_pulse;
    clear_background(Color::from_rgba(4, 6, 16, 255));

    for band in 0..32 {
        let y = band as f32 / 32.0 * height;
        let shade = 0.024 + band as f32 / 32.0 * 0.027;
        draw_rectangle(
            0.0,
            y,
            width,
            height / 32.0 + 1.0,
            Color::new(shade, shade * 1.2, shade * 1.65, 1.0),
        );
    }

    let cart = Rect::new(width * 0.245, height * 0.255, width * 0.51, height * 0.50);
    draw_energy_circuits(cart, time, pulse);

    // Smoked-glass cartridge silhouette.
    draw_rectangle(
        cart.x,
        cart.y,
        cart.w,
        cart.h,
        Color::new(0.012, 0.020, 0.052, 0.72),
    );
    draw_rectangle_lines(
        cart.x,
        cart.y,
        cart.w,
        cart.h,
        2.0,
        Color::new(0.30, 0.38, 0.52, 0.34),
    );
    draw_rectangle(
        cart.x + cart.w * 0.11,
        cart.y + cart.h * 0.17,
        cart.w * 0.78,
        cart.h * 0.52,
        Color::new(0.005, 0.008, 0.018, 0.77),
    );

    let active = 0.48 + pulse * 0.52 + (time * 2.1).sin() * 0.08;
    let edge_colors = [
        Color::new(1.0, 0.25, 0.75, active),
        Color::new(1.0, 0.62, 0.20, active),
        Color::new(0.35, 1.0, 0.65, active),
        Color::new(0.25, 0.80, 1.0, active),
    ];
    let label = Rect::new(
        cart.x + cart.w * 0.11,
        cart.y + cart.h * 0.17,
        cart.w * 0.78,
        cart.h * 0.52,
    );
    draw_line(
        label.x,
        label.y,
        label.x + label.w,
        label.y,
        3.0,
        edge_colors[0],
    );
    draw_line(
        label.x + label.w,
        label.y,
        label.x + label.w,
        label.y + label.h,
        3.0,
        edge_colors[1],
    );
    draw_line(
        label.x + label.w,
        label.y + label.h,
        label.x,
        label.y + label.h,
        3.0,
        edge_colors[2],
    );
    draw_line(
        label.x,
        label.y + label.h,
        label.x,
        label.y,
        3.0,
        edge_colors[3],
    );

    let contact_y = cart.y + cart.h * 0.81;
    for contact in 0..26 {
        let x = cart.x + cart.w * 0.14 + contact as f32 * cart.w * 0.028;
        let glow = 0.32 + 0.24 * (time * 3.0 + contact as f32 * 0.4).sin().abs();
        draw_rectangle(
            x,
            contact_y,
            cart.w * 0.013,
            cart.h * 0.10,
            Color::new(1.0, 0.72, 0.22, glow),
        );
    }

    let core = vec2(cart.x + cart.w * 0.5, cart.y + cart.h * 0.43);
    for ring in (1..8).rev() {
        let radius = ring as f32 * height * 0.019 + pulse * ring as f32 * 1.5;
        let color = hue((ring as f32 * 0.11 + time * 0.045).fract(), 0.78, 1.0);
        draw_circle_lines(
            core.x,
            core.y,
            radius,
            1.2,
            Color::new(color.r, color.g, color.b, 0.10 + pulse * 0.13),
        );
    }
    draw_circle(
        core.x,
        core.y,
        4.0 + pulse * 8.0,
        Color::new(0.92, 0.98, 1.0, 0.76),
    );
}

fn draw_energy_circuits(cart: Rect, time: f32, input_pulse: f32) {
    let width = screen_width();
    let height = screen_height();
    let center = vec2(cart.x + cart.w * 0.5, cart.y + cart.h * 0.47);
    for index in 0..14 {
        let side = if index % 2 == 0 { -1.0 } else { 1.0 };
        let lane = index / 2;
        let start = vec2(
            if side < 0.0 { cart.x } else { cart.x + cart.w },
            cart.y + cart.h * (0.16 + lane as f32 * 0.105),
        );
        let bend_x = if side < 0.0 {
            width * 0.15
        } else {
            width * 0.85
        };
        let end = vec2(
            if side < 0.0 { 0.0 } else { width },
            height * (0.08 + lane as f32 * 0.125),
        );
        let color = hue((index as f32 / 14.0 + time * 0.035).fract(), 0.78, 1.0);
        let alpha = 0.16 + input_pulse * 0.22;
        draw_line(
            start.x,
            start.y,
            bend_x,
            start.y,
            3.0,
            Color::new(color.r, color.g, color.b, alpha * 0.18),
        );
        draw_line(
            bend_x,
            start.y,
            bend_x,
            end.y,
            3.0,
            Color::new(color.r, color.g, color.b, alpha * 0.18),
        );
        draw_line(
            bend_x,
            end.y,
            end.x,
            end.y,
            3.0,
            Color::new(color.r, color.g, color.b, alpha * 0.18),
        );
        draw_line(
            start.x,
            start.y,
            bend_x,
            start.y,
            1.0,
            Color::new(color.r, color.g, color.b, alpha),
        );
        draw_line(
            bend_x,
            start.y,
            bend_x,
            end.y,
            1.0,
            Color::new(color.r, color.g, color.b, alpha),
        );
        draw_line(
            bend_x,
            end.y,
            end.x,
            end.y,
            1.0,
            Color::new(color.r, color.g, color.b, alpha),
        );

        let travel = (time * (0.28 + lane as f32 * 0.025) + index as f32 * 0.087).fract();
        let point = if travel < 0.34 {
            vec2(lerp(start.x, bend_x, travel / 0.34), start.y)
        } else if travel < 0.67 {
            vec2(bend_x, lerp(start.y, end.y, (travel - 0.34) / 0.33))
        } else {
            vec2(lerp(bend_x, end.x, (travel - 0.67) / 0.33), end.y)
        };
        draw_circle(
            point.x,
            point.y,
            7.0 + input_pulse * 3.0,
            Color::new(color.r, color.g, color.b, 0.10 + input_pulse * 0.12),
        );
        draw_circle(
            point.x,
            point.y,
            2.0 + input_pulse,
            Color::new(0.95, 0.99, 1.0, 0.82),
        );
    }
    draw_circle(
        center.x,
        center.y,
        22.0 + input_pulse * 12.0,
        Color::new(0.45, 0.75, 1.0, 0.025),
    );
}

fn draw_console_interior(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    clear_background(Color::from_rgba(4, 10, 18, 255));

    let grid = (width / 26.0).max(22.0);
    let mut x = 0.0;
    while x < width {
        draw_line(x, 0.0, x, height, 1.0, Color::new(0.08, 0.34, 0.42, 0.34));
        x += grid;
    }
    let mut y = 0.0;
    while y < height {
        draw_line(0.0, y, width, y, 1.0, Color::new(0.08, 0.34, 0.42, 0.34));
        y += grid;
    }

    for chip in 0..13 {
        let seed = chip as f32;
        let chip_w = width * (0.07 + hash(seed * 4.3) * 0.09);
        let chip_h = height * (0.06 + hash(seed * 8.9) * 0.10);
        let chip_x = hash(seed * 14.7) * (width - chip_w);
        let chip_y = hash(seed * 21.3) * (height - chip_h);
        draw_rectangle(
            chip_x - 4.0,
            chip_y - 4.0,
            chip_w + 8.0,
            chip_h + 8.0,
            Color::new(0.02, 0.04, 0.06, 0.94),
        );
        draw_rectangle_lines(
            chip_x,
            chip_y,
            chip_w,
            chip_h,
            1.5,
            Color::new(0.18, 0.34, 0.38, 0.38),
        );
        let led = hue((seed * 0.12 + time * 0.035).fract(), 0.75, 1.0);
        draw_circle(
            chip_x + chip_w - 7.0,
            chip_y + 7.0,
            2.5,
            Color::new(led.r, led.g, led.b, 0.45 + state.energy_pulse * 0.4),
        );
    }

    let cpu = Rect::new(width * 0.34, height * 0.29, width * 0.32, height * 0.38);
    draw_rectangle(
        cpu.x - 12.0,
        cpu.y - 12.0,
        cpu.w + 24.0,
        cpu.h + 24.0,
        Color::new(0.01, 0.015, 0.025, 0.92),
    );
    for pin in 0..18 {
        let fraction = (pin as f32 + 0.5) / 18.0;
        draw_rectangle(
            cpu.x + cpu.w * fraction,
            cpu.y - 9.0,
            2.0,
            8.0,
            Color::new(0.62, 0.48, 0.20, 0.52),
        );
        draw_rectangle(
            cpu.x + cpu.w * fraction,
            cpu.y + cpu.h + 1.0,
            2.0,
            8.0,
            Color::new(0.62, 0.48, 0.20, 0.52),
        );
    }
    draw_rectangle(
        cpu.x,
        cpu.y,
        cpu.w,
        cpu.h,
        Color::new(0.018, 0.025, 0.04, 0.96),
    );
    let edge = hue((time * 0.055).fract(), 0.78, 1.0);
    draw_rectangle_lines(
        cpu.x,
        cpu.y,
        cpu.w,
        cpu.h,
        3.0,
        Color::new(edge.r, edge.g, edge.b, 0.45 + state.energy_pulse * 0.4),
    );
    for ring in 0..6 {
        let radius = 18.0 + ring as f32 * height * 0.035 + (time * 2.0 + ring as f32).sin() * 2.0;
        let color = hue((ring as f32 * 0.13 + time * 0.04).fract(), 0.70, 1.0);
        draw_circle_lines(
            cpu.x + cpu.w * 0.5,
            cpu.y + cpu.h * 0.5,
            radius,
            1.0,
            Color::new(color.r, color.g, color.b, 0.16),
        );
    }
}

fn draw_laser_grid(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    let horizon = height * 0.54;
    clear_background(Color::from_rgba(2, 1, 14, 255));

    // Deep layered sky. Drawing this as horizontal bands keeps it crisp at
    // 720p, 1080p, and 1440p while the GPU performs all interpolation.
    for band in 0..64 {
        let fraction = band as f32 / 63.0;
        let y = fraction * horizon;
        let band_height = horizon / 63.0 + 1.0;
        let (red, green, blue) = if fraction < 0.50 {
            let mix = fraction / 0.50;
            (
                lerp(0.004, 0.022, mix),
                lerp(0.002, 0.015, mix),
                lerp(0.030, 0.105, mix),
            )
        } else {
            let mix = (fraction - 0.50) / 0.50;
            (
                lerp(0.022, 0.130, mix),
                lerp(0.015, 0.018, mix),
                lerp(0.105, 0.245, mix),
            )
        };
        draw_rectangle(0.0, y, width, band_height, Color::new(red, green, blue, 1.0));
    }

    // Wide magenta/violet horizon bloom.
    for glow in (1..=16).rev() {
        let fraction = glow as f32 / 16.0;
        let glow_width = width * (0.18 + fraction * 0.50);
        let glow_height = height * (0.018 + fraction * 0.095);
        draw_ellipse(
            width * 0.5,
            horizon,
            glow_width,
            glow_height,
            0.0,
            Color::new(0.68, 0.06, 0.98, 0.012 + (1.0 - fraction) * 0.014),
        );
    }

    for star in 0..180 {
        let seed = star as f32;
        let x = hash(seed * 7.1) * width;
        let y = hash(seed * 11.9) * horizon * 0.94;
        let radius = 0.55 + hash(seed * 3.2) * 1.35;
        let twinkle = 0.18 + (time * (1.2 + hash(seed * 4.9)) + seed).sin().abs() * 0.68;
        let star_color = if star % 9 == 0 {
            Color::new(1.0, 0.16, 0.76, twinkle)
        } else if star % 6 == 0 {
            Color::new(0.05, 0.88, 1.0, twinkle)
        } else {
            Color::new(0.78, 0.88, 1.0, twinkle)
        };
        draw_circle(x, y, radius * 3.5, Color::new(star_color.r, star_color.g, star_color.b, twinkle * 0.05));
        draw_circle(
            x,
            y,
            radius,
            star_color,
        );
    }

    // Sunset disc with layered glow and expanding dark scan gaps.
    let sun = vec2(width * 0.5, horizon - height * 0.055);
    let sun_radius = height * 0.105;
    for glow in (1..=12).rev() {
        let fraction = glow as f32 / 12.0;
        draw_circle(
            sun.x,
            sun.y,
            sun_radius * (1.0 + fraction * 0.72),
            Color::new(1.0, 0.08, 0.68, 0.012 + (1.0 - fraction) * 0.012),
        );
    }
    for slice in 0..40 {
        let fraction = slice as f32 / 39.0;
        let slice_y = sun.y - sun_radius + fraction * sun_radius * 2.0;
        let half = (sun_radius * sun_radius - (slice_y - sun.y).powi(2))
            .max(0.0)
            .sqrt();
        let color = if fraction < 0.48 {
            let mix = fraction / 0.48;
            Color::new(1.0, lerp(0.96, 0.52, mix), lerp(0.63, 0.10, mix), 1.0)
        } else {
            let mix = (fraction - 0.48) / 0.52;
            Color::new(1.0, lerp(0.52, 0.10, mix), lerp(0.10, 0.74, mix), 1.0)
        };
        draw_rectangle(
            sun.x - half,
            slice_y,
            half * 2.0,
            sun_radius * 2.0 / 39.0 + 1.0,
            Color::new(color.r, color.g, color.b, 0.92),
        );
    }
    for stripe in 0..7 {
        let fraction = stripe as f32 / 7.0;
        let stripe_y = sun.y + sun_radius * (0.03 + fraction * 0.87);
        let half = (sun_radius * sun_radius - (stripe_y - sun.y).powi(2))
            .max(0.0)
            .sqrt();
        draw_rectangle(
            sun.x - half,
            stripe_y,
            half * 2.0,
            2.0 + fraction * sun_radius * 0.045,
            Color::new(0.035, 0.008, 0.095, 0.92),
        );
    }

    // Two deterministic mountain ranges drift at different speeds.
    for layer in 0..2 {
        let peak_count = if layer == 0 { 11 } else { 15 };
        let step = width / (peak_count as f32 - 2.0);
        let base_y = horizon + layer as f32 * height * 0.014;
        let amplitude = height * if layer == 0 { 0.115 } else { 0.082 };
        let drift = if layer == 0 {
            (time * width * 0.006).rem_euclid(step)
        } else {
            -(time * width * 0.004).rem_euclid(step)
        };
        let fill = if layer == 0 {
            Color::new(0.045, 0.014, 0.100, 1.0)
        } else {
            Color::new(0.018, 0.012, 0.065, 1.0)
        };
        let edge = if layer == 0 {
            Color::new(0.72, 0.11, 1.0, 0.82)
        } else {
            Color::new(0.05, 0.50, 1.0, 0.60)
        };
        for peak in -2..peak_count {
            let left_x = peak as f32 * step + drift;
            let peak_x = left_x + step * 0.5;
            let right_x = left_x + step;
            let variation = 0.72 + hash(peak as f32 * 9.3 + layer as f32 * 4.1) * 0.34;
            let peak_y = base_y - amplitude * variation;
            draw_triangle(
                vec2(left_x, base_y),
                vec2(peak_x, peak_y),
                vec2(right_x, base_y),
                fill,
            );
            draw_line(left_x, base_y, peak_x, peak_y, 7.0, Color::new(edge.r, edge.g, edge.b, 0.035));
            draw_line(peak_x, peak_y, right_x, base_y, 7.0, Color::new(edge.r, edge.g, edge.b, 0.035));
            draw_line(left_x, base_y, peak_x, peak_y, 1.4, edge);
            draw_line(peak_x, peak_y, right_x, base_y, 1.4, edge);
        }
    }

    // Dark floor gradient beneath the grid.
    for band in 0..48 {
        let fraction = band as f32 / 47.0;
        let y = horizon + fraction * (height - horizon);
        draw_rectangle(
            0.0,
            y,
            width,
            (height - horizon) / 47.0 + 1.0,
            Color::new(
                lerp(0.030, 0.003, fraction),
                lerp(0.010, 0.003, fraction),
                lerp(0.082, 0.025, fraction),
                1.0,
            ),
        );
    }

    // Perspective rays: soft glow pass followed by a crisp core.
    for line in -18..=18 {
        let spread = line as f32 / 18.0;
        let bottom_x = width * 0.5 + spread * width * 1.08;
        let color = hue(((line + 18) as f32 / 37.0 + time * 0.018).fract(), 0.86, 1.0);
        draw_line(
            width * 0.5,
            horizon,
            bottom_x,
            height,
            8.0,
            Color::new(color.r, color.g, color.b, 0.045),
        );
        draw_line(
            width * 0.5,
            horizon,
            bottom_x,
            height,
            if line % 5 == 0 { 1.8 } else { 1.05 },
            Color::new(color.r, color.g, color.b, 0.62),
        );
    }

    // Advancing cross-lines accelerate toward the viewer.
    for line in 0..29 {
        let phase = (line as f32 / 29.0 + time * 0.18).fract();
        let perspective = phase.powf(2.45);
        let y = horizon + perspective * (height - horizon);
        let color = hue((line as f32 * 0.047 + time * 0.026).fract(), 0.86, 1.0);
        let alpha = 0.22 + perspective * 0.64;
        draw_line(0.0, y, width, y, 8.0 + perspective * 5.0, Color::new(color.r, color.g, color.b, alpha * 0.055));
        draw_line(0.0, y, width, y, 1.0 + perspective * 1.3, Color::new(color.r, color.g, color.b, alpha));
    }

    // Multicolor horizon bloom.
    for segment in 0..72 {
        let x0 = segment as f32 / 72.0 * width;
        let x1 = (segment + 1) as f32 / 72.0 * width;
        let color = hue((segment as f32 / 72.0 * 0.72 + time * 0.018).fract(), 0.82, 1.0);
        let center_fade = 1.0 - ((segment as f32 / 71.0 - 0.5).abs() * 2.0).powf(1.8);
        draw_line(x0, horizon, x1, horizon, 11.0, Color::new(color.r, color.g, color.b, center_fade * 0.055));
        draw_line(x0, horizon, x1, horizon, 2.0, Color::new(color.r, color.g, color.b, center_fade * 0.76));
    }

    // Fast sparks along the outer lanes make the floor feel alive.
    for spark in 0..20 {
        let phase = (time * 0.11 + spark as f32 / 20.0).fract();
        let perspective = phase.powf(2.05);
        let side = if spark % 2 == 0 { -1.0 } else { 1.0 };
        let y = horizon + perspective * (height - horizon);
        let x = width * 0.5
            + side * width * (0.10 + perspective * 0.57)
            + (spark as f32 * 11.7).sin() * width * 0.018;
        let length = 3.0 + perspective * width * 0.013;
        let color = hue((spark as f32 * 0.11 + time * 0.04).fract(), 0.88, 1.0);
        draw_line(
            x,
            y,
            x + side * length,
            y + length * 0.26,
            1.2,
            Color::new(color.r, color.g, color.b, perspective * 0.72),
        );
    }

    // Subtle CRT scanlines tie it to the rest of PlayFusion.
    let scan_step = (height / 360.0).max(3.0);
    let mut scan_y = 0.0;
    while scan_y < height {
        draw_rectangle(0.0, scan_y, width, 1.0, Color::new(0.0, 0.0, 0.0, 0.065));
        scan_y += scan_step;
    }
}

fn draw_arcade_wall(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    clear_background(Color::from_rgba(3, 2, 8, 255));
    let horizon = height * 0.50;
    draw_rectangle(
        0.0,
        horizon,
        width,
        height - horizon,
        Color::new(0.025, 0.02, 0.055, 1.0),
    );
    for line in -12..=12 {
        draw_line(
            width * 0.5,
            horizon,
            width * 0.5 + line as f32 * width * 0.095,
            height,
            1.0,
            Color::new(0.45, 0.15, 0.62, 0.15),
        );
    }

    let cabinets = 9;
    for depth_index in (0..cabinets).rev() {
        let phase = (depth_index as f32 / cabinets as f32 + time * 0.012).fract();
        let scale = 0.42 + phase * 0.78;
        let cabinet_w = width * 0.105 * scale;
        let cabinet_h = height * 0.47 * scale;
        let side = if depth_index % 2 == 0 { -1.0 } else { 1.0 };
        let distance = width * (0.12 + phase * 0.39);
        let x = width * 0.5 + side * distance - cabinet_w * 0.5;
        let y = horizon - cabinet_h * 0.46 + (time * 0.7 + depth_index as f32).sin() * 2.0;
        let glow = hue(
            (depth_index as f32 / cabinets as f32 + time * 0.025).fract(),
            0.78,
            1.0,
        );
        draw_rectangle(
            x - 5.0,
            y - 5.0,
            cabinet_w + 10.0,
            cabinet_h + 12.0,
            Color::new(glow.r, glow.g, glow.b, 0.08),
        );
        draw_rectangle(
            x,
            y,
            cabinet_w,
            cabinet_h,
            Color::new(0.015, 0.02, 0.035, 0.94),
        );
        draw_rectangle_lines(
            x,
            y,
            cabinet_w,
            cabinet_h,
            2.0,
            Color::new(glow.r, glow.g, glow.b, 0.34),
        );
        let screen = Rect::new(
            x + cabinet_w * 0.13,
            y + cabinet_h * 0.10,
            cabinet_w * 0.74,
            cabinet_h * 0.55,
        );
        draw_rectangle(
            screen.x,
            screen.y,
            screen.w,
            screen.h,
            Color::new(0.02, 0.03, 0.06, 1.0),
        );
        draw_cover(&state.background_covers, depth_index, screen, 0.54);
        draw_circle(
            x + cabinet_w * 0.38,
            y + cabinet_h * 0.78,
            2.0 * scale,
            Color::new(0.25, 0.92, 1.0, 0.62),
        );
        draw_circle(
            x + cabinet_w * 0.62,
            y + cabinet_h * 0.78,
            2.0 * scale,
            Color::new(1.0, 0.25, 0.64, 0.62),
        );
    }
}

fn draw_space_tunnel(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    clear_background(Color::from_rgba(5, 3, 22, 255));
    let center = vec2(
        width * 0.5 + (time * 0.43).sin() * width * 0.07,
        height * 0.5 + (time * 0.36).cos() * height * 0.05,
    );
    let max_radius = width.max(height) * 0.86;
    for ring in (0..25).rev() {
        let depth = (ring as f32 / 25.0 + time * 0.085).fract();
        let radius = 12.0 + depth.powf(2.0) * max_radius;
        let color = hue((depth * 0.68 + time * 0.028).fract(), 0.78, 1.0);
        draw_poly_lines(
            center.x,
            center.y,
            8,
            radius,
            time * 4.0 + ring as f32 * 4.0,
            1.0 + depth * 2.3,
            Color::new(color.r, color.g, color.b, 0.28 + (1.0 - depth) * 0.30),
        );
    }
    for ray in 0..16 {
        let angle = ray as f32 / 16.0 * std::f32::consts::TAU + time * 0.025;
        let end = center + vec2(angle.cos(), angle.sin()) * max_radius;
        let color = hue((ray as f32 / 16.0 + time * 0.02).fract(), 0.76, 1.0);
        draw_line(
            center.x,
            center.y,
            end.x,
            end.y,
            1.0,
            Color::new(color.r, color.g, color.b, 0.20),
        );
    }
}

fn draw_floating_covers(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    clear_background(Color::from_rgba(2, 3, 9, 255));
    for star in 0..80 {
        draw_circle(
            hash(star as f32 * 4.7) * width,
            hash(star as f32 * 9.9) * height,
            0.6 + hash(star as f32 * 13.0),
            Color::new(0.62, 0.74, 1.0, 0.12),
        );
    }
    if state.background_covers.is_empty() {
        draw_space_tunnel(state);
        return;
    }
    for item in 0..14 {
        let seed = item as f32;
        let depth = (hash(seed * 6.4) + time * (0.018 + hash(seed * 9.1) * 0.020)).fract();
        let scale = 0.35 + depth * 0.85;
        let cover_h = height * 0.30 * scale;
        let cover_w = cover_h * 0.66;
        let lane = hash(seed * 18.1);
        let x = lane * (width + cover_w * 1.5) - cover_w * 0.75
            + (time * (7.0 + seed) + seed).sin() * width * 0.035;
        let y = height * (0.11 + hash(seed * 3.7) * 0.68)
            + (time * (0.45 + hash(seed * 4.2)) + seed).sin() * height * 0.055;
        let rect = Rect::new(x, y, cover_w, cover_h);
        let color = hue((seed / 14.0 + time * 0.018).fract(), 0.65, 1.0);
        draw_rectangle(
            rect.x - 8.0,
            rect.y - 8.0,
            rect.w + 16.0,
            rect.h + 16.0,
            Color::new(color.r, color.g, color.b, 0.045 + depth * 0.055),
        );
        draw_cover(&state.background_covers, item, rect, 0.18 + depth * 0.42);
        draw_rectangle_lines(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            1.5 + depth,
            Color::new(color.r, color.g, color.b, 0.18 + depth * 0.26),
        );
    }
}

fn draw_console_generations(state: &BackgroundState) {
    let phase = (state.procedural_time / 7.0).floor() as i32 % 4;
    match phase {
        0 => draw_pixel_generation(state),
        1 => draw_sprite_generation(state),
        2 => draw_polygon_generation(state),
        _ => draw_console_interior(state),
    }
    let transition = (state.procedural_time % 7.0).min(7.0 - state.procedural_time % 7.0);
    if transition < 0.45 {
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            Color::new(0.0, 0.0, 0.0, (0.45 - transition) / 0.45 * 0.52),
        );
    }
}

fn draw_pixel_generation(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    clear_background(Color::from_rgba(3, 4, 18, 255));
    let pixel = (width / 160.0).max(4.0);
    for star in 0..80 {
        let x = (hash(star as f32 * 9.1) * width / pixel).floor() * pixel;
        let y = (hash(star as f32 * 5.7) * height * 0.62 / pixel).floor() * pixel;
        let blink = if ((time * 3.0) as i32 + star) % 4 == 0 {
            0.75
        } else {
            0.24
        };
        draw_rectangle(x, y, pixel, pixel, Color::new(0.72, 0.82, 1.0, blink));
    }
    for layer in 0..4 {
        let base = height * (0.55 + layer as f32 * 0.09);
        let color = hue(
            (0.55 + layer as f32 * 0.11).fract(),
            0.72,
            0.72 - layer as f32 * 0.09,
        );
        let mut x = 0.0;
        while x < width {
            let hill = ((x / pixel * 0.24 + time * (0.22 + layer as f32 * 0.05)).sin() * 3.0 + 4.0)
                .floor()
                * pixel;
            draw_rectangle(
                x,
                base - hill,
                pixel + 0.5,
                height - base + hill,
                Color::new(color.r, color.g, color.b, 0.62),
            );
            x += pixel;
        }
    }
}

fn draw_sprite_generation(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    clear_background(Color::from_rgba(7, 5, 24, 255));
    for band in 0..10 {
        let y = height * (0.18 + band as f32 * 0.075);
        let color = hue((band as f32 * 0.08 + time * 0.018).fract(), 0.72, 0.92);
        let mut previous = vec2(0.0, y);
        for point in 1..=80 {
            let x = point as f32 / 80.0 * width;
            let wave = (x * 0.018 + time * (0.7 + band as f32 * 0.06) + band as f32).sin()
                * (8.0 + band as f32 * 1.8);
            let current = vec2(x, y + wave);
            draw_line(
                previous.x,
                previous.y,
                current.x,
                current.y,
                2.0,
                Color::new(color.r, color.g, color.b, 0.24),
            );
            previous = current;
        }
    }
    for sprite in 0..18 {
        let x =
            (hash(sprite as f32 * 5.3) * width + time * (12.0 + sprite as f32)).rem_euclid(width);
        let y = height * (0.18 + hash(sprite as f32 * 8.7) * 0.64);
        let color = hue((sprite as f32 / 18.0 + time * 0.025).fract(), 0.75, 1.0);
        draw_rectangle(x, y, 8.0, 8.0, Color::new(color.r, color.g, color.b, 0.60));
        draw_rectangle(
            x + 8.0,
            y + 8.0,
            8.0,
            8.0,
            Color::new(color.r, color.g, color.b, 0.42),
        );
    }
}

fn draw_polygon_generation(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    clear_background(Color::from_rgba(1, 3, 11, 255));
    for poly in 0..18 {
        let seed = poly as f32;
        let x = hash(seed * 4.2) * width;
        let y = hash(seed * 7.8) * height;
        let radius = 18.0 + hash(seed * 11.1) * height * 0.16;
        let color = hue((seed / 18.0 + time * 0.025).fract(), 0.78, 1.0);
        draw_poly_lines(
            x,
            y,
            (3 + poly % 6) as u8,
            radius,
            time * (7.0 + seed * 0.2),
            1.4,
            Color::new(color.r, color.g, color.b, 0.34),
        );
        draw_line(
            width * 0.5,
            height * 0.5,
            x,
            y,
            1.0,
            Color::new(color.r, color.g, color.b, 0.08),
        );
    }
}

fn draw_lava_lamp(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    // The collection's deliberately light option. Settings automatically
    // switch its UI text/cursor to dark colors for reliable contrast.
    clear_background(Color::from_rgba(226, 230, 244, 255));
    for blob in 0..15 {
        let seed = blob as f32;
        let x = width * (0.12 + hash(seed * 4.9) * 0.76)
            + (time * (0.18 + hash(seed * 2.3) * 0.31) + seed).sin() * width * 0.12;
        let y = height * (0.10 + hash(seed * 7.7) * 0.80)
            + (time * (0.24 + hash(seed * 5.1) * 0.27) + seed * 1.7).cos() * height * 0.17;
        let radius =
            height * (0.10 + hash(seed * 11.3) * 0.22) * (0.82 + (time * 0.7 + seed).sin() * 0.15);
        let color = hue((seed / 15.0 + time * 0.018).fract(), 0.76, 1.0);
        for layer in (1..8).rev() {
            let layer_fraction = layer as f32 / 8.0;
            draw_circle(
                x,
                y,
                radius * layer_fraction,
                Color::new(
                    color.r,
                    color.g,
                    color.b,
                    0.050 + (1.0 - layer_fraction) * 0.060,
                ),
            );
        }
    }
    draw_rectangle(0.0, 0.0, width, height, Color::new(1.0, 1.0, 1.0, 0.14));
}

fn draw_circuit_rain(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    clear_background(Color::from_rgba(2, 11, 19, 255));
    let spacing = (width / 42.0).max(13.0);
    let columns = (width / spacing).ceil() as usize;
    for column in 0..columns {
        let seed = column as f32;
        let speed = 32.0 + hash(seed * 5.4) * 86.0;
        let head = (time * speed + hash(seed * 8.9) * (height + 150.0)) % (height + 150.0) - 75.0;
        let color = hue((0.34 + seed * 0.016 + time * 0.014).fract(), 0.84, 1.0);
        let x = column as f32 * spacing + spacing * 0.5;
        draw_line(
            x,
            head - 92.0,
            x,
            head,
            1.0,
            Color::new(color.r, color.g, color.b, 0.38),
        );
        draw_circle(x, head, 2.0, Color::new(0.86, 1.0, 0.94, 0.68));
        for bit in 0..5 {
            let glyph = if (column + bit + (time * 5.0) as usize) % 2 == 0 {
                "1"
            } else {
                "0"
            };
            draw_text(
                glyph,
                x + 3.0,
                head - bit as f32 * 17.0,
                12.0,
                Color::new(color.r, color.g, color.b, 0.32),
            );
        }
        if column % 4 == 0 {
            let junction_y = height * (0.18 + hash(seed * 12.3) * 0.64);
            let direction = if column % 8 == 0 { 1.0 } else { -1.0 };
            draw_line(
                x,
                junction_y,
                x + direction * spacing * 2.5,
                junction_y,
                1.0,
                Color::new(color.r, color.g, color.b, 0.18),
            );
            draw_circle(
                x + direction * spacing * 2.5,
                junction_y,
                3.0,
                Color::new(color.r, color.g, color.b, 0.28),
            );
        }
    }
}

fn draw_vibrant_spectrum(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    clear_background(Color::from_rgba(249, 244, 255, 255));

    let cells_x = 42;
    let cells_y = 24;
    let cell_w = width / cells_x as f32 + 0.5;
    let cell_h = height / cells_y as f32 + 0.5;
    for grid_y in 0..cells_y {
        for grid_x in 0..cells_x {
            let nx = grid_x as f32 / cells_x as f32;
            let ny = grid_y as f32 / cells_y as f32;
            let wave = (nx * 8.0 + time * 0.55).sin()
                + (ny * 7.0 - time * 0.42).cos()
                + ((nx + ny) * 9.0 + time * 0.31).sin();
            let color = hue(
                (nx * 0.62 + ny * 0.25 + wave * 0.035 + time * 0.025).fract(),
                0.62,
                1.0,
            );
            draw_rectangle(
                grid_x as f32 * cell_w,
                grid_y as f32 * cell_h,
                cell_w,
                cell_h,
                Color::new(color.r, color.g, color.b, 0.72),
            );
        }
    }

    for orb in 0..9 {
        let seed = orb as f32;
        let x = width * (0.1 + hash(seed * 3.2) * 0.8)
            + (time * (0.25 + seed * 0.013) + seed).sin() * width * 0.08;
        let y = height * (0.1 + hash(seed * 8.4) * 0.8)
            + (time * (0.22 + seed * 0.017) + seed * 1.4).cos() * height * 0.09;
        let color = hue((seed / 9.0 + time * 0.04).fract(), 0.74, 1.0);
        draw_circle(
            x,
            y,
            height * (0.10 + hash(seed * 12.0) * 0.14),
            Color::new(color.r, color.g, color.b, 0.14),
        );
    }
    draw_rectangle(0.0, 0.0, width, height, Color::new(1.0, 1.0, 1.0, 0.10));
}

fn draw_sunset_pop(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    for band in 0..48 {
        let fraction = band as f32 / 48.0;
        let color = if fraction < 0.52 {
            let mix = fraction / 0.52;
            Color::new(
                lerp(0.40, 1.0, mix),
                lerp(0.70, 0.43, mix),
                lerp(1.0, 0.62, mix),
                1.0,
            )
        } else {
            let mix = (fraction - 0.52) / 0.48;
            Color::new(1.0, lerp(0.43, 0.82, mix), lerp(0.62, 0.34, mix), 1.0)
        };
        draw_rectangle(0.0, fraction * height, width, height / 48.0 + 1.0, color);
    }

    let sun = vec2(width * 0.78, height * 0.25);
    draw_circle(
        sun.x,
        sun.y,
        height * 0.13,
        Color::new(1.0, 0.95, 0.54, 0.50),
    );
    draw_circle(
        sun.x,
        sun.y,
        height * 0.09,
        Color::new(1.0, 0.98, 0.72, 0.95),
    );

    for layer in 0..4 {
        let base = height * (0.62 + layer as f32 * 0.10);
        let color = hue(
            (0.78 + layer as f32 * 0.065 + time * 0.006).fract(),
            0.56,
            0.54 + layer as f32 * 0.09,
        );
        let mut previous = vec2(0.0, base);
        for point in 1..=80 {
            let x = point as f32 / 80.0 * width;
            let wave = (x * (0.010 + layer as f32 * 0.002)
                + time * (0.16 + layer as f32 * 0.025)
                + layer as f32)
                .sin()
                * height
                * (0.035 + layer as f32 * 0.01);
            let current = vec2(x, base + wave);
            let floor_previous = vec2(previous.x, height);
            let floor_current = vec2(current.x, height);
            draw_triangle(
                previous,
                current,
                floor_current,
                Color::new(color.r, color.g, color.b, 0.78),
            );
            draw_triangle(
                previous,
                floor_current,
                floor_previous,
                Color::new(color.r, color.g, color.b, 0.78),
            );
            previous = current;
        }
    }
    draw_rectangle(0.0, 0.0, width, height, Color::new(1.0, 1.0, 1.0, 0.06));
}

fn draw_aqua_pulse(state: &BackgroundState) {
    let width = screen_width();
    let height = screen_height();
    let time = state.procedural_time;
    for band in 0..40 {
        let fraction = band as f32 / 40.0;
        draw_rectangle(
            0.0,
            fraction * height,
            width,
            height / 40.0 + 1.0,
            Color::new(
                lerp(0.66, 0.16, fraction),
                lerp(0.98, 0.78, fraction),
                lerp(0.98, 0.92, fraction),
                1.0,
            ),
        );
    }

    let centers = [
        vec2(width * 0.22, height * 0.34),
        vec2(width * 0.72, height * 0.58),
        vec2(width * 0.48, height * 0.82),
    ];
    for (center_index, center) in centers.iter().enumerate() {
        for ring in 0..10 {
            let phase = (ring as f32 / 10.0 + time * (0.055 + center_index as f32 * 0.008)).fract();
            let radius = 8.0 + phase * height * 0.55;
            let color = hue(
                (0.47 + center_index as f32 * 0.08 + phase * 0.13).fract(),
                0.56,
                0.80,
            );
            draw_circle_lines(
                center.x,
                center.y,
                radius,
                1.0 + (1.0 - phase) * 2.0,
                Color::new(color.r, color.g, color.b, (1.0 - phase) * 0.24),
            );
        }
    }

    for bubble in 0..36 {
        let seed = bubble as f32;
        let x = hash(seed * 4.7) * width + (time * 0.35 + seed).sin() * width * 0.025;
        let y = (height + hash(seed * 8.2) * height - time * (12.0 + hash(seed * 3.1) * 24.0))
            .rem_euclid(height + 20.0)
            - 10.0;
        draw_circle_lines(
            x,
            y,
            3.0 + hash(seed * 9.9) * 8.0,
            1.0,
            Color::new(0.92, 1.0, 1.0, 0.38),
        );
    }
}

fn draw_screensaver_showcase(state: &BackgroundState) {
    let segment = (state.procedural_time / 7.5).floor() as i32 % 5;
    match segment {
        0 => draw_space_tunnel(state),
        1 => draw_laser_grid(state),
        2 => draw_lava_lamp(state),
        3 => draw_circuit_rain(state),
        _ => draw_vibrant_spectrum(state),
    }
    let within = state.procedural_time % 7.5;
    let fade = if within < 0.45 {
        (0.45 - within) / 0.45
    } else if within > 7.05 {
        (within - 7.05) / 0.45
    } else {
        0.0
    };
    if fade > 0.0 {
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            Color::new(0.0, 0.0, 0.0, fade * 0.70),
        );
    }
}

fn draw_cover(covers: &[Texture2D], index: usize, rect: Rect, alpha: f32) {
    if let Some(texture) = covers.get(index % covers.len().max(1)) {
        draw_texture_ex(
            texture,
            rect.x,
            rect.y,
            Color::new(1.0, 1.0, 1.0, alpha),
            DrawTextureParams {
                dest_size: Some(vec2(rect.w, rect.h)),
                ..Default::default()
            },
        );
    }
}

fn draw_vignette() {
    let width = screen_width();
    let height = screen_height();
    for layer in 0..12 {
        let fraction = layer as f32 / 12.0;
        let alpha = (1.0 - fraction) * 0.035;
        let inset_x = fraction * width * 0.16;
        let inset_y = fraction * height * 0.16;
        draw_rectangle(0.0, 0.0, width, inset_y, Color::new(0.0, 0.0, 0.0, alpha));
        draw_rectangle(
            0.0,
            height - inset_y,
            width,
            inset_y,
            Color::new(0.0, 0.0, 0.0, alpha),
        );
        draw_rectangle(
            0.0,
            inset_y,
            inset_x,
            height - inset_y * 2.0,
            Color::new(0.0, 0.0, 0.0, alpha),
        );
        draw_rectangle(
            width - inset_x,
            inset_y,
            inset_x,
            height - inset_y * 2.0,
            Color::new(0.0, 0.0, 0.0, alpha),
        );
    }
}

fn hue(hue: f32, saturation: f32, value: f32) -> Color {
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

fn hash(value: f32) -> f32 {
    ((value * 12.9898).sin() * 43_758.547).fract().abs()
}

fn lerp(start: f32, end: f32, amount: f32) -> f32 {
    start + (end - start) * amount.clamp(0.0, 1.0)
}
