//! Escape pause menu, the controls reference and the on-screen controls
//! badge, plus the focus rule shared by every menu: while any menu is open
//! the cursor is free and gameplay input is gated; otherwise the cursor is
//! captured for mouse look.

use crate::hud::{hud_text, AMBER, CYAN, PANEL};
use crate::{HudFont, InventoryModalState, SettingsModalState, SkillsModalState};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use game_audio::PlaySfxEvent;
use game_input::GameplayInputGate;

/// Controls listed on the controls page.
pub const CONTROL_BINDINGS: [(&str, &str); 16] = [
    ("W A S D", "Move"),
    ("Mouse", "Look"),
    ("Space", "Jump"),
    ("Shift", "Sprint"),
    ("C / Ctrl", "Crouch, slide while sprinting"),
    ("Alt", "Dodge roll"),
    ("LMB", "Katana combo / fire"),
    ("MMB", "Heavy attack"),
    ("RMB", "Guard and parry"),
    ("R", "Reload"),
    ("Q", "Swap weapon"),
    ("F", "Interact"),
    ("V", "Toggle camera perspective"),
    ("Tab", "Inventory"),
    ("K", "Skills"),
    ("Esc", "Pause menu"),
];

/// One-line summary shown in the badge.
pub const CONTROLS_BADGE_TEXT: &str =
    "WASD Move  |  Mouse Look  |  Space Jump  |  Shift Sprint  |  \
C Slide  |  LMB Katana  |  RMB Guard  |  Tab Inventory  |  K Skills  |  Esc Menu  |  F1 Hide";

/// Capture mode for gameplay. Windows cannot lock the cursor, so it is
/// confined there; mouse look reads raw motion either way.
pub const GAMEPLAY_GRAB_MODE: CursorGrabMode = if cfg!(target_os = "windows") {
    CursorGrabMode::Confined
} else {
    CursorGrabMode::Locked
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PausePage {
    #[default]
    Main,
    Controls,
}

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PauseMenuState {
    pub is_open: bool,
    pub page: PausePage,
    /// Set when the player asks to edit their operative; the client opens
    /// the creator and clears it.
    pub profile_requested: bool,
}

impl PauseMenuState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.page = PausePage::Main;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.page = PausePage::Main;
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlsBadgeState {
    pub visible: bool,
}

impl Default for ControlsBadgeState {
    fn default() -> Self {
        Self { visible: true }
    }
}

/// A full-screen client screen (the operative creator) is up: the cursor is
/// free, gameplay input is gated and the HUD is hidden.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScreenOverlay(pub bool);

/// Set by gameplay (death, victory) to freeze the character without
/// freeing the cursor.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GameplayHold(pub bool);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseButton {
    Resume,
    Profile,
    Settings,
    Controls,
    Quit,
    Back,
}

impl PauseButton {
    pub fn label(self) -> &'static str {
        match self {
            Self::Resume => "RESUME GAME",
            Self::Profile => "OPERATIVE PROFILE",
            Self::Settings => "SETTINGS",
            Self::Controls => "CONTROLS & KEYBINDINGS",
            Self::Quit => "QUIT TO DESKTOP",
            Self::Back => "BACK",
        }
    }
}

#[derive(Component)]
pub struct PauseMenuRoot;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PausePageNode(pub PausePage);

#[derive(Component)]
pub struct ControlsBadgeNode;

const BUTTON_IDLE: Color = Color::srgba(0.0, 0.9, 1.0, 0.06);
const BUTTON_HOVER: Color = Color::srgba(0.0, 0.9, 1.0, 0.22);
const BUTTON_PRESS: Color = Color::srgba(1.0, 0.67, 0.0, 0.35);
const SCRIM: Color = Color::srgba(0.0, 0.01, 0.03, 0.62);
const MUTED: Color = Color::srgba(0.8, 0.9, 1.0, 0.7);

/// Whether any menu currently owns the mouse.
pub fn any_menu_open(
    pause: &PauseMenuState,
    skills: &SkillsModalState,
    inventory: &InventoryModalState,
    settings: &SettingsModalState,
) -> bool {
    pause.is_open || skills.is_open || inventory.is_open || settings.is_open
}

/// Applies a pause-menu button; returns `true` when the game should quit.
pub fn apply_pause_button(
    button: PauseButton,
    pause: &mut PauseMenuState,
    settings: &mut SettingsModalState,
) -> bool {
    match button {
        PauseButton::Resume => pause.close(),
        PauseButton::Profile => {
            pause.close();
            pause.profile_requested = true;
        }
        PauseButton::Settings => settings.is_open = true,
        PauseButton::Controls => pause.page = PausePage::Controls,
        PauseButton::Back => pause.page = PausePage::Main,
        PauseButton::Quit => return true,
    }
    false
}

fn visibility(shown: bool) -> Visibility {
    if shown {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

fn spawn_button(parent: &mut ChildBuilder, font: &Handle<Font>, kind: PauseButton) {
    parent
        .spawn((
            Button,
            kind,
            Node {
                width: Val::Px(340.0),
                padding: UiRect::axes(Val::Px(18.0), Val::Px(12.0)),
                margin: UiRect::vertical(Val::Px(5.0)),
                border: UiRect::left(Val::Px(3.0)),
                ..default()
            },
            BorderColor(CYAN),
            BackgroundColor(BUTTON_IDLE),
        ))
        .with_children(|b| {
            b.spawn(hud_text(font, kind.label(), 17.0, Color::WHITE));
        });
}

fn spaced<B: Bundle>(bundle: B, margin: UiRect) -> (B, Node) {
    (
        bundle,
        Node {
            margin,
            ..default()
        },
    )
}

pub fn setup_pause_menu(mut commands: Commands, hud_font: Option<Res<HudFont>>) {
    let font = hud_font.map(|f| f.0.clone()).unwrap_or_default();
    let panel = Node {
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(Val::Px(32.0)),
        border: UiRect::top(Val::Px(2.0)),
        ..default()
    };
    commands
        .spawn((
            PauseMenuRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(SCRIM),
            GlobalZIndex(20),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((
                PausePageNode(PausePage::Main),
                panel.clone(),
                BorderColor(CYAN),
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn(hud_text(&font, "NEON VEIL // ZERO PROTOCOL", 13.0, AMBER));
                p.spawn(spaced(
                    hud_text(&font, "SYSTEM PAUSED", 30.0, Color::WHITE),
                    UiRect::bottom(Val::Px(22.0)),
                ));
                for kind in [
                    PauseButton::Resume,
                    PauseButton::Profile,
                    PauseButton::Settings,
                    PauseButton::Controls,
                    PauseButton::Quit,
                ] {
                    spawn_button(p, &font, kind);
                }
                p.spawn(spaced(
                    hud_text(&font, "Esc to resume", 12.0, MUTED),
                    UiRect::top(Val::Px(16.0)),
                ));
            });
            root.spawn((
                PausePageNode(PausePage::Controls),
                panel,
                BorderColor(CYAN),
                BackgroundColor(PANEL),
                Visibility::Hidden,
            ))
            .with_children(|p| {
                p.spawn(spaced(
                    hud_text(&font, "CONTROLS & KEYBINDINGS", 24.0, Color::WHITE),
                    UiRect::bottom(Val::Px(18.0)),
                ));
                for (key, action) in CONTROL_BINDINGS {
                    p.spawn(Node {
                        width: Val::Px(460.0),
                        justify_content: JustifyContent::SpaceBetween,
                        margin: UiRect::vertical(Val::Px(3.0)),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn(hud_text(&font, key, 15.0, CYAN));
                        row.spawn(hud_text(&font, action, 15.0, Color::WHITE));
                    });
                }
                p.spawn(Node {
                    height: Val::Px(14.0),
                    ..default()
                });
                spawn_button(p, &font, PauseButton::Back);
            });
        });

    commands
        .spawn((
            ControlsBadgeNode,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(24.0),
                bottom: Val::Px(150.0),
                max_width: Val::Px(560.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.04, 0.06, 0.45)),
        ))
        .with_children(|b| {
            b.spawn(hud_text(&font, CONTROLS_BADGE_TEXT, 12.0, MUTED));
        });
}

pub fn toggle_controls_badge(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut badge: ResMut<ControlsBadgeState>,
) {
    if keys.is_some_and(|k| k.just_pressed(KeyCode::F1)) {
        badge.visible = !badge.visible;
    }
}

/// Shows the open page (hidden while the settings window covers it) and
/// the badge (hidden while paused).
pub fn sync_pause_menu_visibility(
    pause: Res<PauseMenuState>,
    overlay: Option<Res<ScreenOverlay>>,
    settings: Res<SettingsModalState>,
    badge: Res<ControlsBadgeState>,
    mut roots: Query<&mut Visibility, (With<PauseMenuRoot>, Without<PausePageNode>)>,
    mut pages: Query<(&PausePageNode, &mut Visibility), Without<PauseMenuRoot>>,
    mut badges: Query<
        &mut Visibility,
        (
            With<ControlsBadgeNode>,
            Without<PauseMenuRoot>,
            Without<PausePageNode>,
        ),
    >,
) {
    for mut v in &mut roots {
        v.set_if_neq(if pause.is_open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    for (page, mut v) in &mut pages {
        v.set_if_neq(visibility(page.0 == pause.page && !settings.is_open));
    }
    for mut v in &mut badges {
        let covered = overlay.as_ref().is_some_and(|o| o.0);
        v.set_if_neq(visibility(badge.visible && !pause.is_open && !covered));
    }
}

pub fn handle_pause_buttons(
    mut buttons: Query<(&Interaction, &PauseButton, &mut BackgroundColor), Changed<Interaction>>,
    mut pause: ResMut<PauseMenuState>,
    mut settings: ResMut<SettingsModalState>,
    mut exit: EventWriter<AppExit>,
    mut sfx: EventWriter<PlaySfxEvent>,
) {
    for (interaction, kind, mut color) in &mut buttons {
        if *interaction == Interaction::Pressed {
            sfx.send(PlaySfxEvent::ui("sfx_ui_click"));
        }
        *color = BackgroundColor(match interaction {
            Interaction::Pressed => BUTTON_PRESS,
            Interaction::Hovered => BUTTON_HOVER,
            Interaction::None => BUTTON_IDLE,
        });
        if *interaction == Interaction::Pressed
            && apply_pause_button(*kind, &mut pause, &mut settings)
        {
            exit.send(AppExit::Success);
        }
    }
}

/// Freezes the simulation while the pause menu (or settings opened from
/// it) is up.
/// Set by full-screen modals outside this crate (the passive tree) that
/// freeze the simulation like the pause menu does.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ModalPause(pub bool);

pub fn pause_simulation_time(
    pause: Res<PauseMenuState>,
    settings: Res<SettingsModalState>,
    modal: Option<Res<ModalPause>>,
    time: Option<ResMut<Time<Virtual>>>,
) {
    let Some(mut time) = time else {
        return;
    };
    let paused = pause.is_open || settings.is_open || modal.is_some_and(|m| m.0);
    if paused && !time.is_paused() {
        time.pause();
    } else if !paused && time.is_paused() {
        time.unpause();
    }
}

/// Gates gameplay input and frees or captures the cursor to match the
/// menus. The cursor is only captured while the window has focus, so
/// clicking back into the window recaptures it.
#[allow(clippy::too_many_arguments)]
pub fn sync_gameplay_focus(
    pause: Res<PauseMenuState>,
    skills: Res<SkillsModalState>,
    inventory: Res<InventoryModalState>,
    settings: Res<SettingsModalState>,
    hold: Option<Res<GameplayHold>>,
    overlay: Option<Res<ScreenOverlay>>,
    mut gate: ResMut<GameplayInputGate>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let menu =
        any_menu_open(&pause, &skills, &inventory, &settings) || overlay.is_some_and(|o| o.0);
    let held = hold.is_some_and(|h| h.0);
    gate.set_if_neq(GameplayInputGate {
        blocked: menu || held,
    });
    for mut window in &mut windows {
        let capture = !menu && window.focused;
        let mode = if capture {
            GAMEPLAY_GRAB_MODE
        } else {
            CursorGrabMode::None
        };
        if window.cursor_options.grab_mode != mode {
            window.cursor_options.grab_mode = mode;
        }
        if window.cursor_options.visible == capture {
            window.cursor_options.visible = !capture;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_drive_the_menu() {
        let mut pause = PauseMenuState::default();
        let mut settings = SettingsModalState::default();
        pause.open();
        assert!(!apply_pause_button(
            PauseButton::Controls,
            &mut pause,
            &mut settings
        ));
        assert_eq!(pause.page, PausePage::Controls);
        apply_pause_button(PauseButton::Back, &mut pause, &mut settings);
        assert_eq!(pause.page, PausePage::Main);
        apply_pause_button(PauseButton::Settings, &mut pause, &mut settings);
        assert!(settings.is_open && pause.is_open);
        apply_pause_button(PauseButton::Resume, &mut pause, &mut settings);
        assert!(!pause.is_open);
        assert!(apply_pause_button(
            PauseButton::Quit,
            &mut pause,
            &mut settings
        ));
    }

    #[test]
    fn badge_lists_every_core_control() {
        for key in ["WASD", "Space", "Shift", "LMB", "Tab", "Esc"] {
            assert!(CONTROLS_BADGE_TEXT.contains(key), "{key}");
        }
        assert!(CONTROL_BINDINGS.iter().any(|(k, _)| *k == "Esc"));
    }
}
