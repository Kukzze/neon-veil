//! Co-op presence: each player sends their operative (look and player
//! card) once connected and whenever they change it; every other player's
//! replicated character is dressed in that operative.

use crate::character::{spawn_operative_body, CharacterAppearance};
use crate::client_network::{
    ClientNetworkState, ClientTransportResource, ConnectedToServerEvent, RemoteEntity,
};
use bevy::prelude::*;
use game_core::protocol::{ClientMessage, MAX_PROFILE_BYTES};
use game_server::player_network_id;

/// Encodes a look for the wire, or `None` if it would be too large.
pub fn encode_profile(look: &CharacterAppearance) -> Option<Vec<u8>> {
    let bytes = serde_json::to_vec(look).ok()?;
    (bytes.len() <= MAX_PROFILE_BYTES).then_some(bytes)
}

/// Decodes and sanitizes a profile another player sent.
pub fn decode_profile(bytes: &[u8]) -> Option<CharacterAppearance> {
    if bytes.len() > MAX_PROFILE_BYTES {
        return None;
    }
    serde_json::from_slice::<CharacterAppearance>(bytes)
        .ok()
        .map(CharacterAppearance::sanitized)
}

/// Sends the local operative on connect and after every change.
pub fn send_local_profile(
    look: Option<Res<CharacterAppearance>>,
    mut connected: EventReader<ConnectedToServerEvent>,
    transport: Option<ResMut<ClientTransportResource>>,
    mut sent: Local<Option<CharacterAppearance>>,
) {
    let just_connected = connected.read().count() > 0;
    let (Some(look), Some(mut transport)) = (look, transport) else {
        return;
    };
    if !just_connected && sent.as_ref() == Some(&*look) {
        return;
    }
    if let Some(bytes) = encode_profile(&look) {
        transport.0.send(ClientMessage::Profile(bytes));
        *sent = Some(look.clone());
    }
}

/// The operative dressed onto a remote player, and the profile it shows.
#[derive(Component, Debug, Clone)]
pub struct RemoteOperative {
    pub body: Entity,
    pub profile: Vec<u8>,
}

pub fn dress_remote_players(
    mut commands: Commands,
    assets: Res<AssetServer>,
    state: Res<ClientNetworkState>,
    remotes: Query<(Entity, &RemoteEntity, Option<&RemoteOperative>)>,
    mut looks: Query<&mut CharacterAppearance>,
) {
    for (player, data) in &state.profiles {
        if Some(*player) == state.player_id {
            continue;
        }
        let id = player_network_id(*player);
        let Some((entity, _, dressed)) = remotes.iter().find(|(_, r, _)| r.network_id == id) else {
            continue;
        };
        if dressed.is_some_and(|d| d.profile == *data) {
            continue;
        }
        let Some(look) = decode_profile(data) else {
            continue;
        };
        match dressed {
            Some(existing) => {
                if let Ok(mut current) = looks.get_mut(existing.body) {
                    *current = look;
                }
                commands.entity(entity).insert(RemoteOperative {
                    body: existing.body,
                    profile: data.clone(),
                });
            }
            None => {
                commands.entity(entity).insert(Visibility::default());
                let body = spawn_operative_body(&mut commands, &assets, entity, look);
                commands.entity(entity).insert(RemoteOperative {
                    body,
                    profile: data.clone(),
                });
            }
        }
    }
}

/// Floating callsign and title over a co-op player.
#[derive(Component, Debug, Clone, Copy)]
pub struct NameTag {
    pub owner: Entity,
}

/// Height of the tag above a player's origin, metres.
const TAG_HEIGHT: f32 = 1.25;
/// Tags fade out beyond this distance, metres.
const TAG_RANGE: f32 = 45.0;

fn tag_text(look: &CharacterAppearance) -> String {
    let card = &look.card;
    if card.title.is_empty() {
        card.callsign.clone()
    } else {
        format!("{}\n{}", card.callsign, card.title)
    }
}

/// Spawns, refreshes and removes name tags for dressed remote players.
pub fn sync_name_tags(
    mut commands: Commands,
    font: Option<Res<game_ui::HudTitleFont>>,
    remotes: Query<(Entity, &RemoteOperative)>,
    looks: Query<&CharacterAppearance>,
    mut tags: Query<(Entity, &NameTag, &mut Text)>,
) {
    for (tag_entity, tag, mut text) in &mut tags {
        let Ok((_, remote)) = remotes.get(tag.owner) else {
            commands.entity(tag_entity).despawn_recursive();
            continue;
        };
        if let Ok(look) = looks.get(remote.body) {
            let wanted = tag_text(look);
            if text.0 != wanted {
                text.0 = wanted;
            }
        }
    }
    for (owner, remote) in &remotes {
        if tags.iter().any(|(_, t, _)| t.owner == owner) {
            continue;
        }
        let Ok(look) = looks.get(remote.body) else {
            continue;
        };
        let mut style = TextFont::from_font_size(15.0);
        if let Some(font) = font.as_deref() {
            style.font = font.0.clone();
        }
        commands.spawn((
            Name::new("Name Tag"),
            NameTag { owner },
            Text::new(tag_text(look)),
            style,
            TextColor(Color::srgb(0.85, 0.97, 1.0)),
            TextLayout::new_with_justify(JustifyText::Center),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Visibility::Hidden,
        ));
    }
}

/// Keeps each tag over its player's head on screen.
pub fn place_name_tags(
    cameras: Query<(&Camera, &GlobalTransform), With<game_camera::CameraRigController>>,
    owners: Query<&GlobalTransform>,
    mut tags: Query<(&NameTag, &mut Node, &mut Visibility, &ComputedNode)>,
) {
    let Some((camera, eye)) = cameras.iter().find(|(c, _)| c.is_active) else {
        return;
    };
    for (tag, mut node, mut visibility, computed) in &mut tags {
        let shown = owners.get(tag.owner).ok().and_then(|owner| {
            let head = owner.translation() + Vec3::Y * TAG_HEIGHT;
            let ahead = (head - eye.translation()).dot(*eye.forward()) > 0.0;
            let near = head.distance(eye.translation()) < TAG_RANGE;
            (ahead && near)
                .then(|| camera.world_to_viewport(eye, head).ok())
                .flatten()
        });
        match shown {
            Some(at) => {
                let size = computed.size() * computed.inverse_scale_factor();
                node.left = Val::Px(at.x - size.x * 0.5);
                node.top = Val::Px(at.y - size.y);
                *visibility = Visibility::Inherited;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
}

pub struct CoopPlugin;

impl Plugin for CoopPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                send_local_profile,
                dress_remote_players,
                sync_name_tags,
                place_name_tags,
            )
                .chain()
                .run_if(resource_exists::<ClientNetworkState>),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Preset;

    #[test]
    fn profiles_round_trip_within_the_size_limit() {
        let look = CharacterAppearance::preset(Preset::Hana);
        let bytes = encode_profile(&look).expect("fits");
        assert!(bytes.len() < MAX_PROFILE_BYTES);
        assert_eq!(decode_profile(&bytes), Some(look.sanitized()));
    }

    #[test]
    fn tags_show_callsign_and_title() {
        let mut look = CharacterAppearance::preset(Preset::Hana);
        look.card.callsign = "kite".into();
        look.card.title = "Night Runner".into();
        assert_eq!(tag_text(&look), "kite\nNight Runner");
        look.card.title.clear();
        assert_eq!(tag_text(&look), "kite");
    }

    #[test]
    fn hostile_profiles_are_rejected_or_cleaned() {
        assert_eq!(decode_profile(b"not json"), None);
        assert_eq!(decode_profile(&vec![b' '; MAX_PROFILE_BYTES + 1]), None);
        let look = CharacterAppearance {
            hair_mesh: "../../etc/passwd".into(),
            ..default()
        };
        let bytes = serde_json::to_vec(&look).expect("json");
        let decoded = decode_profile(&bytes).expect("parses");
        assert!(
            decoded.hair_mesh.is_empty(),
            "unsafe asset names are dropped"
        );
    }
}
