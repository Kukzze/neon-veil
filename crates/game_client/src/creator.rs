//! The operative creator: a lit studio with a live preview of the body and
//! a panel with every body and face slider, presets, colours and the
//! player card. Opens on first launch and from the pause menu.

use crate::character::appearance::{
    ACCENT_COLORS, EYE_COLORS, HAIR_COLORS, SKIN_TONES, SUIT_COLORS,
};
use crate::character::{
    save_appearance, spawn_operative_body, AppearancePath, CharacterAppearance, HairStyle,
    MorphLibrary, OperativeBody, Preset,
};
use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::prepass::{DeferredPrepass, DepthPrepass};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use game_ui::theme::{ACCENT, MUTED, TITLE_FAMILY, WARN};
use game_ui::{PauseMenuState, ScreenOverlay};

/// Where the studio stands, away from the street.
pub const STUDIO_ORIGIN: Vec3 = Vec3::new(0.0, 0.0, 420.0);
const PANEL_WIDTH: f32 = 460.0;
const TAB_STYLE: &str = "Style";

#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum GameFlow {
    #[default]
    Creator,
    Playing,
}

/// Marks everything the studio spawns.
#[derive(Component)]
pub struct StudioPiece;

/// The preview body (excluded when applying the final look to players).
#[derive(Component)]
pub struct StudioBody;

#[derive(Component)]
pub struct StudioPivot;

#[derive(Component)]
pub struct StudioCamera;

#[derive(Resource, Debug, Clone)]
pub struct CreatorState {
    pub draft: CharacterAppearance,
    pub tab: String,
    pub face_view: bool,
    pub yaw: f32,
    pub seed: u64,
}

/// Slider ids in `category`, in library order.
pub fn sliders_in<'a>(
    library: &'a MorphLibrary,
    category: &str,
) -> Vec<&'a crate::character::SliderInfo> {
    library
        .sliders
        .iter()
        .filter(|s| s.category == category)
        .collect()
}

fn swatch(ui: &mut egui::Ui, color: [f32; 3], selected: bool) -> bool {
    let fill = egui::Color32::from_rgb(
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
    );
    let stroke = if selected {
        egui::Stroke::new(2.0, ACCENT)
    } else {
        egui::Stroke::new(1.0, egui::Color32::from_gray(40))
    };
    ui.add(
        egui::Button::new("")
            .fill(fill)
            .stroke(stroke)
            .min_size(egui::vec2(30.0, 30.0)),
    )
    .clicked()
}

fn swatch_row(ui: &mut egui::Ui, label: &str, palette: &[[f32; 3]], value: &mut [f32; 3]) {
    ui.label(egui::RichText::new(label).color(MUTED).size(12.0));
    ui.horizontal_wrapped(|ui| {
        for c in palette {
            if swatch(ui, *c, value == c) {
                *value = *c;
            }
        }
    });
    ui.add_space(6.0);
}

pub fn enter_creator(
    mut commands: Commands,
    assets: Res<AssetServer>,
    look: Option<Res<CharacterAppearance>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cameras: Query<&mut Camera, Without<StudioCamera>>,
    mut overlay: ResMut<ScreenOverlay>,
) {
    let draft = look.as_deref().cloned().unwrap_or_default();
    for mut camera in &mut cameras {
        camera.is_active = false;
    }
    overlay.0 = true;
    let o = STUDIO_ORIGIN;
    let floor = materials.add(StandardMaterial {
        base_color: Color::srgb(0.035, 0.04, 0.05),
        perceptual_roughness: 0.35,
        ..default()
    });
    let backdrop = materials.add(StandardMaterial {
        base_color: Color::srgb(0.02, 0.025, 0.035),
        perceptual_roughness: 0.9,
        ..default()
    });
    let ring = materials.add(StandardMaterial {
        base_color: Color::srgb(0.0, 0.2, 0.25),
        emissive: LinearRgba::rgb(0.0, 2.2, 3.0),
        ..default()
    });
    commands.spawn((
        StudioPiece,
        Mesh3d(meshes.add(Cylinder::new(1.6, 0.06))),
        MeshMaterial3d(floor),
        Transform::from_translation(o + Vec3::Y * -0.03),
    ));
    commands.spawn((
        StudioPiece,
        Mesh3d(meshes.add(Torus::new(1.52, 1.58))),
        MeshMaterial3d(ring),
        Transform::from_translation(o + Vec3::Y * 0.005).with_scale(Vec3::new(1.0, 0.2, 1.0)),
        NotShadowCaster,
    ));
    commands.spawn((
        StudioPiece,
        Mesh3d(meshes.add(Cylinder::new(9.0, 12.0))),
        MeshMaterial3d(backdrop.clone()),
        Transform::from_translation(o + Vec3::Y * 5.0).with_scale(Vec3::new(-1.0, 1.0, -1.0)),
    ));
    // Key, two neon rims and a soft fill.
    commands.spawn((
        StudioPiece,
        SpotLight {
            color: Color::srgb(1.0, 0.95, 0.9),
            intensity: 1_200_000.0,
            range: 14.0,
            outer_angle: 0.6,
            inner_angle: 0.3,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(o + Vec3::new(1.8, 3.2, 3.0)).looking_at(o + Vec3::Y, Vec3::Y),
    ));
    for (offset, color) in [
        (Vec3::new(-2.2, 2.0, -1.8), Color::srgb(0.0, 0.85, 1.0)),
        (Vec3::new(2.4, 1.6, -1.6), Color::srgb(1.0, 0.2, 0.6)),
    ] {
        commands.spawn((
            StudioPiece,
            PointLight {
                color,
                intensity: 220_000.0,
                range: 8.0,
                ..default()
            },
            Transform::from_translation(o + offset),
        ));
    }
    commands.spawn((
        StudioPiece,
        PointLight {
            color: Color::srgb(0.7, 0.8, 1.0),
            intensity: 70_000.0,
            range: 9.0,
            ..default()
        },
        Transform::from_translation(o + Vec3::new(-1.5, 1.4, 3.5)),
    ));
    let pivot = commands
        .spawn((
            StudioPiece,
            StudioPivot,
            Transform::from_translation(o)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            Visibility::default(),
        ))
        .id();
    let body = spawn_operative_body(&mut commands, &assets, pivot, draft.clone());
    commands.entity(body).insert(StudioBody);
    commands.spawn((
        StudioPiece,
        StudioCamera,
        Camera3d::default(),
        Camera {
            hdr: true,
            order: 2,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.012, 0.016, 0.024)),
            ..default()
        },
        // The game renders deferred; this camera needs the same prepasses.
        Msaa::Off,
        DepthPrepass,
        DeferredPrepass,
        Tonemapping::AgX,
        Bloom {
            intensity: 0.1,
            ..Bloom::NATURAL
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 30f32.to_radians(),
            ..default()
        }),
        Transform::from_translation(o + Vec3::new(0.0, 1.0, 4.4)),
    ));
    commands.insert_resource(CreatorState {
        draft,
        tab: "Body".into(),
        face_view: false,
        yaw: 0.0,
        seed: 0x5eed,
    });
}

pub fn exit_creator(
    mut commands: Commands,
    pieces: Query<Entity, With<StudioPiece>>,
    mut cameras: Query<&mut Camera, Without<StudioCamera>>,
    mut overlay: ResMut<ScreenOverlay>,
) {
    for entity in &pieces {
        commands.entity(entity).despawn_recursive();
    }
    for mut camera in &mut cameras {
        camera.is_active = true;
    }
    overlay.0 = false;
    commands.remove_resource::<CreatorState>();
}

/// Frames the camera on the body (or face), offset so it sits right of
/// the panel, and turns the preview with drags outside the panel.
pub fn frame_studio(
    time: Res<Time>,
    state: Option<ResMut<CreatorState>>,
    mut contexts: EguiContexts,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut pivots: Query<&mut Transform, (With<StudioPivot>, Without<StudioCamera>)>,
    mut cameras: Query<&mut Transform, With<StudioCamera>>,
) {
    let Some(mut state) = state else {
        return;
    };
    let over_panel = contexts
        .try_ctx_mut()
        .is_some_and(|c| c.wants_pointer_input());
    if buttons.pressed(MouseButton::Left) && !over_panel {
        state.yaw += motion.delta.x * 0.01;
    }
    for mut pivot in &mut pivots {
        pivot.rotation = Quat::from_rotation_y(std::f32::consts::PI + state.yaw);
    }
    let height = 1.0 + 0.12 * state.draft.slider("height");
    let (eye, target) = if state.face_view {
        (
            Vec3::new(-0.05, 1.62 * height, 1.15),
            Vec3::new(-0.16, 1.58 * height, 0.0),
        )
    } else {
        (Vec3::new(0.0, 1.05, 5.2), Vec3::new(-0.62, 0.95, 0.0))
    };
    let t = 1.0 - (-8.0 * time.delta_secs()).exp();
    for mut camera in &mut cameras {
        let wanted = Transform::from_translation(STUDIO_ORIGIN + eye)
            .looking_at(STUDIO_ORIGIN + target, Vec3::Y);
        camera.translation = camera.translation.lerp(wanted.translation, t);
        camera.rotation = camera.rotation.slerp(wanted.rotation, t);
    }
}

/// Keeps the preview body on the draft look.
pub fn sync_preview(
    state: Option<Res<CreatorState>>,
    mut previews: Query<&mut CharacterAppearance, With<StudioBody>>,
) {
    let Some(state) = state else {
        return;
    };
    if !state.is_changed() {
        return;
    }
    for mut look in &mut previews {
        if *look != state.draft {
            *look = state.draft.clone();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn creator_panel(
    mut contexts: EguiContexts,
    library: Option<Res<MorphLibrary>>,
    state: Option<ResMut<CreatorState>>,
    path: Res<AppearancePath>,
    mut players: Query<&mut CharacterAppearance, (With<OperativeBody>, Without<StudioBody>)>,
    mut commands: Commands,
    mut next: ResMut<NextState<GameFlow>>,
) {
    let (Some(library), Some(mut state)) = (library, state) else {
        return;
    };
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    // Fonts set this frame only apply on the next; fall back until then.
    let title_family = egui::FontFamily::Name(TITLE_FAMILY.into());
    let title_family = if ctx.fonts(|f| f.families().contains(&title_family)) {
        title_family
    } else {
        egui::FontFamily::Proportional
    };
    let mut confirm = false;
    let mut draft = state.draft.clone();
    let mut tab = state.tab.clone();
    let mut face_view = state.face_view;
    let mut seed = state.seed;
    egui::SidePanel::left("operative_creator")
        .exact_width(PANEL_WIDTH)
        .resizable(false)
        .show(ctx, |ui| {
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new("NEON VEIL // OPERATIVE PROFILE")
                    .color(WARN)
                    .size(12.0),
            );
            ui.label(
                egui::RichText::new("Build your operative")
                    .family(title_family.clone())
                    .size(34.0),
            );
            ui.add_space(8.0);
            egui::Grid::new("card")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("CALLSIGN").color(MUTED).size(12.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.card.callsign)
                            .char_limit(16)
                            .desired_width(260.0),
                    );
                    ui.end_row();
                    ui.label(egui::RichText::new("TITLE").color(MUTED).size(12.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.card.title)
                            .char_limit(24)
                            .desired_width(260.0),
                    );
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for preset in Preset::ALL {
                    if ui.button(preset.label()).clicked() {
                        let card = draft.card.clone();
                        draft = CharacterAppearance::preset(preset);
                        draft.card = card;
                    }
                }
                if ui.button("Randomize").clicked() {
                    seed = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let ids: Vec<&str> = library.sliders.iter().map(|s| s.id.as_str()).collect();
                    draft.randomize(&ids, seed);
                }
                if ui.button("Reset").clicked() {
                    draft.sliders.clear();
                }
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for category in library.categories().into_iter().chain([TAB_STYLE]) {
                    ui.selectable_value(&mut tab, category.to_owned(), category);
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 110.0)
                .show(ui, |ui| {
                    if tab == TAB_STYLE {
                        swatch_row(ui, "SKIN TONE", &SKIN_TONES, &mut draft.skin_tone);
                        swatch_row(ui, "EYES", &EYE_COLORS, &mut draft.eye_color);
                        ui.label(egui::RichText::new("HAIR").color(MUTED).size(12.0));
                        ui.horizontal(|ui| {
                            for style in HairStyle::ALL {
                                ui.selectable_value(&mut draft.hair, style, style.label());
                            }
                        });
                        swatch_row(ui, "HAIR COLOUR", &HAIR_COLORS, &mut draft.hair_color);
                        swatch_row(ui, "SUIT", &SUIT_COLORS, &mut draft.suit);
                        swatch_row(ui, "ACCENT", &ACCENT_COLORS, &mut draft.accent);
                    } else {
                        for slider in sliders_in(&library, &tab) {
                            let mut value = draft.slider(&slider.id);
                            let bidirectional = slider.low.is_some();
                            let range = if bidirectional { -1.0..=1.0 } else { 0.0..=1.0 };
                            ui.label(egui::RichText::new(&slider.label).size(13.0));
                            let response = ui.add(
                                egui::Slider::new(&mut value, range)
                                    .show_value(false)
                                    .trailing_fill(true),
                            );
                            if response.double_clicked() {
                                value = 0.0;
                            }
                            if response.changed() || response.double_clicked() {
                                draft.set_slider(&slider.id, value);
                            }
                        }
                        ui.label(
                            egui::RichText::new("Double-click a slider to reset it.")
                                .color(MUTED)
                                .size(11.0),
                        );
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                ui.selectable_value(&mut face_view, false, "Full body");
                ui.selectable_value(&mut face_view, true, "Face");
                ui.label(
                    egui::RichText::new("Drag the preview to turn")
                        .color(MUTED)
                        .size(11.0),
                );
            });
            ui.add_space(6.0);
            let confirm_button = egui::Button::new(
                egui::RichText::new("CONFIRM OPERATIVE")
                    .size(18.0)
                    .color(egui::Color32::BLACK),
            )
            .fill(ACCENT)
            .min_size(egui::vec2(PANEL_WIDTH - 24.0, 44.0));
            if ui.add(confirm_button).clicked() {
                confirm = true;
            }
        });

    if draft != state.draft {
        state.draft = draft;
    }
    if tab != state.tab {
        state.tab = tab;
    }
    if face_view != state.face_view {
        state.face_view = face_view;
    }
    state.seed = seed;

    if confirm {
        let final_look = state.draft.clone().sanitized();
        if let Err(error) = save_appearance(&path.0, &final_look) {
            warn!("could not save the operative: {error}");
        }
        for mut look in &mut players {
            *look = final_look.clone();
        }
        commands.insert_resource(final_look);
        next.set(GameFlow::Playing);
    }
}

/// Opens the creator when the pause menu asks for it.
pub fn open_from_pause(mut pause: ResMut<PauseMenuState>, mut next: ResMut<NextState<GameFlow>>) {
    if pause.profile_requested {
        pause.profile_requested = false;
        next.set(GameFlow::Creator);
    }
}

/// Shows the operative's callsign on the HUD.
pub fn sync_callsign(
    look: Option<Res<CharacterAppearance>>,
    mut name: ResMut<game_ui::OperativeName>,
) {
    if let Some(look) = look.filter(|l| l.is_changed()) {
        if name.0 != look.card.callsign {
            name.0.clone_from(&look.card.callsign);
        }
    }
}

pub struct CreatorPlugin {
    pub start: GameFlow,
}

impl Plugin for CreatorPlugin {
    fn build(&self, app: &mut App) {
        app.insert_state(self.start)
            .add_systems(OnEnter(GameFlow::Creator), enter_creator)
            .add_systems(OnExit(GameFlow::Creator), exit_creator)
            .add_systems(
                Update,
                (
                    (creator_panel, sync_preview, frame_studio)
                        .chain()
                        .run_if(in_state(GameFlow::Creator)),
                    open_from_pause.run_if(in_state(GameFlow::Playing)),
                    sync_callsign,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_library_category_has_sliders() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let library = MorphLibrary::load(&root).expect("library");
        let categories = library.categories();
        for wanted in [
            "Body", "Torso", "Chest", "Arms", "Hands", "Legs", "Head", "Nose", "Eyes",
        ] {
            assert!(categories.contains(&wanted), "{wanted}");
            assert!(!sliders_in(&library, wanted).is_empty());
        }
    }
}
