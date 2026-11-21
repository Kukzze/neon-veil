//! Live physics tuning window and developer console (`bevy_egui`).

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use egui_plot::{Line, Plot, PlotPoints};
use game_core::identifiers::PlayerCharacterMarker;
use std::collections::VecDeque;

pub const DEV_CONSOLE_TITLE: &str = "NEON VEIL // DEVELOPER ENGINE CONSOLE";
/// Maximum number of lines kept in the console history.
pub const CONSOLE_HISTORY_LIMIT: usize = 32;
pub const MIN_SKILL_LEVEL: u8 = 1;
pub const MAX_SKILL_LEVEL: u8 = 99;

/// Bounds, default and label of one tunable parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TuningParameter {
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl TuningParameter {
    /// Clamps `value` into bounds; non-finite values fall back to the default.
    pub fn clamp(&self, value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(self.min, self.max)
        } else {
            self.default
        }
    }
}

pub const SPRINT_SPEED: TuningParameter = TuningParameter {
    label: "Sprint Speed (m/s)",
    min: 5.0,
    max: 25.0,
    default: 13.5,
};
pub const JUMP_IMPULSE: TuningParameter = TuningParameter {
    label: "Jump Impulse (N*s)",
    min: 4.0,
    max: 18.0,
    default: 8.8,
};
pub const GRAVITY_ACCELERATION: TuningParameter = TuningParameter {
    label: "Gravity (m/s^2)",
    min: 9.8,
    max: 50.0,
    default: 24.5,
};
pub const WALL_RUN_SLIDE_SPEED: TuningParameter = TuningParameter {
    label: "Wall Descent (m/s)",
    min: 0.0,
    max: 5.0,
    default: 1.2,
};
pub const WALL_KICK_IMPULSE: TuningParameter = TuningParameter {
    label: "Wall Kick Force",
    min: 5.0,
    max: 20.0,
    default: 10.5,
};
pub const SLIDE_FRICTION: TuningParameter = TuningParameter {
    label: "Slide Friction Decay",
    min: 0.85,
    max: 0.999,
    default: 0.965,
};
pub const COYOTE_TIME_SECONDS: TuningParameter = TuningParameter {
    label: "Coyote Window (s)",
    min: 0.05,
    max: 0.30,
    default: 0.15,
};
pub const JUMP_BUFFER_SECONDS: TuningParameter = TuningParameter {
    label: "Jump Buffer (s)",
    min: 0.05,
    max: 0.30,
    default: 0.12,
};
pub const FLOW_MULTIPLIER: TuningParameter = TuningParameter {
    label: "Flow Speed Multiplier",
    min: 1.0,
    max: 2.0,
    default: 1.25,
};

/// Movement constants editable at runtime from the developer console.
#[derive(Resource, Reflect, Debug, Clone, PartialEq)]
#[reflect(Resource, Default, Debug)]
pub struct LivePhysicsTuning {
    pub sprint_speed: f32,
    pub jump_impulse: f32,
    pub gravity_acceleration: f32,
    pub wall_run_slide_speed: f32,
    pub wall_kick_impulse: f32,
    pub slide_friction: f32,
    pub coyote_time_seconds: f32,
    pub jump_buffer_seconds: f32,
    pub flow_multiplier: f32,
}

impl Default for LivePhysicsTuning {
    fn default() -> Self {
        Self {
            sprint_speed: SPRINT_SPEED.default,
            jump_impulse: JUMP_IMPULSE.default,
            gravity_acceleration: GRAVITY_ACCELERATION.default,
            wall_run_slide_speed: WALL_RUN_SLIDE_SPEED.default,
            wall_kick_impulse: WALL_KICK_IMPULSE.default,
            slide_friction: SLIDE_FRICTION.default,
            coyote_time_seconds: COYOTE_TIME_SECONDS.default,
            jump_buffer_seconds: JUMP_BUFFER_SECONDS.default,
            flow_multiplier: FLOW_MULTIPLIER.default,
        }
    }
}

impl LivePhysicsTuning {
    /// Every parameter paired with its bounds, in display order.
    pub fn parameters_mut(&mut self) -> [(&'static TuningParameter, &mut f32); 9] {
        [
            (&SPRINT_SPEED, &mut self.sprint_speed),
            (&JUMP_IMPULSE, &mut self.jump_impulse),
            (&GRAVITY_ACCELERATION, &mut self.gravity_acceleration),
            (&WALL_RUN_SLIDE_SPEED, &mut self.wall_run_slide_speed),
            (&WALL_KICK_IMPULSE, &mut self.wall_kick_impulse),
            (&SLIDE_FRICTION, &mut self.slide_friction),
            (&COYOTE_TIME_SECONDS, &mut self.coyote_time_seconds),
            (&JUMP_BUFFER_SECONDS, &mut self.jump_buffer_seconds),
            (&FLOW_MULTIPLIER, &mut self.flow_multiplier),
        ]
    }

    /// Forces every parameter into its valid range.
    pub fn clamp_to_bounds(&mut self) {
        for (parameter, value) in self.parameters_mut() {
            *value = parameter.clamp(*value);
        }
    }
}

/// Developer console visibility, overlay toggles and console state.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct DevToolsState {
    pub is_open: bool,
    pub show_hitboxes: bool,
    pub show_shapecasts: bool,
    pub show_profiler: bool,
    pub god_mode: bool,
    pub noclip: bool,
    pub command_input: String,
    /// Feedback from executed commands, newest last.
    pub console_history: VecDeque<String>,
}

impl Default for DevToolsState {
    fn default() -> Self {
        Self {
            is_open: cfg!(debug_assertions),
            show_hitboxes: false,
            show_shapecasts: false,
            show_profiler: false,
            god_mode: false,
            noclip: false,
            command_input: String::new(),
            console_history: VecDeque::with_capacity(CONSOLE_HISTORY_LIMIT),
        }
    }
}

impl DevToolsState {
    pub fn log(&mut self, line: impl Into<String>) {
        if self.console_history.len() == CONSOLE_HISTORY_LIMIT {
            self.console_history.pop_front();
        }
        self.console_history.push_back(line.into());
    }
}

/// A parsed developer console command.
#[derive(Debug, Clone, PartialEq)]
pub enum ConsoleCommand {
    Teleport(Vec3),
    ToggleGodMode,
    SetSkill { name: String, level: u8 },
    Heal,
}

/// Raw text submitted from the console input field.
#[derive(Event, Debug, Clone, PartialEq)]
pub struct ConsoleSubmitEvent(pub String);

/// A successfully parsed command, broadcast so gameplay crates can handle
/// commands outside this crate's reach (`/heal`, `/set_skill`).
#[derive(Event, Debug, Clone, PartialEq)]
pub struct ConsoleCommandEvent(pub ConsoleCommand);

fn parse_finite(s: &str) -> Option<f32> {
    s.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Parses a console line. The leading `/` is optional and the command name is
/// case-insensitive. Returns `None` for unknown commands, wrong argument
/// counts, non-finite coordinates or out-of-range skill levels.
pub fn parse_console_command(cmd: &str) -> Option<ConsoleCommand> {
    let trimmed = cmd.trim();
    let body = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let mut parts = body.split_whitespace();
    let name = parts.next()?.to_ascii_lowercase();
    let args: Vec<&str> = parts.collect();

    match (name.as_str(), args.as_slice()) {
        ("teleport" | "tp", [x, y, z]) => Some(ConsoleCommand::Teleport(Vec3::new(
            parse_finite(x)?,
            parse_finite(y)?,
            parse_finite(z)?,
        ))),
        ("godmode" | "god", []) => Some(ConsoleCommand::ToggleGodMode),
        ("set_skill", [skill, level]) => {
            let valid_name = skill.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            let level: u8 = level.parse().ok()?;
            if !valid_name || !(MIN_SKILL_LEVEL..=MAX_SKILL_LEVEL).contains(&level) {
                return None;
            }
            Some(ConsoleCommand::SetSkill {
                name: skill.to_ascii_lowercase(),
                level,
            })
        }
        ("heal", []) => Some(ConsoleCommand::Heal),
        _ => None,
    }
}

/// Parses submitted console lines, applies the commands this crate owns and
/// broadcasts every valid command as a [`ConsoleCommandEvent`].
pub fn parse_and_execute_console_command(
    mut submissions: EventReader<ConsoleSubmitEvent>,
    mut state: ResMut<DevToolsState>,
    mut players: Query<&mut Transform, With<PlayerCharacterMarker>>,
    mut commands_out: EventWriter<ConsoleCommandEvent>,
) {
    for ConsoleSubmitEvent(line) in submissions.read() {
        let Some(command) = parse_console_command(line) else {
            state.log(format!("unknown or malformed command: {}", line.trim()));
            continue;
        };
        match &command {
            ConsoleCommand::Teleport(target) => {
                let mut moved = 0;
                for mut transform in &mut players {
                    transform.translation = *target;
                    moved += 1;
                }
                if moved == 0 {
                    state.log("teleport: no player character to move");
                } else {
                    state.log(format!("teleported to {target}"));
                }
            }
            ConsoleCommand::ToggleGodMode => {
                state.god_mode = !state.god_mode;
                let label = if state.god_mode { "on" } else { "off" };
                state.log(format!("godmode {label}"));
            }
            ConsoleCommand::SetSkill { name, level } => {
                state.log(format!("set_skill {name} {level} sent"));
            }
            ConsoleCommand::Heal => state.log("heal sent"),
        }
        commands_out.send(ConsoleCommandEvent(command));
    }
}

/// Toggles the developer console with F1.
pub fn toggle_dev_console(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<DevToolsState>) {
    if keys.just_pressed(KeyCode::F1) {
        state.is_open = !state.is_open;
    }
}

/// Draws the developer console window.
pub fn render_dev_gui(
    mut contexts: EguiContexts,
    mut dev_state: ResMut<DevToolsState>,
    mut tuning: ResMut<LivePhysicsTuning>,
    diagnostics: Option<Res<DiagnosticsStore>>,
    mut submissions: EventWriter<ConsoleSubmitEvent>,
) {
    if !dev_state.is_open {
        return;
    }
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };

    // Write through `bypass_change_detection` and mark changed only when a
    // widget actually changed, so downstream `resource_changed` systems do
    // not run every frame.
    let mut tuning_changed = false;
    let mut state_changed = false;
    let state = dev_state.bypass_change_detection();
    let params = tuning.bypass_change_detection();

    egui::Window::new(DEV_CONSOLE_TITLE)
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.heading("Kinematic Physics Tuning");
            for (parameter, value) in params.parameters_mut() {
                tuning_changed |= ui
                    .add(
                        egui::Slider::new(value, parameter.min..=parameter.max)
                            .text(parameter.label),
                    )
                    .changed();
            }
            if ui.button("Reset to defaults").clicked() {
                *params = LivePhysicsTuning::default();
                tuning_changed = true;
            }

            ui.separator();
            ui.heading("Spatial & Visualizer Overlays");
            for (flag, label) in [
                (&mut state.show_hitboxes, "Draw Melee Hitboxes & Hurtboxes"),
                (&mut state.show_shapecasts, "Draw Parkour Shapecast Probes"),
                (&mut state.show_profiler, "Show Frame-Time Performance HUD"),
                (&mut state.god_mode, "Godmode (Infinite Health/Stamina)"),
                (&mut state.noclip, "Noclip Flying Mode"),
            ] {
                state_changed |= ui.checkbox(flag, label).changed();
            }

            if state.show_profiler {
                ui.separator();
                draw_profiler(ui, diagnostics.as_deref());
            }

            ui.separator();
            ui.heading("Console Commands");
            ui.horizontal(|ui| {
                ui.label("/");
                let response = ui.text_edit_singleline(&mut state.command_input);
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let line = std::mem::take(&mut state.command_input);
                    if !line.trim().is_empty() {
                        submissions.send(ConsoleSubmitEvent(line));
                    }
                    response.request_focus();
                }
            });
            for line in &state.console_history {
                ui.monospace(line);
            }
        });

    if tuning_changed {
        tuning.set_changed();
    }
    if state_changed {
        dev_state.set_changed();
    }
}

fn draw_profiler(ui: &mut egui::Ui, diagnostics: Option<&DiagnosticsStore>) {
    let Some(diagnostics) = diagnostics else {
        ui.label("Diagnostics unavailable");
        return;
    };
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed());
    let frame_time = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME);
    let frame_ms = frame_time.and_then(|d| d.smoothed());

    match (fps, frame_ms) {
        (Some(fps), Some(ms)) => ui.label(format!("{fps:.0} FPS  |  {ms:.2} ms")),
        _ => ui.label("Collecting frame samples..."),
    };

    if let Some(frame_time) = frame_time {
        let points: PlotPoints = frame_time
            .values()
            .enumerate()
            .map(|(i, ms)| [i as f64, *ms])
            .collect();
        Plot::new("frame_time_plot")
            .height(80.0)
            .include_y(0.0)
            .include_y(33.3)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show_x(false)
            .show(ui, |plot_ui| plot_ui.line(Line::new(points)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuning_defaults_match_spec() {
        let t = LivePhysicsTuning::default();
        assert_eq!(t.sprint_speed, 13.5);
        assert_eq!(t.jump_impulse, 8.8);
        assert_eq!(t.gravity_acceleration, 24.5);
        assert_eq!(t.wall_run_slide_speed, 1.2);
        assert_eq!(t.wall_kick_impulse, 10.5);
        assert_eq!(t.slide_friction, 0.965);
        assert_eq!(t.coyote_time_seconds, 0.15);
        assert_eq!(t.jump_buffer_seconds, 0.12);
        assert_eq!(t.flow_multiplier, 1.25);
    }

    #[test]
    fn defaults_lie_inside_bounds() {
        let mut t = LivePhysicsTuning::default();
        let before = t.clone();
        t.clamp_to_bounds();
        assert_eq!(t, before);
        for (p, v) in t.parameters_mut() {
            assert!(p.min < p.max, "{}", p.label);
            assert!((p.min..=p.max).contains(v), "{}", p.label);
        }
    }

    #[test]
    fn clamping_enforces_ranges_and_rejects_nan() {
        let mut t = LivePhysicsTuning {
            sprint_speed: 999.0,
            jump_impulse: -3.0,
            gravity_acceleration: f32::NAN,
            wall_run_slide_speed: f32::INFINITY,
            slide_friction: 1.5,
            ..LivePhysicsTuning::default()
        };
        t.clamp_to_bounds();
        assert_eq!(t.sprint_speed, 25.0);
        assert_eq!(t.jump_impulse, 4.0);
        assert_eq!(t.gravity_acceleration, 24.5);
        assert_eq!(t.wall_run_slide_speed, 1.2);
        assert_eq!(t.slide_friction, 0.999);
    }

    #[test]
    fn parses_valid_commands() {
        assert_eq!(
            parse_console_command("/teleport 10 2.5 -40"),
            Some(ConsoleCommand::Teleport(Vec3::new(10.0, 2.5, -40.0)))
        );
        assert_eq!(
            parse_console_command("  TP -1 0 1e2 "),
            Some(ConsoleCommand::Teleport(Vec3::new(-1.0, 0.0, 100.0)))
        );
        assert_eq!(
            parse_console_command("/godmode"),
            Some(ConsoleCommand::ToggleGodMode)
        );
        assert_eq!(parse_console_command("heal"), Some(ConsoleCommand::Heal));
        assert_eq!(
            parse_console_command("/set_skill Agility 99"),
            Some(ConsoleCommand::SetSkill {
                name: "agility".to_owned(),
                level: 99
            })
        );
        assert_eq!(
            parse_console_command("/set_skill net_running 1"),
            Some(ConsoleCommand::SetSkill {
                name: "net_running".to_owned(),
                level: 1
            })
        );
    }

    #[test]
    fn rejects_invalid_commands() {
        for bad in [
            "",
            "   ",
            "/",
            "/fly",
            "/teleport",
            "/teleport 1 2",
            "/teleport 1 2 3 4",
            "/teleport x 2 3",
            "/teleport NaN 0 0",
            "/teleport inf 0 0",
            "/teleport 0 -inf 0",
            "/godmode now",
            "/heal 50",
            "/set_skill agility",
            "/set_skill agility 0",
            "/set_skill agility 100",
            "/set_skill agility -5",
            "/set_skill agility 9.5",
            "/set_skill agi-lity 50",
            "/set_skill agility 50 extra",
        ] {
            assert_eq!(parse_console_command(bad), None, "accepted {bad:?}");
        }
    }

    #[test]
    fn console_history_is_bounded() {
        let mut state = DevToolsState::default();
        for i in 0..(CONSOLE_HISTORY_LIMIT + 10) {
            state.log(format!("line {i}"));
        }
        assert_eq!(state.console_history.len(), CONSOLE_HISTORY_LIMIT);
        assert_eq!(
            state.console_history.front().map(String::as_str),
            Some("line 10")
        );
    }

    fn console_app() -> App {
        let mut app = App::new();
        app.init_resource::<DevToolsState>()
            .add_event::<ConsoleSubmitEvent>()
            .add_event::<ConsoleCommandEvent>()
            .add_systems(Update, parse_and_execute_console_command);
        app
    }

    fn submit(app: &mut App, line: &str) {
        app.world_mut()
            .send_event(ConsoleSubmitEvent(line.to_owned()));
        app.update();
    }

    #[test]
    fn teleport_moves_player_characters_only() {
        let mut app = console_app();
        let player = app
            .world_mut()
            .spawn((PlayerCharacterMarker, Transform::default()))
            .id();
        let prop = app.world_mut().spawn(Transform::default()).id();

        submit(&mut app, "/teleport 5 10 -15");

        let world = app.world();
        assert_eq!(
            world.get::<Transform>(player).map(|t| t.translation),
            Some(Vec3::new(5.0, 10.0, -15.0))
        );
        assert_eq!(
            world.get::<Transform>(prop).map(|t| t.translation),
            Some(Vec3::ZERO)
        );
    }

    #[test]
    fn godmode_toggles_and_commands_are_broadcast() {
        let mut app = console_app();
        submit(&mut app, "/godmode");
        assert!(app.world().resource::<DevToolsState>().god_mode);
        submit(&mut app, "/heal");
        submit(&mut app, "/godmode");
        assert!(!app.world().resource::<DevToolsState>().god_mode);

        let events = app.world().resource::<Events<ConsoleCommandEvent>>();
        let mut reader = events.get_cursor();
        let sent: Vec<_> = reader.read(events).map(|e| e.0.clone()).collect();
        assert!(sent.contains(&ConsoleCommand::Heal));
        assert!(sent.contains(&ConsoleCommand::ToggleGodMode));
    }

    #[test]
    fn malformed_command_is_logged_not_broadcast() {
        let mut app = console_app();
        submit(&mut app, "/teleport nan 0 0");
        let state = app.world().resource::<DevToolsState>();
        assert!(state
            .console_history
            .back()
            .is_some_and(|l| l.starts_with("unknown or malformed")));
        let events = app.world().resource::<Events<ConsoleCommandEvent>>();
        assert!(events.is_empty());
    }
}
