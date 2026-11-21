//! Settings menu (hotkey Escape) and the persisted user configuration.
//!
//! Settings load from `config/settings.json` at startup, apply live to the
//! audio mixer and look controls whenever they change, and are written back
//! when the player presses Save. Unknown or missing fields fall back to
//! defaults and every value is clamped to its allowed range on load.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use game_audio::AudioMixerSettings;
use game_input::LookSettings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_SETTINGS_PATH: &str = "config/settings.json";
/// Look sensitivity (rad per pixel) at a UI sensitivity of 1.0.
pub const BASE_MOUSE_SENSITIVITY: f32 = 0.0022;

pub const FOV_RANGE: (f32, f32) = (70.0, 110.0);
pub const MOUSE_SENSITIVITY_RANGE: (f32, f32) = (0.1, 5.0);
pub const STICK_DEADZONE_RANGE: (f32, f32) = (0.0, 0.30);
pub const SUBTITLE_SIZE_RANGE: (u32, u32) = (12, 40);
pub const RESOLUTIONS: [(u32, u32); 4] = [(1920, 1080), (2560, 1440), (3840, 2160), (3440, 1440)];
/// 0 means uncapped.
pub const FPS_CAPS: [u32; 5] = [60, 120, 144, 240, 0];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Video,
    Audio,
    Controls,
    Accessibility,
}

impl SettingsTab {
    pub const ALL: [Self; 4] = [
        Self::Video,
        Self::Audio,
        Self::Controls,
        Self::Accessibility,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Video => "VIDEO",
            Self::Audio => "AUDIO",
            Self::Controls => "CONTROLS",
            Self::Accessibility => "ACCESSIBILITY",
        }
    }
}

#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct SettingsModalState {
    pub is_open: bool,
    pub active_tab: SettingsTab,
    /// Result of the last save, shown under the buttons.
    pub status: Option<String>,
}

impl SettingsModalState {
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DisplayMode {
    Fullscreen,
    #[default]
    BorderlessWindowed,
    Windowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GraphicsPreset {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColorblindMode {
    #[default]
    None,
    Protanopia,
    Deuteranopia,
    Tritanopia,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoSettings {
    pub display_mode: DisplayMode,
    pub resolution: (u32, u32),
    /// 0 = uncapped.
    pub fps_cap: u32,
    pub fov_degrees: f32,
    pub graphics_preset: GraphicsPreset,
    pub motion_blur_enabled: bool,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            display_mode: DisplayMode::default(),
            resolution: (1920, 1080),
            fps_cap: 144,
            fov_degrees: 90.0,
            graphics_preset: GraphicsPreset::default(),
            motion_blur_enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub master_volume: f32,
    pub music_volume: f32,
    pub sfx_volume: f32,
    pub voice_volume: f32,
    pub ambience_volume: f32,
    pub audio_ducking_enabled: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        let mixer = AudioMixerSettings::default();
        Self {
            master_volume: mixer.master_volume,
            music_volume: mixer.music_volume,
            sfx_volume: mixer.sfx_volume,
            voice_volume: mixer.voice_volume,
            ambience_volume: mixer.ambience_volume,
            audio_ducking_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlsSettings {
    pub mouse_sensitivity: f32,
    pub invert_y_axis: bool,
    pub stick_deadzone: f32,
}

impl Default for ControlsSettings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 1.0,
            invert_y_axis: false,
            stick_deadzone: 0.05,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccessibilitySettings {
    pub subtitles_enabled: bool,
    pub subtitle_font_size: u32,
    pub colorblind_mode: ColorblindMode,
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        Self {
            subtitles_enabled: true,
            subtitle_font_size: 18,
            colorblind_mode: ColorblindMode::default(),
        }
    }
}

#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserSettingsConfig {
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub controls: ControlsSettings,
    pub accessibility: AccessibilitySettings,
}

fn clamp_finite(value: f32, (lo, hi): (f32, f32), fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        fallback
    }
}

impl UserSettingsConfig {
    /// Copy with every value inside its allowed range.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        let d = Self::default();
        let unit = |v: f32, fallback: f32| clamp_finite(v, (0.0, 1.0), fallback);
        let mut s = self.clone();
        s.video.fov_degrees = clamp_finite(s.video.fov_degrees, FOV_RANGE, d.video.fov_degrees);
        if s.video.resolution.0 == 0 || s.video.resolution.1 == 0 {
            s.video.resolution = d.video.resolution;
        }
        if s.video.fps_cap != 0 {
            s.video.fps_cap = s.video.fps_cap.clamp(30, 1000);
        }
        s.audio.master_volume = unit(s.audio.master_volume, d.audio.master_volume);
        s.audio.music_volume = unit(s.audio.music_volume, d.audio.music_volume);
        s.audio.sfx_volume = unit(s.audio.sfx_volume, d.audio.sfx_volume);
        s.audio.voice_volume = unit(s.audio.voice_volume, d.audio.voice_volume);
        s.audio.ambience_volume = unit(s.audio.ambience_volume, d.audio.ambience_volume);
        s.controls.mouse_sensitivity = clamp_finite(
            s.controls.mouse_sensitivity,
            MOUSE_SENSITIVITY_RANGE,
            d.controls.mouse_sensitivity,
        );
        s.controls.stick_deadzone = clamp_finite(
            s.controls.stick_deadzone,
            STICK_DEADZONE_RANGE,
            d.controls.stick_deadzone,
        );
        s.accessibility.subtitle_font_size = s
            .accessibility
            .subtitle_font_size
            .clamp(SUBTITLE_SIZE_RANGE.0, SUBTITLE_SIZE_RANGE.1);
        s
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings serialize")
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str::<Self>(json).map(|s| s.sanitized())
    }

    /// Writes via a temporary file and rename, so a crash mid-write never
    /// leaves a truncated settings file.
    pub fn save_to_file(&self, path: &str) -> std::io::Result<()> {
        let path = Path::new(path);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, self.to_json())?;
        std::fs::rename(&temp, path)
    }

    pub fn load_from_file(path: &str) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Pushes the audio settings into the mixer.
    pub fn apply_to_mixer(&self, mixer: &mut AudioMixerSettings) {
        mixer.master_volume = self.audio.master_volume;
        mixer.music_volume = self.audio.music_volume;
        mixer.sfx_volume = self.audio.sfx_volume;
        mixer.voice_volume = self.audio.voice_volume;
        mixer.ambience_volume = self.audio.ambience_volume;
        mixer.ducking_enabled = self.audio.audio_ducking_enabled;
    }

    pub fn apply_to_look(&self, look: &mut LookSettings) {
        look.mouse_sensitivity = BASE_MOUSE_SENSITIVITY * self.controls.mouse_sensitivity;
        look.invert_y = self.controls.invert_y_axis;
    }
}

/// Where settings are stored.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct UserSettingsPath(pub PathBuf);

impl Default for UserSettingsPath {
    fn default() -> Self {
        Self(PathBuf::from(DEFAULT_SETTINGS_PATH))
    }
}

/// Loads saved settings if the file exists; a damaged file is reported and
/// defaults are kept.
pub fn load_user_settings(path: Res<UserSettingsPath>, mut config: ResMut<UserSettingsConfig>) {
    let Some(path_str) = path.0.to_str() else {
        return;
    };
    if !path.0.exists() {
        return;
    }
    match UserSettingsConfig::load_from_file(path_str) {
        Ok(loaded) => *config = loaded,
        Err(error) => warn!(
            "ignoring unreadable settings file {}: {error}",
            path.0.display()
        ),
    }
}

/// Applies settings to live systems whenever they change.
pub fn apply_user_settings(
    config: Res<UserSettingsConfig>,
    mixer: Option<ResMut<AudioMixerSettings>>,
    look: Option<ResMut<LookSettings>>,
) {
    if !config.is_changed() {
        return;
    }
    if let Some(mut mixer) = mixer {
        config.apply_to_mixer(&mut mixer);
    }
    if let Some(mut look) = look {
        config.apply_to_look(&mut look);
    }
}

fn enum_combo<T: PartialEq + Copy + std::fmt::Debug>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[T],
) {
    egui::ComboBox::from_label(label)
        .selected_text(format!("{value:?}"))
        .show_ui(ui, |ui| {
            for option in options {
                ui.selectable_value(value, *option, format!("{option:?}"));
            }
        });
}

fn percent_slider(ui: &mut egui::Ui, label: &str, value: &mut f32) {
    ui.add(
        egui::Slider::new(value, 0.0..=1.0)
            .text(label)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
    );
}

pub fn render_settings_menu_egui(
    mut contexts: EguiContexts,
    mut state: ResMut<SettingsModalState>,
    mut config: ResMut<UserSettingsConfig>,
    path: Res<UserSettingsPath>,
) {
    if !state.is_open {
        return;
    }
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let mut draft = config.clone();
    let mut open = true;
    let mut save = false;
    let mut reset = false;
    egui::Window::new("NEON VEIL // SYSTEM CONFIGURATION")
        .open(&mut open)
        .collapsible(false)
        .default_width(460.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for tab in SettingsTab::ALL {
                    ui.selectable_value(&mut state.active_tab, tab, tab.label());
                }
            });
            ui.separator();
            match state.active_tab {
                SettingsTab::Video => {
                    let v = &mut draft.video;
                    enum_combo(
                        ui,
                        "Display mode",
                        &mut v.display_mode,
                        &[
                            DisplayMode::Fullscreen,
                            DisplayMode::BorderlessWindowed,
                            DisplayMode::Windowed,
                        ],
                    );
                    egui::ComboBox::from_label("Resolution")
                        .selected_text(format!("{} x {}", v.resolution.0, v.resolution.1))
                        .show_ui(ui, |ui| {
                            for r in RESOLUTIONS {
                                ui.selectable_value(
                                    &mut v.resolution,
                                    r,
                                    format!("{} x {}", r.0, r.1),
                                );
                            }
                        });
                    egui::ComboBox::from_label("Frame rate cap")
                        .selected_text(if v.fps_cap == 0 {
                            "Uncapped".to_string()
                        } else {
                            format!("{} FPS", v.fps_cap)
                        })
                        .show_ui(ui, |ui| {
                            for cap in FPS_CAPS {
                                let text = if cap == 0 {
                                    "Uncapped".to_string()
                                } else {
                                    format!("{cap} FPS")
                                };
                                ui.selectable_value(&mut v.fps_cap, cap, text);
                            }
                        });
                    ui.add(
                        egui::Slider::new(&mut v.fov_degrees, FOV_RANGE.0..=FOV_RANGE.1)
                            .text("Field of view")
                            .suffix("\u{b0}"),
                    );
                    enum_combo(
                        ui,
                        "Graphics preset",
                        &mut v.graphics_preset,
                        &[
                            GraphicsPreset::Low,
                            GraphicsPreset::Medium,
                            GraphicsPreset::High,
                            GraphicsPreset::Ultra,
                        ],
                    );
                    ui.checkbox(&mut v.motion_blur_enabled, "Motion blur");
                }
                SettingsTab::Audio => {
                    let a = &mut draft.audio;
                    percent_slider(ui, "Master", &mut a.master_volume);
                    percent_slider(ui, "Music", &mut a.music_volume);
                    percent_slider(ui, "SFX & foley", &mut a.sfx_volume);
                    percent_slider(ui, "Voiceover", &mut a.voice_volume);
                    percent_slider(ui, "Ambience", &mut a.ambience_volume);
                    ui.checkbox(&mut a.audio_ducking_enabled, "Lower music during dialogue");
                }
                SettingsTab::Controls => {
                    let c = &mut draft.controls;
                    ui.add(
                        egui::Slider::new(
                            &mut c.mouse_sensitivity,
                            MOUSE_SENSITIVITY_RANGE.0..=MOUSE_SENSITIVITY_RANGE.1,
                        )
                        .text("Mouse sensitivity"),
                    );
                    ui.checkbox(&mut c.invert_y_axis, "Invert Y axis");
                    ui.add(
                        egui::Slider::new(
                            &mut c.stick_deadzone,
                            STICK_DEADZONE_RANGE.0..=STICK_DEADZONE_RANGE.1,
                        )
                        .text("Stick deadzone"),
                    );
                }
                SettingsTab::Accessibility => {
                    let a = &mut draft.accessibility;
                    ui.checkbox(&mut a.subtitles_enabled, "Subtitles");
                    ui.add(
                        egui::Slider::new(
                            &mut a.subtitle_font_size,
                            SUBTITLE_SIZE_RANGE.0..=SUBTITLE_SIZE_RANGE.1,
                        )
                        .text("Subtitle size"),
                    );
                    enum_combo(
                        ui,
                        "Colorblind filter",
                        &mut a.colorblind_mode,
                        &[
                            ColorblindMode::None,
                            ColorblindMode::Protanopia,
                            ColorblindMode::Deuteranopia,
                            ColorblindMode::Tritanopia,
                        ],
                    );
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                save = ui.button("Save").clicked();
                reset = ui.button("Reset tab to defaults").clicked();
            });
            if let Some(status) = &state.status {
                ui.weak(status);
            }
        });
    if reset {
        let d = UserSettingsConfig::default();
        match state.active_tab {
            SettingsTab::Video => draft.video = d.video,
            SettingsTab::Audio => draft.audio = d.audio,
            SettingsTab::Controls => draft.controls = d.controls,
            SettingsTab::Accessibility => draft.accessibility = d.accessibility,
        }
    }
    let draft = draft.sanitized();
    if draft != *config {
        *config = draft;
    }
    if save {
        state.status = Some(match path.0.to_str().map(|p| config.save_to_file(p)) {
            Some(Ok(())) => format!("Saved to {}", path.0.display()),
            Some(Err(error)) => format!("Save failed: {error}"),
            None => "Save failed: path is not valid UTF-8".to_string(),
        });
    }
    if !open {
        state.is_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "neon_veil_settings_test_{}_{name}",
            std::process::id()
        ))
    }

    #[test]
    fn toggle_and_tabs() {
        let mut state = SettingsModalState::default();
        assert!(!state.is_open);
        state.toggle();
        assert!(state.is_open);
        state.toggle();
        assert!(!state.is_open);
        assert_eq!(state.active_tab, SettingsTab::Video);
        assert_eq!(SettingsTab::ALL.len(), 4);
    }

    #[test]
    fn defaults_match_the_design() {
        let s = UserSettingsConfig::default();
        assert_eq!(s.video.resolution, (1920, 1080));
        assert_eq!(
            (
                s.video.fps_cap,
                s.video.fov_degrees,
                s.video.motion_blur_enabled
            ),
            (144, 90.0, false)
        );
        assert_eq!(
            (
                s.audio.sfx_volume,
                s.audio.music_volume,
                s.audio.ambience_volume
            ),
            (0.85, 0.70, 0.60)
        );
        assert!(s.audio.audio_ducking_enabled);
        assert_eq!(
            (
                s.controls.mouse_sensitivity,
                s.controls.invert_y_axis,
                s.controls.stick_deadzone
            ),
            (1.0, false, 0.05)
        );
        assert_eq!(
            (
                s.accessibility.subtitles_enabled,
                s.accessibility.subtitle_font_size
            ),
            (true, 18)
        );
        assert_eq!(s.accessibility.colorblind_mode, ColorblindMode::None);
        assert_eq!(s.sanitized(), s, "defaults are already in range");
    }

    #[test]
    fn json_roundtrip_keeps_every_field() {
        let mut s = UserSettingsConfig::default();
        s.video.fov_degrees = 105.0;
        s.video.display_mode = DisplayMode::Fullscreen;
        s.video.graphics_preset = GraphicsPreset::Ultra;
        s.controls.mouse_sensitivity = 2.5;
        s.controls.invert_y_axis = true;
        s.audio.music_volume = 0.3;
        s.accessibility.colorblind_mode = ColorblindMode::Deuteranopia;
        let json = s.to_json();
        assert!(
            json.contains("\"fov_degrees\": 105.0") && json.contains("\"Deuteranopia\""),
            "{json}"
        );
        let back = UserSettingsConfig::from_json(&json).unwrap();
        assert_eq!(back, s);
        assert_eq!(
            (back.video.fov_degrees, back.controls.mouse_sensitivity),
            (105.0, 2.5)
        );
    }

    #[test]
    fn file_save_and_load() {
        let path = temp_path("roundtrip").join("nested").join("settings.json");
        let mut s = UserSettingsConfig::default();
        s.video.fov_degrees = 105.0;
        s.controls.mouse_sensitivity = 2.5;
        s.save_to_file(path.to_str().unwrap()).unwrap();
        assert!(
            !path.with_extension("json.tmp").exists(),
            "temp file renamed away"
        );
        let loaded = UserSettingsConfig::load_from_file(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded, s);
        let missing =
            UserSettingsConfig::load_from_file(temp_path("missing.json").to_str().unwrap());
        assert_eq!(missing.unwrap_err().kind(), std::io::ErrorKind::NotFound);
        std::fs::write(&path, "{ not json").unwrap();
        let broken = UserSettingsConfig::load_from_file(path.to_str().unwrap());
        assert_eq!(broken.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
        let _ = std::fs::remove_dir_all(temp_path("roundtrip"));
    }

    #[test]
    fn partial_and_out_of_range_files_are_repaired() {
        let json = r#"{ "video": { "fov_degrees": 170.0 }, "controls": { "mouse_sensitivity": -3.0, "stick_deadzone": 0.9 },
                        "audio": { "master_volume": 4.0 }, "accessibility": { "subtitle_font_size": 400 } }"#;
        let s = UserSettingsConfig::from_json(json).unwrap();
        assert_eq!(s.video.fov_degrees, 110.0);
        assert_eq!(
            s.video.resolution,
            (1920, 1080),
            "missing fields take defaults"
        );
        assert_eq!(s.controls.mouse_sensitivity, 0.1);
        assert_eq!(s.controls.stick_deadzone, 0.30);
        assert_eq!(s.audio.master_volume, 1.0);
        assert_eq!(s.audio.sfx_volume, 0.85);
        assert_eq!(s.accessibility.subtitle_font_size, 40);
        assert_eq!(
            UserSettingsConfig::from_json("{}").unwrap(),
            UserSettingsConfig::default()
        );
    }

    #[test]
    fn settings_drive_the_mixer_and_look_controls() {
        let mut s = UserSettingsConfig::default();
        s.audio.master_volume = 0.5;
        s.audio.audio_ducking_enabled = false;
        s.controls.mouse_sensitivity = 2.0;
        s.controls.invert_y_axis = true;
        let mut mixer = AudioMixerSettings::default();
        s.apply_to_mixer(&mut mixer);
        assert_eq!((mixer.master_volume, mixer.ducking_enabled), (0.5, false));
        let mut look = LookSettings::default();
        s.apply_to_look(&mut look);
        assert!((look.mouse_sensitivity - 0.0044).abs() < 1e-7 && look.invert_y);
    }

    #[test]
    fn startup_load_and_live_apply() {
        let path = temp_path("startup.json");
        let mut saved = UserSettingsConfig::default();
        saved.audio.music_volume = 0.25;
        saved.save_to_file(path.to_str().unwrap()).unwrap();

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<UserSettingsConfig>()
            .init_resource::<AudioMixerSettings>()
            .init_resource::<LookSettings>()
            .insert_resource(UserSettingsPath(path.clone()))
            .add_systems(Startup, load_user_settings)
            .add_systems(Update, apply_user_settings);
        app.update();
        assert_eq!(
            app.world().resource::<AudioMixerSettings>().music_volume,
            0.25
        );
        app.world_mut()
            .resource_mut::<UserSettingsConfig>()
            .controls
            .mouse_sensitivity = 3.0;
        app.update();
        assert!((app.world().resource::<LookSettings>().mouse_sensitivity - 0.0066).abs() < 1e-7);
        let _ = std::fs::remove_file(path);
    }
}
