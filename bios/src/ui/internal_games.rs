use crate::{
    config::{get_user_data_dir, Config},
    get_current_font, render_background, render_ui_overlay_without_version, save,
    text_with_config_color,
    types::{AnimationState, BackgroundState, BatteryInfo},
    video::VideoPlayer,
};
use macroquad::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs,
    io::{BufRead, BufReader},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{channel, Receiver, Sender},
    thread,
};

pub const INTERNAL_GAMES_DIR: &str = "/var/kazeta/internal-games";
pub const FTP_PORT: u16 = 2121;
const OPERATION_PROGRESS_FILE: &str = "/run/kazeta/operation-progress";
const GRID_COLUMNS: usize = 4;
const GRID_ROWS: usize = 2;
const GAMES_PER_PAGE: usize = GRID_COLUMNS * GRID_ROWS;

#[derive(Clone)]
pub struct InstallTarget {
    kind: String,
    token: String,
    label: String,
}

#[derive(Clone)]
pub enum InternalGamesMode {
    Gallery,
    ManagerActions {
        selection: usize,
    },
    MediaSelection {
        games: Vec<(save::CartInfo, PathBuf)>,
        selection: usize,
    },
    InstallDestination {
        cart: save::CartInfo,
        kzi_path: PathBuf,
        targets: Vec<InstallTarget>,
        selection: usize,
    },
    DiscDestination {
        cart: save::CartInfo,
        kzi_path: PathBuf,
        targets: Vec<InstallTarget>,
        selection: usize,
    },
    ConfirmFormat {
        cart: save::CartInfo,
        kzi_path: PathBuf,
        target: InstallTarget,
        yes: bool,
    },
    ExportDestination {
        cart: save::CartInfo,
        kzi_path: PathBuf,
        targets: Vec<InstallTarget>,
        selection: usize,
    },
    ConfirmExportFormat {
        cart: save::CartInfo,
        kzi_path: PathBuf,
        target: InstallTarget,
        yes: bool,
    },
    ConfirmDelete {
        yes: bool,
    },
    Busy {
        message: String,
        progress: Option<f32>,
    },
    Message(String),
}

enum OperationUpdate {
    Progress { percent: f32, phase: Option<String> },
    Finished(Result<String, String>),
}

pub enum InternalGamesEvent {
    None,
    Back,
    Launch(save::CartInfo, PathBuf),
    Move,
    Select,
    Reject,
}

pub struct InternalGamesState {
    pub games: Vec<(save::CartInfo, PathBuf)>,
    pub all_games: Vec<(save::CartInfo, PathBuf)>,
    pub game_sizes: HashMap<String, u64>,
    pub game_systems: HashMap<String, String>,
    pub systems: Vec<String>,
    pub system_selection: usize,
    pub active_system: Option<String>,
    pub selection: usize,
    pub cover_cache: HashMap<String, Texture2D>,
    cover_queue: Vec<(String, PathBuf)>,
    pub loaded: bool,
    pub status: String,
    pub free_space: String,
    pub manager_mode: bool,
    pub theme_name: String,
    pub mode: InternalGamesMode,
    operation_rx: Receiver<OperationUpdate>,
}

impl Default for InternalGamesState {
    fn default() -> Self {
        let (_operation_tx, operation_rx) = channel();
        Self {
            games: Vec::new(),
            all_games: Vec::new(),
            game_sizes: HashMap::new(),
            game_systems: HashMap::new(),
            systems: Vec::new(),
            system_selection: 0,
            active_system: None,
            selection: 0,
            cover_cache: HashMap::new(),
            cover_queue: Vec::new(),
            loaded: false,
            status: String::new(),
            free_space: String::new(),
            manager_mode: false,
            theme_name: "Default".to_string(),
            mode: InternalGamesMode::Gallery,
            operation_rx,
        }
    }
}

impl InternalGamesState {
    pub fn refresh(&mut self) {
        self.games.clear();
        self.all_games.clear();
        self.game_sizes.clear();
        self.game_systems.clear();
        self.systems.clear();
        self.system_selection = 0;
        self.active_system = None;
        self.cover_cache.clear();
        self.cover_queue.clear();
        self.selection = 0;
        self.free_space = internal_free_space();

        let system_library = Path::new(INTERNAL_GAMES_DIR);
        if let Err(error) = fs::create_dir_all(system_library) {
            self.status = format!("LIBRARY ERROR: {error}");
            self.loaded = true;
            return;
        }

        let mut rejected = 0usize;
        let mut ids = HashSet::new();
        let libraries = internal_library_roots();
        for library in &libraries {
            for folder_path in library_game_folders(library) {
                let kzi_path = match find_kzi(&folder_path) {
                    Some(path) => path,
                    None => {
                        rejected += 1;
                        continue;
                    }
                };
                let cart = match save::parse_kzi_file(&kzi_path) {
                    Ok(cart) => cart,
                    Err(_) => {
                        rejected += 1;
                        continue;
                    }
                };

                if !ids.insert(cart.id.clone()) || !valid_cart_paths(&folder_path, &cart, library) {
                    rejected += 1;
                    continue;
                }

                let cover_relative = cart
                    .cover
                    .as_deref()
                    .filter(|relative| folder_path.join(relative).is_file())
                    .unwrap_or(&cart.icon);
                self.cover_queue
                    .push((cart.id.clone(), folder_path.join(cover_relative)));
                self.game_sizes
                    .insert(cart.id.clone(), directory_size(&folder_path));
                self.game_systems
                    .insert(cart.id.clone(), system_for_cart(&cart));
                self.games.push((cart, kzi_path));
            }
        }

        self.games.sort_by(|(a, _), (b, _)| {
            a.name
                .as_deref()
                .unwrap_or(&a.id)
                .to_lowercase()
                .cmp(&b.name.as_deref().unwrap_or(&b.id).to_lowercase())
        });
        self.all_games = self.games.clone();
        self.systems = self
            .game_systems
            .values()
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        self.systems.sort_by(|left, right| {
            system_sort_order(left)
                .cmp(&system_sort_order(right))
                .then_with(|| left.cmp(right))
        });
        for system in &self.systems {
            self.cover_queue.push((
                format!("system:{system}"),
                system_cover_path(&self.theme_name, system),
            ));
        }
        self.status = if rejected == 0 {
            format!(
                "{} GAME(S)  |  {} STORAGE LOCATION(S)",
                self.games.len(),
                libraries.len()
            )
        } else {
            format!(
                "{} GAME(S)  |  {} INCOMPLETE/INVALID",
                self.games.len(),
                rejected
            )
        };
        self.loaded = true;
    }

    pub async fn load_next_cover(&mut self) {
        if let Some((id, path)) = self.cover_queue.pop() {
            if let Ok(texture) = load_common_image_texture(&path) {
                texture.set_filter(FilterMode::Linear);
                self.cover_cache.insert(id, texture);
            }
        }
    }

    fn poll_operation(&mut self) {
        if let InternalGamesMode::Busy { message, progress } = &mut self.mode {
            if let Ok(raw) = fs::read_to_string(OPERATION_PROGRESS_FILE) {
                let mut fields = raw.trim().splitn(2, '\t');
                if let Some(value) = fields.next() {
                    if let Ok(value) = value.trim().parse::<f32>() {
                        *progress = Some(value.clamp(0.0, 100.0));
                    }
                }
                if let Some(phase) = fields
                    .next()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    *message = format!("{phase}...");
                }
            }
        }
        while let Ok(update) = self.operation_rx.try_recv() {
            match update {
                OperationUpdate::Progress { percent, phase } => {
                    if let InternalGamesMode::Busy { message, progress } = &mut self.mode {
                        *progress = Some(percent.clamp(0.0, 100.0));
                        if let Some(phase) = phase {
                            *message = format!("{phase}...");
                        }
                    }
                }
                OperationUpdate::Finished(result) => match result {
                    Ok(message) => {
                        self.refresh();
                        self.mode = InternalGamesMode::Message(message);
                    }
                    Err(error) => {
                        self.mode = InternalGamesMode::Message(format!("ERROR: {error}"));
                    }
                },
            }
        }
    }

    fn launch_helper_operation(
        arguments: Vec<OsString>,
        success_message: String,
        default_error: &'static str,
        operation_tx: Sender<OperationUpdate>,
    ) {
        thread::spawn(move || {
            let mut child = match Command::new("sudo")
                .arg("/usr/bin/kazeta-internal-game-helper")
                .args(arguments)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(error) => {
                    operation_tx
                        .send(OperationUpdate::Finished(Err(format!(
                            "Failed to start operation: {error}"
                        ))))
                        .ok();
                    return;
                }
            };

            let (line_tx, line_rx) = channel::<String>();
            if let Some(stdout) = child.stdout.take() {
                let tx = line_tx.clone();
                thread::spawn(move || {
                    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                        tx.send(line).ok();
                    }
                });
            }
            if let Some(stderr) = child.stderr.take() {
                let tx = line_tx.clone();
                thread::spawn(move || {
                    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                        tx.send(line).ok();
                    }
                });
            }
            drop(line_tx);

            let mut last_detail = String::new();
            for line in line_rx {
                if let Some(rest) = line.strip_prefix("PF_PROGRESS\t") {
                    let mut fields = rest.splitn(2, '\t');
                    if let Some(percent) = fields.next() {
                        if let Ok(percent) = percent.parse::<f32>() {
                            operation_tx
                                .send(OperationUpdate::Progress {
                                    percent,
                                    phase: fields
                                        .next()
                                        .map(str::trim)
                                        .filter(|value| !value.is_empty())
                                        .map(str::to_string),
                                })
                                .ok();
                            continue;
                        }
                    }
                }
                let detail = line.trim();
                if !detail.is_empty() {
                    last_detail = detail.to_string();
                }
            }

            let result = match child.wait() {
                Ok(status) if status.success() => Ok(success_message),
                Ok(_) => {
                    let detail = if last_detail.is_empty() {
                        default_error
                    } else {
                        last_detail.strip_prefix("Error: ").unwrap_or(&last_detail)
                    };
                    Err(detail.to_string())
                }
                Err(error) => Err(format!("Operation wait failed: {error}")),
            };
            operation_tx.send(OperationUpdate::Finished(result)).ok();
        });
    }

    fn scan_install_media(&mut self) {
        let mut games = Vec::new();
        let mut seen = HashSet::new();
        let installed_ids = self
            .all_games
            .iter()
            .map(|(cart, _)| cart.id.clone())
            .collect::<HashSet<_>>();
        let mut already_installed = 0usize;
        for root in [Path::new("/run/media"), Path::new("/media")] {
            if !root.exists() {
                continue;
            }
            for entry in walkdir::WalkDir::new(root)
                .max_depth(6)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| {
                    if entry.depth() != 1 || !entry.file_type().is_dir() {
                        return true;
                    }

                    // Dedicated PlayFusion library disks are also mounted
                    // below /run/media for convenient administration. They
                    // are internal storage, not inserted install media, and
                    // scanning them here makes already-installed games appear
                    // as removable carts after every reboot.
                    !entry.path().join(".playfusion-storage").exists()
                        && entry.file_name() != "frzr_efi"
                })
                .filter_map(Result::ok)
            {
                let path = entry.path();
                if !entry.file_type().is_file()
                    || path.extension().and_then(|ext| ext.to_str()) != Some("kzi")
                {
                    continue;
                }
                let cart = match save::parse_kzi_file(path) {
                    Ok(cart) => cart,
                    Err(_) => continue,
                };
                let folder = match path.parent() {
                    Some(folder) => folder,
                    None => continue,
                };
                if !seen.insert(cart.id.clone()) || !valid_source_cart_paths(folder, &cart, root) {
                    continue;
                }
                if installed_ids.contains(&cart.id) {
                    already_installed += 1;
                    continue;
                }
                games.push((cart, path.to_path_buf()));
            }
        }
        games.sort_by(|(a, _), (b, _)| {
            a.name
                .as_deref()
                .unwrap_or(&a.id)
                .to_lowercase()
                .cmp(&b.name.as_deref().unwrap_or(&b.id).to_lowercase())
        });
        self.mode = if games.is_empty() && already_installed > 0 {
            InternalGamesMode::Message(
                "ALL GAMES ON INSERTED MEDIA ARE ALREADY INSTALLED".to_string(),
            )
        } else if games.is_empty() {
            InternalGamesMode::Message("NO VALID KAZETA GAMES FOUND ON INSERTED MEDIA".to_string())
        } else {
            InternalGamesMode::MediaSelection {
                games,
                selection: 0,
            }
        };
    }

    fn open_selected_system(&mut self) -> bool {
        let Some(system) = self.systems.get(self.system_selection).cloned() else {
            return false;
        };
        self.games = self
            .all_games
            .iter()
            .filter(|(cart, _)| self.game_systems.get(&cart.id) == Some(&system))
            .cloned()
            .collect();
        self.selection = 0;
        self.active_system = Some(system);
        true
    }

    fn show_system_gallery(&mut self) {
        self.games = self.all_games.clone();
        self.selection = 0;
        self.active_system = None;
        self.mode = InternalGamesMode::Gallery;
    }

    fn internal_targets() -> Vec<InstallTarget> {
        let mut targets = Vec::new();
        if let Ok(output) = Command::new("sudo")
            .arg("/usr/bin/kazeta-internal-game-helper")
            .arg("list-library-targets")
            .output()
        {
            if output.status.success() {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let fields = line.split('\t').collect::<Vec<_>>();
                    if fields.len() != 5 || fields[0].is_empty() {
                        continue;
                    }
                    targets.push(InstallTarget {
                        kind: if fields[0] == "system" {
                            "internal".to_string()
                        } else {
                            "internal-storage".to_string()
                        },
                        token: fields[0].to_string(),
                        label: format!("{}  |  {} FREE OF {}", fields[1], fields[3], fields[2]),
                    });
                }
            }
        }
        targets
    }

    fn choose_install_destination(&mut self, cart: save::CartInfo, kzi_path: PathBuf) {
        let targets = Self::internal_targets();
        self.mode = if targets.is_empty() {
            InternalGamesMode::Message("NO WRITABLE INTERNAL STORAGE FOUND".to_string())
        } else {
            InternalGamesMode::InstallDestination {
                cart,
                kzi_path,
                targets,
                selection: 0,
            }
        };
    }

    pub fn begin_install_from_cart(&mut self, cart: save::CartInfo, kzi_path: PathBuf) {
        if !self.loaded {
            self.refresh();
        }
        self.choose_install_destination(cart, kzi_path);
    }

    fn start_install(&mut self, cart: save::CartInfo, kzi_path: PathBuf, target: InstallTarget) {
        let source = match kzi_path.parent() {
            Some(path) => path.to_path_buf(),
            None => {
                self.mode = InternalGamesMode::Message("INVALID SOURCE GAME FOLDER".to_string());
                return;
            }
        };
        let id = cart.id.clone();
        let display_name = cart.name.clone().unwrap_or_else(|| id.clone());
        let target_label = target.label.clone();
        let storage_token = target.token;
        let (operation_tx, operation_rx) = channel();
        self.operation_rx = operation_rx;
        self.mode = InternalGamesMode::Busy {
            message: format!("INSTALLING {display_name} TO {target_label}..."),
            progress: Some(0.0),
        };
        Self::launch_helper_operation(
            vec![
                OsString::from("install-to"),
                source.into_os_string(),
                OsString::from(id),
                OsString::from(storage_token),
            ],
            format!("INSTALLED {display_name} TO {target_label}"),
            "Game installation failed",
            operation_tx,
        );
    }

    fn is_supported_optical_cart(cart: &save::CartInfo, kzi_path: &Path) -> bool {
        kzi_path.starts_with("/run/media/kazeta-optical") && cart.id.starts_with("optical-")
    }

    fn choose_disc_destination(&mut self, cart: save::CartInfo, kzi_path: PathBuf) {
        let mut targets = Self::internal_targets();
        if let Ok(output) = Command::new("sudo")
            .arg("/usr/bin/kazeta-internal-game-helper")
            .arg("list-targets")
            .output()
        {
            if output.status.success() {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let fields = line.split('\t').collect::<Vec<_>>();
                    if fields.len() < 6 || fields[0].is_empty() || fields[1].is_empty() {
                        continue;
                    }
                    if fields[0] == "burn" {
                        continue;
                    }
                    let label = if fields[0] == "format" {
                        format!("ERASE + FORMAT EXT4  {}  {}", fields[5], fields[3])
                    } else if fields[2].is_empty() {
                        format!("REMOVABLE {}  {}  {}", fields[5], fields[3], fields[4])
                    } else {
                        format!("{}  {}  {}", fields[2], fields[3], fields[4])
                    };
                    targets.push(InstallTarget {
                        kind: fields[0].to_string(),
                        token: fields[1].to_string(),
                        label,
                    });
                }
            }
        }
        self.mode = InternalGamesMode::DiscDestination {
            cart,
            kzi_path,
            targets,
            selection: 0,
        };
    }

    fn removable_targets() -> Vec<InstallTarget> {
        let mut targets = Vec::new();
        if let Ok(output) = Command::new("sudo")
            .arg("/usr/bin/kazeta-internal-game-helper")
            .arg("list-targets")
            .output()
        {
            if output.status.success() {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let fields = line.split('\t').collect::<Vec<_>>();
                    if fields.len() < 6 || fields[0].is_empty() || fields[1].is_empty() {
                        continue;
                    }
                    let label = if fields[0] == "burn" {
                        format!("BURN DISC  {}  {}", fields[2], fields[3])
                    } else if fields[0] == "format" {
                        format!("ERASE + FORMAT EXT4  {}  {}", fields[5], fields[3])
                    } else if fields[2].is_empty() {
                        format!("REMOVABLE {}  {}  {}", fields[5], fields[3], fields[4])
                    } else {
                        format!("{}  {}  {}", fields[2], fields[3], fields[4])
                    };
                    targets.push(InstallTarget {
                        kind: fields[0].to_string(),
                        token: fields[1].to_string(),
                        label,
                    });
                }
            }
        }
        targets
    }

    fn choose_export_destination(&mut self, cart: save::CartInfo, kzi_path: PathBuf) {
        let targets = Self::removable_targets();
        self.mode = if targets.is_empty() {
            InternalGamesMode::Message(
                "INSERT A USB DRIVE OR SD CARD, THEN PRESS X TO REFRESH".to_string(),
            )
        } else {
            InternalGamesMode::ExportDestination {
                cart,
                kzi_path,
                targets,
                selection: 0,
            }
        };
    }

    fn start_export(&mut self, cart: save::CartInfo, kzi_path: PathBuf, target: InstallTarget) {
        if !kzi_path.is_file() {
            self.mode =
                InternalGamesMode::Message("THE INTERNAL GAME IS NO LONGER AVAILABLE".to_string());
            return;
        }
        let id = cart.id.clone();
        let display_name = cart.name.clone().unwrap_or_else(|| id.clone());
        let target_label = target.label.clone();
        let destination = target.kind;
        let is_burn = destination == "burn";
        let (operation_tx, operation_rx) = channel();
        self.operation_rx = operation_rx;
        self.mode = InternalGamesMode::Busy {
            message: if is_burn {
                format!("PREPARING {display_name} FOR {target_label}...")
            } else {
                format!("COPYING {display_name} TO {target_label}...")
            },
            progress: Some(0.0),
        };
        Self::launch_helper_operation(
            vec![
                OsString::from("export"),
                OsString::from(destination),
                OsString::from(target.token),
                OsString::from(id),
            ],
            format!("COPIED {display_name} TO {target_label}"),
            "Game export failed",
            operation_tx,
        );
    }

    fn start_disc_install(
        &mut self,
        cart: save::CartInfo,
        kzi_path: PathBuf,
        target: InstallTarget,
    ) {
        let source_still_present = kzi_path.is_file();
        if !source_still_present {
            self.mode =
                InternalGamesMode::Message("THE PLAYSTATION DISC IS NO LONGER READY".to_string());
            return;
        }
        let id = cart.id.clone();
        let display_name = cart.name.clone().unwrap_or_else(|| id.clone());
        let target_label = target.label.clone();
        let destination = target.kind;
        let destination_argument = if destination == "internal" {
            "-".to_string()
        } else {
            target.token
        };
        let (operation_tx, operation_rx) = channel();
        self.operation_rx = operation_rx;
        self.mode = InternalGamesMode::Busy {
            message: format!("READING {display_name} TO {target_label}..."),
            progress: Some(0.0),
        };
        Self::launch_helper_operation(
            vec![
                OsString::from("install-disc"),
                OsString::from(destination),
                OsString::from(destination_argument),
                OsString::from(id),
            ],
            format!("INSTALLED {display_name} TO {target_label}"),
            "Disc installation failed",
            operation_tx,
        );
    }

    fn start_delete(&mut self) {
        let (cart, kzi_path) = match self.games.get(self.selection).cloned() {
            Some(game) => game,
            None => {
                self.mode = InternalGamesMode::Message("NO GAME SELECTED".to_string());
                return;
            }
        };
        let id = cart.id.clone();
        let game_folder = match kzi_path.parent() {
            Some(folder) => folder.to_path_buf(),
            None => {
                self.mode = InternalGamesMode::Message("INVALID INTERNAL GAME PATH".to_string());
                return;
            }
        };
        let display_name = cart.name.clone().unwrap_or_else(|| id.clone());
        let (operation_tx, operation_rx) = channel();
        self.operation_rx = operation_rx;
        self.mode = InternalGamesMode::Busy {
            message: format!("DELETING {display_name}..."),
            progress: None,
        };
        thread::spawn(move || {
            let result = Command::new("sudo")
                .arg("/usr/bin/kazeta-internal-game-helper")
                .arg("delete-path")
                .arg(&game_folder)
                .arg(&id)
                .status()
                .map_err(|error| format!("Failed to start deletion: {error}"))
                .and_then(|status| {
                    if status.success() {
                        Ok(format!("DELETED {display_name}"))
                    } else {
                        Err(format!("Deletion exited with {status}"))
                    }
                });
            operation_tx.send(OperationUpdate::Finished(result)).ok();
        });
    }

    pub fn handle_input(&mut self, input: &crate::InputState) -> InternalGamesEvent {
        self.poll_operation();
        match self.mode.clone() {
            InternalGamesMode::Gallery => {
                if self.active_system.is_none() {
                    if move_system_selection(self, input.left, input.right, input.up, input.down) {
                        return InternalGamesEvent::Move;
                    }
                    if input.secondary {
                        self.refresh();
                        return InternalGamesEvent::Select;
                    }
                    if self.manager_mode && input.next {
                        self.scan_install_media();
                        return InternalGamesEvent::Select;
                    }
                    if input.back {
                        return InternalGamesEvent::Back;
                    }
                    if input.select {
                        return if self.open_selected_system() {
                            InternalGamesEvent::Select
                        } else {
                            InternalGamesEvent::Reject
                        };
                    }
                    return InternalGamesEvent::None;
                }
                if move_selection(self, input.left, input.right, input.up, input.down) {
                    return InternalGamesEvent::Move;
                }
                if input.secondary {
                    self.refresh();
                    return InternalGamesEvent::Select;
                }
                if self.manager_mode && input.cycle {
                    if let Some((cart, kzi)) = self.games.get(self.selection).cloned() {
                        self.choose_export_destination(cart, kzi);
                        return InternalGamesEvent::Select;
                    }
                    return InternalGamesEvent::Reject;
                }
                if self.manager_mode && input.next {
                    self.scan_install_media();
                    return InternalGamesEvent::Select;
                }
                if self.manager_mode && input.prev {
                    if self.games.is_empty() {
                        return InternalGamesEvent::Reject;
                    }
                    self.mode = InternalGamesMode::ConfirmDelete { yes: false };
                    return InternalGamesEvent::Select;
                }
                if input.back {
                    self.show_system_gallery();
                    return InternalGamesEvent::Select;
                }
                if input.select {
                    if self.manager_mode {
                        self.mode = InternalGamesMode::ManagerActions { selection: 0 };
                        return InternalGamesEvent::Select;
                    }
                    if let Some((cart, kzi)) = self.games.get(self.selection).cloned() {
                        return InternalGamesEvent::Launch(cart, kzi);
                    }
                    return InternalGamesEvent::Reject;
                }
            }
            InternalGamesMode::ManagerActions { mut selection } => {
                const ACTION_COUNT: usize = 3;
                if input.up {
                    selection = if selection == 0 {
                        ACTION_COUNT - 1
                    } else {
                        selection - 1
                    };
                    self.mode = InternalGamesMode::ManagerActions { selection };
                    return InternalGamesEvent::Move;
                }
                if input.down {
                    selection = (selection + 1) % ACTION_COUNT;
                    self.mode = InternalGamesMode::ManagerActions { selection };
                    return InternalGamesEvent::Move;
                }
                if input.back {
                    self.mode = InternalGamesMode::Gallery;
                    return InternalGamesEvent::Select;
                }
                if input.select {
                    match selection {
                        0 => {
                            if let Some((cart, kzi)) = self.games.get(self.selection).cloned() {
                                self.choose_export_destination(cart, kzi);
                            } else {
                                return InternalGamesEvent::Reject;
                            }
                        }
                        1 => {
                            if self.games.is_empty() {
                                return InternalGamesEvent::Reject;
                            }
                            self.mode = InternalGamesMode::ConfirmDelete { yes: false };
                        }
                        _ => self.mode = InternalGamesMode::Gallery,
                    }
                    return InternalGamesEvent::Select;
                }
            }
            InternalGamesMode::MediaSelection {
                games,
                mut selection,
            } => {
                if input.up && selection > 0 {
                    selection -= 1;
                    self.mode = InternalGamesMode::MediaSelection { games, selection };
                    return InternalGamesEvent::Move;
                }
                if input.down && selection + 1 < games.len() {
                    selection += 1;
                    self.mode = InternalGamesMode::MediaSelection { games, selection };
                    return InternalGamesEvent::Move;
                }
                if input.select {
                    if let Some((cart, kzi)) = games.get(selection).cloned() {
                        if Self::is_supported_optical_cart(&cart, &kzi) {
                            self.choose_disc_destination(cart, kzi);
                        } else {
                            self.choose_install_destination(cart, kzi);
                        }
                        return InternalGamesEvent::Select;
                    }
                }
                if input.back {
                    self.mode = InternalGamesMode::Gallery;
                    return InternalGamesEvent::Select;
                }
            }
            InternalGamesMode::InstallDestination {
                cart,
                kzi_path,
                targets,
                mut selection,
            } => {
                if input.up && selection > 0 {
                    selection -= 1;
                    self.mode = InternalGamesMode::InstallDestination {
                        cart,
                        kzi_path,
                        targets,
                        selection,
                    };
                    return InternalGamesEvent::Move;
                }
                if input.down && selection + 1 < targets.len() {
                    selection += 1;
                    self.mode = InternalGamesMode::InstallDestination {
                        cart,
                        kzi_path,
                        targets,
                        selection,
                    };
                    return InternalGamesEvent::Move;
                }
                if input.select {
                    if let Some(target) = targets.get(selection).cloned() {
                        self.start_install(cart, kzi_path, target);
                        return InternalGamesEvent::Select;
                    }
                }
                if input.back {
                    self.mode = InternalGamesMode::Gallery;
                    return InternalGamesEvent::Select;
                }
            }
            InternalGamesMode::DiscDestination {
                cart,
                kzi_path,
                targets,
                mut selection,
            } => {
                if input.up && selection > 0 {
                    selection -= 1;
                    self.mode = InternalGamesMode::DiscDestination {
                        cart,
                        kzi_path,
                        targets,
                        selection,
                    };
                    return InternalGamesEvent::Move;
                }
                if input.down && selection + 1 < targets.len() {
                    selection += 1;
                    self.mode = InternalGamesMode::DiscDestination {
                        cart,
                        kzi_path,
                        targets,
                        selection,
                    };
                    return InternalGamesEvent::Move;
                }
                if input.select {
                    if let Some(target) = targets.get(selection).cloned() {
                        if target.kind == "format" {
                            self.mode = InternalGamesMode::ConfirmFormat {
                                cart,
                                kzi_path,
                                target,
                                yes: false,
                            };
                        } else {
                            self.start_disc_install(cart, kzi_path, target);
                        }
                        return InternalGamesEvent::Select;
                    }
                }
                if input.back {
                    self.mode = InternalGamesMode::Gallery;
                    return InternalGamesEvent::Select;
                }
            }
            InternalGamesMode::ConfirmFormat {
                cart,
                kzi_path,
                target,
                mut yes,
            } => {
                if input.left || input.right {
                    yes = !yes;
                    self.mode = InternalGamesMode::ConfirmFormat {
                        cart,
                        kzi_path,
                        target,
                        yes,
                    };
                    return InternalGamesEvent::Move;
                }
                if input.select {
                    if yes {
                        self.start_disc_install(cart, kzi_path, target);
                    } else {
                        self.mode = InternalGamesMode::Gallery;
                    }
                    return InternalGamesEvent::Select;
                }
                if input.back {
                    self.mode = InternalGamesMode::Gallery;
                    return InternalGamesEvent::Select;
                }
            }
            InternalGamesMode::ExportDestination {
                cart,
                kzi_path,
                targets,
                mut selection,
            } => {
                if input.up && selection > 0 {
                    selection -= 1;
                    self.mode = InternalGamesMode::ExportDestination {
                        cart,
                        kzi_path,
                        targets,
                        selection,
                    };
                    return InternalGamesEvent::Move;
                }
                if input.down && selection + 1 < targets.len() {
                    selection += 1;
                    self.mode = InternalGamesMode::ExportDestination {
                        cart,
                        kzi_path,
                        targets,
                        selection,
                    };
                    return InternalGamesEvent::Move;
                }
                if input.select {
                    if let Some(target) = targets.get(selection).cloned() {
                        if target.kind == "format" {
                            self.mode = InternalGamesMode::ConfirmExportFormat {
                                cart,
                                kzi_path,
                                target,
                                yes: false,
                            };
                        } else {
                            self.start_export(cart, kzi_path, target);
                        }
                        return InternalGamesEvent::Select;
                    }
                }
                if input.back {
                    self.mode = InternalGamesMode::Gallery;
                    return InternalGamesEvent::Select;
                }
            }
            InternalGamesMode::ConfirmExportFormat {
                cart,
                kzi_path,
                target,
                mut yes,
            } => {
                if input.left || input.right {
                    yes = !yes;
                    self.mode = InternalGamesMode::ConfirmExportFormat {
                        cart,
                        kzi_path,
                        target,
                        yes,
                    };
                    return InternalGamesEvent::Move;
                }
                if input.select {
                    if yes {
                        self.start_export(cart, kzi_path, target);
                    } else {
                        self.mode = InternalGamesMode::Gallery;
                    }
                    return InternalGamesEvent::Select;
                }
                if input.back {
                    self.mode = InternalGamesMode::Gallery;
                    return InternalGamesEvent::Select;
                }
            }
            InternalGamesMode::ConfirmDelete { mut yes } => {
                if input.left || input.right {
                    yes = !yes;
                    self.mode = InternalGamesMode::ConfirmDelete { yes };
                    return InternalGamesEvent::Move;
                }
                if input.select {
                    if yes {
                        self.start_delete();
                    } else {
                        self.mode = InternalGamesMode::Gallery;
                    }
                    return InternalGamesEvent::Select;
                }
                if input.back {
                    self.mode = InternalGamesMode::Gallery;
                    return InternalGamesEvent::Select;
                }
            }
            InternalGamesMode::Busy { .. } => {}
            InternalGamesMode::Message(_) => {
                if input.select || input.back {
                    self.mode = InternalGamesMode::Gallery;
                    return InternalGamesEvent::Select;
                }
            }
        }
        InternalGamesEvent::None
    }
}

fn directory_size(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                if let Ok(metadata) = entry.metadata() {
                    total = total.saturating_add(metadata.len());
                }
            }
        }
    }
    total
}

fn format_game_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.2} GB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KB", bytes / KIB)
    } else {
        format!("{} B", bytes as u64)
    }
}

fn load_common_image_texture(path: &Path) -> Result<Texture2D, String> {
    // Macroquad's built-in loader is compiled with a limited set of formats
    // and panics on JPEG. Decode through the image crate so a bad or
    // unsupported cover becomes a normal error and the UI can use its
    // placeholder instead of terminating the entire Kazeta session.
    let decoded = ::image::open(path)
        .map_err(|error| format!("Unable to decode {}: {error}", path.display()))?
        .to_rgba8();
    let (width, height) = decoded.dimensions();
    let width =
        u16::try_from(width).map_err(|_| format!("Cover is too wide: {}", path.display()))?;
    let height =
        u16::try_from(height).map_err(|_| format!("Cover is too tall: {}", path.display()))?;
    Ok(Texture2D::from_rgba8(width, height, decoded.as_raw()))
}

fn internal_free_space() -> String {
    let roots = internal_library_roots();
    let mut total = 0u64;
    let mut counted = 0usize;
    for root in &roots {
        let Ok(output) = Command::new("df")
            .args(["-B1", "--output=avail"])
            .arg(root)
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        if let Some(bytes) = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.trim().parse::<u64>().ok())
            .last()
        {
            total = total.saturating_add(bytes);
            counted += 1;
        }
    }
    if counted == 0 {
        "FREE SPACE UNKNOWN".to_string()
    } else {
        let gibibytes = total as f64 / 1024.0 / 1024.0 / 1024.0;
        if counted == 1 {
            format!("{gibibytes:.1} GB FREE")
        } else {
            format!("{gibibytes:.1} GB FREE ACROSS {counted} DRIVES")
        }
    }
}

pub(crate) fn internal_library_roots() -> Vec<PathBuf> {
    let _ = Command::new("sudo")
        .arg("/usr/bin/kazeta-internal-game-helper")
        .arg("mount-storage")
        .status();
    let mut roots = vec![PathBuf::from(INTERNAL_GAMES_DIR)];
    let storage_root = Path::new("/var/kazeta/storage");
    let Ok(entries) = fs::read_dir(storage_root) else {
        return roots;
    };
    let mut expansion_roots = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let mount = entry.path();
            let uuid = entry.file_name().to_string_lossy().to_string();
            if uuid.is_empty()
                || uuid
                    .chars()
                    .any(|character| !(character.is_ascii_alphanumeric() || character == '-'))
                || !mount.join(".playfusion-storage").is_file()
            {
                return None;
            }
            let output = Command::new("findmnt")
                .args(["-nro", "UUID", "--target"])
                .arg(&mount)
                .output()
                .ok()?;
            if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != uuid {
                return None;
            }
            let games = mount.join("games");
            games.is_dir().then_some(games)
        })
        .collect::<Vec<_>>();
    expansion_roots.sort();
    roots.extend(expansion_roots);
    roots
}

pub(crate) fn library_game_folders(library: &Path) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    let Ok(entries) = fs::read_dir(library) else {
        return folders;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if find_kzi(&path).is_some() {
            folders.push(path);
            continue;
        }
        let Ok(children) = fs::read_dir(&path) else {
            continue;
        };
        for child in children.filter_map(Result::ok) {
            let child_path = child.path();
            if child_path.is_dir()
                && !child.file_name().to_string_lossy().starts_with('.')
                && find_kzi(&child_path).is_some()
            {
                folders.push(child_path);
            }
        }
    }
    folders.sort();
    folders
}

fn find_kzi(folder: &Path) -> Option<PathBuf> {
    let mut files = fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("kzi"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    files.sort();
    files.into_iter().next()
}

fn system_for_cart(cart: &save::CartInfo) -> String {
    let runtime = cart.runtime.as_deref().unwrap_or("").to_ascii_lowercase();
    let exec = cart.exec.to_ascii_lowercase();
    let id = cart.id.to_ascii_lowercase();
    let value = format!("{runtime} {exec} {id}");

    if value.contains("playfusion-arcade") || value.contains("finalburn") || value.contains("mame")
    {
        "Arcade"
    } else if value.contains("playstation2") || value.contains("pcsx2") {
        "PlayStation 2"
    } else if value.contains("playstation") || value.contains("duckstation") {
        "PlayStation"
    } else if value.contains("vita3k") {
        "PlayStation Vita"
    } else if value.contains("ppsspp") || value.contains("psp-") {
        "PSP"
    } else if value.contains("cemu") || value.contains("wii-u") || value.contains("wiiu") {
        "Wii U"
    } else if value.contains("dolphin") {
        if exec.ends_with(".wbfs") || exec.ends_with(".wad") || id.contains("wii") {
            "Wii"
        } else {
            "GameCube"
        }
    } else if value.contains("nintendo64") || value.contains("mupen64") {
        "Nintendo 64"
    } else if value.contains("azahar") || value.contains("citra") {
        "Nintendo 3DS"
    } else if value.contains("melonds") {
        "Nintendo DS"
    } else if value.contains("mgba") {
        if exec.ends_with(".gba") {
            "Game Boy Advance"
        } else {
            "Game Boy"
        }
    } else if value.contains("snes") {
        "SNES"
    } else if value.contains("nes") {
        "NES"
    } else if value.contains("dreamcast") {
        "Dreamcast"
    } else if value.contains("segacd") || value.contains("sega-cd") {
        "Sega CD"
    } else if value.contains("saturn") {
        "Sega Saturn"
    } else if exec.ends_with(".32x") || value.contains("32x") {
        "Sega 32X"
    } else if exec.ends_with(".gg") || value.contains("gamegear") {
        "Game Gear"
    } else if value.contains("megadrive")
        || value.contains("genesis")
        || value.contains("picodrive")
    {
        "Sega Genesis"
    } else if value.contains("dosbox") {
        "DOS"
    } else if value.contains("puae") || value.contains("amiga") {
        "Amiga"
    } else if value.contains("vice") || value.contains("commodore64") {
        "Commodore 64"
    } else if value.contains("stella") || value.contains("atari2600") {
        "Atari 2600"
    } else if value.contains("prosystem") || value.contains("atari7800") {
        "Atari 7800"
    } else if value.contains("handy") || value.contains("lynx") {
        "Atari Lynx"
    } else if value.contains("xemu") {
        "Original Xbox"
    } else if value.contains("waydroid") || exec.ends_with(".apk") {
        "Android"
    } else if value.contains("windows") || value.contains("scummvm") {
        "PC Games"
    } else {
        "Other"
    }
    .to_string()
}

fn system_asset_slug(system: &str) -> String {
    system
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn system_cover_path(theme_name: &str, system: &str) -> PathBuf {
    let slug = system_asset_slug(system);
    let built_in = PathBuf::from("/usr/share/playfusion/system-covers").join(format!("{slug}.png"));
    if theme_name.is_empty()
        || theme_name == "."
        || theme_name == ".."
        || theme_name.contains('/')
        || theme_name.contains('\\')
    {
        return built_in;
    }
    let Some(data_root) = get_user_data_dir() else {
        return built_in;
    };
    let theme_root = data_root.join("themes").join(theme_name);
    for folder in ["system-folders", "system-covers", "folders"] {
        for extension in ["png", "jpg", "jpeg", "webp", "avif"] {
            let candidate = theme_root.join(folder).join(format!("{slug}.{extension}"));
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    built_in
}

fn system_sort_order(system: &str) -> usize {
    [
        "Arcade",
        "NES",
        "SNES",
        "Nintendo 64",
        "Game Boy",
        "Game Boy Advance",
        "Nintendo DS",
        "Nintendo 3DS",
        "GameCube",
        "Wii",
        "Wii U",
        "Sega Genesis",
        "Sega 32X",
        "Game Gear",
        "Sega CD",
        "Sega Saturn",
        "Dreamcast",
        "PlayStation",
        "PlayStation 2",
        "PSP",
        "PlayStation Vita",
        "Original Xbox",
        "Atari 2600",
        "Atari 7800",
        "Atari Lynx",
        "DOS",
        "Amiga",
        "Commodore 64",
        "Android",
        "PC Games",
        "Other",
    ]
    .iter()
    .position(|candidate| *candidate == system)
    .unwrap_or(usize::MAX)
}

fn valid_cart_paths(folder: &Path, cart: &save::CartInfo, library: &Path) -> bool {
    if cart.id.is_empty()
        || cart
            .id
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    {
        return false;
    }

    let root = match folder.canonicalize() {
        Ok(root) => root,
        Err(_) => return false,
    };
    let library = match library.canonicalize() {
        Ok(library) => library,
        Err(_) => return false,
    };
    let root_text = root.to_string_lossy();
    if !root.starts_with(&library)
        || root_text.contains('\'')
        || root_text.contains('\n')
        || root_text.contains('\r')
    {
        return false;
    }
    for relative in [cart.exec.as_str(), cart.icon.as_str()] {
        let path = Path::new(relative);
        if path.is_absolute() {
            return false;
        }
        let resolved = match folder.join(path).canonicalize() {
            Ok(resolved) => resolved,
            Err(_) => return false,
        };
        if !resolved.starts_with(&root) {
            return false;
        }
    }
    if let Some(relative) = cart.cover.as_deref() {
        let path = Path::new(relative);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return false;
        }
        let candidate = folder.join(path);
        if candidate.exists() {
            let resolved = match candidate.canonicalize() {
                Ok(resolved) => resolved,
                Err(_) => return false,
            };
            if !resolved.starts_with(&root) {
                return false;
            }
        }
    }
    true
}

fn valid_source_cart_paths(folder: &Path, cart: &save::CartInfo, media_root: &Path) -> bool {
    if cart.id.is_empty()
        || cart
            .id
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    {
        return false;
    }
    let root = match folder.canonicalize() {
        Ok(root) => root,
        Err(_) => return false,
    };
    let allowed = match media_root.canonicalize() {
        Ok(allowed) => allowed,
        Err(_) => return false,
    };
    if !root.starts_with(&allowed) {
        return false;
    }
    for relative in [cart.exec.as_str(), cart.icon.as_str()] {
        if relative == cart.exec
            && cart.id.starts_with("optical-")
            && relative.starts_with("device:/dev/sr")
            && root.starts_with("/run/media/kazeta-optical")
        {
            continue;
        }
        let path = Path::new(relative);
        if path.is_absolute() {
            return false;
        }
        let resolved = match folder.join(path).canonicalize() {
            Ok(resolved) => resolved,
            Err(_) => return false,
        };
        if !resolved.starts_with(&root)
            && !(relative == cart.exec && valid_loose_rom_payload_target(cart, &root, &resolved))
        {
            return false;
        }
    }
    if let Some(relative) = cart.cover.as_deref() {
        let path = Path::new(relative);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return false;
        }
        let candidate = folder.join(path);
        if candidate.exists() {
            let resolved = match candidate.canonicalize() {
                Ok(resolved) => resolved,
                Err(_) => return false,
            };
            if !resolved.starts_with(&root) {
                return false;
            }
        }
    }
    true
}

fn valid_loose_rom_payload_target(cart: &save::CartInfo, cart_root: &Path, target: &Path) -> bool {
    if !cart.id.starts_with("loose-") || !cart_root.starts_with("/run/media/playfusion-loose-rom") {
        return false;
    }

    for media_root in [Path::new("/run/media"), Path::new("/media")] {
        let relative = match target.strip_prefix(media_root) {
            Ok(relative) => relative,
            Err(_) => continue,
        };
        let Some(Component::Normal(mount_name)) = relative.components().next() else {
            continue;
        };
        let mount = media_root.join(mount_name);
        if mount == Path::new("/run/media/playfusion-loose-rom")
            || mount == Path::new("/run/media/kazeta-optical")
            || mount.join(".playfusion-storage").exists()
        {
            return false;
        }
        return true;
    }
    false
}

pub fn ftp_endpoint() -> String {
    let address = route_address().or_else(hostname_address);
    let endpoint = match address {
        Some(address) => format!("FTP: {address}:{FTP_PORT}"),
        None => format!("FTP: OFFLINE  PORT {FTP_PORT}"),
    };
    format!("{endpoint}  |  {}", internal_free_space())
}

fn route_address() -> Option<String> {
    let output = Command::new("ip")
        .args(["-4", "route", "get", "1.1.1.1"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let fields = text.split_whitespace().collect::<Vec<_>>();
    fields
        .windows(2)
        .find(|pair| pair[0] == "src")
        .map(|pair| pair[1].to_string())
        .filter(|address| address != "127.0.0.1")
}

fn hostname_address() -> Option<String> {
    let output = Command::new("hostname").arg("-I").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .find(|address| address.contains('.') && *address != "127.0.0.1")
        .map(str::to_string)
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    state: &InternalGamesState,
    placeholder: &Texture2D,
    animation_state: &AnimationState,
    logo_cache: &HashMap<String, Texture2D>,
    background_cache: &HashMap<String, Texture2D>,
    video_cache: &mut HashMap<String, VideoPlayer>,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    background_state: &mut BackgroundState,
    battery_info: &Option<BatteryInfo>,
    current_time_str: &str,
    gcc_adapter_poll_rate: &Option<u32>,
    scale_factor: f32,
) {
    render_background(background_cache, video_cache, config, background_state);
    render_ui_overlay_without_version(
        logo_cache,
        font_cache,
        config,
        battery_info,
        current_time_str,
        gcc_adapter_poll_rate,
        scale_factor,
    );

    let font = get_current_font(font_cache, config);
    if state.active_system.is_none() && state.systems.is_empty() {
        let message = "NO COMPLETE INTERNAL GAMES FOUND";
        let message_size = (16.0 * scale_factor) as u16;
        let width = measure_text(message, Some(font), message_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            message,
            (screen_width() - width) / 2.0,
            screen_height() / 2.0,
            message_size,
        );
    } else if state.active_system.is_none() {
        draw_system_page(
            state,
            placeholder,
            animation_state,
            font_cache,
            config,
            scale_factor,
        );
    } else if state.games.is_empty() {
        let message = "NO GAMES FOUND IN THIS SYSTEM";
        let message_size = (16.0 * scale_factor) as u16;
        let width = measure_text(message, Some(font), message_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            message,
            (screen_width() - width) / 2.0,
            screen_height() / 2.0,
            message_size,
        );
    } else {
        draw_page(
            state,
            placeholder,
            animation_state,
            font_cache,
            config,
            scale_factor,
        );
    }

    let info_size = (12.0 * scale_factor) as u16;
    let info = if state.active_system.is_none() {
        if let Some(system) = state.systems.get(state.system_selection) {
            let ids = state
                .all_games
                .iter()
                .filter(|(cart, _)| state.game_systems.get(&cart.id) == Some(system))
                .map(|(cart, _)| cart.id.as_str())
                .collect::<Vec<_>>();
            let bytes = ids
                .iter()
                .filter_map(|id| state.game_sizes.get(*id))
                .copied()
                .sum::<u64>();
            format!(
                "{system}  |  {} GAME(S)  |  {}  |  {}",
                ids.len(),
                format_game_size(bytes),
                state.free_space
            )
        } else {
            format!("{}  |  {}", state.status, state.free_space)
        }
    } else if let Some((cart, _)) = state.games.get(state.selection) {
        let name = cart.name.as_deref().unwrap_or(&cart.id);
        let runtime = cart.runtime.as_deref().unwrap_or("linux");
        let game_size = state
            .game_sizes
            .get(&cart.id)
            .copied()
            .map(format_game_size)
            .unwrap_or_else(|| "SIZE UNKNOWN".to_string());
        format!(
            "{name}  |  {runtime}  |  {game_size}  |  {}",
            state.free_space
        )
    } else {
        format!("{}  |  {}", state.status, state.free_space)
    };
    let info_width = measure_text(&info, Some(font), info_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        &info,
        (screen_width() - info_width) / 2.0,
        screen_height() - (26.0 * scale_factor),
        info_size,
    );

    let controls = if state.active_system.is_none() && state.manager_mode {
        "A OPEN   X REFRESH   RB INSTALL   B BACK"
    } else if state.active_system.is_none() {
        "A OPEN   X REFRESH   B BACK"
    } else if state.manager_mode {
        "A MANAGE   X REFRESH   RB INSTALL   B SYSTEMS"
    } else {
        "A PLAY   X REFRESH   B SYSTEMS"
    };
    let controls_size = (10.0 * scale_factor) as u16;
    let controls_width = measure_text(controls, Some(font), controls_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        controls,
        (screen_width() - controls_width) / 2.0,
        screen_height() - (8.0 * scale_factor),
        controls_size,
    );

    draw_mode_overlay(state, font_cache, config, scale_factor);
}

fn draw_mode_overlay(
    state: &InternalGamesState,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    scale_factor: f32,
) {
    let font = get_current_font(font_cache, config);
    let font_size = (14.0 * scale_factor) as u16;
    let line_height = 24.0 * scale_factor;
    let width = screen_width() * 0.72;
    let text_max_width = width - 40.0 * scale_factor;
    let (title, lines): (&str, Vec<String>) = match &state.mode {
        InternalGamesMode::Gallery => return,
        InternalGamesMode::ManagerActions { selection } => {
            let options = ["COPY TO MEDIA", "DELETE GAME", "CANCEL"];
            let mut lines = Vec::new();
            for (index, option) in options.iter().enumerate() {
                lines.push(format!(
                    "{} {}",
                    if index == *selection { ">" } else { " " },
                    option
                ));
            }
            lines.push(String::new());
            lines.push("A SELECT   B CANCEL".to_string());
            ("MANAGE GAME", lines)
        }
        InternalGamesMode::MediaSelection { games, selection } => {
            let maximum_rows = ((screen_height() * 0.72 / line_height).floor() as usize)
                .saturating_sub(5)
                .clamp(3, 10);
            let maximum_start = games.len().saturating_sub(maximum_rows);
            let start = selection
                .saturating_sub(maximum_rows / 2)
                .min(maximum_start);
            let end = (start + maximum_rows).min(games.len());
            let mut lines = vec![format!("SHOWING {}-{} OF {}", start + 1, end, games.len())];
            lines.extend(
                games[start..end]
                    .iter()
                    .enumerate()
                    .map(|(offset, (cart, _))| {
                        let index = start + offset;
                        let name = cart.name.as_deref().unwrap_or(&cart.id);
                        let line = if index == *selection {
                            format!("> {name}")
                        } else {
                            format!("  {name}")
                        };
                        fit_overlay_text(&line, font, font_size, text_max_width)
                    }),
            );
            lines.push("A INSTALL   B CANCEL".to_string());
            ("INSTALL FROM MEDIA", lines)
        }
        InternalGamesMode::InstallDestination {
            cart,
            targets,
            selection,
            ..
        } => {
            let name = cart.name.as_deref().unwrap_or(&cart.id);
            let mut lines = vec![name.to_string()];
            lines.extend(targets.iter().enumerate().map(|(index, target)| {
                if index == *selection {
                    format!("> {}", target.label)
                } else {
                    format!("  {}", target.label)
                }
            }));
            lines.push("A INSTALL   B CANCEL".to_string());
            ("INSTALL INTERNAL GAME TO", lines)
        }
        InternalGamesMode::DiscDestination {
            cart,
            targets,
            selection,
            ..
        } => {
            let name = cart.name.as_deref().unwrap_or(&cart.id);
            let mut lines = vec![name.to_string()];
            lines.extend(targets.iter().enumerate().map(|(index, target)| {
                if index == *selection {
                    format!("> {}", target.label)
                } else {
                    format!("  {}", target.label)
                }
            }));
            lines.push("A INSTALL DISC   B CANCEL".to_string());
            ("INSTALL CONSOLE DISC TO", lines)
        }
        InternalGamesMode::ConfirmFormat { target, yes, .. } => (
            "ERASE REMOVABLE DRIVE?",
            vec![
                target.label.clone(),
                "ALL FILES ON THIS DRIVE WILL BE LOST".to_string(),
                if *yes {
                    "  NO        > YES".to_string()
                } else {
                    "> NO          YES".to_string()
                },
                "A CONFIRM   B CANCEL".to_string(),
            ],
        ),
        InternalGamesMode::ExportDestination {
            cart,
            targets,
            selection,
            ..
        } => {
            let name = cart.name.as_deref().unwrap_or(&cart.id);
            let mut lines = vec![name.to_string()];
            lines.extend(targets.iter().enumerate().map(|(index, target)| {
                if index == *selection {
                    format!("> {}", target.label)
                } else {
                    format!("  {}", target.label)
                }
            }));
            lines.push("A COPY CART   B CANCEL".to_string());
            ("COPY INTERNAL GAME TO", lines)
        }
        InternalGamesMode::ConfirmExportFormat { target, yes, .. } => (
            "ERASE DRIVE AND COPY?",
            vec![
                target.label.clone(),
                "ALL FILES ON THIS DRIVE WILL BE LOST".to_string(),
                if *yes {
                    "  NO        > YES".to_string()
                } else {
                    "> NO          YES".to_string()
                },
                "A CONFIRM   B CANCEL".to_string(),
            ],
        ),
        InternalGamesMode::ConfirmDelete { yes } => {
            let name = state
                .games
                .get(state.selection)
                .map(|(cart, _)| cart.name.as_deref().unwrap_or(&cart.id))
                .unwrap_or("SELECTED GAME");
            (
                "DELETE INTERNAL GAME?",
                vec![
                    name.to_string(),
                    if *yes {
                        "  NO        > YES".to_string()
                    } else {
                        "> NO          YES".to_string()
                    },
                    "A CONFIRM   B CANCEL".to_string(),
                ],
            )
        }
        InternalGamesMode::Busy { message, progress } => (
            "PLEASE WAIT",
            vec![
                message.clone(),
                progress
                    .map(|value| format!("{value:.0}%"))
                    .unwrap_or_else(|| "WORKING...".to_string()),
            ],
        ),
        InternalGamesMode::Message(message) => (
            "INTERNAL GAMES",
            vec![message.clone(), "A OR B TO CONTINUE".to_string()],
        ),
    };

    let busy_progress = match &state.mode {
        InternalGamesMode::Busy {
            progress: Some(value),
            ..
        } => Some(value.clamp(0.0, 100.0)),
        _ => None,
    };
    let progress_height = if busy_progress.is_some() {
        line_height * 1.1
    } else {
        0.0
    };
    let height =
        ((lines.len() as f32 + 2.5) * line_height + progress_height).min(screen_height() * 0.72);
    let x = (screen_width() - width) / 2.0;
    let y = (screen_height() - height) / 2.0;
    draw_rectangle(x, y, width, height, Color::new(0.0, 0.0, 0.0, 0.94));
    draw_rectangle_lines(x, y, width, height, 3.0 * scale_factor, WHITE);

    let title_width = measure_text(title, Some(font), font_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        title,
        (screen_width() - title_width) / 2.0,
        y + line_height,
        font_size,
    );
    for (index, line) in lines.iter().enumerate() {
        let text_width = measure_text(line, Some(font), font_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            line,
            (screen_width() - text_width) / 2.0,
            y + line_height * (2.2 + index as f32),
            font_size,
        );
    }
    if let Some(percent) = busy_progress {
        let bar_width = width * 0.76;
        let bar_height = (line_height * 0.42).max(10.0 * scale_factor);
        let bar_x = x + (width - bar_width) / 2.0;
        let bar_y = y + line_height * (2.55 + lines.len() as f32);
        let inner_width = (bar_width - 6.0 * scale_factor).max(0.0);
        let filled_width = inner_width * (percent / 100.0);
        draw_rectangle(
            bar_x,
            bar_y,
            bar_width,
            bar_height,
            Color::new(0.03, 0.04, 0.10, 1.0),
        );
        draw_rectangle_lines(
            bar_x,
            bar_y,
            bar_width,
            bar_height,
            2.0 * scale_factor,
            Color::new(0.20, 0.90, 1.0, 1.0),
        );
        let segments = 28;
        let segment_width = inner_width / segments as f32;
        for segment in 0..segments {
            let start = segment as f32 * segment_width;
            if start >= filled_width {
                break;
            }
            let fraction = segment as f32 / (segments - 1) as f32;
            let color = Color::new(0.10 + 0.85 * fraction, 0.90 - 0.55 * fraction, 1.0, 1.0);
            draw_rectangle(
                bar_x + 3.0 * scale_factor + start,
                bar_y + 3.0 * scale_factor,
                segment_width.min(filled_width - start) + 0.5,
                (bar_height - 6.0 * scale_factor).max(1.0),
                color,
            );
        }
    }
}

fn fit_overlay_text(text: &str, font: &Font, font_size: u16, maximum_width: f32) -> String {
    if measure_text(text, Some(font), font_size, 1.0).width <= maximum_width {
        return text.to_string();
    }
    let mut shortened = text.chars().collect::<Vec<_>>();
    while !shortened.is_empty() {
        shortened.pop();
        let candidate = format!("{}...", shortened.iter().collect::<String>());
        if measure_text(&candidate, Some(font), font_size, 1.0).width <= maximum_width {
            return candidate;
        }
    }
    "...".to_string()
}

fn draw_page(
    state: &InternalGamesState,
    placeholder: &Texture2D,
    animation_state: &AnimationState,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    scale_factor: f32,
) {
    let page = state.selection / GAMES_PER_PAGE;
    let page_start = page * GAMES_PER_PAGE;
    let page_end = (page_start + GAMES_PER_PAGE).min(state.games.len());
    let cover_width = 72.0 * scale_factor;
    let cover_height = 96.0 * scale_factor;
    let horizontal_gap = 24.0 * scale_factor;
    let vertical_gap = 12.0 * scale_factor;
    let total_width =
        GRID_COLUMNS as f32 * cover_width + (GRID_COLUMNS - 1) as f32 * horizontal_gap;
    let start_x = (screen_width() - total_width) / 2.0;
    let start_y = 92.0 * scale_factor;

    for (offset, (cart, _)) in state.games[page_start..page_end].iter().enumerate() {
        let column = offset % GRID_COLUMNS;
        let row = offset / GRID_COLUMNS;
        let x = start_x + column as f32 * (cover_width + horizontal_gap);
        let y = start_y + row as f32 * (cover_height + vertical_gap);
        let texture = state.cover_cache.get(&cart.id).unwrap_or(placeholder);

        draw_rectangle(x, y, cover_width, cover_height, BLACK);
        draw_texture_ex(
            texture,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(cover_width, cover_height)),
                ..Default::default()
            },
        );

        if let Some(bytes) = state.game_sizes.get(&cart.id) {
            let size_label = format_game_size(*bytes);
            let badge_font_size = (7.0 * scale_factor).max(7.0) as u16;
            let font = get_current_font(font_cache, config);
            let label_width = measure_text(&size_label, Some(font), badge_font_size, 1.0).width;
            let badge_padding = 3.0 * scale_factor;
            let badge_height = 11.0 * scale_factor;
            let badge_x = x + cover_width - label_width - badge_padding * 2.0;
            let badge_y = y + cover_height - badge_height;
            draw_rectangle(
                badge_x,
                badge_y,
                label_width + badge_padding * 2.0,
                badge_height,
                Color::new(0.0, 0.0, 0.0, 0.82),
            );
            text_with_config_color(
                font_cache,
                config,
                &size_label,
                badge_x + badge_padding,
                badge_y + 8.0 * scale_factor,
                badge_font_size,
            );
        }

        if page_start + offset == state.selection {
            let cursor_scale = animation_state.get_cursor_scale();
            let base_width = cover_width + (6.0 * scale_factor);
            let base_height = cover_height + (6.0 * scale_factor);
            let width = base_width * cursor_scale;
            let height = base_height * cursor_scale;
            crate::ui::draw_configured_cursor_frame(
                config,
                animation_state,
                x - (3.0 * scale_factor) - (width - base_width) / 2.0,
                y - (3.0 * scale_factor) - (height - base_height) / 2.0,
                width,
                height,
                4.0 * scale_factor,
            );
        }
    }
}

fn draw_system_page(
    state: &InternalGamesState,
    placeholder: &Texture2D,
    animation_state: &AnimationState,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    scale_factor: f32,
) {
    let page = state.system_selection / GAMES_PER_PAGE;
    let page_start = page * GAMES_PER_PAGE;
    let page_end = (page_start + GAMES_PER_PAGE).min(state.systems.len());
    let cover_width = 72.0 * scale_factor;
    let cover_height = 96.0 * scale_factor;
    let horizontal_gap = 24.0 * scale_factor;
    let vertical_gap = 12.0 * scale_factor;
    let total_width =
        GRID_COLUMNS as f32 * cover_width + (GRID_COLUMNS - 1) as f32 * horizontal_gap;
    let start_x = (screen_width() - total_width) / 2.0;
    let start_y = 92.0 * scale_factor;

    for (offset, system) in state.systems[page_start..page_end].iter().enumerate() {
        let column = offset % GRID_COLUMNS;
        let row = offset / GRID_COLUMNS;
        let x = start_x + column as f32 * (cover_width + horizontal_gap);
        let y = start_y + row as f32 * (cover_height + vertical_gap);
        let key = format!("system:{system}");
        let texture = state.cover_cache.get(&key).unwrap_or(placeholder);

        draw_rectangle(x, y, cover_width, cover_height, BLACK);
        draw_texture_ex(
            texture,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(cover_width, cover_height)),
                ..Default::default()
            },
        );

        let count = state
            .all_games
            .iter()
            .filter(|(cart, _)| state.game_systems.get(&cart.id) == Some(system))
            .count();
        let badge = format!("{count} GAME{}", if count == 1 { "" } else { "S" });
        let font = get_current_font(font_cache, config);
        let badge_font_size = (7.0 * scale_factor).max(7.0) as u16;
        let badge_width = measure_text(&badge, Some(font), badge_font_size, 1.0).width;
        let badge_padding = 3.0 * scale_factor;
        let badge_height = 11.0 * scale_factor;
        draw_rectangle(
            x + cover_width - badge_width - badge_padding * 2.0,
            y + cover_height - badge_height,
            badge_width + badge_padding * 2.0,
            badge_height,
            Color::new(0.0, 0.0, 0.0, 0.84),
        );
        text_with_config_color(
            font_cache,
            config,
            &badge,
            x + cover_width - badge_width - badge_padding,
            y + cover_height - 3.0 * scale_factor,
            badge_font_size,
        );

        if page_start + offset == state.system_selection {
            let cursor_scale = animation_state.get_cursor_scale();
            let base_width = cover_width + (6.0 * scale_factor);
            let base_height = cover_height + (6.0 * scale_factor);
            let width = base_width * cursor_scale;
            let height = base_height * cursor_scale;
            crate::ui::draw_configured_cursor_frame(
                config,
                animation_state,
                x - (3.0 * scale_factor) - (width - base_width) / 2.0,
                y - (3.0 * scale_factor) - (height - base_height) / 2.0,
                width,
                height,
                4.0 * scale_factor,
            );
        }
    }
}

fn move_system_selection(
    state: &mut InternalGamesState,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
) -> bool {
    if state.systems.is_empty() {
        return false;
    }
    let old = state.system_selection;
    if left && state.system_selection > 0 {
        state.system_selection -= 1;
    } else if right && state.system_selection + 1 < state.systems.len() {
        state.system_selection += 1;
    } else if up && state.system_selection >= GRID_COLUMNS {
        state.system_selection -= GRID_COLUMNS;
    } else if down && state.system_selection + GRID_COLUMNS < state.systems.len() {
        state.system_selection += GRID_COLUMNS;
    }
    old != state.system_selection
}

pub fn move_selection(
    state: &mut InternalGamesState,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
) -> bool {
    if state.games.is_empty() {
        return false;
    }
    let old = state.selection;
    if left && state.selection > 0 {
        state.selection -= 1;
    } else if right && state.selection + 1 < state.games.len() {
        state.selection += 1;
    } else if up && state.selection >= GRID_COLUMNS {
        state.selection -= GRID_COLUMNS;
    } else if down && state.selection + GRID_COLUMNS < state.games.len() {
        state.selection += GRID_COLUMNS;
    }
    old != state.selection
}
