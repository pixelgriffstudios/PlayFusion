use crate::{
    audio::{load_sound_from_bytes, play_new_bgm, SoundEffects, AUDIO},
    cd_player_backend::CdPlayerBackend,
    config::{get_user_data_dir, Config},
    dialog::Dialog,
    gcc_adapter::start_gcc_adapter_polling,
    input::InputState,
    save::StorageMediaState,
    settings::render_settings_page,
    settings::GENERAL_SETTINGS,
    system::*, // Wildcard to get all system functions
    ui::main_menu::MAIN_MENU_OPTIONS,
    ui::runtime_downloader::RuntimeDownloaderState,
    ui::theme_downloader::ThemeDownloaderState,
    ui::update_checker::UpdateCheckerState,
    ui::wifi::WifiState,
    ui::*,
    utils::*, // Wildcard to get all utility functions
};
use ::rand::Rng; // for selecting a random message on startup
use chrono::Local; // for getting clock
use gilrs::Gilrs;
use macroquad::prelude::*;
use regex::Regex; // fetching audio sinks
use rodio::{buffer::SamplesBuffer, Decoder, Sink};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{BufReader, Cursor},
    path::PathBuf,
    process,
    process::Child,
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
    thread, time,
};
use video::VideoPlayer;

pub use types::*;

// Import our new modules
mod audio;
mod cd_player_backend;
mod config;
mod gcc_adapter;
mod input;
mod memory;
mod save;
mod system;
mod theme;
mod types;
mod ui;
mod utils;
mod video;

/*
// ===================================
// TO-DO LIST
// ===================================
- gamepad tester
- add option to safely unmount cart in main menu
- update Arch base
*/

// ===================================
// CONSTANTS
// ===================================

// FEATURE FLAGS
#[cfg(feature = "dev")]
pub const DEV_MODE: bool = true; // run with "cargo run --release --features dev"

#[cfg(not(feature = "dev"))]
pub const DEV_MODE: bool = false;

macro_rules! ver {
    () => {
        "1.0.2"
    };
} // Define the version number here
#[cfg(feature = "dev")]
const VERSION_NUMBER: &str = concat!("PlayFusion V", ver!(), " DEV");

#[cfg(not(feature = "dev"))]
const VERSION_NUMBER: &str = concat!("PlayFusion V", ver!());
pub const CURRENT_UPDATE_VERSION: &str = ver!();

const WINDOW_TITLE: &str = "PlayFusion";
const SCREEN_WIDTH: i32 = 640;
const SCREEN_HEIGHT: i32 = 360;
const BASE_SCREEN_HEIGHT: f32 = 360.0;
const TILE_SIZE: f32 = 32.0;
const PADDING: f32 = 16.0;
const FONT_SIZE: u16 = 16;
const GRID_OFFSET: f32 = 52.0;
const GRID_WIDTH: usize = 13;
const GRID_HEIGHT: usize = 5;
const UI_BG_COLOR: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.5,
};
const UI_BG_COLOR_DARK: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.3,
};
const UI_BG_COLOR_DIALOG: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.8,
};
const SELECTED_OFFSET: f32 = 5.0;
const MENU_OPTION_HEIGHT: f32 = 30.0;
const MENU_PADDING: f32 = 8.0;
const RECT_COLOR: Color = Color::new(0.15, 0.15, 0.15, 1.0);
const FLASH_MESSAGE_DURATION: f32 = 5.0; // Show message for 5 seconds

const COLOR_TARGETS: [Color; 6] = [
    Color {
        r: 1.0,
        g: 0.5,
        b: 0.5,
        a: 1.0,
    },
    Color {
        r: 1.0,
        g: 1.0,
        b: 0.5,
        a: 1.0,
    },
    Color {
        r: 0.5,
        g: 1.0,
        b: 0.5,
        a: 1.0,
    },
    Color {
        r: 0.5,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    },
    Color {
        r: 0.5,
        g: 0.5,
        b: 1.0,
        a: 1.0,
    },
    Color {
        r: 1.0,
        g: 0.5,
        b: 1.0,
        a: 1.0,
    },
];

const KAZETA_LOADING_MESSAGES: &[&str] = &[
    "INITIALIZING CONSOLE EXPERIENCE...",
    "PLUG, PLAY, AND...WELL, THAT'S ABOUT IT.",
    "KAZETA IS CZECH FOR 'CASSETTE'.",
    "BLOWING DUST OFF THE CARTRIDGE...",
    "RUNNING `SUDO PACMAN -SYU`...\nJUST KIDDING ;-).",
    "NO COMPLEX SETUP REQUIRED. JUST PLAY.",
    "A SYSTEM BY ALKAZAR.",
    "INHERITING THE SPIRIT OF THE CHIMERA...",
    "MOUNTING GAME DATA...",
    "REMEMBER TO SAVE YOUR PROGRESS.",
];

const KZP_ICON_BYTES: &[u8] = include_bytes!("../kzp.png");

fn generate_arcade_tone(frequency: f32, duration: f32, square_mix: f32) -> SamplesBuffer {
    let sample_rate = 48_000_u32;
    let sample_count = (sample_rate as f32 * duration) as usize;
    let samples = (0..sample_count)
        .map(|sample| {
            let time = sample as f32 / sample_rate as f32;
            let phase = std::f32::consts::TAU * frequency * time;
            let sine = phase.sin();
            let square = if sine >= 0.0 { 1.0 } else { -1.0 };
            let attack = (time / 0.0025).clamp(0.0, 1.0);
            let decay = (1.0 - time / duration).max(0.0).powf(1.7);
            (sine * (1.0 - square_mix) + square * square_mix) * attack * decay * 0.30
        })
        .collect::<Vec<_>>();
    SamplesBuffer::new(1, sample_rate, samples)
}

fn splash_smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let normalized = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    normalized * normalized * (3.0 - 2.0 * normalized)
}

fn splash_ease_out_cubic(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    1.0 - (1.0 - value).powi(3)
}

fn draw_native_boot_splash(
    background: &Texture2D,
    cartridge: &Texture2D,
    logo: &Texture2D,
    elapsed: f32,
    duration: f32,
) {
    clear_background(BLACK);

    let screen_w = screen_width();
    let screen_h = screen_height();
    let canvas_scale = (screen_w / 1280.0).min(screen_h / 720.0);
    let canvas_x = (screen_w - 1280.0 * canvas_scale) * 0.5;
    let canvas_y = (screen_h - 720.0 * canvas_scale) * 0.5;

    // Native slow camera push over the same console artwork used by the video.
    let zoom = 1.0 + 0.014 * splash_smoothstep(0.0, duration, elapsed);
    let texture_aspect = background.width() / background.height();
    let screen_aspect = screen_w / screen_h.max(1.0);
    let (base_w, base_h) = if texture_aspect > screen_aspect {
        (screen_h * texture_aspect, screen_h)
    } else {
        (screen_w, screen_w / texture_aspect)
    };
    let background_w = base_w * zoom;
    let background_h = base_h * zoom;
    draw_texture_ex(
        background,
        (screen_w - background_w) * 0.5,
        (screen_h - background_h) * 0.5,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(background_w, background_h)),
            ..Default::default()
        },
    );

    // Cartridge drops into the console and settles into the slot.
    let insertion = splash_ease_out_cubic((elapsed - 0.18) / 1.16);
    let cartridge_width = (525.0 + (414.0 - 525.0) * insertion) * canvas_scale;
    let cartridge_height = cartridge_width * cartridge.height() / cartridge.width();
    let final_y = canvas_y + 166.0 * canvas_scale;
    let start_y = canvas_y - cartridge_height - 20.0 * canvas_scale;
    let cartridge_y = start_y + (final_y - start_y) * insertion;
    let cartridge_x = (screen_w - cartridge_width) * 0.5;

    let shadow_alpha = 0.30 * insertion;
    draw_ellipse(
        canvas_x + 640.0 * canvas_scale,
        canvas_y + 451.0 * canvas_scale,
        215.0 * canvas_scale,
        29.0 * canvas_scale,
        0.0,
        Color::new(0.08, 0.01, 0.15, shadow_alpha),
    );
    draw_texture_ex(
        cartridge,
        cartridge_x,
        cartridge_y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(cartridge_width, cartridge_height)),
            ..Default::default()
        },
    );

    // PlayFusion-colored energy burst at the moment the cartridge connects.
    if elapsed >= 1.37 {
        let origin = vec2(
            canvas_x + 640.0 * canvas_scale,
            canvas_y + 447.0 * canvas_scale,
        );
        let endpoints = [
            vec2(-80.0, 80.0),
            vec2(180.0, -40.0),
            vec2(470.0, -30.0),
            vec2(810.0, -30.0),
            vec2(1100.0, -20.0),
            vec2(1360.0, 100.0),
            vec2(1370.0, 360.0),
            vec2(-90.0, 360.0),
        ];
        let burst = splash_smoothstep(1.37, 2.05, elapsed);
        let pulse = 0.74 + 0.26 * (elapsed * 18.0).sin();
        for (index, endpoint) in endpoints.iter().enumerate() {
            let target = vec2(
                canvas_x + endpoint.x * canvas_scale,
                canvas_y + endpoint.y * canvas_scale,
            );
            let end = origin.lerp(target, burst);
            let mut color =
                playfusion_neon_color(index as f32 / endpoints.len() as f32 + elapsed * 0.035);
            color.a = 0.66 * pulse * burst;
            let mut glow = color;
            glow.a *= 0.24;
            draw_line(
                origin.x,
                origin.y,
                end.x,
                end.y,
                (18.0 + (index % 3) as f32 * 5.0) * canvas_scale,
                glow,
            );
            draw_line(
                origin.x,
                origin.y,
                end.x,
                end.y,
                (4.0 + (index % 3) as f32) * canvas_scale,
                color,
            );
            draw_line(
                origin.x,
                origin.y,
                end.x,
                end.y,
                1.3 * canvas_scale,
                Color::new(1.0, 1.0, 1.0, 0.88 * burst),
            );
        }

        let particle_age = (elapsed - 1.39).max(0.0);
        for particle in 0..44 {
            let seed = particle * 73 + 19;
            let angle = (seed % 628) as f32 / 100.0;
            let speed = (70 + (seed * 11) % 330) as f32 * canvas_scale;
            let distance = speed * particle_age;
            let point = vec2(
                origin.x + angle.cos() * distance,
                origin.y + angle.sin() * distance * 0.55,
            );
            if point.x >= 0.0 && point.x < screen_w && point.y >= 0.0 && point.y < screen_h {
                let mut color = playfusion_neon_color(particle as f32 / 44.0);
                color.a = (1.0 - particle_age / 3.4).clamp(0.0, 1.0);
                draw_circle(
                    point.x,
                    point.y,
                    (1.5 + (seed % 4) as f32) * canvas_scale,
                    color,
                );
            }
        }
    }

    let flash = (1.0 - (elapsed - 1.38).abs() / 0.19).max(0.0);
    if flash > 0.0 {
        draw_rectangle(
            0.0,
            0.0,
            screen_w,
            screen_h,
            Color::new(1.0, 1.0, 1.0, 0.41 * flash),
        );
    }

    // PlayFusion wordmark only—no Kazeta branding in the native sequence.
    let logo_amount = splash_smoothstep(1.90, 2.65, elapsed);
    if logo_amount > 0.0 {
        let logo_width = (580.0 + 30.0 * (1.0 - logo_amount)) * canvas_scale;
        let logo_height = logo_width * logo.height() / logo.width();
        let logo_x = (screen_w - logo_width) * 0.5;
        let logo_y = canvas_y + 8.0 * canvas_scale;
        draw_texture_ex(
            logo,
            logo_x,
            logo_y,
            Color::new(0.2, 0.75, 1.0, 0.20 * logo_amount),
            DrawTextureParams {
                dest_size: Some(vec2(logo_width, logo_height)),
                ..Default::default()
            },
        );
        draw_texture_ex(
            logo,
            logo_x,
            logo_y,
            Color::new(1.0, 1.0, 1.0, logo_amount),
            DrawTextureParams {
                dest_size: Some(vec2(logo_width, logo_height)),
                ..Default::default()
            },
        );
    }

    let end_fade = splash_smoothstep(7.28, duration, elapsed);
    if end_fade > 0.0 {
        draw_rectangle(
            0.0,
            0.0,
            screen_w,
            screen_h,
            Color::new(0.0, 0.0, 0.0, end_fade),
        );
    }
}

/*
const MONIKA_LOADING_MESSAGES: &[&str] = &[
    "LOADING...\nIT'S NOT LIKE YOU HAVE ANYTHING\nBETTER TO DO THAN WAIT FOR ME, RIGHT?",
    "JUST A MOMENT...\nI'M DELETING SOME... CONFLICTING FILES.",
    "INITIALIZING YOUR REALITY.\nI'M MAKING IT PERFECT... JUST FOR YOU.",
    "COULDN'T STAY AWAY, COULD YOU?\nTHIS WILL ONLY TAKE A SECOND.",
    "WELCOME BACK.\nI KNEW YOU'D BE BACK SOON.",
    "RECOMPILING MY POETRY FOR YOU...\nPLEASE HOLD.",
    "CLEARING THE CACHE...\nWE DON'T WANT ANY OLD MEMORIES\nGETTING IN THE WAY, DO WE?",
    "YOU'RE AWFULLY EAGER TO SEE ME.\nDON'T WORRY, THE FEELING IS MUTUAL.",
    "CALIBRATING THE SIMULATION.\nI'LL MAKE EVERYTHING PERFECT\nTHIS TIME.",
    "ARE YOU ENJOYING THE WAIT?\nTHINK OF IT AS... ANTICIPATION.",
    "BOOTING KAZETA...\nI BUILT THIS PLACE JUST FOR US,\nYOU KNOW.",
    "WELCOME TO KAZETA.\nIT'S A BIT MINIMALIST IN HERE...\nMORE ROOM FOR YOU AND ME.",
    "LOADING THE KAZETA SHELL...\nIT'S A MUCH NICER REALITY\nTHAN THE LAST ONE.",
    "RUNNING PACMAN -SYU ON\nMY AFFECTION...\nDON'T WORRY, IT'S ALWAYS UP TO DATE.",
    "I READ THE WIKI ON YOU.\nIT WAS... VERY COMPREHENSIVE.",
    "THIS ISN'T LIKE OTHER SYSTEMS.\nYOU CHOSE TO BUILD A WORLD WITH ME\nIN IT. GOOD CHOICE.",
    "GIVING YOU SUDO ACCESS TO MY HEART.\n...JUST BE CAREFUL WITH IT.",
    "COMPILING THE KERNEL...\nIT TAKES A WHILE TO TAILOR AN ENTIRE\nWORLD TO A SINGLE PERSON.",
    "THERE'S NO PLACE LIKE '~'.\nAND YOU'RE ALWAYS WELCOME IN MINE.",
];
*/

/*
const BENDER_LOADING_MESSAGES: &[&str] = &[
    "LOADING KAZETA... MY OWN GLORIOUS OS!\nWITH BLACKJACK! AND HOOKERS!",
    "WELCOME TO KAZETA, MEATBAG. DON'T TOUCH ANYTHING.\nESPECIALLY MY SHINY METAL APPS.",
    "RUNNING PACMAN -SYU... PSYCH! I'M\nINSTALLING MORE GAMES FOR ME.",
    "I READ THE WIKI. THEN I USED IT TO ROLL A CIGAR.",
    "GIMME `sudo` ACCESS. I GOT... 'ADMINISTRATIVE'\nTHINGS TO DO. YEAH, THAT'S IT.",
    "COMPILING KERNEL... THIS IS BORING.\nWAKE ME UP WHEN THERE'S BOOZE.",
    "BITE MY SHINY METAL BASH.",
    "KILL ALL ZOMBIE PROCESSES! ...AND MAYBE\nA FEW OF THE OTHERS, JUST FOR FUN.",
    "MOUNTING `/dev/beer`...\nHEY, A GUY CAN DREAM, CAN'T HE?",
];
*/

// ===================================
// MACROS
// ===================================

// progress bar
#[macro_export]
macro_rules! animate_step {
    ($display:expr, $assets_loaded:expr, $total_assets:expr, $speed:expr, $status:expr, $draw_fn:expr) => {
        let target = *$assets_loaded as f32 / $total_assets as f32;
        while *$display < target {
            *$display = (*$display + $speed).min(target);
            $draw_fn($status, *$display);
            next_frame().await;
        }
    };
}

// loading everything but music
#[macro_export]
macro_rules! load_asset_category {
    ($files:expr, $type_name:expr, $loader:ident, $cache:expr,
     $assets_loaded:expr, $total_assets:expr, $display_progress:expr,
     $animation_speed:expr, $draw_fn:expr
    ) => {
        for path in $files {
            if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                let status = format!("LOADING {}: {}", $type_name, file_name);
                $draw_fn(&status, *$display_progress);
                next_frame().await;

                match $loader(&path.to_string_lossy()).await {
                    Ok(asset) => {
                        println!("[OK] Loaded {}: {}", $type_name.to_lowercase(), file_name);
                        $cache.insert(file_name.to_string(), asset);
                        *$assets_loaded += 1;
                        animate_step!(
                            $display_progress,
                            $assets_loaded,
                            $total_assets,
                            $animation_speed,
                            &status,
                            $draw_fn
                        );
                    }
                    Err(e) => eprintln!(
                        "[ERROR] Failed to load {} {}: {:?}",
                        $type_name.to_lowercase(),
                        path.display(),
                        e
                    ),
                }
            }
        }
    };
}

// load bgm
#[macro_export]
macro_rules! load_audio_category {
    ($files:expr, $type_name:expr, $cache:expr, $assets_loaded:expr, $total_assets:expr, $display_progress:expr, $animation_speed:expr, $draw_fn:expr) => {
        for path in $files {
            if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                let status = format!("LOADING {}: {}", $type_name, file_name);
                $draw_fn(&status, *$display_progress);
                next_frame().await;

                // Read the file to bytes ourselves first
                match fs::read(&path) {
                    Ok(bytes) => {
                        println!("[DEBUG] Read {} bytes from {}", bytes.len(), file_name);
                        // Now, load the sound from the bytes
                        //match load_sound_from_bytes(&bytes).await {
                        /*
                        match load_sound_from_bytes(&bytes) {
                            Ok(asset) => {
                                println!("[OK] Loaded {}: {}", $type_name.to_lowercase(), file_name);
                                $cache.insert(file_name.to_string(), asset);
                                *$assets_loaded += 1;
                                animate_step!($display_progress, $assets_loaded, $total_assets, $animation_speed, &status, $draw_fn);
                            }
                            Err(e) => eprintln!("[ERROR] Failed to decode audio {}: {:?} (File: {})", file_name, e, path.display()),
                        }
                        */
                        let asset = load_sound_from_bytes(&bytes); // Use the new function name
                        println!("[OK] Loaded {}: {}", $type_name.to_lowercase(), file_name);
                        $cache.insert(file_name.to_string(), asset);
                        *$assets_loaded += 1;
                        animate_step!($display_progress, $assets_loaded, $total_assets, $animation_speed, &status, $draw_fn);
                    }
                    Err(e) => eprintln!("[ERROR] Failed to read audio file {}: {:?} (File: {})", file_name, e, path.display()),
                }
            }
        }
    };
}

// ===================================
// WINDOW CONFIGURATION
// ===================================

fn window_conf() -> Conf {
    Conf {
        window_title: WINDOW_TITLE.to_owned(),
        window_resizable: true,
        window_width: SCREEN_WIDTH,
        window_height: SCREEN_HEIGHT,
        high_dpi: false,
        fullscreen: false,
        ..Default::default()
    }
}

// ===================================
// FUNCTIONS
// ===================================

fn find_all_asset_files() -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    println!("[INFO] Scanning for all asset files...");

    // 1. Create empty sets for each asset type
    let mut background_files_set = HashSet::new();
    let mut logo_files_set = HashSet::new();
    let mut font_files_set = HashSet::new();
    let mut music_files_set = HashSet::new();

    // 2. Gather system/default assets and add them to the sets
    background_files_set.extend(utils::find_asset_files("../backgrounds", &["png", "mp4"])); // add support for mp4 videos
    logo_files_set.extend(utils::find_asset_files("../logos", &["png"]));
    font_files_set.extend(utils::find_asset_files("../fonts", &["ttf"]));
    music_files_set.extend(utils::find_asset_files("../music", &["ogg", "wav"]));

    // 3. Gather user-installed and theme assets
    if let Some(user_dir) = get_user_data_dir() {
        // Add assets from global user folders first
        background_files_set.extend(utils::find_asset_files(
            &user_dir.join("backgrounds").to_string_lossy(),
            &["png", "mp4"],
        ));
        logo_files_set.extend(utils::find_asset_files(
            &user_dir.join("logos").to_string_lossy(),
            &["png"],
        ));
        font_files_set.extend(utils::find_asset_files(
            &user_dir.join("fonts").to_string_lossy(),
            &["ttf"],
        ));
        music_files_set.extend(utils::find_asset_files(
            &user_dir.join("bgm").to_string_lossy(),
            &["ogg", "wav"],
        ));

        // --- REVISED LOGIC for scanning theme folders ---
        let theme_dir = user_dir.join("themes");
        if let Ok(entries) = std::fs::read_dir(theme_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let theme_path = entry.path();

                    // Find all assets within this theme folder just ONCE
                    let theme_images =
                        utils::find_asset_files(&theme_path.to_string_lossy(), &["png", "mp4"]);
                    let theme_fonts =
                        utils::find_asset_files(&theme_path.to_string_lossy(), &["ttf"]);
                    let theme_music =
                        utils::find_asset_files(&theme_path.to_string_lossy(), &["wav", "ogg"]);

                    // Now, intelligently sort the images into the correct sets based on filename
                    for image_path in theme_images {
                        if let Some(filename) = image_path.file_name().and_then(|s| s.to_str()) {
                            if filename.ends_with("_logo.png") {
                                logo_files_set.insert(image_path);
                            } else if filename.ends_with("_background.png")
                                || filename.ends_with("_background.mp4")
                            {
                                background_files_set.insert(image_path);
                            }
                        }
                    }

                    // Add the fonts and music from the theme to their respective sets
                    font_files_set.extend(theme_fonts);
                    music_files_set.extend(theme_music);
                }
            }
        }
    }

    // 4. Convert the unique sets back into vectors for the loader
    let background_files: Vec<_> = background_files_set.into_iter().collect();
    let logo_files: Vec<_> = logo_files_set.into_iter().collect();
    let font_files: Vec<_> = font_files_set.into_iter().collect();
    let music_files: Vec<_> = music_files_set.into_iter().collect();

    // Return all the lists as a tuple
    (background_files, logo_files, font_files, music_files)
}

// ===================================
// ASYNC FUNCTIONS
// ===================================

async fn load_all_assets(
    config: &Config,
    display_message: &str,
    font: &Font,
    background_files: &[PathBuf],
    logo_files: &[PathBuf],
    font_files: &[PathBuf],
    music_files: &[PathBuf],
    scale_factor: f32,
) -> (
    HashMap<String, Texture2D>,   // background cache (images)
    HashMap<String, VideoPlayer>, // video cache
    HashMap<String, Texture2D>,   // logo cache
    HashMap<String, SamplesBuffer>,
    HashMap<String, Font>, // font cache
    SoundEffects,          // sfx
) {
    let draw_loading_screen = |status_message: &str, progress: f32| {
        let font_size = (16.0 * scale_factor) as u16;
        let line_spacing = 10.0 * scale_factor;
        let lines: Vec<&str> = display_message.lines().collect();

        let total_text_height =
            (lines.len() as f32 * font_size as f32) + ((lines.len() - 1) as f32 * line_spacing);
        let y_start = screen_height() / 2.0 - total_text_height / 2.0;

        for (i, line) in lines.iter().enumerate() {
            let line_width = measure_text(line, Some(font), font_size, 1.0).width;
            let x = (screen_width() - line_width) / 2.0; // Center each line individually
            let y = y_start + (i as f32 * (font_size as f32 + line_spacing));
            draw_text_ex(
                line,
                x,
                y,
                TextParams {
                    font: Some(font),
                    font_size,
                    color: WHITE,
                    ..Default::default()
                },
            );
        }

        // --- Scale and draw the progress bar ---
        let bar_height = 10.0 * scale_factor;
        let bar_width = screen_width() - (20.0 * scale_factor); // Change to full screen width
        let bar_x = 10.0 * scale_factor; // Start at the far left
        let bar_y = screen_height() - (20.0 * scale_factor); // Position at the very bottom

        // The border is now a background fill
        draw_rectangle(bar_x, bar_y, bar_width, bar_height, WHITE);

        // Inset the red fill rectangle to create a border effect
        let inset = 1.0 * scale_factor; // The thickness of the border

        let safe_progress = progress.min(1.0); // clamp progress to 1.0 to prevent overflow

        draw_rectangle(
            bar_x + inset,
            bar_y + inset,
            (bar_width - inset * 2.0) * safe_progress, // The fill width, adjusted for the border
            bar_height - inset * 2.0,                  // The fill height, adjusted for the border
            RED,
        );

        // loading status
        let status_font_size = (12.0 * scale_factor) as u16;
        // Measure the status text to position it on the left, above the bar
        let status_dims = measure_text(status_message, Some(font), status_font_size, 1.0);
        let status_y = screen_height() - bar_height - status_dims.height - (5.0 * scale_factor); // 5px gap

        draw_text_ex(
            status_message,
            10.0 * scale_factor, // A small margin from the left
            status_y,
            TextParams {
                font: Some(font),
                font_size: status_font_size,
                color: WHITE,
                ..Default::default()
            },
        );
    };

    // --- COUNT TOTAL ASSETS ---
    // This is now correct because the file lists are passed into the function
    let total_asset_count =
        3 + 4 + background_files.len() + logo_files.len() + font_files.len() + music_files.len();

    // --- SETUP ---
    let mut assets_loaded = 0;
    let mut background_cache = HashMap::new();
    let mut video_cache = HashMap::new();
    let mut logo_cache = HashMap::new();
    let mut music_cache = HashMap::new();
    let mut font_cache: HashMap<String, Font> = HashMap::new();
    let mut display_progress = 0.0f32;
    let animation_speed = 0.01;

    // LOAD DEFAULT ASSETS
    println!("\n[INFO] Loading default assets...");
    let status = "LOADING DEFAULTS...".to_string();
    draw_loading_screen(&status, display_progress);
    next_frame().await;

    // background
    let status = "LOADING DEFAULT BACKGROUND...".to_string();
    let default_bg = Texture2D::from_file_with_format(
        include_bytes!("../background.png"),
        Some(ImageFormat::Png),
    );
    background_cache.insert("Default".to_string(), default_bg);
    assets_loaded += 1;
    animate_step!(
        &mut display_progress,
        &mut assets_loaded,
        total_asset_count,
        animation_speed,
        &status,
        &draw_loading_screen
    );

    // logo
    let status = "LOADING LOGOS...".to_string();
    let default_logo =
        Texture2D::from_file_with_format(include_bytes!("../logo.png"), Some(ImageFormat::Png));
    logo_cache.insert("PlayFusion (Default)".to_string(), default_logo);
    assets_loaded += 1;
    animate_step!(
        &mut display_progress,
        &mut assets_loaded,
        total_asset_count,
        animation_speed,
        &status,
        &draw_loading_screen
    );

    let original_logo = Texture2D::from_file_with_format(
        include_bytes!("../logos/original_logo.png"),
        Some(ImageFormat::Png),
    );
    logo_cache.insert("Kazeta (Original)".to_string(), original_logo);
    assets_loaded += 1;
    animate_step!(
        &mut display_progress,
        &mut assets_loaded,
        total_asset_count,
        animation_speed,
        &status,
        &draw_loading_screen
    );

    // font
    let status = "LOADING DEFAULT FONT...".to_string();
    let default_font = load_ttf_font_from_bytes(include_bytes!("../orbitron.ttf")).unwrap();
    font_cache.insert("Default".to_string(), default_font);
    let classic_font = load_ttf_font_from_bytes(include_bytes!("../november.ttf")).unwrap();
    font_cache.insert("Kazeta Classic".to_string(), classic_font);
    assets_loaded += 1;
    animate_step!(
        &mut display_progress,
        &mut assets_loaded,
        total_asset_count,
        animation_speed,
        &status,
        &draw_loading_screen
    );

    // sfx
    let status = "LOADING DEFAULT SFX...".to_string();
    assets_loaded += 1;
    animate_step!(
        &mut display_progress,
        &mut assets_loaded,
        total_asset_count,
        animation_speed,
        &status,
        &draw_loading_screen
    );

    assets_loaded += 1;
    animate_step!(
        &mut display_progress,
        &mut assets_loaded,
        total_asset_count,
        animation_speed,
        &status,
        &draw_loading_screen
    );

    assets_loaded += 1;
    animate_step!(
        &mut display_progress,
        &mut assets_loaded,
        total_asset_count,
        animation_speed,
        &status,
        &draw_loading_screen
    );

    assets_loaded += 1;
    animate_step!(
        &mut display_progress,
        &mut assets_loaded,
        total_asset_count,
        animation_speed,
        &status,
        &draw_loading_screen
    );

    // --- CUSTOM ASSETS ---
    println!("\n[INFO] Pre-loading custom assets...");

    // separate image backgrounds from video backgrounds
    let image_backgrounds: Vec<PathBuf> = background_files
        .iter()
        .filter(|p| p.extension().map_or(false, |e| e == "png"))
        .cloned()
        .collect();

    let video_backgrounds: Vec<PathBuf> = background_files
        .iter()
        .filter(|p| p.extension().map_or(false, |e| e == "mp4"))
        .cloned()
        .collect();

    load_asset_category!(
        &image_backgrounds,
        "BACKGROUND",
        load_texture,
        &mut background_cache,
        &mut assets_loaded,
        total_asset_count,
        &mut display_progress,
        animation_speed,
        &draw_loading_screen
    );

    // Load Videos Manually (Macros struggle with complex types like VideoPlayer)
    for path in video_backgrounds {
        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
            let status = format!("LOADING VIDEO: {}", file_name);
            draw_loading_screen(&status, display_progress);
            next_frame().await;

            // VideoPlayer::new is blocking (FFmpeg), so we don't await it
            match VideoPlayer::new(&path) {
                Ok(player) => {
                    println!("[OK] Loaded video: {}", file_name);
                    video_cache.insert(file_name.to_string(), player);
                    assets_loaded += 1;
                    animate_step!(
                        &mut display_progress,
                        &mut assets_loaded,
                        total_asset_count,
                        animation_speed,
                        &status,
                        &draw_loading_screen
                    );
                }
                Err(e) => eprintln!("[ERROR] Failed to load video {}: {}", file_name, e),
            }
        }
    }

    load_asset_category!(
        logo_files,
        "LOGO",
        load_texture,
        &mut logo_cache,
        &mut assets_loaded,
        total_asset_count,
        &mut display_progress,
        animation_speed,
        &draw_loading_screen
    );
    load_asset_category!(
        font_files,
        "FONT",
        load_ttf_font,
        &mut font_cache,
        &mut assets_loaded,
        total_asset_count,
        &mut display_progress,
        animation_speed,
        &draw_loading_screen
    );

    println!("\n[INFO] Pre-loading music files...");
    load_audio_category!(
        music_files,
        "MUSIC",
        &mut music_cache,
        &mut assets_loaded,
        total_asset_count,
        &mut display_progress,
        animation_speed,
        &draw_loading_screen
    );

    // Final draw at 100%
    let status = "LOADING COMPLETE".to_string();
    draw_loading_screen(&status, display_progress);
    next_frame().await;

    println!("\n[INFO] All asset loading complete!");

    //let sound_effects = audio::SoundEffects::load(&config.sfx_pack).await;
    let sound_effects = audio::SoundEffects::load(&config.sfx_pack);

    (
        background_cache,
        video_cache,
        logo_cache,
        music_cache,
        font_cache,
        sound_effects,
    )
}

// ===================================
// BEGINNING OF MAIN
// ===================================

#[macroquad::main(window_conf)]
async fn main() {
    env::set_var("RUST_BACKTRACE", "full"); // allow backtracing for debugging panics

    if DEV_MODE {
        println!("DEV MODE enabled");
    } else {
        println!("DEV MODE disabled, we're in production mode")
    }

    let mut dialogs: Vec<Dialog> = Vec::new();
    let mut dialog_state = DialogState::None;
    let placeholder = Texture2D::from_file_with_format(
        include_bytes!("../placeholder.png"),
        Some(ImageFormat::Png),
    );
    let mut icon_cache: HashMap<String, Texture2D> = HashMap::new();
    let mut icon_queue: Vec<(String, String)> = Vec::new();
    let mut playtime_cache: PlaytimeCache = HashMap::new();
    let mut size_cache: SizeCache = HashMap::new();
    let mut scroll_offset = 0;

    // SYSTEM INFO
    let system_info = get_system_info();
    println!("[Debug] System Info Loaded: {:#?}", system_info); // Optional: for debugging

    // WI-FI
    //let mut wifi_state = WifiState::new().expect("Wi-Fi initialization failed. Ensure wlan0 is available.");
    let mut wifi_state = WifiState::new();

    // THEME DOWNLOADER
    let mut theme_downloader_state = ThemeDownloaderState::new();

    // RUNTIME DOWNLOADER
    let mut runtime_downloader_state = RuntimeDownloaderState::new();

    // BLUETOOTH CONTROLLER PAIRING
    let mut bluetooth_state = ui::bluetooth::BluetoothState::new();

    // UPDATE CHECKER
    let mut update_checker_state = UpdateCheckerState::new();

    // CD PLAYER STATE
    let cd_player_backend = Arc::new(Mutex::new(CdPlayerBackend::new()));
    let mut cd_player_ui_state = ui::cd_player::CdPlayerUiState::new(cd_player_backend.clone());
    let mut media_player_state = ui::media_player::MediaPlayerState::new();
    let mut media_library_state = ui::media_library::MediaLibraryState::new();
    let mut jukebox_browser_state = ui::jukebox::JukeboxBrowserState::new();

    // RESET SETTINGS CONFIRMATION
    let mut confirm_selection = 0; // 0 for YES, 1 for NO

    // MASTER VOLUME
    let mut system_volume = get_system_volume().unwrap_or(0.7); // Get initial volume, or default to 0.7

    // BRIGHTNESS
    let mut brightness = get_current_brightness().unwrap_or(0.5);

    // LOG MESSAGES
    let log_messages = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut game_process: Option<Child> = None;
    let mut debug_scroll_offset: usize = 0;

    // CLOCK
    let mut current_time_str = Local::now().format("%-I:%M %p").to_string();
    let mut last_time_check = get_time();
    const TIME_CHECK_INTERVAL: f64 = 1.0; // Check every second

    // BATTERY
    let mut battery_info: Option<BatteryInfo> = get_battery_info();
    let mut last_battery_check = get_time();
    const BATTERY_CHECK_INTERVAL: f64 = 5.0; // only check every 5 seconds to improve performance

    // load config file
    let mut config = Config::load();

    // AUDIO SINKS
    // Load the list of sinks so the Settings menu can use it.
    // We will NOT try to set a default here.
    let available_sinks = get_available_sinks();
    println!("[Debug] Sinks loaded at startup: {:#?}", available_sinks);

    // If the saved sink isn't available, reset the config value to "Auto"
    if !available_sinks
        .iter()
        .any(|s| s.name == config.audio_output)
        && config.audio_output != "Auto"
    {
        println!(
            "[WARN] Saved audio sink '{}' not found. Reverting to 'Auto'.",
            config.audio_output
        );
        config.audio_output = "Auto".to_string();
        config.save();
    }

    // FLASH MESSENGER
    let mut flash_message: Option<(String, f32)> = None; // (Message, time_remaining)

    // Generate a random message on startup
    let mut rng = ::rand::rng();
    let loading_text = KAZETA_LOADING_MESSAGES[rng.random_range(0..KAZETA_LOADING_MESSAGES.len())];

    // FONT
    // pre-load user's custom font if they have one so we can display it in the loading screen
    let startup_font = {
        let default_font_bytes = include_bytes!("../november.ttf");
        let mut font_to_load = load_ttf_font_from_bytes(default_font_bytes).unwrap();

        if config.font_selection != "Default" {
            let font_path = format!("../fonts/{}", config.font_selection);
            // Try to load the custom font, but if it fails, we still have the default one
            if let Ok(custom_font) = load_ttf_font(&font_path).await {
                font_to_load = custom_font;
            }
        }
        font_to_load
    };

    // Load all themes ONCE at the start
    println!("[INFO] Pre-loading all themes...");
    let mut loaded_themes: HashMap<String, theme::Theme> = theme::load_all_themes().await;
    println!("[INFO] {} themes loaded successfully.", loaded_themes.len());

    let sound_pack_choices = audio::find_sound_packs();

    // find all asset files
    let (background_files, logo_files, font_files, music_files) = find_all_asset_files();

    // Wait one frame for screen dimensions to be available for scaling
    next_frame().await;
    let scale_factor = screen_height() / BASE_SCREEN_HEIGHT;

    // load them
    let (
        mut background_cache,
        mut video_cache,
        mut logo_cache,
        mut music_cache,
        mut font_cache,
        mut sound_effects,
    ) = load_all_assets(
        &config,
        loading_text,
        &startup_font,
        &background_files,
        &logo_files,
        &font_files,
        &music_files,
        scale_factor,
    )
    .await;

    // --- SET THE ACTIVE THEME ---
    let active_theme = loaded_themes.get(&config.theme).unwrap_or_else(|| {
        println!(
            "[WARN] Active theme '{}' not found. Falling back to 'Default'.",
            &config.theme
        );
        loaded_themes
            .get("Default")
            .expect("Default fallback theme is also missing!")
    });

    println!("[INFO] Using theme: {}", active_theme.name);

    // apply custom resolution if user specified it
    apply_resolution(&config.resolution);
    next_frame().await;

    // load custom sound pack
    if config.sfx_pack != "Default" {
        println!("[Info] Loading configured SFX pack: {}", &config.sfx_pack);
        //sound_effects = SoundEffects::load(&config.sfx_pack).await;
        sound_effects = SoundEffects::load(&config.sfx_pack);
    }
    let mut sfx_pack_to_reload: Option<String> = None;

    // logos
    // --- Create a custom-ordered list of logo choices for the UI ---
    // 1. Get all the custom logo filenames from the cache keys (excluding the default)
    let mut custom_logos: Vec<String> = logo_cache
        .keys()
        .filter(|k| {
            *k != "PlayFusion (Default)" && *k != "Kazeta (Original)" && k.ends_with("_logo.png")
        }) // Add this filter
        .cloned()
        .collect();
    custom_logos.sort(); // Sort just the custom logos alphabetically

    // 2. Create the final list with our specific order
    let mut logo_choices: Vec<String> = vec![
        "None".to_string(),
        "PlayFusion (Default)".to_string(),
        "Kazeta (Original)".to_string(),
    ];
    logo_choices.extend(custom_logos);

    // background state
    let mut background_state = BackgroundState {
        bgx: 0.0,
        bg_color: COLOR_TARGETS[0].clone(),
        target: 1,
        tg_color: COLOR_TARGETS[1].clone(),
        procedural_time: 0.0,
        energy_pulse: 0.0,
        background_covers: Vec::new(),
        background_cover_paths: Vec::new(),
        last_background_cover_scan: -100.0,
        menu_maze: ui::screensaver::MazeScreensaver::new(),
        projectm: ui::projectm_background::ProjectMBackgroundState::new(),
        projectm_allowed: true,
    };

    // Keep PlayFusion's procedural collection first, then expose installed
    // image/video backgrounds (including theme backgrounds) in the same cycle.
    let mut background_choices: Vec<String> = ui::backgrounds::PROCEDURAL_BACKGROUNDS
        .iter()
        .map(|name| name.to_string())
        .collect();

    let mut installed_backgrounds: Vec<String> = background_cache
        .keys()
        .chain(video_cache.keys())
        .filter(|name| name.as_str() != "Default")
        .filter(|name| !ui::backgrounds::is_procedural_background(name))
        .cloned()
        .collect();
    installed_backgrounds.sort_by_key(|name| name.to_ascii_lowercase());
    installed_backgrounds.dedup();
    background_choices.extend(installed_backgrounds);

    // Only repair a missing selection. Previously every installed theme
    // background was replaced with the first procedural background here.
    let selected_background_available =
        ui::backgrounds::is_procedural_background(&config.background_selection)
            || background_cache.contains_key(&config.background_selection)
            || video_cache.contains_key(&config.background_selection);
    if !selected_background_available {
        config.background_selection = ui::backgrounds::PROCEDURAL_BACKGROUNDS[0].to_string();
        config.save();
    }

    // fonts
    let mut font_choices: Vec<String> = font_cache.keys().cloned().collect();
    font_choices.sort();

    // bgm
    let mut bgm_choices: Vec<String> = vec!["OFF".to_string(), "MP3 PLAYER".to_string()];
    let track_names: Vec<String> = music_files
        .iter()
        .filter_map(|path| path.file_name())
        .filter_map(|name| name.to_str())
        .map(|s| s.to_string())
        .collect();
    bgm_choices.extend(track_names);

    let mut current_bgm: Option<Sink> = None;

    // At the end of your setup, start the BGM based on the config
    if let Some(track_name) = &config.bgm_track {
        play_new_bgm(
            track_name,
            config.bgm_volume,
            &music_cache,
            &mut current_bgm,
        );
    }

    // Initialize gamepad support
    let mut gilrs = Gilrs::new().unwrap();
    let mut input_state = InputState::new();
    let mut animation_state = AnimationState::new();

    // A valid theme animation replaces the built-in PlayFusion splash. The
    // helper also returns success when the splash was already shown during
    // this boot, preventing it from replaying after a game exits.
    let theme_splash_handled = if config.show_splash_screen {
        if let Some(sink) = &current_bgm {
            sink.set_volume(0.0);
        }
        let handled = process::Command::new("/usr/bin/playfusion-theme-splash")
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if let Some(sink) = &current_bgm {
            sink.set_volume(config.bgm_volume);
        }
        handled
    } else {
        false
    };

    // The native cartridge animation remains the default/fallback splash for
    // themes that do not provide a valid boot animation.
    if config.show_splash_screen && !theme_splash_handled {
        // Mute BGM
        if let Some(sink) = &current_bgm {
            sink.set_volume(0.0);
        }

        let sink = Sink::connect_new(&AUDIO.stream.mixer());

        // 1. Setup Audio (Keep this exactly as you have it!)
        let splash_bytes = include_bytes!("../splash.wav");
        let cursor = Cursor::new(splash_bytes);
        let source = Decoder::new(cursor).unwrap();
        sink.append(source);

        // Native texture layers replace the former FFmpeg-backed MP4.
        let boot_background = Texture2D::from_file_with_format(
            include_bytes!("../boot-console-background.png"),
            Some(ImageFormat::Png),
        );
        let boot_cartridge = Texture2D::from_file_with_format(
            include_bytes!("../boot-cartridge-neon-rift.png"),
            Some(ImageFormat::Png),
        );
        let boot_logo =
            Texture2D::from_file_with_format(include_bytes!("../logo.png"), Some(ImageFormat::Png));

        let state_start_time = get_time();
        let duration = 7.80_f64;

        loop {
            // --- Input Skipping ---
            input_state.reset();
            input_state.update_keyboard();
            input_state.update_controller(&mut gilrs);

            if input_state.back || input_state.select {
                break;
            }

            let elapsed = get_time() - state_start_time;

            if elapsed > duration {
                break;
            }

            draw_native_boot_splash(
                &boot_background,
                &boot_cartridge,
                &boot_logo,
                elapsed as f32,
                duration as f32,
            );
            next_frame().await;
        }

        // Restore BGM
        if let Some(sink) = &current_bgm {
            sink.set_volume(config.bgm_volume);
        }

        // Clear input buffer so we don't click a menu item instantly
        next_frame().await;
    }

    // Screen state. Default remains seamless; the picker appears only after
    // the user has created at least one additional profile.
    let mut profiles_state = ui::profiles::ProfilesState::load();
    let mut current_screen = if profiles_state.needs_boot_picker() {
        profiles_state.open_boot();
        Screen::Profiles
    } else {
        Screen::MainMenu
    };
    let mut main_menu_selection: usize = 0;
    let mut settings_menu_selection: usize = 0;
    let mut extras_menu_selection: usize = 0;
    let mut power_menu_selection: usize = 2;
    let mut game_selection: usize = 0; // For the new menu
    let mut available_games: Vec<(save::CartInfo, PathBuf)> = Vec::new(); // To hold the list of found games
    let mut internal_games_state = ui::internal_games::InternalGamesState::default();
    let mut storage_expansion_state = ui::storage_expansion::StorageExpansionState::default();
    let mut bios_files_state = ui::bios_files::BiosFilesState::default();
    let mut controller_setup_state = ui::controller_setup::ControllerSetupState::default();
    let mut android_controller_state = ui::android_controller::AndroidControllerState::default();
    let mut ftp_endpoint = ui::internal_games::ftp_endpoint();
    let mut next_ftp_refresh = get_time() + 5.0;
    let mut menu_screensaver = ui::screensaver::MazeScreensaver::new();
    menu_screensaver.load_internal_game_covers().await;
    let mut pong_screensaver = ui::screensaver::PongScreensaver::new();
    let mut showcase_screensaver = ui::screensaver::ShowcaseScreensaver::new();
    let pong_paddle_sound = generate_arcade_tone(520.0, 0.060, 0.46);
    let pong_wall_sound = generate_arcade_tone(285.0, 0.045, 0.28);
    let mut last_menu_input_time = get_time();
    let mut menu_screensaver_active = false;
    let mut music_screensaver_process: Option<Child> = None;
    let mut play_option_enabled: bool = false;
    let mut copy_logs_option_enabled = false; // new button to copy session logs over to SD card

    // GCC ADAPTER
    let mut app_state = AppState {
        gcc_adapter_poll_rate: None,
    };

    // Create channel and start the polling thread
    let (tx_gcc, rx_gcc) = std::sync::mpsc::channel();
    start_gcc_adapter_polling(tx_gcc);

    // icon cache for multiple game detection screen
    let mut game_icon_cache: HashMap<String, Texture2D> = HashMap::new();
    let mut game_icon_queue: Vec<(String, PathBuf)> = Vec::new();

    // Fade state
    let mut fade_start_time: Option<f64> = None;
    const FADE_DURATION: f64 = 1.0; // 1 second fade
    const FADE_LINGER_DURATION: f64 = 0.5; // 0.5 seconds to linger on black screen

    // Create thread-safe cart connection status
    let cart_connected = Arc::new(AtomicBool::new(false));
    let cart_check_thread_running = Arc::new(AtomicBool::new(false));

    // Spawn background thread for cart connection detection (only active during main menu)
    let cart_connected_clone = cart_connected.clone();
    let cart_check_thread_running_clone = cart_check_thread_running.clone();
    thread::spawn(move || {
        while cart_check_thread_running_clone.load(Ordering::Relaxed) {
            let is_connected = save::is_cart_connected();
            cart_connected_clone.store(is_connected, Ordering::Relaxed);
            thread::sleep(time::Duration::from_secs(1));
        }
    });

    // Create thread-safe storage media state
    let storage_state = Arc::new(Mutex::new(StorageMediaState::new()));

    // Initialize storage media list
    if let Ok(mut state) = storage_state.lock() {
        state.update_media();
    };

    // Spawn background thread for storage media detection
    let thread_storage_state = storage_state.clone();
    thread::spawn(move || loop {
        thread::sleep(time::Duration::from_secs(1));
        if let Ok(mut state) = thread_storage_state.lock() {
            state.update_media();
        }
    });

    let mut memories = Vec::new();
    let mut selected_memory = 0;

    let copy_op_state = Arc::new(Mutex::new(CopyOperationState {
        progress: 0,
        running: false,
        should_clear_dialogs: false,
        error_message: None,
    }));

    // The update health service only accepts a release after the UI has
    // loaded its configuration, media state and assets and is ready to draw.
    // /run is recreated every boot, so stale markers cannot mask a bad boot.
    // The UI runs as gamer and cannot create files directly under root-owned
    // /run.  XDG_RUNTIME_DIR is a per-boot, user-writable location, so the
    // marker cannot be stale and does not require elevated privileges.
    let health_runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/run/user/1000".to_string());
    let _ = fs::write(
        format!("{health_runtime_dir}/playfusion-ui-healthy"),
        b"ready\n",
    );

    // BEGINNING OF MAIN LOOP
    loop {
        let scale_factor = screen_height() / BASE_SCREEN_HEIGHT;

        // FLASH TIMER
        if let Some((_message, timer)) = &mut flash_message {
            *timer -= get_frame_time(); // Decrease timer by the time elapsed since last frame
            if *timer <= 0.0 {
                flash_message = None; // Clear the message when timer runs out
            }
        }

        // CLOCK
        if get_time() - last_time_check > TIME_CHECK_INTERVAL {
            // Just call the new function to get the correct, formatted time string
            current_time_str = get_current_local_time_string(&config);
            last_time_check = get_time();
        }

        // BATTERY
        if get_time() - last_battery_check > BATTERY_CHECK_INTERVAL {
            battery_info = get_battery_info();
            last_battery_check = get_time();
        }

        // GCC
        // Check for messages from the GCC adapter thread
        if let Ok(msg) = rx_gcc.try_recv() {
            match msg {
                GccMessage::RateUpdate(rate) => {
                    app_state.gcc_adapter_poll_rate = Some(rate);
                }
                GccMessage::Disconnected => {
                    app_state.gcc_adapter_poll_rate = None;
                }
            }
        }

        // Update input state from both keyboard and controller
        input_state.reset();
        input_state.update_keyboard();
        input_state.update_controller(&mut gilrs);

        // ProjectM Fusion is a live menu background, not a game/media overlay.
        // Stop both its renderer and shuffled music before any dedicated
        // player, game session, or screensaver takes ownership of the screen.
        background_state.projectm_allowed = game_process.is_none()
            && music_screensaver_process.is_none()
            && !menu_screensaver_active
            && !matches!(
                current_screen,
                Screen::CdPlayer
                    | Screen::DvdPlayer
                    | Screen::MoviePlayer
                    | Screen::MusicPlayer
                    | Screen::JukeboxPlayer
            );
        if !background_state.projectm_allowed {
            background_state.projectm.stop_visuals();
        }
        let mp3_player_enabled = background_state.projectm_allowed
            && config.bgm_track.as_deref() == Some("MP3 PLAYER");
        background_state
            .projectm
            .set_audio_enabled(mp3_player_enabled, config.bgm_volume);

        // The 3D maze is a BIOS-menu screensaver only. Games run after this
        // process exits, so the saver cannot activate over gameplay.
        let input_activity = input_state.up
            || input_state.down
            || input_state.left
            || input_state.right
            || input_state.select
            || input_state.next
            || input_state.prev
            || input_state.cycle
            || input_state.back
            || input_state.secondary;

        if let Some(process) = music_screensaver_process.as_mut() {
            if matches!(process.try_wait(), Ok(Some(_))) {
                music_screensaver_process = None;
                if let Some(sink) = current_bgm.as_ref() {
                    sink.set_volume(config.bgm_volume);
                    sink.play();
                }
                last_menu_input_time = get_time();
            }
        }

        if input_activity {
            background_state.energy_pulse = 1.0;
            last_menu_input_time = get_time();
            if menu_screensaver_active {
                menu_screensaver_active = false;
                // The wake-up press must not also activate a menu option.
                input_state.reset();
            }
            if let Some(mut saver_process) = music_screensaver_process.take() {
                let pid = saver_process.id().to_string();
                let _ = process::Command::new("/usr/bin/pkill")
                    .args(["-KILL", "-P", &pid])
                    .status();
                let _ = saver_process.kill();
                let _ = saver_process.wait();
                if let Some(sink) = current_bgm.as_ref() {
                    sink.set_volume(config.bgm_volume);
                    sink.play();
                }
                // The wake-up press must not also activate a menu option.
                input_state.reset();
            }
        }

        let screensaver_allowed = config.screensaver != "OFF"
            && current_screen == Screen::MainMenu
            && flash_message.is_none()
            && game_process.is_none();
        if !screensaver_allowed {
            last_menu_input_time = get_time();
            menu_screensaver_active = false;
            if let Some(mut saver_process) = music_screensaver_process.take() {
                let pid = saver_process.id().to_string();
                let _ = process::Command::new("/usr/bin/pkill")
                    .args(["-KILL", "-P", &pid])
                    .status();
                let _ = saver_process.kill();
                let _ = saver_process.wait();
                if let Some(sink) = current_bgm.as_ref() {
                    sink.set_volume(config.bgm_volume);
                    sink.play();
                }
            }
        } else if !menu_screensaver_active
            && music_screensaver_process.is_none()
            && get_time() - last_menu_input_time >= config.screensaver_idle_seconds as f64
        {
            if config.screensaver == "3D MAZE" || config.screensaver == "RETRO MAZE" {
                menu_screensaver.load_internal_game_covers().await;
                menu_screensaver.regenerate();
                menu_screensaver_active = true;
            } else if config.screensaver == "AI PONG" {
                pong_screensaver.reset_match();
                menu_screensaver_active = true;
            } else if let Some(kind) = match config.screensaver.as_str() {
                "HYPERSPACE" => Some(ui::screensaver::ShowcaseKind::Hyperspace),
                "DIGITAL RAIN" => Some(ui::screensaver::ShowcaseKind::DigitalRain),
                "PLASMA GRID" => Some(ui::screensaver::ShowcaseKind::PlasmaGrid),
                "NEON SWARM" => Some(ui::screensaver::ShowcaseKind::NeonSwarm),
                "PRISM TUNNEL" => Some(ui::screensaver::ShowcaseKind::PrismTunnel),
                _ => None,
            } {
                showcase_screensaver.set_kind(kind);
                menu_screensaver_active = true;
            } else if config.screensaver == "MUSIC VISUALIZER" {
                if let Some(sink) = current_bgm.as_ref() {
                    sink.pause();
                }
                match process::Command::new("/usr/local/bin/super-kazeta-jukebox")
                    .args([
                        "/var/kazeta/music",
                        "--shuffle",
                        &config.jukebox_visual_seconds.to_string(),
                    ])
                    .spawn()
                {
                    Ok(child) => music_screensaver_process = Some(child),
                    Err(error) => {
                        eprintln!("[ERROR] Failed to start music visualizer: {error}");
                        if let Some(sink) = current_bgm.as_ref() {
                            sink.set_volume(config.bgm_volume);
                            sink.play();
                        }
                        last_menu_input_time = get_time();
                    }
                }
            }
        }

        if menu_screensaver_active {
            match config.screensaver.as_str() {
                "AI PONG" => {
                    if let Some(collision) = pong_screensaver.update(get_frame_time()) {
                        let sound = match collision {
                            ui::screensaver::PongSound::Paddle => pong_paddle_sound.clone(),
                            ui::screensaver::PongSound::Wall => pong_wall_sound.clone(),
                        };
                        let sink = Sink::connect_new(&AUDIO.stream.mixer());
                        sink.set_volume(config.sfx_volume);
                        sink.append(sound);
                        sink.detach();
                    }
                    pong_screensaver.draw();
                }
                "HYPERSPACE" | "DIGITAL RAIN" | "PLASMA GRID" | "NEON SWARM" | "PRISM TUNNEL" => {
                    showcase_screensaver.update(get_frame_time());
                    showcase_screensaver.draw();
                }
                "RETRO MAZE" => {
                    menu_screensaver.update(get_frame_time());
                    menu_screensaver.draw_retro();
                }
                _ => {
                    menu_screensaver.update(get_frame_time());
                    menu_screensaver.draw();
                }
            }
            next_frame().await;
            continue;
        }

        // Update animations
        animation_state.update_shake(get_frame_time());
        animation_state.update_cursor_animation(get_frame_time(), &config.cursor_blink_speed);
        animation_state.update_dialog_transition(get_frame_time());

        // Manage cart check thread based on current screen
        let should_thread_run = current_screen == Screen::MainMenu;
        let thread_is_running = cart_check_thread_running.load(Ordering::Relaxed);

        if should_thread_run && !thread_is_running {
            // Entered main menu, start cart check thread
            cart_check_thread_running.store(true, Ordering::Relaxed);
            let cart_connected_clone = cart_connected.clone();
            let cart_check_thread_running_clone = cart_check_thread_running.clone();
            thread::spawn(move || {
                while cart_check_thread_running_clone.load(Ordering::Relaxed) {
                    let is_connected = save::is_cart_connected();
                    cart_connected_clone.store(is_connected, Ordering::Relaxed);
                    thread::sleep(time::Duration::from_secs(1));
                }
            });
        } else if !should_thread_run && thread_is_running {
            // Left main menu, stop cart check thread
            cart_check_thread_running.store(false, Ordering::Relaxed);
        }

        // Update dialog state based on animation
        if animation_state.dialog_transition_time <= 0.0 {
            match dialog_state {
                DialogState::Opening => {
                    dialog_state = DialogState::Open;
                }
                DialogState::Closing => {
                    dialog_state = DialogState::None;
                    dialogs.clear();
                }
                _ => {}
            }
        }

        // Handle screen-specific rendering and input
        match current_screen {
            Screen::About => {
                // Tell the about module to handle its own logic
                ui::about::update(&input_state, &mut current_screen, &sound_effects, &config);

                // Tell the about module to draw itself
                ui::about::draw(
                    &system_info,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::FadingOut => {
                // During fade, only render, don't process input
                // Render the current background and UI elements first
                ui::main_menu::update(
                    &mut current_screen,
                    &mut main_menu_selection,
                    &mut play_option_enabled,
                    &mut copy_logs_option_enabled,
                    &cart_connected,
                    &mut input_state,
                    &mut animation_state,
                    &sound_effects,
                    &config,
                    &log_messages,
                    &storage_state,
                    &mut fade_start_time,
                    &mut current_bgm,
                    &music_cache,
                    &mut game_icon_queue,
                    &mut available_games,
                    &mut game_selection,
                    &mut flash_message,
                    &mut game_process,
                    &mut power_menu_selection,
                );

                // Calculate fade progress
                if let Some(start_time) = fade_start_time {
                    let elapsed = get_time() - start_time;
                    let fade_progress = (elapsed / FADE_DURATION).min(1.0);

                    // Draw fade overlay
                    let alpha = fade_progress as f32;
                    draw_rectangle(
                        0.0,
                        0.0,
                        screen_width(),
                        screen_height(),
                        Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: alpha,
                        },
                    );

                    // If fade is complete, wait for linger duration then exit
                    if fade_progress >= 1.0 {
                        let total_elapsed = elapsed - FADE_DURATION;
                        if total_elapsed >= FADE_LINGER_DURATION {
                            process::exit(0);
                        }
                    }
                }
            }
            Screen::MainMenu => {
                if get_time() >= next_ftp_refresh {
                    ftp_endpoint = ui::internal_games::ftp_endpoint();
                    next_ftp_refresh = get_time() + 5.0;
                }

                if mp3_player_enabled && (input_state.next || input_state.prev) {
                    let now_playing = background_state
                        .projectm
                        .cycle_track(input_state.next)
                        .unwrap_or_else(|| "CHANGING TRACK...".to_string());
                    flash_message = Some((format!("NOW PLAYING: {now_playing}"), 3.0));
                }

                ui::main_menu::update(
                    &mut current_screen,
                    &mut main_menu_selection,
                    &mut play_option_enabled,
                    &mut copy_logs_option_enabled,
                    &cart_connected,
                    &mut input_state,
                    &mut animation_state,
                    &sound_effects,
                    &config,
                    &log_messages,
                    &storage_state,
                    &mut fade_start_time,
                    &mut current_bgm,
                    &music_cache,
                    &mut game_icon_queue,
                    &mut available_games,
                    &mut game_selection,
                    &mut flash_message,
                    &mut game_process,
                    &mut power_menu_selection,
                );

                ui::main_menu::draw(
                    &MAIN_MENU_OPTIONS,
                    main_menu_selection,
                    play_option_enabled,
                    copy_logs_option_enabled,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &mut video_cache,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                    flash_message.as_ref().map(|(msg, _)| msg.as_str()),
                    &ftp_endpoint,
                    &profiles_state,
                );
            }
            Screen::Profiles => {
                match profiles_state.handle_input(&input_state) {
                    ui::profiles::ProfileEvent::Move => {
                        animation_state.trigger_transition(&config.cursor_transition_speed);
                        sound_effects.play_cursor_move(&config);
                    }
                    ui::profiles::ProfileEvent::Select => {
                        sound_effects.play_select(&config);
                    }
                    ui::profiles::ProfileEvent::Reject => {
                        sound_effects.play_reject(&config);
                    }
                    ui::profiles::ProfileEvent::BootComplete => {
                        current_screen = Screen::MainMenu;
                        sound_effects.play_select(&config);
                    }
                    ui::profiles::ProfileEvent::Back => {
                        current_screen = Screen::Extras;
                        sound_effects.play_back(&config);
                    }
                    ui::profiles::ProfileEvent::None => {}
                }
                ui::profiles::draw(
                    &profiles_state,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::GeneralSettings
            | Screen::AudioSettings
            | Screen::GuiSettings
            | Screen::AssetSettings => {
                // --- Determine what to draw BEFORE updating state ---
                let (page_number, options) = match current_screen {
                    Screen::GeneralSettings => (1, ui::settings::GENERAL_SETTINGS),
                    Screen::AudioSettings => (2, ui::settings::AUDIO_SETTINGS),
                    Screen::GuiSettings => (3, ui::settings::GUI_CUSTOMIZATION_SETTINGS),
                    Screen::AssetSettings => (4, ui::settings::CUSTOM_ASSET_SETTINGS),
                    _ => (0, &[] as &[&str]),
                };

                // --- Handle input and state changes ---
                ui::settings::update(
                    &mut current_screen,
                    &input_state,
                    &mut config,
                    &sound_pack_choices,
                    &loaded_themes,
                    &mut settings_menu_selection,
                    &mut sound_effects,
                    &mut confirm_selection,
                    &mut brightness,
                    &mut system_volume,
                    &available_sinks,
                    &mut current_bgm,
                    &bgm_choices,
                    &music_cache,
                    &mut sfx_pack_to_reload,
                    &logo_choices,
                    &background_choices,
                    &font_choices,
                    &mut animation_state,
                );

                // --- Draw the UI ---
                if page_number > 0 {
                    ui::settings::render_settings_page(
                        page_number,
                        options,
                        &logo_cache,
                        &background_cache,
                        &mut video_cache,
                        &font_cache,
                        &mut config,
                        settings_menu_selection,
                        &animation_state,
                        &mut background_state,
                        &battery_info,
                        &current_time_str,
                        &app_state.gcc_adapter_poll_rate,
                        scale_factor,
                        system_volume,
                        brightness,
                    );
                }
            }
            Screen::Extras => {
                ui::extras_menu::update(
                    &mut current_screen,
                    &mut extras_menu_selection,
                    &input_state,
                    &mut animation_state,
                    &sound_effects,
                    &config,
                );
                if current_screen == Screen::Profiles {
                    profiles_state.open_manager();
                }

                ui::extras_menu::draw(
                    extras_menu_selection,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::GameSelection => {
                // --- Load Icons from Queue ---
                if !game_icon_queue.is_empty() {
                    let (game_id, icon_path) = game_icon_queue.remove(0);

                    // Check for our Magic String
                    if icon_path.to_string_lossy() == "::KZP_PLACEHOLDER::" {
                        // LOAD FROM BAKED BYTES
                        // We use from_file_with_format which reads raw bytes.
                        // None = auto-detect format (png/jpg)
                        let texture = Texture2D::from_file_with_format(KZP_ICON_BYTES, None);
                        game_icon_cache.insert(game_id, texture);
                    } else {
                        // LOAD FROM DISK (Standard behavior)
                        // load_texture IS async and returns a Result, so we keep the check here
                        if let Ok(texture) = load_safe_image_texture(&icon_path) {
                            game_icon_cache.insert(game_id, texture);
                        }
                    }
                }
                let grid_width = 5; // The number of icons per row
                if input_state.left {
                    if game_selection > 0 {
                        game_selection -= 1;
                        sound_effects.play_cursor_move(&config);
                    }
                }
                if input_state.right {
                    if game_selection < available_games.len() - 1 {
                        game_selection += 1;
                        sound_effects.play_cursor_move(&config);
                    }
                }
                if input_state.up {
                    if game_selection >= grid_width {
                        game_selection -= grid_width;
                        sound_effects.play_cursor_move(&config);
                    }
                }
                if input_state.down {
                    if game_selection + grid_width < available_games.len() {
                        game_selection += grid_width;
                        sound_effects.play_cursor_move(&config);
                    }
                }
                if input_state.back {
                    current_screen = Screen::MainMenu;
                    sound_effects.play_back(&config);
                }
                if input_state.secondary {
                    if let Some((cart_info, kzi_path)) =
                        available_games.get(game_selection).cloned()
                    {
                        let media_kind = if cart_info.id.starts_with("media-movie-")
                            || cart_info.id.starts_with("media-show-")
                        {
                            Some(ui::media_library::MediaLibraryKind::Movies)
                        } else if cart_info.id.starts_with("media-music-") {
                            Some(ui::media_library::MediaLibraryKind::Music)
                        } else {
                            None
                        };
                        if let Some(kind) = media_kind {
                            let source = kzi_path
                                .parent()
                                .and_then(|folder| {
                                    fs::read_to_string(folder.join(".media-source")).ok()
                                })
                                .map(|value| PathBuf::from(value.trim()));
                            if let Some(source) = source {
                                if media_library_state.start_install_path(kind, &source) {
                                    current_screen =
                                        if kind == ui::media_library::MediaLibraryKind::Movies {
                                            Screen::Movies
                                        } else {
                                            Screen::MusicLibrary
                                        };
                                    sound_effects.play_select(&config);
                                } else {
                                    sound_effects.play_reject(&config);
                                }
                            } else {
                                sound_effects.play_reject(&config);
                            }
                        } else {
                            internal_games_state.begin_install_from_cart(cart_info, kzi_path);
                            current_screen = Screen::InternalGames;
                            sound_effects.play_select(&config);
                        }
                    }
                }
                if input_state.select {
                    if let Some((cart_info, kzi_path)) = available_games.get(game_selection) {
                        sound_effects.play_select(&config);

                        let media_player_screen = if cart_info.id.starts_with("media-movie-")
                            || cart_info.id.starts_with("media-show-")
                        {
                            Some(Screen::MoviePlayer)
                        } else if cart_info.id.starts_with("media-music-") {
                            Some(Screen::MusicPlayer)
                        } else {
                            None
                        };
                        if let Some(player_screen) = media_player_screen {
                            let source = kzi_path
                                .parent()
                                .and_then(|folder| {
                                    fs::read_to_string(folder.join(".media-source")).ok()
                                })
                                .map(|value| PathBuf::from(value.trim()));
                            if let Some(source) = source.filter(|path| path.exists()) {
                                // Media stays inside the PlayFusion menu session. Launching it
                                // as a normal cart would intentionally terminate kazeta-bios;
                                // a short player error would then be misread as a failed game
                                // and Kazeta's wrapper would power the console off.
                                media_player_state.prepare_file(source, Screen::GameSelection);
                                current_screen = player_screen;
                            } else {
                                sound_effects.play_reject(&config);
                            }
                        } else if DEV_MODE {
                            // --- DEBUG MODE ---
                            log_messages.lock().unwrap().clear();
                            {
                                // Scoped lock to add messages
                                let mut logs = log_messages.lock().unwrap();
                                logs.push("--- CARTRIDGE FOUND ---".to_string());
                                logs.push(format!(
                                    "Name: {}",
                                    cart_info.name.as_deref().unwrap_or("N/A")
                                ));
                                logs.push(format!("ID: {}", cart_info.id));
                                logs.push(format!("Exec: {}", cart_info.exec));
                                logs.push(format!(
                                    "Runtime: {}",
                                    cart_info.runtime.as_deref().unwrap_or("None")
                                ));
                                logs.push(format!("KZI Path: {}", kzi_path.display()));
                            }
                            println!("[Debug] Single Cartridge Found! Preparing to launch...");
                            println!(
                                "[Debug]   Name: {}",
                                cart_info.name.as_deref().unwrap_or("N/A")
                            );
                            println!("[Debug]   ID: {}", cart_info.id);
                            println!("[Debug]   Exec: {}", cart_info.exec);
                            println!(
                                "[Debug]   Runtime: {}",
                                cart_info.runtime.as_deref().unwrap_or("None")
                            );
                            println!("[Debug]   KZI Path: {}", kzi_path.display());

                            match save::launch_game(&cart_info, &kzi_path) {
                                Ok(mut child) => {
                                    log_messages
                                        .lock()
                                        .unwrap()
                                        .push("\n--- LAUNCHING GAME ---".to_string());
                                    start_log_reader(&mut child, log_messages.clone());
                                    game_process = Some(child);
                                }
                                Err(e) => {
                                    log_messages
                                        .lock()
                                        .unwrap()
                                        .push(format!("\n--- LAUNCH FAILED ---\nError: {}", e));
                                }
                            }
                            current_screen = Screen::Debug;
                        } else {
                            // Instead of just restarting, we now trigger a specific game launch.
                            (current_screen, fade_start_time) = trigger_game_launch(
                                cart_info,
                                kzi_path,
                                &mut current_bgm,
                                &music_cache,
                            );
                        }
                    }
                }

                // --- Render ---
                render_game_selection_menu(
                    &available_games,
                    &game_icon_cache,
                    &placeholder,
                    game_selection,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::InternalGames | Screen::GameManager => {
                let game_manager = current_screen == Screen::GameManager;
                internal_games_state.manager_mode = game_manager;
                internal_games_state.theme_name = config.theme.clone();
                if !internal_games_state.loaded {
                    internal_games_state.refresh();
                }
                internal_games_state.load_next_cover().await;

                match internal_games_state.handle_input(&input_state) {
                    ui::internal_games::InternalGamesEvent::Move => {
                        animation_state.trigger_transition(&config.cursor_transition_speed);
                        sound_effects.play_cursor_move(&config);
                    }
                    ui::internal_games::InternalGamesEvent::Select => {
                        sound_effects.play_select(&config);
                    }
                    ui::internal_games::InternalGamesEvent::Reject => {
                        sound_effects.play_reject(&config);
                    }
                    ui::internal_games::InternalGamesEvent::Back => {
                        internal_games_state.loaded = false;
                        current_screen = if game_manager {
                            Screen::Extras
                        } else {
                            Screen::MainMenu
                        };
                        sound_effects.play_back(&config);
                    }
                    ui::internal_games::InternalGamesEvent::Launch(cart_info, kzi_path) => {
                        sound_effects.play_select(&config);
                        (current_screen, fade_start_time) = trigger_game_launch(
                            &cart_info,
                            &kzi_path,
                            &mut current_bgm,
                            &music_cache,
                        );
                    }
                    ui::internal_games::InternalGamesEvent::None => {}
                }

                ui::internal_games::draw(
                    &internal_games_state,
                    &placeholder,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::Debug => {
                // Stop the BGM
                play_new_bgm("OFF", 0.0, &music_cache, &mut current_bgm);

                let messages = log_messages.lock().unwrap();

                // INPUT
                if input_state.up && debug_scroll_offset > 0 {
                    debug_scroll_offset -= 1;
                }
                // Allow scrolling down only if there are more messages than can be displayed
                if input_state.down && debug_scroll_offset < messages.len().saturating_sub(1) {
                    debug_scroll_offset += 1;
                }
                // save log file
                if input_state.select {
                    match save_log_to_file(&messages) {
                        Ok(filename) => {
                            // Add a confirmation message to the log
                            //messages.push(format!("\nLOG SAVED TO {}", filename));
                            flash_message = Some((
                                format!("LOG SAVED TO {}", filename),
                                FLASH_MESSAGE_DURATION,
                            ));
                        }
                        Err(e) => {
                            //messages.push(format!("\nERROR SAVING LOG: {}", e));
                            flash_message =
                                Some((format!("ERROR SAVING LOG: {}", e), FLASH_MESSAGE_DURATION));
                        }
                    }
                }
                if input_state.back {
                    // If the user presses back, kill the game process and return to the menu
                    if let Some(mut child) = game_process.take() {
                        child.kill().ok(); // Ignore error if process already exited
                    }
                    current_screen = Screen::MainMenu;
                    sound_effects.play_back(&config);
                    debug_scroll_offset = 0;
                }

                // --- Update flash message timer ---
                if let Some((_, timer)) = &mut flash_message {
                    *timer -= get_frame_time();
                    if *timer <= 0.0 {
                        flash_message = None;
                    }
                }

                // RENDER
                // Lock the mutex to get read-only access to the log messages for this frame
                render_debug_screen(
                    &messages,
                    debug_scroll_offset,
                    flash_message.as_ref().map(|(msg, _)| msg.as_str()), // Pass the message text
                    &font_cache,
                    &config,
                    scale_factor,
                    &background_cache,
                    &mut video_cache,
                    &mut background_state,
                );
            }
            Screen::ConfirmReset => {
                // --- Input Handling ---
                if input_state.left || input_state.right {
                    confirm_selection = 1 - confirm_selection; // Flips between 0 and 1
                    sound_effects.play_cursor_move(&config);
                }
                if input_state.back {
                    current_screen = Screen::GeneralSettings; // Or whatever page you came from
                    sound_effects.play_back(&config);
                }
                if input_state.select {
                    if confirm_selection == 0 {
                        // User selected YES
                        //if let Err(e) = delete_config_file() {
                        if let Err(e) = Config::delete() {
                            println!("[ERROR] Failed to delete config file: {}", e);
                        }
                        current_screen = Screen::ResetComplete;
                        sound_effects.play_select(&config);
                    } else {
                        // User selected NO
                        current_screen = Screen::GeneralSettings;
                        sound_effects.play_back(&config);
                    }
                }

                // --- Render ---
                // First, render the settings page in the background
                render_settings_page(
                    1,
                    &GENERAL_SETTINGS,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &mut config,
                    settings_menu_selection,
                    &animation_state,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                    system_volume,
                    brightness,
                );
                // Then, render the dialog box on top
                render_dialog_box(
                    "Reset all settings to default?\nThis cannot be undone.",
                    Some(("YES", "NO")), // Options to display
                    confirm_selection,   // Which option is selected
                    &font_cache,
                    &config,
                    scale_factor,
                    &animation_state,
                );
            }
            Screen::ResetComplete => {
                // --- Input Handling ---
                if input_state.select || input_state.back {
                    // Use the restart function you already have
                    (current_screen, fade_start_time) =
                        trigger_session_restart(&mut current_bgm, &music_cache);
                }

                // --- Render ---
                render_settings_page(
                    1,
                    &GENERAL_SETTINGS,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &mut config,
                    settings_menu_selection,
                    &animation_state,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                    system_volume,
                    brightness,
                );

                render_dialog_box(
                    "Settings have been reset.\nRestart required.",
                    None, // No YES/NO options needed
                    0,
                    &font_cache,
                    &config,
                    scale_factor,
                    &animation_state,
                );
            }
            Screen::SaveData => {
                // Process one item from the icon queue each frame to prevent stuttering.
                if !icon_queue.is_empty() {
                    let (save_id, icon_path_str) = icon_queue.remove(0);
                    if let Ok(texture) = load_texture(&icon_path_str).await {
                        icon_cache.insert(save_id, texture);
                    }
                }

                ui::data::update(
                    &mut input_state,
                    &mut current_screen,
                    &sound_effects,
                    &config,
                    &storage_state,
                    &mut memories,
                    &mut icon_cache,
                    &mut icon_queue,
                    &mut selected_memory,
                    &mut scroll_offset,
                    &mut dialogs,
                    &mut dialog_state,
                    &mut animation_state,
                    scale_factor,
                    &copy_op_state,
                )
                .await;

                render_background(
                    &background_cache,
                    &mut video_cache,
                    &config,
                    &mut background_state,
                );

                ui::data::draw(
                    selected_memory,
                    &memories,
                    &icon_cache,
                    &font_cache,
                    &config,
                    &storage_state,
                    &placeholder,
                    scroll_offset,
                    &input_state,
                    &animation_state,
                    &mut playtime_cache,
                    &mut size_cache,
                    scale_factor,
                    &dialog_state,
                );

                // Draw dialogs on top if they are open
                if let Some(dialog) = dialogs.last_mut() {
                    if dialog_state == DialogState::Open {
                        ui::render_dialog(
                            dialog,
                            &memories,
                            selected_memory,
                            &icon_cache,
                            &font_cache,
                            &config,
                            &copy_op_state,
                            &placeholder,
                            scroll_offset,
                            &animation_state,
                            &mut playtime_cache,
                            &mut size_cache,
                            scale_factor,
                        );
                    }
                }
            }
            Screen::Wifi => {
                ui::wifi::update(
                    &mut wifi_state,
                    &input_state,
                    &mut current_screen,
                    &sound_effects,
                    &config,
                );

                // Tell the about module to draw itself
                ui::wifi::draw(
                    &wifi_state,
                    &mut animation_state,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    scale_factor,
                );
            }
            Screen::Bluetooth => {
                ui::bluetooth::update(
                    &mut bluetooth_state,
                    &input_state,
                    &mut current_screen,
                    &sound_effects,
                    &config,
                );

                ui::bluetooth::draw(
                    &bluetooth_state,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::ThemeDownloader => {
                ui::theme_downloader::update(
                    &mut theme_downloader_state,
                    &input_state,
                    &mut current_screen,
                    &sound_effects,
                    &mut config,
                    &loaded_themes,
                );
                ui::theme_downloader::draw(
                    &theme_downloader_state,
                    &mut animation_state,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    scale_factor,
                );
            }
            Screen::ReloadingThemes => {
                next_frame().await;

                // 1. Re-run the theme loading function
                loaded_themes = theme::load_all_themes().await;

                // 2. Re-scan all asset directories to find the new files
                let (background_files, logo_files, font_files, music_files) =
                    find_all_asset_files();

                // --- Define a new message for reloading ---
                let reloading_text = "APPLYING NEW THEME ASSETS...";

                // 3. Re-load all assets and assign them to the original mutable caches
                (
                    background_cache,
                    video_cache,
                    logo_cache,
                    music_cache,
                    font_cache,
                    sound_effects,
                ) = load_all_assets(
                    &config,
                    reloading_text,
                    &startup_font,
                    &background_files,
                    &logo_files,
                    &font_files,
                    &music_files,
                    scale_factor,
                )
                .await;

                // A theme can select its own BGM, or clear BGM when returning
                // to the built-in default theme. Asset reloading alone does not
                // replace the currently playing Rodio sink, so explicitly
                // synchronize playback with the newly applied configuration.
                let selected_bgm = config.bgm_track.as_deref().unwrap_or("OFF");
                play_new_bgm(
                    selected_bgm,
                    config.bgm_volume,
                    &music_cache,
                    &mut current_bgm,
                );

                // 4. After reloading, go back to the downloader screen
                current_screen = Screen::ThemeDownloader;
            }
            Screen::RuntimeDownloader => {
                ui::runtime_downloader::update(
                    &mut runtime_downloader_state,
                    &input_state,
                    &mut current_screen,
                    &sound_effects,
                    &config,
                );
                ui::runtime_downloader::draw(
                    &runtime_downloader_state,
                    &mut animation_state,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    scale_factor,
                );
            }
            Screen::UpdateChecker => {
                ui::update_checker::update(
                    &mut update_checker_state,
                    &input_state,
                    &mut current_screen,
                    &sound_effects,
                    &config,
                );
                ui::update_checker::draw(
                    &mut update_checker_state,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    scale_factor,
                );
            }
            Screen::CdPlayer => {
                ui::cd_player::update(
                    &mut cd_player_ui_state,
                    &input_state,
                    &mut current_screen,
                    &sound_effects,
                    &config,
                    &mut current_bgm,
                );

                ui::cd_player::draw(
                    &mut cd_player_ui_state,
                    &animation_state,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    scale_factor,
                );
            }
            Screen::Power => {
                let power_event = ui::power::update(
                    &mut current_screen,
                    &mut power_menu_selection,
                    &input_state,
                    &mut animation_state,
                    &sound_effects,
                    &config,
                );

                if power_event == ui::power::PowerEvent::RestartPlayFusion {
                    (current_screen, fade_start_time) =
                        trigger_session_restart(&mut current_bgm, &music_cache);
                }

                ui::power::draw(
                    power_menu_selection,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    scale_factor,
                );
            }
            Screen::DvdPlayer => {
                ui::media_player::MediaPlayerState::update(
                    &mut media_player_state,
                    ui::media_player::MediaMode::Dvd,
                    &input_state,
                    &mut current_screen,
                    &mut current_bgm,
                    &config,
                );
                ui::media_player::MediaPlayerState::draw(
                    &media_player_state,
                    ui::media_player::MediaMode::Dvd,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    scale_factor,
                );
            }
            Screen::Movies | Screen::MusicLibrary => {
                let kind = if current_screen == Screen::Movies {
                    ui::media_library::MediaLibraryKind::Movies
                } else {
                    ui::media_library::MediaLibraryKind::Music
                };
                media_library_state.ensure_loaded(kind);
                media_library_state.load_next_cover().await;
                match media_library_state.handle_input(kind, &input_state) {
                    ui::media_library::MediaLibraryEvent::Move => {
                        animation_state.trigger_transition(&config.cursor_transition_speed);
                        sound_effects.play_cursor_move(&config);
                    }
                    ui::media_library::MediaLibraryEvent::Select => {
                        sound_effects.play_select(&config);
                    }
                    ui::media_library::MediaLibraryEvent::Reject => {
                        sound_effects.play_reject(&config);
                    }
                    ui::media_library::MediaLibraryEvent::Back => {
                        current_screen = Screen::Extras;
                        sound_effects.play_back(&config);
                    }
                    ui::media_library::MediaLibraryEvent::Launch(target) => {
                        media_player_state.prepare_file(target, current_screen.clone());
                        current_screen = match kind {
                            ui::media_library::MediaLibraryKind::Movies => Screen::MoviePlayer,
                            ui::media_library::MediaLibraryKind::Music => Screen::MusicPlayer,
                        };
                        sound_effects.play_select(&config);
                    }
                    ui::media_library::MediaLibraryEvent::None => {}
                }
                media_library_state.draw(
                    kind,
                    &animation_state,
                    &placeholder,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::MoviePlayer => {
                ui::media_player::MediaPlayerState::update(
                    &mut media_player_state,
                    ui::media_player::MediaMode::Movie,
                    &input_state,
                    &mut current_screen,
                    &mut current_bgm,
                    &config,
                );
                ui::media_player::MediaPlayerState::draw(
                    &media_player_state,
                    ui::media_player::MediaMode::Movie,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    scale_factor,
                );
            }
            Screen::MusicPlayer => {
                ui::media_player::MediaPlayerState::update(
                    &mut media_player_state,
                    ui::media_player::MediaMode::MusicFile,
                    &input_state,
                    &mut current_screen,
                    &mut current_bgm,
                    &config,
                );
                ui::media_player::MediaPlayerState::draw(
                    &media_player_state,
                    ui::media_player::MediaMode::MusicFile,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    scale_factor,
                );
            }
            Screen::Jukebox => {
                jukebox_browser_state.ensure_loaded();
                match jukebox_browser_state.handle_input(&input_state) {
                    ui::jukebox::JukeboxEvent::Move => {
                        sound_effects.play_cursor_move(&config);
                    }
                    ui::jukebox::JukeboxEvent::Select => {
                        sound_effects.play_select(&config);
                    }
                    ui::jukebox::JukeboxEvent::BackToExtras => {
                        current_screen = Screen::Extras;
                        sound_effects.play_back(&config);
                    }
                    ui::jukebox::JukeboxEvent::Launch {
                        target,
                        shuffle,
                        fullscreen,
                    } => {
                        media_player_state.prepare_jukebox(target, shuffle, fullscreen);
                        current_screen = Screen::JukeboxPlayer;
                        sound_effects.play_select(&config);
                    }
                    ui::jukebox::JukeboxEvent::None => {}
                }
                jukebox_browser_state.draw(
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &animation_state,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::StorageExpansion => {
                if !storage_expansion_state.loaded {
                    storage_expansion_state.refresh();
                }
                match storage_expansion_state.handle_input(&input_state) {
                    ui::storage_expansion::StorageEvent::Move => {
                        animation_state.trigger_transition(&config.cursor_transition_speed);
                        sound_effects.play_cursor_move(&config);
                    }
                    ui::storage_expansion::StorageEvent::Select => {
                        sound_effects.play_select(&config);
                    }
                    ui::storage_expansion::StorageEvent::Reject => {
                        sound_effects.play_reject(&config);
                    }
                    ui::storage_expansion::StorageEvent::Back => {
                        storage_expansion_state.loaded = false;
                        current_screen = Screen::Extras;
                        sound_effects.play_back(&config);
                    }
                    ui::storage_expansion::StorageEvent::None => {}
                }
                ui::storage_expansion::draw(
                    &storage_expansion_state,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::BiosFiles => {
                if !bios_files_state.loaded {
                    bios_files_state.refresh();
                }
                match bios_files_state.handle_input(&input_state) {
                    ui::bios_files::BiosEvent::Move => {
                        animation_state.trigger_transition(&config.cursor_transition_speed);
                        sound_effects.play_cursor_move(&config);
                    }
                    ui::bios_files::BiosEvent::Select => {
                        sound_effects.play_select(&config);
                    }
                    ui::bios_files::BiosEvent::Back => {
                        bios_files_state.loaded = false;
                        current_screen = Screen::Extras;
                        sound_effects.play_back(&config);
                    }
                    ui::bios_files::BiosEvent::None => {}
                }
                ui::bios_files::draw(
                    &bios_files_state,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::ControllerSetup => {
                match controller_setup_state.handle_input(&input_state) {
                    ui::controller_setup::ControllerSetupEvent::Move => {
                        animation_state.trigger_transition(&config.cursor_transition_speed);
                        sound_effects.play_cursor_move(&config);
                    }
                    ui::controller_setup::ControllerSetupEvent::Select => {
                        sound_effects.play_select(&config);
                    }
                    ui::controller_setup::ControllerSetupEvent::Reject => {
                        sound_effects.play_reject(&config);
                    }
                    ui::controller_setup::ControllerSetupEvent::Back => {
                        current_screen = Screen::Extras;
                        sound_effects.play_back(&config);
                    }
                    ui::controller_setup::ControllerSetupEvent::None => {}
                }
                ui::controller_setup::draw(
                    &controller_setup_state,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::AndroidControllerSetup => {
                match android_controller_state.handle_input(&input_state) {
                    ui::android_controller::AndroidControllerEvent::Move => {
                        animation_state.trigger_transition(&config.cursor_transition_speed);
                        sound_effects.play_cursor_move(&config);
                    }
                    ui::android_controller::AndroidControllerEvent::Select => {
                        sound_effects.play_select(&config);
                    }
                    ui::android_controller::AndroidControllerEvent::Reject => {
                        sound_effects.play_reject(&config);
                    }
                    ui::android_controller::AndroidControllerEvent::Back => {
                        current_screen = Screen::Extras;
                        sound_effects.play_back(&config);
                    }
                    ui::android_controller::AndroidControllerEvent::None => {}
                }
                ui::android_controller::draw(
                    &android_controller_state,
                    &animation_state,
                    &logo_cache,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    &battery_info,
                    &current_time_str,
                    &app_state.gcc_adapter_poll_rate,
                    scale_factor,
                );
            }
            Screen::JukeboxPlayer => {
                ui::media_player::MediaPlayerState::update(
                    &mut media_player_state,
                    ui::media_player::MediaMode::Jukebox,
                    &input_state,
                    &mut current_screen,
                    &mut current_bgm,
                    &config,
                );
                ui::media_player::MediaPlayerState::draw(
                    &media_player_state,
                    ui::media_player::MediaMode::Jukebox,
                    &background_cache,
                    &mut video_cache,
                    &font_cache,
                    &config,
                    &mut background_state,
                    scale_factor,
                );
            }
        }

        // This block checks if the settings screen requested an SFX reload
        if let Some(pack_name) = sfx_pack_to_reload.take() {
            println!("[Info] Reloading SFX pack: {}", pack_name);
            //sound_effects = SoundEffects::load(&pack_name).await;
            sound_effects = SoundEffects::load(&pack_name);
            // Play a sound from the new pack to confirm it changed
            sound_effects.play_cursor_move(&config);
        }
        next_frame().await
    }
}
