//! Syndicate patrols out in the city: once the operative leaves Kuroshio
//! Avenue, squads of two to four turn up at nearby street intersections
//! every so often, up to a cap. They fight, drop loot and give experience
//! like any enemy, and quietly leave once far behind.

use crate::client_network::LocalPredictedPlayer;
use crate::tutorial::TutorialActive;
use bevy::prelude::*;
use game_ai::{spawn_enemy, EnemyArchetype};
use game_world::city::{self, BLOCK_PITCH, MAP_HALF};

/// Patrol enemies alive at once, at most.
pub const MAX_PATROL_ENEMIES: usize = 10;
/// Seconds between squads.
const SPAWN_INTERVAL: f32 = 18.0;
/// Squads turn up this far from the operative, metres.
const SPAWN_MIN: f32 = 70.0;
const SPAWN_MAX: f32 = 260.0;
/// Patrols further than this are removed.
const DESPAWN_DISTANCE: f32 = 450.0;
/// Capsule centre height of a standing enemy.
const STANDING_Y: f32 = 0.95;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatrolMember;

/// Kuroshio Avenue, where Night One's own encounters happen.
pub fn in_avenue(p: Vec3) -> bool {
    p.x.abs() < 60.0 && p.z < 60.0 && p.z > -300.0
}

/// Street intersections sit between blocks.
pub fn intersection(i: i32, j: i32) -> Vec3 {
    Vec3::new(
        (i as f32 + 0.5) * BLOCK_PITCH,
        0.0,
        (j as f32 + 0.5) * BLOCK_PITCH,
    )
}

/// An intersection near `player` a squad can use, chosen by `roll`.
pub fn pick_intersection(player: Vec3, roll: u64) -> Option<Vec3> {
    let (bx, bz) = city::block_of(player);
    let mut options: Vec<Vec3> = Vec::new();
    for i in bx - 3..=bx + 2 {
        for j in bz - 3..=bz + 2 {
            let at = intersection(i, j);
            let d = at.distance(player.with_y(0.0));
            let inside = at.x.abs() < MAP_HALF - 60.0 && at.z.abs() < MAP_HALF - 60.0;
            if (SPAWN_MIN..=SPAWN_MAX).contains(&d) && inside && !in_avenue(at) {
                options.push(at);
            }
        }
    }
    (!options.is_empty()).then(|| options[(roll % options.len() as u64) as usize])
}

/// A squad's members for `roll`: mostly grunts, with ninjas, netrunners
/// and the odd heavy.
pub fn squad(roll: u64) -> Vec<EnemyArchetype> {
    let size = 2 + (roll % 3) as usize;
    (0..size)
        .map(|k| match (roll >> (8 + k * 7)) % 20 {
            0..=9 => EnemyArchetype::StreetGrunt,
            10..=13 => EnemyArchetype::CyberNinja,
            14..=16 => EnemyArchetype::Netrunner,
            _ => EnemyArchetype::ShieldedHeavy,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn direct_patrols(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: Local<f32>,
    mut seed: Local<u64>,
    tutorial: Option<Res<TutorialActive>>,
    players: Query<&Transform, With<LocalPredictedPlayer>>,
    patrols: Query<(Entity, &Transform), With<PatrolMember>>,
) {
    let Ok(player) = players.get_single() else {
        return;
    };
    let player = player.translation;
    for (entity, at) in &patrols {
        if at.translation.distance(player) > DESPAWN_DISTANCE {
            commands.entity(entity).despawn_recursive();
        }
    }
    if tutorial.is_some() || in_avenue(player) {
        *timer = 0.0;
        return;
    }
    *timer += time.delta_secs();
    if *timer < SPAWN_INTERVAL || patrols.iter().count() >= MAX_PATROL_ENEMIES {
        return;
    }
    *timer = 0.0;
    *seed = game_rpg::arsenal::mix(*seed ^ time.elapsed_secs_f64().to_bits());
    let Some(at) = pick_intersection(player, *seed) else {
        return;
    };
    let members = squad(*seed);
    info!(
        "syndicate patrol of {} at {:.0}, {:.0}",
        members.len(),
        at.x,
        at.z
    );
    for (k, archetype) in members.into_iter().enumerate() {
        let offset = Vec3::new((k as f32 - 1.5) * 2.2, STANDING_Y, (k % 2) as f32 * 2.0);
        let enemy = spawn_enemy(&mut commands, archetype, at + offset);
        commands.entity(enemy).insert(PatrolMember);
    }
}

pub struct PatrolPlugin;

impl Plugin for PatrolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            direct_patrols.run_if(in_state(crate::creator::GameFlow::Playing)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squads_meet_at_intersections_away_from_the_avenue() {
        let player = Vec3::new(800.0, 0.0, 600.0);
        for roll in 0..200 {
            let at = pick_intersection(player, roll).expect("an intersection");
            let d = at.distance(player);
            assert!((SPAWN_MIN..=SPAWN_MAX).contains(&d), "{d}");
            // On the road grid, never inside a block.
            let (bx, bz) = city::block_of(at);
            let c = city::block_center(bx, bz);
            let half = (BLOCK_PITCH - city::ROAD_WIDTH) * 0.5;
            assert!((at.x - c.x).abs() > half || (at.z - c.z).abs() > half);
            assert!(!in_avenue(at));
        }
        assert!(in_avenue(Vec3::new(0.0, 0.0, -100.0)));
    }

    #[test]
    fn squads_are_two_to_four_strong() {
        for roll in 0..500u64 {
            let s = squad(game_rpg::arsenal::mix(roll));
            assert!((2..=4).contains(&s.len()));
        }
    }
}
