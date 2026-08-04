use macroquad::prelude::*;
use serde_json::Value;
use std::{
    ffi::{c_void, CString},
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::{Child, Command},
    time::Duration,
};

const CONFIG_PATH: &str = "/var/kazeta/state/projectm-home/.projectM/config.inp";
const AUDIO_HELPER: &str = "/usr/local/bin/playfusion-projectm-theme-audio";
const IPC_SOCKET: &str = "/run/user/1000/playfusion-mp3-player.sock";

extern "C" {
    fn playfusion_projectm_create(
        config_path: *const i8,
        monitor: *const i8,
        width: i32,
        height: i32,
    ) -> *mut c_void;
    fn playfusion_projectm_render(handle: *mut c_void);
    fn playfusion_projectm_pixels(handle: *mut c_void) -> *const u8;
    fn playfusion_projectm_width(handle: *mut c_void) -> i32;
    fn playfusion_projectm_height(handle: *mut c_void) -> i32;
    fn playfusion_projectm_destroy(handle: *mut c_void);
}

pub struct ProjectMBackgroundState {
    handle: *mut c_void,
    texture: Option<Texture2D>,
    audio_process: Option<Child>,
    last_start_attempt: f64,
    render_width: i32,
    render_height: i32,
    source_width: i32,
    source_height: i32,
    audio_volume: u32,
    warmup_started: f64,
}

impl ProjectMBackgroundState {
    pub fn new() -> Self {
        Self {
            handle: std::ptr::null_mut(),
            texture: None,
            audio_process: None,
            last_start_attempt: -10.0,
            render_width: 0,
            render_height: 0,
            source_width: 0,
            source_height: 0,
            audio_volume: 70,
            warmup_started: -10.0,
        }
    }

    fn child_running(child: &mut Option<Child>) -> bool {
        match child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    pub fn set_audio_enabled(&mut self, enabled: bool, volume: f32) {
        if !enabled {
            self.stop_audio();
            return;
        }
        let wanted_volume = (volume.clamp(0.0, 1.0) * 100.0).round() as u32;
        if !Self::child_running(&mut self.audio_process) || self.audio_volume != wanted_volume {
            self.stop_audio();
            self.audio_volume = wanted_volume;
            self.audio_process = Command::new(AUDIO_HELPER)
                .args(["/var/kazeta/music", &wanted_volume.to_string()])
                .spawn()
                .ok();
        }
    }

    fn active_monitor() -> String {
        let sink = Command::new("/usr/bin/pactl")
            .args(["get-default-sink"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_default();
        if sink.is_empty() { String::new() } else { format!("{sink}.monitor") }
    }

    fn ensure_visual_started(&mut self) {
        let wanted_width = screen_width().round().max(640.0) as i32;
        let wanted_height = screen_height().round().max(360.0) as i32;
        if !self.handle.is_null()
            && self.render_width == wanted_width
            && self.render_height == wanted_height
        {
            return;
        }
        if !self.handle.is_null() {
            self.stop_visuals();
        }
        if !Path::new(CONFIG_PATH).is_file() || get_time() - self.last_start_attempt < 2.0 {
            return;
        }
        self.last_start_attempt = get_time();
        let Ok(config_path) = CString::new(CONFIG_PATH) else { return; };
        let Ok(monitor) = CString::new(Self::active_monitor()) else { return; };

        unsafe {
            let mut gl = macroquad::window::get_internal_gl();
            gl.flush();
            gl.quad_gl.reset();
            self.handle = playfusion_projectm_create(
                config_path.as_ptr(),
                monitor.as_ptr(),
                wanted_width,
                wanted_height,
            );
            gl.quad_gl.reset();
            if !self.handle.is_null() {
                let actual_width = playfusion_projectm_width(self.handle);
                let actual_height = playfusion_projectm_height(self.handle);
                if actual_width > 0 && actual_height > 0 {
                    let blank = vec![
                        0_u8;
                        actual_width as usize * actual_height as usize * 4
                    ];
                    let texture = Texture2D::from_rgba8(
                        actual_width as u16,
                        actual_height as u16,
                        &blank,
                    );
                    texture.set_filter(FilterMode::Linear);
                    self.texture = Some(texture);
                    self.render_width = wanted_width;
                    self.render_height = wanted_height;
                    self.source_width = actual_width;
                    self.source_height = actual_height;
                    self.warmup_started = get_time();
                }
            }
        }
    }

    pub fn draw(&mut self, allowed: bool) {
        if !allowed {
            self.stop_visuals();
            Self::draw_fallback();
            return;
        }
        self.ensure_visual_started();
        if !self.handle.is_null() {
            unsafe {
                let mut gl = macroquad::window::get_internal_gl();
                gl.flush();
                gl.quad_gl.reset();
                playfusion_projectm_render(self.handle);
                gl.quad_gl.reset();
                if let Some(texture) = self.texture.as_ref() {
                    let pixels = playfusion_projectm_pixels(self.handle);
                    if !pixels.is_null() && self.source_width > 0 && self.source_height > 0 {
                        let byte_count = self.source_width as usize
                            * self.source_height as usize
                            * 4;
                        let bytes = std::slice::from_raw_parts(pixels, byte_count);
                        texture.update_from_bytes(
                            self.source_width as u32,
                            self.source_height as u32,
                            bytes,
                        );
                    }
                }
            }
        }
        if let Some(texture) = self.texture.as_ref() {
            draw_texture_ex(
                texture,
                -2.0,
                -2.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(screen_width() + 4.0, screen_height() + 4.0)),
                    flip_y: true,
                    ..Default::default()
                },
            );
            draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.01, 0.0, 0.04, 0.10));
        } else {
            Self::draw_fallback();
        }
    }

    fn draw_fallback() {
        clear_background(Color::new(0.004, 0.0, 0.03, 1.0));
        let pulse = (get_time() as f32 * 0.8).sin() * 0.5 + 0.5;
        draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.03, 0.0, 0.16 + pulse * 0.05, 1.0));
    }

    fn ipc_command(command: &str) -> Option<Value> {
        let mut stream = UnixStream::connect(IPC_SOCKET).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(700)));
        writeln!(stream, "{command}").ok()?;
        let mut response = String::new();
        BufReader::new(stream).read_line(&mut response).ok()?;
        serde_json::from_str(&response).ok()
    }

    pub fn cycle_track(&mut self, next: bool) -> Option<String> {
        if !Self::child_running(&mut self.audio_process) { return None; }
        let direction = if next { "playlist-next" } else { "playlist-prev" };
        let _ = Self::ipc_command(&format!(r#"{{"command":["{direction}","force"]}}"#));
        std::thread::sleep(Duration::from_millis(120));
        let response = Self::ipc_command(r#"{"command":["get_property","filename/no-ext"]}"#)?;
        response.get("data")?.as_str().map(str::to_string)
    }

    pub fn stop_visuals(&mut self) {
        self.texture = None;
        if !self.handle.is_null() {
            unsafe {
                let mut gl = macroquad::window::get_internal_gl();
                gl.flush();
                gl.quad_gl.reset();
                playfusion_projectm_destroy(self.handle);
                gl.quad_gl.reset();
            }
            self.handle = std::ptr::null_mut();
        }
        self.render_width = 0;
        self.render_height = 0;
        self.source_width = 0;
        self.source_height = 0;
        self.warmup_started = -10.0;
    }

    pub fn stop_audio(&mut self) {
        if let Some(mut process) = self.audio_process.take() {
            let pid = process.id().to_string();
            let _ = Command::new("/usr/bin/pkill").args(["-TERM", "-P", &pid]).status();
            let _ = process.kill();
            let _ = process.wait();
        }
        let _ = std::fs::remove_file(IPC_SOCKET);
    }

    pub fn stop(&mut self) {
        self.stop_visuals();
        self.stop_audio();
    }
}

impl Drop for ProjectMBackgroundState {
    fn drop(&mut self) { self.stop(); }
}
