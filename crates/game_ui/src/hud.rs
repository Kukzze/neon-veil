//! In-game HUD: vitals, flow meter, ammo, compass, crosshair bloom and hit
//! markers.
//!
//! Text is rewritten in place (the existing `String` buffer is reused) and
//! only when a displayed value changes, so a steady HUD costs no
//! allocations and triggers no re-layout. Bars ease toward their values.

use bevy::prelude::*;
use game_combat::{
    AttackPhase, BallisticProfile, MeleeCombatState, MeleeHitEvent, RangedHitEvent,
    RangedWeaponArchetype,
};
use game_core::identifiers::PlayerCharacterMarker;
use game_core::input::ViewAngles;
use game_parkour::flow_state::{FlowTier, FLOW_MAX, FLOW_OVERDRIVE_MULTIPLIER};
use game_parkour::{FlowState, KinematicVelocity, ParkourPhysicsConstants};
use game_rpg::{Health, Shields};
use std::fmt::Write;

pub const CYAN: Color = Color::srgb(0.0, 229.0 / 255.0, 1.0);
pub const EMERALD: Color = Color::srgb(46.0 / 255.0, 204.0 / 255.0, 113.0 / 255.0);
pub const AMBER: Color = Color::srgb(1.0, 170.0 / 255.0, 0.0);
pub const PANEL: Color = Color::srgba(0.02, 0.035, 0.055, 0.78);
/// Opaque panel tone, used to cut segment ticks into bars.
pub const PANEL_SOLID: Color = Color::srgb(0.02, 0.035, 0.055);
pub const BAR_TRACK: Color = Color::srgba(1.0, 1.0, 1.0, 0.07);
pub const TEXT: Color = Color::srgba(0.93, 0.96, 1.0, 0.95);
pub const MUTED: Color = Color::srgba(0.75, 0.84, 0.95, 0.55);
pub const HAIRLINE: Color = Color::srgba(0.93, 0.96, 1.0, 0.1);
pub const DANGER: Color = Color::srgb(1.0, 0.27, 0.33);
const HEALTH_OK: Color = Color::srgb(0.9, 0.95, 1.0);
/// Damage trail behind shield and health fills.
const TRAIL: Color = Color::srgba(1.0, 0.85, 0.75, 0.55);
/// Health fraction below which the bar turns red.
pub const LOW_HEALTH_FRACTION: f32 = 0.3;
const VITAL_BAR_WIDTH: f32 = 230.0;
const COMPASS_WIDTH: f32 = 460.0;
/// Characters in the compass tape; each covers [`COMPASS_DEGREES_PER_CHAR`].
pub const COMPASS_TAPE_CHARS: usize = 45;
pub const COMPASS_DEGREES_PER_CHAR: i32 = 5;
/// Damage trails catch up slower than fills, 1/s.
const TRAIL_EASE_RATE: f32 = 3.0;

/// Bar easing rate, 1/s.
const BAR_EASE_RATE: f32 = 12.0;
/// Crosshair easing rate, 1/s.
const CROSSHAIR_EASE_RATE: f32 = 20.0;
pub const CROSSHAIR_MIN_GAP_PX: f32 = 6.0;
pub const CROSSHAIR_MAX_GAP_PX: f32 = 64.0;
const CROSSHAIR_PX_PER_SPREAD_DEGREE: f32 = 6.0;
const CROSSHAIR_PX_PER_SPEED: f32 = 0.6;
const CROSSHAIR_ARM_LENGTH_PX: f32 = 10.0;
const CROSSHAIR_ARM_THICKNESS_PX: f32 = 2.0;
pub const HIT_MARKER_SECONDS: f32 = 0.25;

#[derive(Component, Debug, Default)]
pub struct HudRootNode;

/// Shield and health readout.
#[derive(Component, Debug, Default)]
pub struct HealthShieldBarUi;

#[derive(Component, Debug, Default)]
pub struct FlowMeterUi;

#[derive(Component, Debug, Default)]
pub struct AmmoCounterUi;

#[derive(Component, Debug, Default)]
pub struct CompassStripUi;

/// The player's callsign, shown on the vitals card.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct OperativeName(pub String);

impl Default for OperativeName {
    fn default() -> Self {
        Self("Operative".into())
    }
}

#[derive(Component, Debug, Default)]
pub struct OperativeNameUi;

#[derive(Component, Debug, Default)]
pub struct CompassTapeUi;

#[derive(Component, Debug, Default)]
pub struct HealthValueUi;

#[derive(Component, Debug, Default)]
pub struct WeaponNameUi;

#[derive(Component, Debug, Default)]
pub struct FireModeUi;

/// One of the three melee combo indicators.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComboPip(pub u8);

/// Crosshair container; `gap_px` is the current arm distance from center.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CrosshairUi {
    pub gap_px: f32,
}

/// One crosshair arm; `axis` points from the center toward the arm.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CrosshairArm {
    pub axis: IVec2,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct HitMarkerUi {
    pub fade_timer: f32,
    pub critical: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudBarKind {
    Shield,
    Health,
    Stamina,
    Flow,
    ShieldTrail,
    HealthTrail,
}

/// A fill bar; `displayed` eases toward the live fraction.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct HudBar {
    pub kind: HudBarKind,
    pub displayed: f32,
}

/// The tracked objective shown on the compass strip.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HudObjective {
    pub label: String,
    pub target: Option<Vec3>,
}

pub fn fill_fraction(current: f32, max: f32) -> f32 {
    if max > 0.0 && current.is_finite() {
        (current / max).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Rounded up so a sliver of health never reads as zero.
fn display_value(value: f32) -> u32 {
    if value.is_finite() && value > 0.0 {
        value.ceil() as u32
    } else {
        0
    }
}

pub fn write_vital_value(out: &mut String, value: f32) {
    out.clear();
    let _ = write!(out, "{}", display_value(value));
}

pub fn flow_tier_label(out: &mut String, tier: FlowTier, overdrive_multiplier: f32) {
    match tier {
        FlowTier::Base => out.push_str("base"),
        FlowTier::Tier1 => out.push_str("tier 1"),
        FlowTier::Tier2 => out.push_str("tier 2"),
        FlowTier::Overdrive => {
            let _ = write!(out, "overdrive {:.0}%", overdrive_multiplier * 100.0);
        }
    }
}

pub fn write_flow_text(out: &mut String, flow: f32, overdrive_multiplier: f32) {
    out.clear();
    let percent = (fill_fraction(flow, FLOW_MAX) * 100.0).round() as u32;
    let _ = write!(out, "Flow {percent:>3}% · ");
    flow_tier_label(out, FlowTier::from_flow(flow), overdrive_multiplier);
}

pub fn fire_mode_label(archetype: RangedWeaponArchetype) -> &'static str {
    match archetype {
        RangedWeaponArchetype::KineticPistol | RangedWeaponArchetype::TechSniperRifle => "Semi",
        RangedWeaponArchetype::SmartSubmachineGun | RangedWeaponArchetype::PulseAssaultRifle => {
            "Auto"
        }
        RangedWeaponArchetype::PlasmaShotgun => "Pump",
        RangedWeaponArchetype::ChargeRailgun => "Charge",
    }
}

pub fn write_ammo_text(out: &mut String, weapon: Option<&BallisticProfile>) {
    out.clear();
    match weapon {
        Some(w) if w.reload_timer > 0.0 => out.push_str("Reloading"),
        Some(w) => {
            let _ = write!(out, "{} / {}", w.current_ammo, w.reserve_ammo);
        }
        None => out.push_str("3-hit combo"),
    }
}

/// Name shown on the weapon card.
pub fn weapon_display_name(weapon: Option<&BallisticProfile>) -> &'static str {
    match weapon.map(|w| w.archetype) {
        None => "Cyber-katana",
        Some(RangedWeaponArchetype::KineticPistol) => "Kinetic pistol",
        Some(RangedWeaponArchetype::SmartSubmachineGun) => "Smart SMG",
        Some(RangedWeaponArchetype::PulseAssaultRifle) => "Pulse rifle",
        Some(RangedWeaponArchetype::PlasmaShotgun) => "Plasma shotgun",
        Some(RangedWeaponArchetype::TechSniperRifle) => "Tech sniper",
        Some(RangedWeaponArchetype::ChargeRailgun) => "Charge railgun",
    }
}

/// Clockwise compass heading in whole degrees (0..=359) for a view yaw.
///
/// Yaw follows the engine convention: 0 faces -Z (north) and positive yaw
/// turns left (counter-clockwise from above; mouse right decreases it).
/// A compass reads clockwise, so the heading is the negated yaw: turning
/// right goes N -> E -> S -> W.
pub fn compass_heading_degrees(yaw: f32) -> u32 {
    if !yaw.is_finite() {
        return 0;
    }
    ((-yaw.to_degrees()).rem_euclid(360.0).round() as u32) % 360
}

pub fn cardinal_direction(heading_degrees: u32) -> &'static str {
    const NAMES: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    NAMES[(((heading_degrees % 360) as f32 + 22.5) / 45.0) as usize % 8]
}

/// A scrolling compass tape centred on `heading_degrees`: cardinal and
/// intercardinal labels every 45 degrees, ticks every 15.
pub fn write_compass_tape(out: &mut String, heading_degrees: u32) {
    const LABELS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let mut cells = [' '; COMPASS_TAPE_CHARS];
    let half = (COMPASS_TAPE_CHARS / 2) as i32;
    // The tape scrolls one character (5 degrees) at a time.
    let step = COMPASS_DEGREES_PER_CHAR;
    let heading = ((heading_degrees as i32 + step / 2) / step) * step;
    for (i, cell) in cells.iter_mut().enumerate() {
        let angle = (heading + (i as i32 - half) * COMPASS_DEGREES_PER_CHAR).rem_euclid(360);
        if angle % 15 == 0 && angle % 45 != 0 {
            *cell = '\u{b7}';
        }
    }
    for i in 0..COMPASS_TAPE_CHARS {
        let angle = (heading + (i as i32 - half) * COMPASS_DEGREES_PER_CHAR).rem_euclid(360);
        if angle % 45 == 0 {
            let label = LABELS[(angle / 45) as usize];
            for (k, ch) in label.chars().enumerate() {
                if let Some(cell) = cells.get_mut(i + k) {
                    *cell = ch;
                }
            }
        }
    }
    out.clear();
    out.extend(cells);
}

pub fn write_compass_text(out: &mut String, yaw: f32, objective: Option<(&str, f32)>) {
    out.clear();
    let heading = compass_heading_degrees(yaw);
    let _ = write!(out, "{} {heading:03}\u{b0}", cardinal_direction(heading));
    if let Some((label, distance)) = objective {
        let _ = write!(out, " // {label} [{:.0}m]", distance.max(0.0));
    }
}

/// Crosshair arm distance for a weapon spread and movement speed.
pub fn crosshair_gap_px(spread_degrees: f32, horizontal_speed: f32) -> f32 {
    let spread = if spread_degrees.is_finite() {
        spread_degrees.max(0.0)
    } else {
        0.0
    };
    let speed = if horizontal_speed.is_finite() {
        horizontal_speed.max(0.0)
    } else {
        0.0
    };
    (CROSSHAIR_MIN_GAP_PX
        + spread * CROSSHAIR_PX_PER_SPREAD_DEGREE
        + speed * CROSSHAIR_PX_PER_SPEED)
        .clamp(CROSSHAIR_MIN_GAP_PX, CROSSHAIR_MAX_GAP_PX)
}

/// Top-left offset of an arm relative to the crosshair center.
pub fn crosshair_arm_offset(axis: IVec2, gap: f32) -> Vec2 {
    let (len, thick) = (CROSSHAIR_ARM_LENGTH_PX, CROSSHAIR_ARM_THICKNESS_PX);
    match (axis.x.signum(), axis.y.signum()) {
        (1, _) => Vec2::new(gap, -thick * 0.5),
        (-1, _) => Vec2::new(-gap - len, -thick * 0.5),
        (_, 1) => Vec2::new(-thick * 0.5, gap),
        _ => Vec2::new(-thick * 0.5, -gap - len),
    }
}

fn ease_factor(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt.max(0.0)).exp()
}

pub(crate) fn hud_text(font: &Handle<Font>, text: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font: font.clone(),
            font_size: size,
            ..default()
        },
        TextColor(color),
    )
}

/// One labelled vital: label, segmented bar with a damage trail, value.
fn vital_row(
    parent: &mut ChildBuilder,
    font: &Handle<Font>,
    label: &str,
    fill: (HudBarKind, Color),
    trail: HudBarKind,
    value: impl Bundle,
) {
    parent
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            margin: UiRect::vertical(Val::Px(3.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                hud_text(font, label, 11.0, MUTED),
                Node {
                    width: Val::Px(52.0),
                    ..default()
                },
            ));
            row.spawn((
                Node {
                    width: Val::Px(VITAL_BAR_WIDTH),
                    height: Val::Px(10.0),
                    ..default()
                },
                BackgroundColor(BAR_TRACK),
            ))
            .with_children(|track| {
                let full = Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                };
                track.spawn((
                    full.clone(),
                    BackgroundColor(TRAIL),
                    HudBar {
                        kind: trail,
                        displayed: 1.0,
                    },
                ));
                track.spawn((
                    full,
                    BackgroundColor(fill.1),
                    HudBar {
                        kind: fill.0,
                        displayed: 1.0,
                    },
                ));
                // Segment ticks every 10%.
                for i in 1..10 {
                    track.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(i as f32 * 10.0),
                            width: Val::Px(2.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(PANEL_SOLID),
                    ));
                }
            });
            row.spawn((Node {
                width: Val::Px(40.0),
                justify_content: JustifyContent::FlexEnd,
                ..default()
            },))
                .with_children(|cell| {
                    cell.spawn(value);
                });
        });
}

fn card(accent: Color) -> (Node, BackgroundColor, BorderColor) {
    (
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::new(Val::Px(14.0), Val::Px(16.0), Val::Px(10.0), Val::Px(12.0)),
            border: UiRect::left(Val::Px(3.0)),
            ..default()
        },
        BackgroundColor(PANEL),
        BorderColor(accent),
    )
}

/// Spawns the HUD tree.
/// Typeface for HUD text; without it the HUD uses Bevy's default font.
#[derive(Resource, Debug, Clone)]
pub struct HudFont(pub Handle<Font>);

/// Display face for headings and the weapon name.
#[derive(Resource, Debug, Clone)]
pub struct HudTitleFont(pub Handle<Font>);

pub fn setup_cyberpunk_hud(
    mut commands: Commands,
    hud_font: Option<Res<HudFont>>,
    title_font: Option<Res<HudTitleFont>>,
) {
    let font = hud_font.map(|f| f.0.clone()).unwrap_or_default();
    let title = title_font.map_or_else(|| font.clone(), |f| f.0.clone());
    commands
        .spawn((
            Name::new("HUD"),
            HudRootNode,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(24.0)),
                ..default()
            },
        ))
        .with_children(|root| {
            // Top centre: compass tape with a centre notch, heading below.
            root.spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|top| {
                top.spawn((
                    Node {
                        width: Val::Px(COMPASS_WIDTH),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                        border: UiRect::bottom(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                    BorderColor(HAIRLINE),
                ))
                .with_children(|strip| {
                    strip.spawn((
                        Node {
                            width: Val::Px(2.0),
                            height: Val::Px(6.0),
                            margin: UiRect::bottom(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(CYAN),
                    ));
                    strip.spawn((hud_text(&font, "", 13.0, TEXT), CompassTapeUi));
                });
                top.spawn((
                    hud_text(&font, "N 000\u{b0}", 12.0, CYAN),
                    CompassStripUi,
                    Node {
                        margin: UiRect::top(Val::Px(6.0)),
                        ..default()
                    },
                ));
            });

            // Centre: crosshair and hit marker.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(50.0),
                    width: Val::Px(0.0),
                    height: Val::Px(0.0),
                    ..default()
                },
                CrosshairUi {
                    gap_px: CROSSHAIR_MIN_GAP_PX,
                },
            ))
            .with_children(|center| {
                for axis in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
                    let offset = crosshair_arm_offset(axis, CROSSHAIR_MIN_GAP_PX);
                    let horizontal = axis.x != 0;
                    center.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(offset.x),
                            top: Val::Px(offset.y),
                            width: Val::Px(if horizontal {
                                CROSSHAIR_ARM_LENGTH_PX
                            } else {
                                CROSSHAIR_ARM_THICKNESS_PX
                            }),
                            height: Val::Px(if horizontal {
                                CROSSHAIR_ARM_THICKNESS_PX
                            } else {
                                CROSSHAIR_ARM_LENGTH_PX
                            }),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                        CrosshairArm { axis },
                    ));
                }
                center.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-1.5),
                        top: Val::Px(-1.5),
                        width: Val::Px(3.0),
                        height: Val::Px(3.0),
                        ..default()
                    },
                    BackgroundColor(CYAN),
                ));
                center.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-10.0),
                        top: Val::Px(-11.0),
                        ..default()
                    },
                    hud_text(&font, "X", 18.0, Color::NONE),
                    HitMarkerUi::default(),
                    Visibility::Hidden,
                ));
            });

            // Bottom: vitals card on the left, weapon card on the right.
            root.spawn(Node {
                width: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::FlexEnd,
                ..default()
            })
            .with_children(|bottom| {
                bottom.spawn(card(CYAN)).with_children(|vitals| {
                    vitals.spawn((
                        OperativeNameUi,
                        hud_text(&title, "Operative", 15.0, TEXT),
                        Node {
                            margin: UiRect::bottom(Val::Px(6.0)),
                            ..default()
                        },
                    ));
                    vital_row(
                        vitals,
                        &font,
                        "Shield",
                        (HudBarKind::Shield, CYAN),
                        HudBarKind::ShieldTrail,
                        (hud_text(&font, "0", 15.0, CYAN), HealthShieldBarUi),
                    );
                    vital_row(
                        vitals,
                        &font,
                        "Health",
                        (HudBarKind::Health, HEALTH_OK),
                        HudBarKind::HealthTrail,
                        (hud_text(&font, "0", 15.0, TEXT), HealthValueUi),
                    );
                    vitals
                        .spawn(Node {
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(10.0),
                            margin: UiRect::top(Val::Px(8.0)),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Node {
                                    width: Val::Px(VITAL_BAR_WIDTH * 0.55),
                                    height: Val::Px(3.0),
                                    ..default()
                                },
                                BackgroundColor(BAR_TRACK),
                            ))
                            .with_children(|track| {
                                track.spawn((
                                    Node {
                                        width: Val::Percent(0.0),
                                        height: Val::Percent(100.0),
                                        ..default()
                                    },
                                    BackgroundColor(AMBER),
                                    HudBar {
                                        kind: HudBarKind::Flow,
                                        displayed: 0.0,
                                    },
                                ));
                            });
                            row.spawn((
                                hud_text(&font, "Flow   0% · base", 11.0, AMBER),
                                FlowMeterUi,
                            ));
                        });
                });
                bottom.spawn(card(AMBER)).with_children(|weapon| {
                    weapon.spawn((hud_text(&font, "Melee", 11.0, MUTED), FireModeUi));
                    weapon.spawn((hud_text(&title, "Cyber-katana", 26.0, TEXT), WeaponNameUi));
                    weapon
                        .spawn(Node {
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            margin: UiRect::top(Val::Px(4.0)),
                            ..default()
                        })
                        .with_children(|row| {
                            for index in 0..3 {
                                row.spawn((
                                    Node {
                                        width: Val::Px(18.0),
                                        height: Val::Px(4.0),
                                        ..default()
                                    },
                                    BackgroundColor(BAR_TRACK),
                                    ComboPip(index),
                                ));
                            }
                            row.spawn((
                                hud_text(&font, "3-hit combo", 12.0, AMBER),
                                AmmoCounterUi,
                                Node {
                                    margin: UiRect::left(Val::Px(6.0)),
                                    ..default()
                                },
                            ));
                        });
                });
            });
        });
}

/// Values shown last frame, to skip unchanged text.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct VitalsShown {
    numbers: Option<[u32; 4]>,
    flow: Option<(u32, FlowTier)>,
}

type PlayerVitals<'a> = (
    Option<&'a Health>,
    Option<&'a Shields>,
    Option<&'a FlowState>,
);

/// Updates the vitals readout, flow meter and bars from the player. Fills
/// ease quickly; damage trails linger behind them, and health turns red
/// when low.
#[allow(clippy::type_complexity)]
pub fn update_hud_vitals(
    time: Res<Time>,
    constants: Option<Res<ParkourPhysicsConstants>>,
    players: Query<PlayerVitals, With<PlayerCharacterMarker>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<HealthShieldBarUi>>,
        Query<&mut Text, With<HealthValueUi>>,
        Query<&mut Text, With<FlowMeterUi>>,
    )>,
    mut bars: Query<(&mut HudBar, &mut Node, &mut BackgroundColor)>,
    mut shown: Local<VitalsShown>,
) {
    let Some((health, shields, flow)) = players.iter().next() else {
        return;
    };
    let hp = health.map_or((0.0, 0.0), |h| (h.current, h.max));
    let sh = shields.map_or((0.0, 0.0), |s| (s.current, s.max));
    let numbers = [
        display_value(sh.0),
        display_value(sh.1),
        display_value(hp.0),
        display_value(hp.1),
    ];
    if shown.numbers != Some(numbers) {
        for mut text in &mut texts.p0() {
            write_vital_value(&mut text.0, sh.0);
        }
        for mut text in &mut texts.p1() {
            write_vital_value(&mut text.0, hp.0);
        }
        shown.numbers = Some(numbers);
    }

    let flow_value = flow.map_or(0.0, |f| f.current_flow);
    let overdrive = constants.map_or(FLOW_OVERDRIVE_MULTIPLIER, |c| c.flow_overdrive_multiplier);
    let flow_key = (
        (fill_fraction(flow_value, FLOW_MAX) * 100.0).round() as u32,
        FlowTier::from_flow(flow_value),
    );
    if shown.flow != Some(flow_key) {
        for mut text in &mut texts.p2() {
            write_flow_text(&mut text.0, flow_value, overdrive);
        }
        shown.flow = Some(flow_key);
    }

    let dt = time.delta_secs();
    let fast = ease_factor(BAR_EASE_RATE, dt);
    let slow = ease_factor(TRAIL_EASE_RATE, dt);
    let health_fraction = fill_fraction(hp.0, hp.1);
    for (mut bar, mut node, mut color) in &mut bars {
        let (target, trail) = match bar.kind {
            HudBarKind::Shield => (fill_fraction(sh.0, sh.1), false),
            HudBarKind::Health => (health_fraction, false),
            HudBarKind::ShieldTrail => (fill_fraction(sh.0, sh.1), true),
            HudBarKind::HealthTrail => (health_fraction, true),
            HudBarKind::Stamina => (
                flow.map_or(0.0, |f| fill_fraction(f.stamina_current, f.stamina_max)),
                false,
            ),
            HudBarKind::Flow => (fill_fraction(flow_value, FLOW_MAX), false),
        };
        if bar.kind == HudBarKind::Health {
            let wanted = if health_fraction < LOW_HEALTH_FRACTION {
                DANGER
            } else {
                HEALTH_OK
            };
            if color.0 != wanted {
                color.0 = wanted;
            }
        }
        // Trails only lag on the way down; healing snaps them up.
        let next = if trail && target > bar.displayed {
            target
        } else {
            let rate = if trail { slow } else { fast };
            let next = bar.displayed + (target - bar.displayed) * rate;
            if (next - target).abs() < 1e-4 {
                target
            } else {
                next
            }
        };
        if next != bar.displayed {
            bar.displayed = next;
            node.width = Val::Percent(next * 100.0);
        }
    }
}

/// Weapon card: name, fire mode and ammunition (or the melee combo).
#[allow(clippy::type_complexity)]
pub fn update_ammo_counter(
    weapons: Query<Option<&BallisticProfile>, With<PlayerCharacterMarker>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<AmmoCounterUi>>,
        Query<&mut Text, With<WeaponNameUi>>,
        Query<&mut Text, With<FireModeUi>>,
    )>,
    mut shown: Local<Option<(u32, u32, bool, Option<RangedWeaponArchetype>)>>,
) {
    let Some(weapon) = weapons.iter().next() else {
        return;
    };
    let key = weapon.map_or((0, 0, false, None), |w| {
        (
            w.current_ammo,
            w.reserve_ammo,
            w.reload_timer > 0.0,
            Some(w.archetype),
        )
    });
    if *shown == Some(key) {
        return;
    }
    for mut text in &mut texts.p0() {
        write_ammo_text(&mut text.0, weapon);
    }
    for mut text in &mut texts.p1() {
        weapon_display_name(weapon).clone_into(&mut text.0);
    }
    for mut text in &mut texts.p2() {
        weapon
            .map_or("Melee", |w| fire_mode_label(w.archetype))
            .clone_into(&mut text.0);
    }
    *shown = Some(key);
}

/// Lights one pip per hit of the running melee combo.
pub fn update_combo_pips(
    fighters: Query<&MeleeCombatState, With<PlayerCharacterMarker>>,
    mut pips: Query<(&ComboPip, &mut BackgroundColor)>,
) {
    let lit = fighters
        .iter()
        .next()
        .and_then(|m| (m.attack_phase != AttackPhase::Idle).then_some(m.current_combo_index % 3));
    for (pip, mut color) in &mut pips {
        let wanted = match lit {
            Some(step) if pip.0 <= step => AMBER,
            _ => BAR_TRACK,
        };
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
}

/// Heading, objective range and the scrolling compass tape.
pub fn update_compass_strip(
    objective: Option<Res<HudObjective>>,
    players: Query<(&Transform, Option<&ViewAngles>), With<PlayerCharacterMarker>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<CompassStripUi>>,
        Query<&mut Text, With<CompassTapeUi>>,
    )>,
    mut shown: Local<Option<(u32, Option<u32>, String)>>,
) {
    let Some((transform, view)) = players.iter().next() else {
        return;
    };
    let yaw = view.map_or_else(|| transform.rotation.to_euler(EulerRot::YXZ).0, |v| v.yaw);
    let target = objective.as_deref().and_then(|o| {
        o.target
            .map(|t| (o.label.as_str(), t.distance(transform.translation)))
    });
    let heading = compass_heading_degrees(yaw);
    let key = (
        heading,
        target.map(|(_, d)| d.round() as u32),
        objective
            .as_deref()
            .map_or_else(String::new, |o| o.label.clone()),
    );
    if shown.as_ref() == Some(&key) {
        return;
    }
    for mut text in &mut texts.p0() {
        write_compass_text(&mut text.0, yaw, target);
    }
    for mut text in &mut texts.p1() {
        write_compass_tape(&mut text.0, heading);
    }
    *shown = Some(key);
}

/// Writes the callsign onto the vitals card in capitals.
pub fn update_operative_name(
    name: Option<Res<OperativeName>>,
    mut texts: Query<&mut Text, With<OperativeNameUi>>,
) {
    let Some(name) = name.filter(|n| n.is_changed()) else {
        return;
    };
    let shown = name.0.clone();
    for mut text in &mut texts {
        if text.0 != shown {
            text.0.clone_from(&shown);
        }
    }
}

/// Hides the in-game HUD while a full-screen client screen is up.
pub fn hide_hud_under_overlay(
    overlay: Option<Res<crate::ScreenOverlay>>,
    mut roots: Query<&mut Visibility, With<HudRootNode>>,
) {
    let Some(overlay) = overlay else {
        return;
    };
    if !overlay.is_changed() {
        return;
    }
    for mut visibility in &mut roots {
        *visibility = if overlay.0 {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

/// Opens the crosshair with weapon spread and movement speed.
pub fn update_crosshair_bloom(
    time: Res<Time>,
    players: Query<
        (Option<&BallisticProfile>, Option<&KinematicVelocity>),
        With<PlayerCharacterMarker>,
    >,
    mut crosshairs: Query<&mut CrosshairUi>,
    mut arms: Query<(&CrosshairArm, &mut Node)>,
) {
    let Some((weapon, velocity)) = players.iter().next() else {
        return;
    };
    let spread = weapon.map_or(0.0, |w| w.current_spread_degrees);
    let speed = velocity.map_or(0.0, |v| Vec2::new(v.0.x, v.0.z).length());
    let target = crosshair_gap_px(spread, speed);
    let t = ease_factor(CROSSHAIR_EASE_RATE, time.delta_secs());
    for mut crosshair in &mut crosshairs {
        let next = crosshair.gap_px + (target - crosshair.gap_px) * t;
        let next = if (next - target).abs() < 0.01 {
            target
        } else {
            next
        };
        if next == crosshair.gap_px {
            continue;
        }
        crosshair.gap_px = next;
        for (arm, mut node) in &mut arms {
            let offset = crosshair_arm_offset(arm.axis, next);
            node.left = Val::Px(offset.x);
            node.top = Val::Px(offset.y);
        }
    }
}

/// Flashes the hit marker for the player's confirmed hits (amber on a
/// critical) and fades it out.
pub fn update_hit_markers(
    time: Res<Time>,
    players: Query<Entity, With<PlayerCharacterMarker>>,
    mut ranged: EventReader<RangedHitEvent>,
    mut melee: EventReader<MeleeHitEvent>,
    mut markers: Query<(&mut HitMarkerUi, &mut TextColor, &mut Visibility)>,
) {
    let player = players.iter().next();
    let mut hit = None;
    for e in ranged.read() {
        if Some(e.shooter) == player && !e.deflected {
            hit = Some(hit.unwrap_or(false) || e.headshot);
        }
    }
    for e in melee.read() {
        if Some(e.attacker) == player {
            hit = Some(hit.unwrap_or(false));
        }
    }
    let dt = time.delta_secs();
    for (mut marker, mut color, mut visibility) in &mut markers {
        if let Some(critical) = hit {
            marker.fade_timer = HIT_MARKER_SECONDS;
            marker.critical = critical;
        } else if marker.fade_timer > 0.0 {
            marker.fade_timer = (marker.fade_timer - dt).max(0.0);
        } else {
            continue;
        }
        let alpha = marker.fade_timer / HIT_MARKER_SECONDS;
        let base = if marker.critical { AMBER } else { Color::WHITE };
        color.0 = base.with_alpha(alpha);
        let wanted = if alpha > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    #[test]
    fn compass_reads_clockwise_from_north() {
        // Engine yaw turns left when positive; the compass reads clockwise.
        let cases = [
            (0.0, "N 000\u{b0}"),
            (-FRAC_PI_2, "E 090\u{b0}"),
            (PI, "S 180\u{b0}"),
            (-PI, "S 180\u{b0}"),
            (FRAC_PI_2, "W 270\u{b0}"),
            (-FRAC_PI_4, "NE 045\u{b0}"),
            (FRAC_PI_4, "NW 315\u{b0}"),
        ];
        let mut out = String::new();
        for (yaw, expected) in cases {
            write_compass_text(&mut out, yaw, None);
            assert_eq!(out, expected, "yaw {yaw}");
        }
        assert_eq!(compass_heading_degrees(f32::NAN), 0);
        assert_eq!(
            compass_heading_degrees(-2.0 * PI + 0.0001),
            0,
            "wraps, never 360"
        );
    }

    #[test]
    fn turning_right_with_the_mouse_increases_the_heading() {
        // game_input applies `delta_yaw = -mouse.x * sensitivity`.
        let mut view = ViewAngles::default();
        let before = compass_heading_degrees(view.yaw);
        view.rotate(-0.6, 0.0); // mouse moved right, 34 degrees
        let after = compass_heading_degrees(view.yaw);
        assert!(after > before && after < 90, "{before} -> {after}");
        assert_eq!(cardinal_direction(after), "NE");
    }

    #[test]
    fn cardinal_sectors_are_45_degrees_wide() {
        assert_eq!(cardinal_direction(0), "N");
        assert_eq!(cardinal_direction(22), "N");
        assert_eq!(cardinal_direction(23), "NE");
        assert_eq!(cardinal_direction(337), "NW");
        assert_eq!(cardinal_direction(338), "N");
        assert_eq!(cardinal_direction(180), "S");
        assert_eq!(cardinal_direction(359), "N");
    }

    #[test]
    fn compass_shows_objective_distance() {
        let mut out = String::new();
        write_compass_text(
            &mut out,
            -0.733,
            Some(("OBJECTIVE: SECTOR 4 EXTRACTION", 185.4)),
        );
        assert_eq!(out, "NE 042\u{b0} // OBJECTIVE: SECTOR 4 EXTRACTION [185m]");
    }

    #[test]
    fn vitals_and_flow_text() {
        let mut out = String::new();
        write_vital_value(&mut out, 99.2);
        assert_eq!(out, "100", "a sliver rounds up");
        write_vital_value(&mut out, 0.0);
        assert_eq!(out, "0");
        write_vital_value(&mut out, f32::NAN);
        assert_eq!(out, "0");
        let labels: Vec<String> = [0.0, 30.0, 60.0, 90.0]
            .into_iter()
            .map(|flow| {
                write_flow_text(&mut out, flow, 1.25);
                out.clone()
            })
            .collect();
        assert_eq!(
            labels,
            vec![
                "Flow   0% · base",
                "Flow  30% · tier 1",
                "Flow  60% · tier 2",
                "Flow  90% · overdrive 125%"
            ]
        );
    }

    #[test]
    fn text_buffers_are_reused() {
        let mut out = String::with_capacity(64);
        let ptr = out.as_ptr();
        for i in 0..100 {
            write_vital_value(&mut out, i as f32);
        }
        assert_eq!(out.as_ptr(), ptr, "no reallocation while values change");
    }

    #[test]
    fn ammo_text() {
        let mut out = String::new();
        write_ammo_text(&mut out, None);
        assert_eq!(out, "3-hit combo");
        assert_eq!(weapon_display_name(None), "Cyber-katana");
        let mut smg = BallisticProfile::for_archetype(RangedWeaponArchetype::SmartSubmachineGun);
        smg.current_ammo = 30;
        smg.reserve_ammo = 120;
        write_ammo_text(&mut out, Some(&smg));
        assert_eq!(out, "30 / 120");
        assert_eq!(weapon_display_name(Some(&smg)), "Smart SMG");
        smg.reload_timer = 1.2;
        write_ammo_text(&mut out, Some(&smg));
        assert_eq!(out, "Reloading");
        assert_eq!(
            fire_mode_label(RangedWeaponArchetype::ChargeRailgun),
            "Charge"
        );
    }

    #[test]
    fn compass_tape_centres_on_the_heading() {
        let mut out = String::new();
        write_compass_tape(&mut out, 0);
        assert_eq!(out.chars().count(), COMPASS_TAPE_CHARS);
        let centre = COMPASS_TAPE_CHARS / 2;
        assert_eq!(out.chars().nth(centre), Some('N'));
        assert!(out.contains("NE") && out.contains("NW"));
        write_compass_tape(&mut out, 92);
        assert_eq!(out.chars().nth(centre), Some('E'), "snaps to 5 degrees");
        assert!(!out.contains('W'));
    }

    #[test]
    fn crosshair_gap_grows_with_spread_and_speed() {
        assert_eq!(crosshair_gap_px(0.0, 0.0), CROSSHAIR_MIN_GAP_PX);
        assert!(crosshair_gap_px(2.0, 0.0) > crosshair_gap_px(0.5, 0.0));
        assert!(crosshair_gap_px(0.5, 13.5) > crosshair_gap_px(0.5, 0.0));
        assert_eq!(crosshair_gap_px(100.0, 100.0), CROSSHAIR_MAX_GAP_PX);
        assert_eq!(crosshair_gap_px(f32::NAN, -5.0), CROSSHAIR_MIN_GAP_PX);
        let right = crosshair_arm_offset(IVec2::X, 10.0);
        let left = crosshair_arm_offset(IVec2::NEG_X, 10.0);
        assert_eq!(right.x, 10.0);
        assert_eq!(left.x, -10.0 - CROSSHAIR_ARM_LENGTH_PX);
        assert_eq!(
            crosshair_arm_offset(IVec2::Y, 10.0).y,
            10.0,
            "screen +y is down"
        );
    }

    #[test]
    fn fill_fraction_is_safe() {
        assert_eq!(fill_fraction(50.0, 100.0), 0.5);
        assert_eq!(fill_fraction(150.0, 100.0), 1.0);
        assert_eq!(fill_fraction(5.0, 0.0), 0.0);
        assert_eq!(fill_fraction(f32::NAN, 100.0), 0.0);
    }
}
