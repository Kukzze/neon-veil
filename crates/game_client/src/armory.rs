//! The armory in play: weapons drop from the Syndicate, the operative
//! picks them up by walking over them, and the equipped weapon turns every
//! katana hit into elemental damage with crits, statuses and sustain.
//! I opens the Armory screen to compare, equip and salvage.

use crate::client_network::LocalPredictedPlayer;
use crate::creator::GameFlow;
use crate::lattice::Lattice;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use game_ai::{BossDefeatedEvent, BossGorgon, EnemyDefeatedEvent};
use game_combat::combat_dummy::spawn_floating_combat_text;
use game_combat::status_effects::{ActiveStatusEffects, StatusEffect, StatusEffectType};
use game_combat::{MeleeCombatState, MeleeHitEvent, MeleeWeaponClass};
use game_rpg::{
    DamageEvent, Element, Health, MeleeDamageScale, PassiveStat, PassiveTree, Rarity, Shields,
    WeaponClass, WeaponItem, WeaponStats,
};
use game_ui::{
    Banner, BannerTone, FireModeUi, MissionHud, ModalPause, ScreenOverlay, WeaponNameUi,
};
use serde::{Deserialize, Serialize};

/// Weapons kept at most; drops past this are refused.
pub const ARMORY_CAPACITY: usize = 80;
/// Chance a defeated enemy drops a weapon, percent.
const DROP_CHANCE: u64 = 45;
/// Pick-up radius around a dropped weapon, metres.
const PICKUP_RADIUS: f32 = 2.0;

/// Saved weapons and which one is equipped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArmoryData {
    pub weapons: Vec<WeaponItem>,
    pub equipped: usize,
}

impl Default for ArmoryData {
    fn default() -> Self {
        Self {
            weapons: vec![WeaponItem::starter()],
            equipped: 0,
        }
    }
}

impl ArmoryData {
    /// Drops unknown bases and mods (older saves), keeps the starter.
    pub fn sanitized(mut self) -> Self {
        let bases = game_rpg::weapon_bases().len();
        let mods = game_rpg::weapon_mods().len() as u32;
        self.weapons.retain(|w| {
            usize::from(w.base) < bases && w.mods.iter().all(|(id, r)| *id < mods && r.is_finite())
        });
        self.weapons.truncate(ARMORY_CAPACITY);
        if self.weapons.is_empty() {
            self.weapons.push(WeaponItem::starter());
        }
        self.equipped = self.equipped.min(self.weapons.len() - 1);
        self
    }

    pub fn equipped(&self) -> &WeaponItem {
        &self.weapons[self.equipped.min(self.weapons.len() - 1)]
    }

    /// Adds a weapon; false when the armory is full.
    pub fn add(&mut self, weapon: WeaponItem) -> bool {
        if self.weapons.len() >= ARMORY_CAPACITY {
            return false;
        }
        self.weapons.push(weapon);
        true
    }

    /// Salvages a weapon (never the equipped one).
    pub fn salvage(&mut self, index: usize) -> bool {
        if index >= self.weapons.len() || index == self.equipped || self.weapons.len() == 1 {
            return false;
        }
        self.weapons.remove(index);
        if self.equipped > index {
            self.equipped -= 1;
        }
        true
    }
}

/// The player's armory and the screen's state.
#[derive(Resource, Debug, Default)]
pub struct Armory {
    pub data: ArmoryData,
    pub open: bool,
    pub selected: usize,
    pub dirty: bool,
}

/// Combat stats of the equipped weapon, refreshed when it changes.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct EquippedStats(pub WeaponStats);

/// The combo set a weapon class swings with.
pub fn combo_class(class: WeaponClass) -> MeleeWeaponClass {
    match class {
        WeaponClass::Katana | WeaponClass::Spear => MeleeWeaponClass::CyberKatana,
        WeaponClass::DualBlades => MeleeWeaponClass::MantisDualBlades,
        WeaponClass::Baton => MeleeWeaponClass::ShockBaton,
        WeaponClass::Gauntlet | WeaponClass::Hammer => MeleeWeaponClass::KineticHeavyGauntlet,
        WeaponClass::Whip | WeaponClass::Scythe => MeleeWeaponClass::MonofilamentWhip,
        WeaponClass::Claymore | WeaponClass::Axe => MeleeWeaponClass::ThermalClaymores,
    }
}

/// Stats with the class's own combo damage factored out (combo sets
/// already hit harder or softer by class).
pub fn equipped_stats(item: &WeaponItem) -> WeaponStats {
    let mut stats = item.stats();
    stats.damage /= item.base().class.profile().0;
    stats
}

/// A weapon base equipped for one session (`--weapon`), never saved.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrialWeapon(pub u16);

pub fn load_armory(
    mut armory: ResMut<Armory>,
    path: Res<crate::tutorial::ProgressPath>,
    trial: Option<Res<TrialWeapon>>,
) {
    armory.data = crate::tutorial::load_progress(&path.0).armory.sanitized();
    if let Some(trial) = trial {
        if usize::from(trial.0) < game_rpg::weapon_bases().len() {
            // A separate armory so nothing saved is touched.
            armory.data = ArmoryData {
                weapons: vec![WeaponItem {
                    base: trial.0,
                    rarity: Rarity::Rare,
                    mods: Vec::new(),
                }],
                equipped: 0,
            };
        }
    }
}

pub fn save_armory(
    mut armory: ResMut<Armory>,
    path: Res<crate::tutorial::ProgressPath>,
    trial: Option<Res<TrialWeapon>>,
) {
    if !armory.dirty || trial.is_some() {
        return;
    }
    armory.dirty = false;
    let mut progress = crate::tutorial::load_progress(&path.0);
    progress.armory = armory.data.clone();
    if let Err(error) = crate::tutorial::save_progress(&path.0, &progress) {
        warn!("could not save the armory: {error}");
    }
}

/// Keeps the equipped stats, the combo set and the HUD weapon card in
/// step with the armory.
#[allow(clippy::type_complexity)]
pub fn sync_equipped(
    mut commands: Commands,
    armory: Res<Armory>,
    mut stats: ResMut<EquippedStats>,
    mut players: Query<(Entity, &mut MeleeCombatState), With<LocalPredictedPlayer>>,
    mut texts: ParamSet<(
        Query<(&mut Text, &mut TextColor), With<WeaponNameUi>>,
        Query<&mut Text, With<FireModeUi>>,
    )>,
) {
    let item = armory.data.equipped();
    let wanted = EquippedStats(equipped_stats(item));
    if *stats != wanted {
        *stats = wanted;
    }
    let class = combo_class(item.base().class);
    for (entity, mut melee) in &mut players {
        // The player's own hits deal no base damage: weapon_hits deals it
        // all, split by element.
        commands.entity(entity).insert(MeleeDamageScale(0.0));
        if melee.weapon_class != class && melee.attack_phase == game_combat::AttackPhase::Idle {
            melee.weapon_class = class;
        }
    }
    let name = item.base().name.to_uppercase();
    let [r, g, b] = item.rarity.color();
    for (mut text, mut color) in &mut texts.p0() {
        if text.0 != name {
            text.0.clone_from(&name);
        }
        let want = Color::srgb_u8(r, g, b);
        if color.0 != want {
            color.0 = want;
        }
    }
    let base = item.base();
    let line = format!(
        "{} // {}",
        base.element.label().to_uppercase(),
        base.class.label().to_uppercase()
    );
    for mut text in &mut texts.p1() {
        if text.0 != line {
            text.0.clone_from(&line);
        }
    }
}

/// Small xorshift for crit and status rolls.
fn next_roll(state: &mut u64) -> f32 {
    if *state == 0 {
        *state = 0x5EED_1234_ABCD_9876;
    }
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u64 << 24) as f32 * 100.0
}

/// Turns each of the player's melee hits into the weapon's damage: split
/// by element, scaled by the passive tree, critical rolls, statuses and
/// life on hit.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn weapon_hits(
    mut commands: Commands,
    mut hits: EventReader<MeleeHitEvent>,
    mut damage: EventWriter<DamageEvent>,
    stats: Res<EquippedStats>,
    lattice: Res<Lattice>,
    tree: Res<PassiveTree>,
    mut rng: Local<u64>,
    players: Query<Entity, With<LocalPredictedPlayer>>,
    mut healths: Query<&mut Health>,
    mut statuses: Query<&mut ActiveStatusEffects>,
    places: Query<&GlobalTransform>,
    bosses: Query<(), With<BossGorgon>>,
) {
    let Ok(player) = players.get_single() else {
        hits.clear();
        return;
    };
    let s = stats.0;
    let b = lattice.progress.bonuses(&tree);
    let melee = b.get(PassiveStat::MeleeDamage);
    let all = b.get(PassiveStat::ElementalDamage);
    let bonus = [
        melee,
        melee + all + b.get(PassiveStat::FireDamage),
        melee + all + b.get(PassiveStat::FrostDamage),
        melee + all + b.get(PassiveStat::ShockDamage),
        melee + all + b.get(PassiveStat::ToxinDamage),
        melee + all + b.get(PassiveStat::VoidDamage),
    ];
    for hit in hits.read().filter(|h| h.attacker == player) {
        let mut dealt = s.hit(hit.damage, &bonus, next_roll(&mut rng));
        let mut extra = 1.0;
        if bosses.contains(hit.target) {
            extra += s.boss_pct / 100.0;
        }
        if let Ok(h) = healths.get(hit.target) {
            if h.fraction() < 1.0 / 3.0 {
                extra += s.execute_pct / 100.0;
            }
        }
        for v in &mut dealt.by_element {
            *v *= extra;
        }
        let total = dealt.total();
        damage.send(DamageEvent {
            target: hit.target,
            source: Some(player),
            amount: total,
        });
        if let Ok(at) = places.get(hit.target) {
            spawn_floating_combat_text(
                &mut commands,
                at.translation() + Vec3::Y,
                total,
                dealt.critical,
            );
        }
        // Statuses: ignite, chill, overload, corrode.
        for (element, kind) in [
            (Element::Fire, StatusEffectType::ThermalBurn),
            (Element::Frost, StatusEffectType::CryoGlitch),
            (Element::Shock, StatusEffectType::ShockEmp),
            (Element::Toxin, StatusEffectType::CorrosiveAcid),
        ] {
            let i = element.index();
            if s.status_chance[i] <= 0.0 || next_roll(&mut rng) >= s.status_chance[i] {
                continue;
            }
            let potency = kind.default_potency() * (1.0 + s.status_potency[i] / 100.0);
            let effect = StatusEffect::with(kind, kind.default_duration(), potency);
            match statuses.get_mut(hit.target) {
                Ok(mut active) => active.apply_effect(effect),
                Err(_) => {
                    commands.entity(hit.target).insert(ActiveStatusEffects {
                        effects: vec![effect],
                    });
                }
            }
        }
        if s.life_on_hit > 0.0 {
            if let Ok(mut health) = healths.get_mut(player) {
                if !health.is_dead() {
                    health.heal(s.life_on_hit);
                }
            }
        }
    }
}

/// A weapon lying in the street.
#[derive(Component, Debug, Clone)]
pub struct LootDrop {
    pub item: WeaponItem,
    pub age: f32,
}

/// Meshes and materials for dropped weapons.
#[derive(Resource)]
pub struct LootAssets {
    beam: Handle<Mesh>,
    blade: Handle<Mesh>,
    rarity: [Handle<StandardMaterial>; 4],
}

impl FromWorld for LootAssets {
    fn from_world(world: &mut World) -> Self {
        let (beam, blade) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (
                meshes.add(Cylinder::new(0.05, 1.0)),
                meshes.add(Cuboid::new(0.06, 0.06, 1.0)),
            )
        };
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let rarity = [
            Rarity::Common,
            Rarity::Enhanced,
            Rarity::Rare,
            Rarity::Prototype,
        ]
        .map(|r| {
            let [cr, cg, cb] = r.color();
            let c = Color::srgb_u8(cr, cg, cb).to_linear();
            materials.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: LinearRgba::rgb(c.red * 6.0, c.green * 6.0, c.blue * 6.0),
                unlit: true,
                ..default()
            })
        });
        Self {
            beam,
            blade,
            rarity,
        }
    }
}

fn rarity_index(r: Rarity) -> usize {
    match r {
        Rarity::Common => 0,
        Rarity::Enhanced => 1,
        Rarity::Rare => 2,
        Rarity::Prototype => 3,
    }
}

fn spawn_drop(commands: &mut Commands, assets: &LootAssets, at: Vec3, item: WeaponItem) {
    let material = assets.rarity[rarity_index(item.rarity)].clone();
    let height = 1.5 + rarity_index(item.rarity) as f32 * 1.5;
    commands
        .spawn((
            Name::new("Weapon Drop"),
            LootDrop { item, age: 0.0 },
            Transform::from_translation(at.with_y(0.25)),
            Visibility::default(),
        ))
        .with_children(|drop| {
            // A light beam marks it from across the street.
            drop.spawn((
                Mesh3d(assets.beam.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(0.0, height * 0.5, 0.0).with_scale(Vec3::new(1.0, height, 1.0)),
                bevy::pbr::NotShadowCaster,
            ));
            drop.spawn((
                Mesh3d(assets.blade.clone()),
                MeshMaterial3d(material),
                Transform::from_xyz(0.0, 0.35, 0.0),
                bevy::pbr::NotShadowCaster,
            ));
        });
}

/// Defeated enemies sometimes drop a weapon; the Gorgon always drops two
/// rare or better.
#[allow(clippy::too_many_arguments)]
pub fn drop_loot(
    mut commands: Commands,
    assets: Res<LootAssets>,
    lattice: Res<Lattice>,
    time: Res<Time>,
    mut kills: EventReader<EnemyDefeatedEvent>,
    mut bosses: EventReader<BossDefeatedEvent>,
    players: Query<&GlobalTransform, With<LocalPredictedPlayer>>,
    mut counter: Local<u64>,
) {
    let level = lattice.progress.level();
    let seed = |counter: &mut u64| {
        *counter += 1;
        game_rpg::arsenal::mix(*counter ^ (time.elapsed_secs_f64().to_bits()))
    };
    for kill in kills.read() {
        let s = seed(&mut counter);
        if s % 100 < DROP_CHANCE {
            let item = WeaponItem::roll(s, level, Rarity::Common);
            spawn_drop(&mut commands, &assets, kill.position, item);
        }
    }
    let near = players.iter().next().map(|p| p.translation());
    for _ in bosses.read() {
        let at = near.unwrap_or(Vec3::ZERO);
        for k in 0..2 {
            let s = seed(&mut counter);
            let item = WeaponItem::roll(s, level + 6, Rarity::Rare);
            spawn_drop(
                &mut commands,
                &assets,
                at + Vec3::new(1.5 - 3.0 * k as f32, 0.0, -2.5),
                item,
            );
        }
    }
}

/// Spins drops and picks them up when the operative walks over them.
pub fn collect_loot(
    mut commands: Commands,
    time: Res<Time>,
    mut armory: ResMut<Armory>,
    mut hud: ResMut<MissionHud>,
    mut drops: Query<(Entity, &mut LootDrop, &mut Transform)>,
    players: Query<&GlobalTransform, With<LocalPredictedPlayer>>,
) {
    let player = players.iter().next().map(|p| p.translation());
    for (entity, mut drop, mut transform) in &mut drops {
        drop.age += time.delta_secs();
        transform.rotation = Quat::from_rotation_y(drop.age * 1.6);
        let Some(player) = player else {
            continue;
        };
        if player
            .with_y(0.0)
            .distance(transform.translation.with_y(0.0))
            > PICKUP_RADIUS
        {
            continue;
        }
        let item = drop.item.clone();
        let name = item.name();
        let rarity = item.rarity;
        if armory.data.add(item) {
            armory.dirty = true;
            commands.entity(entity).despawn_recursive();
            if rarity >= Rarity::Rare {
                hud.banner = Some(Banner {
                    title: format!("{} WEAPON", rarity.label().to_uppercase()),
                    subtitle: format!("{name}  //  press I"),
                    tone: BannerTone::Success,
                });
            }
        } else if drop.age % 4.0 < time.delta_secs() {
            hud.banner = Some(Banner {
                title: "ARMORY FULL".into(),
                subtitle: "Salvage weapons in the Armory (I)".into(),
                tone: BannerTone::Danger,
            });
        }
    }
}

/// Heal, shields and experience on kill from the equipped weapon.
pub fn weapon_kill_rewards(
    stats: Res<EquippedStats>,
    mut lattice: ResMut<Lattice>,
    mut kills: EventReader<EnemyDefeatedEvent>,
    mut players: Query<(&mut Health, Option<&mut Shields>), With<LocalPredictedPlayer>>,
) {
    let count = kills.read().count();
    if count == 0 {
        return;
    }
    let s = stats.0;
    for (mut health, shields) in &mut players {
        if health.is_dead() {
            continue;
        }
        if s.heal_on_kill > 0.0 {
            health.heal(s.heal_on_kill * count as f32);
        }
        if let (true, Some(mut shields)) = (s.shield_on_kill > 0.0, shields) {
            shields.current = (shields.current + s.shield_on_kill * count as f32).min(shields.max);
        }
    }
    if s.xp_pct > 0.0 {
        let bonus = (count as f32 * 30.0 * s.xp_pct / 100.0).round() as u32;
        lattice.gain(bonus);
    }
}

/// I opens and closes the Armory; the game pauses while it is open.
pub fn toggle_armory(
    keys: Res<ButtonInput<KeyCode>>,
    mut armory: ResMut<Armory>,
    lattice: Res<Lattice>,
    mut overlay: ResMut<ScreenOverlay>,
    mut modal: ResMut<ModalPause>,
    pause: Res<game_ui::PauseMenuState>,
) {
    if lattice.open {
        return;
    }
    let toggle =
        keys.just_pressed(KeyCode::KeyI) || (armory.open && keys.just_pressed(KeyCode::Escape));
    if toggle && !pause.is_open {
        armory.open = !armory.open;
        overlay.0 = armory.open;
        modal.0 = armory.open;
    }
}

const PANEL: egui::Color32 = egui::Color32::from_rgb(9, 12, 17);
const TEXT: egui::Color32 = egui::Color32::from_rgb(214, 226, 236);
const MUTED: egui::Color32 = egui::Color32::from_rgb(120, 134, 148);

fn color(rgb: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// The Armory screen: weapons on the left, details and actions right.
pub fn draw_armory(mut contexts: EguiContexts, mut armory: ResMut<Armory>) {
    if !armory.open {
        return;
    }
    let ctx = contexts.ctx_mut();
    let armory = &mut *armory;
    egui::SidePanel::left("armory_list")
        .exact_width(420.0)
        .frame(
            egui::Frame::none()
                .fill(PANEL)
                .inner_margin(egui::Margin::same(20.0)),
        )
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("ARMORY")
                    .size(22.0)
                    .color(TEXT)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!(
                    "{} / {ARMORY_CAPACITY} weapons    I / ESC close",
                    armory.data.weapons.len()
                ))
                .size(12.0)
                .color(MUTED),
            );
            ui.add_space(10.0);
            let mut order: Vec<usize> = (0..armory.data.weapons.len()).collect();
            order.sort_by(|a, b| {
                let (wa, wb) = (&armory.data.weapons[*a], &armory.data.weapons[*b]);
                wb.rarity
                    .cmp(&wa.rarity)
                    .then(wb.base().tier.cmp(&wa.base().tier))
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                for i in order {
                    let w = &armory.data.weapons[i];
                    let base = w.base();
                    let equipped = i == armory.data.equipped;
                    let label = format!(
                        "{}{}\n{}  T{}  {}",
                        if equipped { "> " } else { "" },
                        w.name(),
                        base.class.label(),
                        base.tier,
                        base.element.label()
                    );
                    let text = egui::RichText::new(label)
                        .color(color(w.rarity.color()))
                        .size(13.0);
                    if ui.selectable_label(armory.selected == i, text).clicked() {
                        armory.selected = i;
                    }
                    ui.add_space(4.0);
                }
            });
        });
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(4, 6, 10, 235))
                .inner_margin(egui::Margin::same(36.0)),
        )
        .show(ctx, |ui| {
            let index = armory.selected.min(armory.data.weapons.len() - 1);
            let w = armory.data.weapons[index].clone();
            let base = w.base();
            let s = equipped_stats(&w);
            ui.label(
                egui::RichText::new(w.name())
                    .size(24.0)
                    .color(color(w.rarity.color()))
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!(
                    "{} {}  //  Tier {}  //  {}",
                    w.rarity.label(),
                    base.class.label(),
                    base.tier,
                    base.name
                ))
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(12.0);
            let dps = 100.0 * s.damage * s.attack_speed * (1.0 + s.physical_pct / 100.0);
            for (k, v) in [
                ("Damage", format!("{:.0}%", s.damage * 100.0)),
                ("Attack speed", format!("{:.2}", s.attack_speed)),
                ("Weapon rating", format!("{dps:.0}")),
                (
                    "Element",
                    if base.element == Element::Physical {
                        "Physical".to_string()
                    } else {
                        format!(
                            "{} ({:.0}% of each hit)",
                            base.element.label(),
                            base.element_share * 100.0
                        )
                    },
                ),
                (
                    "Critical",
                    format!("{:.1}% for {:.0}%", s.crit_chance, s.crit_multiplier),
                ),
            ] {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(k).size(13.0).color(MUTED));
                    ui.label(egui::RichText::new(v).size(13.0).color(TEXT));
                });
            }
            ui.add_space(12.0);
            if w.mods.is_empty() {
                ui.label(egui::RichText::new("No mods").size(13.0).color(MUTED));
            }
            for (line, prefix) in w.mod_lines() {
                let tint = if prefix {
                    egui::Color32::from_rgb(140, 200, 255)
                } else {
                    egui::Color32::from_rgb(200, 170, 255)
                };
                ui.label(egui::RichText::new(line).size(14.0).color(tint));
            }
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                let equipped = index == armory.data.equipped;
                if ui
                    .add_enabled(
                        !equipped,
                        egui::Button::new(egui::RichText::new("EQUIP").size(15.0)),
                    )
                    .clicked()
                {
                    armory.data.equipped = index;
                    armory.dirty = true;
                }
                if ui
                    .add_enabled(
                        !equipped,
                        egui::Button::new(egui::RichText::new("SALVAGE").size(15.0)),
                    )
                    .clicked()
                    && armory.data.salvage(index)
                {
                    armory.selected = armory.data.equipped;
                    armory.dirty = true;
                }
            });
        });
}

pub struct ArmoryPlugin;

impl Plugin for ArmoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Armory>()
            .init_resource::<EquippedStats>()
            .init_resource::<LootAssets>()
            .init_resource::<crate::tutorial::ProgressPath>()
            .add_systems(Startup, load_armory)
            .add_systems(
                Update,
                (
                    toggle_armory.run_if(in_state(GameFlow::Playing)),
                    sync_equipped,
                    weapon_hits,
                    drop_loot,
                    collect_loot,
                    weapon_kill_rewards,
                    save_armory,
                    draw_armory.run_if(in_state(GameFlow::Playing)),
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armory_keeps_the_starter_and_the_equipped_weapon() {
        let mut data = ArmoryData::default();
        assert_eq!(data.equipped().base().class, WeaponClass::Katana);
        assert!(!data.salvage(0), "the equipped weapon stays");
        let drop = WeaponItem::roll(9, 10, Rarity::Rare);
        assert!(data.add(drop.clone()));
        data.equipped = 1;
        assert!(data.salvage(0));
        assert_eq!(data.equipped, 0);
        assert_eq!(data.equipped(), &drop);
        for k in 0..200 {
            data.add(WeaponItem::roll(k, 3, Rarity::Common));
        }
        assert_eq!(data.weapons.len(), ARMORY_CAPACITY);
    }

    #[test]
    fn hostile_saves_are_cleaned() {
        let data = ArmoryData {
            weapons: vec![WeaponItem {
                base: 60_000,
                rarity: Rarity::Rare,
                mods: vec![],
            }],
            equipped: 9,
        }
        .sanitized();
        assert_eq!(data.weapons, vec![WeaponItem::starter()]);
        assert_eq!(data.equipped, 0);
    }

    #[test]
    fn heavy_combos_are_not_double_counted() {
        let hammer = game_rpg::weapon_bases()
            .iter()
            .find(|b| b.class == WeaponClass::Hammer && b.tier == 1)
            .expect("a hammer");
        let item = WeaponItem {
            base: hammer.id,
            rarity: Rarity::Common,
            mods: vec![],
        };
        // Tier-1 weapons sit near 1x on top of their own combo damage.
        assert!((equipped_stats(&item).damage - 1.0).abs() < 0.12);
        assert_eq!(
            combo_class(WeaponClass::Hammer),
            MeleeWeaponClass::KineticHeavyGauntlet
        );
    }
}
