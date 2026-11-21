//! HUD, menus and in-game UI.
//!
//! The HUD is Bevy UI; the skills, inventory and settings panels are egui
//! windows that only draw when an egui context exists, so every system here
//! also runs headless.

pub mod hud;
pub mod inventory_ui;
pub mod settings_ui;
pub mod skills_ui;

pub use hud::{HudFont, HudObjective, HudRootNode};
pub use inventory_ui::{InventoryGrid, InventoryModalState, InventorySlotData};
pub use settings_ui::{SettingsModalState, SettingsTab, UserSettingsConfig, UserSettingsPath};
pub use skills_ui::{SkillRow, SkillsModalState, SkillsSummary};

use bevy::prelude::*;
use bevy_egui::EguiContext;
use game_combat::{MeleeHitEvent, RangedHitEvent};
use game_input::{LocalPlayerInput, PlayerAction};
use leafwing_input_manager::prelude::ActionState;

/// Panel toggle requests for one frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiToggleInput {
    pub skills: bool,
    pub inventory: bool,
    pub escape: bool,
}

/// K and Tab toggle their panels. Escape backs out: it closes the skills
/// or inventory panel if one is open, and otherwise toggles settings.
pub fn apply_ui_toggles(
    input: UiToggleInput,
    skills: &mut SkillsModalState,
    inventory: &mut InventoryModalState,
    settings: &mut SettingsModalState,
) {
    if input.skills {
        skills.toggle();
    }
    if input.inventory {
        inventory.toggle();
    }
    if input.escape {
        if skills.is_open || inventory.is_open {
            skills.is_open = false;
            if inventory.is_open {
                inventory.toggle();
            }
        } else {
            settings.toggle();
        }
    }
}

/// Reads the local player's rebindable actions (keyboard or gamepad) when
/// present, otherwise the raw K / Tab keys; Escape is always the raw key.
pub fn handle_ui_keyboard_toggles(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    actions: Query<&ActionState<PlayerAction>, With<LocalPlayerInput>>,
    mut skills: ResMut<SkillsModalState>,
    mut inventory: ResMut<InventoryModalState>,
    mut settings: ResMut<SettingsModalState>,
) {
    let pressed = |key| keys.as_ref().is_some_and(|k| k.just_pressed(key));
    let input = match actions.iter().next() {
        Some(action) => UiToggleInput {
            skills: action.just_pressed(&PlayerAction::OpenSkills),
            inventory: action.just_pressed(&PlayerAction::OpenInventory),
            escape: pressed(KeyCode::Escape),
        },
        None => UiToggleInput {
            skills: pressed(KeyCode::KeyK),
            inventory: pressed(KeyCode::Tab),
            escape: pressed(KeyCode::Escape),
        },
    };
    if input != UiToggleInput::default() {
        apply_ui_toggles(input, &mut skills, &mut inventory, &mut settings);
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        let has_egui = any_with_component::<EguiContext>;
        app.init_resource::<SkillsModalState>()
            .init_resource::<InventoryModalState>()
            .init_resource::<SettingsModalState>()
            .init_resource::<UserSettingsConfig>()
            .init_resource::<UserSettingsPath>()
            .init_resource::<HudObjective>()
            .add_event::<RangedHitEvent>()
            .add_event::<MeleeHitEvent>()
            .add_systems(
                Startup,
                (hud::setup_cyberpunk_hud, settings_ui::load_user_settings),
            )
            .add_systems(
                Update,
                (
                    inventory_ui::sync_inventory_grids,
                    hud::update_hud_vitals,
                    hud::update_ammo_counter,
                    hud::update_compass_strip,
                    hud::update_crosshair_bloom,
                    hud::update_hit_markers,
                    (
                        handle_ui_keyboard_toggles,
                        skills_ui::render_skills_modal_egui.run_if(has_egui),
                        inventory_ui::render_inventory_egui.run_if(has_egui),
                        settings_ui::render_settings_menu_egui.run_if(has_egui),
                        settings_ui::apply_user_settings,
                    )
                        .chain(),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use game_audio::AudioMixerSettings;
    use game_combat::{BallisticProfile, RangedWeaponArchetype};
    use game_core::identifiers::PlayerCharacterMarker;
    use game_core::input::ViewAngles;
    use game_parkour::{FlowState, KinematicVelocity};
    use game_rpg::{Health, Shields};
    use hud::{
        AmmoCounterUi, CompassStripUi, CrosshairUi, FlowMeterUi, HealthShieldBarUi, HitMarkerUi,
        HudBar, HudBarKind,
    };
    use std::f32::consts::FRAC_PI_2;
    use std::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                1.0 / 60.0,
            )))
            .insert_resource(UserSettingsPath(
                std::env::temp_dir().join("neon_veil_ui_missing_settings.json"),
            ))
            .init_resource::<AudioMixerSettings>()
            .add_plugins(UiPlugin);
        app.update();
        app
    }

    fn text<M: Component>(app: &mut App) -> String {
        let world = app.world_mut();
        world
            .query_filtered::<&Text, With<M>>()
            .single(world)
            .0
            .clone()
    }

    fn spawn_player(app: &mut App) -> Entity {
        let mut health = Health::new(100.0);
        health.current = 64.5;
        let shields = Shields {
            current: 30.0,
            ..default()
        };
        let mut rifle = BallisticProfile::for_archetype(RangedWeaponArchetype::PulseAssaultRifle);
        rifle.current_ammo = 24;
        rifle.reserve_ammo = 90;
        rifle.current_spread_degrees = 2.0;
        app.world_mut()
            .spawn((
                PlayerCharacterMarker,
                health,
                shields,
                FlowState {
                    current_flow: 80.0,
                    ..default()
                },
                ViewAngles::new(FRAC_PI_2, 0.0),
                rifle,
                KinematicVelocity(Vec3::new(10.0, 0.0, 0.0)),
                Transform::default(),
            ))
            .id()
    }

    #[test]
    fn hud_tree_spawns_once() {
        let mut app = app();
        let world = app.world_mut();
        assert_eq!(world.query::<&HudRootNode>().iter(world).count(), 1);
        assert_eq!(world.query::<&HudBar>().iter(world).count(), 4);
        assert_eq!(world.query::<&hud::CrosshairArm>().iter(world).count(), 4);
        assert_eq!(text::<AmmoCounterUi>(&mut app), "-- // -- [MELEE]");
    }

    #[test]
    fn hud_reflects_the_player() {
        let mut app = app();
        spawn_player(&mut app);
        app.world_mut().resource_mut::<HudObjective>().label = "OBJECTIVE: CLINIC EXIT".into();
        app.world_mut().resource_mut::<HudObjective>().target = Some(Vec3::new(0.0, 0.0, -120.0));
        for _ in 0..90 {
            app.update();
        }
        let shield_max = Shields::default().max as u32;
        assert_eq!(
            text::<HealthShieldBarUi>(&mut app),
            format!("SHIELDS 30 / {shield_max}  |  HP 65 / 100")
        );
        assert_eq!(text::<FlowMeterUi>(&mut app), "FLOW  80% // Overdrive 125%");
        assert_eq!(text::<AmmoCounterUi>(&mut app), "24 // 90 [AUTO]");
        assert_eq!(
            text::<CompassStripUi>(&mut app),
            "W 270\u{b0} // OBJECTIVE: CLINIC EXIT [120m]"
        );

        let world = app.world_mut();
        let bars: Vec<(HudBarKind, f32)> = world
            .query::<&HudBar>()
            .iter(world)
            .map(|b| (b.kind, b.displayed))
            .collect();
        let health = bars.iter().find(|b| b.0 == HudBarKind::Health).unwrap().1;
        assert!((health - 0.645).abs() < 1e-3, "{health}");
        let flow = bars.iter().find(|b| b.0 == HudBarKind::Flow).unwrap().1;
        assert!((flow - 0.8).abs() < 1e-3);
        let gap = world.query::<&CrosshairUi>().single(world).gap_px;
        assert!(
            (gap - hud::crosshair_gap_px(2.0, 10.0)).abs() < 0.05,
            "{gap}"
        );
    }

    #[test]
    fn bars_ease_instead_of_snapping() {
        let mut app = app();
        let player = spawn_player(&mut app);
        for _ in 0..90 {
            app.update();
        }
        app.world_mut().get_mut::<Health>(player).unwrap().current = 0.0;
        app.update();
        let world = app.world_mut();
        let health = world
            .query::<&HudBar>()
            .iter(world)
            .find(|b| b.kind == HudBarKind::Health)
            .unwrap()
            .displayed;
        assert!(
            health > 0.4 && health < 0.645,
            "one frame later the bar is on its way: {health}"
        );
    }

    #[test]
    fn hit_markers_flash_and_fade() {
        let mut app = app();
        let player = spawn_player(&mut app);
        let target = app.world_mut().spawn_empty().id();
        app.world_mut().send_event(RangedHitEvent {
            shooter: player,
            target,
            damage: 30.0,
            headshot: true,
            status_applied: None,
            deflected: false,
        });
        app.update();
        let marker = |app: &mut App| {
            let world = app.world_mut();
            let (m, v) = world.query::<(&HitMarkerUi, &Visibility)>().single(world);
            (*m, *v)
        };
        let (m, v) = marker(&mut app);
        assert!(m.critical && m.fade_timer > 0.2);
        assert_eq!(v, Visibility::Inherited);
        for _ in 0..30 {
            app.update();
        }
        let (m, v) = marker(&mut app);
        assert_eq!((m.fade_timer, v), (0.0, Visibility::Hidden));
    }

    #[test]
    fn hotkeys_toggle_panels() {
        let mut app = app();
        app.init_resource::<ButtonInput<KeyCode>>();
        let tap = |app: &mut App, key: KeyCode| {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            app.update();
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(key);
            keys.clear();
        };
        tap(&mut app, KeyCode::KeyK);
        assert!(app.world().resource::<SkillsModalState>().is_open);
        tap(&mut app, KeyCode::Tab);
        assert!(app.world().resource::<InventoryModalState>().is_open);
        tap(&mut app, KeyCode::Escape);
        assert!(
            !app.world().resource::<SkillsModalState>().is_open,
            "escape backs out first"
        );
        assert!(!app.world().resource::<InventoryModalState>().is_open);
        assert!(!app.world().resource::<SettingsModalState>().is_open);
        tap(&mut app, KeyCode::Escape);
        assert!(app.world().resource::<SettingsModalState>().is_open);
        tap(&mut app, KeyCode::Escape);
        assert!(!app.world().resource::<SettingsModalState>().is_open);
    }

    #[test]
    fn toggle_rules() {
        let (mut s, mut i, mut c) = (
            SkillsModalState::default(),
            InventoryModalState::default(),
            SettingsModalState::default(),
        );
        let press = |skills, inventory, escape| UiToggleInput {
            skills,
            inventory,
            escape,
        };
        apply_ui_toggles(press(true, false, false), &mut s, &mut i, &mut c);
        assert!(s.is_open);
        apply_ui_toggles(press(true, false, false), &mut s, &mut i, &mut c);
        assert!(!s.is_open);
        apply_ui_toggles(press(false, false, true), &mut s, &mut i, &mut c);
        assert!(c.is_open);
        apply_ui_toggles(press(false, true, false), &mut s, &mut i, &mut c);
        assert!(i.is_open && c.is_open);
    }

    #[test]
    fn settings_changes_reach_the_mixer() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<UserSettingsConfig>()
            .audio
            .master_volume = 0.4;
        app.update();
        assert_eq!(
            app.world().resource::<AudioMixerSettings>().master_volume,
            0.4
        );
    }
}
