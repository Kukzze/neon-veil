//! Mission presentation: the objective panel, voiced subtitles and the
//! centre banner used for encounter titles, death and victory screens.
//! Gameplay code writes [`MissionHud`]; subtitles follow voice lines.

use crate::hud::{hud_text, AMBER, CYAN, PANEL};
use crate::{HudFont, UserSettingsConfig};
use bevy::prelude::*;
use game_audio::SubtitleDisplayEvent;

const DANGER: Color = Color::srgb(1.0, 0.25, 0.3);
const SUBTITLE_SPEAKER: Color = Color::srgb(0.0, 0.9, 1.0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BannerTone {
    #[default]
    Info,
    Danger,
    Success,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Banner {
    pub title: String,
    pub subtitle: String,
    pub tone: BannerTone,
}

/// What the mission layer wants on screen.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct MissionHud {
    pub title: String,
    pub objective: String,
    pub banner: Option<Banner>,
}

/// The subtitle currently shown and its remaining time.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct ActiveSubtitle {
    pub speaker: String,
    pub text: String,
    pub remaining: f32,
}

#[derive(Component)]
pub struct MissionTitleText;
#[derive(Component)]
pub struct MissionObjectiveText;
#[derive(Component)]
pub struct MissionPanel;
#[derive(Component)]
pub struct SubtitleRoot;
#[derive(Component)]
pub struct SubtitleSpeakerText;
#[derive(Component)]
pub struct SubtitleLineText;
#[derive(Component)]
pub struct BannerRoot;
#[derive(Component)]
pub struct BannerTitleText;
#[derive(Component)]
pub struct BannerSubtitleText;

pub fn setup_mission_hud(mut commands: Commands, hud_font: Option<Res<HudFont>>) {
    let font = hud_font.map(|f| f.0.clone()).unwrap_or_default();
    commands
        .spawn((
            MissionPanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(24.0),
                top: Val::Px(24.0),
                max_width: Val::Px(420.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                border: UiRect::left(Val::Px(3.0)),
                ..default()
            },
            BorderColor(AMBER),
            BackgroundColor(PANEL),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((MissionTitleText, hud_text(&font, "", 12.0, AMBER)));
            p.spawn((
                MissionObjectiveText,
                hud_text(&font, "", 17.0, Color::WHITE),
            ));
        });

    commands
        .spawn((
            SubtitleRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(220.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    max_width: Val::Px(880.0),
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                    column_gap: Val::Px(10.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            ))
            .with_children(|b| {
                b.spawn((
                    SubtitleSpeakerText,
                    hud_text(&font, "", 18.0, SUBTITLE_SPEAKER),
                    Node {
                        flex_shrink: 0.0,
                        ..default()
                    },
                ));
                b.spawn((SubtitleLineText, hud_text(&font, "", 18.0, Color::WHITE)));
            });
        });

    commands
        .spawn((
            BannerRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(30.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            GlobalZIndex(10),
            Visibility::Hidden,
        ))
        .with_children(|b| {
            b.spawn((BannerTitleText, hud_text(&font, "", 46.0, CYAN)));
            b.spawn((BannerSubtitleText, hud_text(&font, "", 18.0, Color::WHITE)));
        });
}

/// Starts each voiced subtitle and counts it down.
pub fn track_subtitles(
    time: Res<Time>,
    mut lines: EventReader<SubtitleDisplayEvent>,
    mut active: ResMut<ActiveSubtitle>,
) {
    for line in lines.read() {
        *active = ActiveSubtitle {
            speaker: line.speaker.clone(),
            text: line.text.clone(),
            remaining: line.duration_seconds.max(1.0),
        };
    }
    if active.remaining > 0.0 {
        active.remaining = (active.remaining - time.delta_secs()).max(0.0);
    }
}

fn shown(on: bool) -> Visibility {
    if on {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

fn tone_color(tone: BannerTone) -> Color {
    match tone {
        BannerTone::Info => CYAN,
        BannerTone::Danger => DANGER,
        BannerTone::Success => AMBER,
    }
}

#[allow(clippy::type_complexity)]
pub fn render_mission_hud(
    hud: Res<MissionHud>,
    subtitle: Res<ActiveSubtitle>,
    settings: Option<Res<UserSettingsConfig>>,
    mut panels: Query<
        &mut Visibility,
        (
            With<MissionPanel>,
            Without<SubtitleRoot>,
            Without<BannerRoot>,
        ),
    >,
    mut subtitle_roots: Query<
        &mut Visibility,
        (
            With<SubtitleRoot>,
            Without<MissionPanel>,
            Without<BannerRoot>,
        ),
    >,
    mut banners: Query<
        &mut Visibility,
        (
            With<BannerRoot>,
            Without<MissionPanel>,
            Without<SubtitleRoot>,
        ),
    >,
    mut texts: ParamSet<(
        Query<&mut Text, With<MissionTitleText>>,
        Query<&mut Text, With<MissionObjectiveText>>,
        Query<&mut Text, With<SubtitleSpeakerText>>,
        Query<&mut Text, With<SubtitleLineText>>,
        Query<(&mut Text, &mut TextColor), With<BannerTitleText>>,
        Query<&mut Text, With<BannerSubtitleText>>,
    )>,
) {
    if !hud.is_changed() && !subtitle.is_changed() {
        return;
    }
    let subtitles_on = settings.is_none_or(|s| s.accessibility.subtitles_enabled);
    let show_subtitle = subtitles_on && subtitle.remaining > 0.0 && !subtitle.text.is_empty();
    for mut v in &mut panels {
        v.set_if_neq(shown(!hud.objective.is_empty()));
    }
    for mut v in &mut subtitle_roots {
        v.set_if_neq(shown(show_subtitle));
    }
    for mut v in &mut banners {
        v.set_if_neq(shown(hud.banner.is_some()));
    }
    let set = |text: &mut Text, value: &str| {
        if text.0 != value {
            value.clone_into(&mut text.0);
        }
    };
    for mut t in &mut texts.p0() {
        set(&mut t, &hud.title);
    }
    for mut t in &mut texts.p1() {
        set(&mut t, &hud.objective);
    }
    for mut t in &mut texts.p2() {
        set(&mut t, &format!("{}:", subtitle.speaker));
    }
    for mut t in &mut texts.p3() {
        set(&mut t, &subtitle.text);
    }
    let banner = hud.banner.clone().unwrap_or_default();
    for (mut t, mut color) in &mut texts.p4() {
        set(&mut t, &banner.title);
        color.0 = tone_color(banner.tone);
    }
    for mut t in &mut texts.p5() {
        set(&mut t, &banner.subtitle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn subtitles_start_and_expire() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                500,
            )))
            .init_resource::<ActiveSubtitle>()
            .add_event::<SubtitleDisplayEvent>()
            .add_systems(Update, track_subtitles);
        app.update();
        app.world_mut().send_event(SubtitleDisplayEvent {
            speaker: "AURA".into(),
            text: "Neural link restored.".into(),
            duration_seconds: 1.2,
        });
        app.update();
        let active = app.world().resource::<ActiveSubtitle>();
        assert_eq!(active.speaker, "AURA");
        assert!(active.remaining > 0.0);
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(app.world().resource::<ActiveSubtitle>().remaining, 0.0);
    }
}
