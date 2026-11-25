//! The Neural Lattice in play: kills earn experience, levels earn points,
//! points buy passive nodes on a pannable tree screen (P), and the owned
//! nodes reshape the operative's health, shields, stamina and blade.

use crate::client_network::LocalPredictedPlayer;
use crate::creator::GameFlow;
use crate::tutorial::{load_progress, save_progress, ProgressPath};
use crate::visuals::PLAYER_MAX_HEALTH;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use game_ai::{BossDefeatedEvent, EnemyArchetype, EnemyDefeatedEvent};
use game_combat::MeleeHitEvent;
use game_parkour::FlowState;
use game_rpg::{
    level_progress, Branch, Health, IncomingDamageScale, NodeKind, PassiveProgress, PassiveStat,
    PassiveTree, Shields,
};
use game_ui::{Banner, BannerTone, MissionHud, ModalPause, ScreenOverlay};

/// XP for the Gorgon.
pub const BOSS_XP: u32 = 450;

/// XP for defeating one Syndicate enemy.
pub fn archetype_xp(archetype: EnemyArchetype) -> u32 {
    match archetype {
        EnemyArchetype::SwarmDrone => 20,
        EnemyArchetype::StreetGrunt => 30,
        EnemyArchetype::Netrunner => 45,
        EnemyArchetype::CyberNinja => 50,
        EnemyArchetype::ShieldedHeavy => 70,
        EnemyArchetype::MechaEnforcer => 120,
    }
}

/// The player's lattice and the tree screen's view.
#[derive(Resource, Debug, Clone)]
pub struct Lattice {
    pub progress: PassiveProgress,
    pub open: bool,
    pub pan: egui::Vec2,
    pub zoom: f32,
    /// Progress changed since the last save.
    pub dirty: bool,
}

impl Default for Lattice {
    fn default() -> Self {
        Self {
            progress: PassiveProgress::default(),
            open: false,
            pan: egui::Vec2::ZERO,
            zoom: 0.32,
            dirty: false,
        }
    }
}

impl Lattice {
    /// Adds experience; returns the new level if it went up.
    pub fn gain(&mut self, xp: u32) -> Option<u32> {
        let before = self.progress.level();
        self.progress.xp = self.progress.xp.saturating_add(xp);
        self.dirty = true;
        let after = self.progress.level();
        (after > before).then_some(after)
    }
}

pub fn load_lattice(mut lattice: ResMut<Lattice>, path: Res<ProgressPath>, tree: Res<PassiveTree>) {
    lattice.progress = load_progress(&path.0).passive.sanitized(&tree);
}

/// Kill and boss experience, level-up banners and heal-on-kill.
#[allow(clippy::too_many_arguments)]
pub fn grant_experience(
    mut lattice: ResMut<Lattice>,
    tree: Res<PassiveTree>,
    mut kills: EventReader<EnemyDefeatedEvent>,
    mut bosses: EventReader<BossDefeatedEvent>,
    mut hud: ResMut<MissionHud>,
    mut players: Query<(&mut Health, Option<&mut Shields>), With<LocalPredictedPlayer>>,
) {
    let mut gained = 0;
    let mut defeated = 0;
    for kill in kills.read() {
        gained += archetype_xp(kill.archetype);
        defeated += 1;
    }
    for _ in bosses.read() {
        gained += BOSS_XP;
        defeated += 1;
    }
    if defeated == 0 {
        return;
    }
    let bonuses = lattice.progress.bonuses(&tree);
    let heal = bonuses.get(PassiveStat::HealOnKill) * defeated as f32;
    let spark = bonuses.get(PassiveStat::ShieldOnKill) * defeated as f32;
    for (mut health, shields) in &mut players {
        if health.is_dead() {
            continue;
        }
        if heal > 0.0 {
            health.heal(heal);
        }
        if let (true, Some(mut shields)) = (spark > 0.0, shields) {
            shields.current = (shields.current + spark).min(shields.max);
        }
    }
    let gained = (gained as f32 * bonuses.scale(PassiveStat::XpGain)).round() as u32;
    if let Some(level) = lattice.gain(gained) {
        hud.banner = Some(Banner {
            title: format!("LEVEL {level}"),
            subtitle: "Neural point ready  ·  press P".into(),
            tone: BannerTone::Success,
        });
    }
}

/// The player's vitals and blade with the owned nodes applied.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AppliedLattice {
    pub max_health: f32,
    pub max_shields: f32,
    pub recharge: f32,
    pub stamina: f32,
    pub melee: f32,
    /// Multiplier on damage taken.
    pub taken: f32,
    /// Seconds before shields start recharging.
    pub shield_delay: f32,
}

impl AppliedLattice {
    pub fn from_progress(progress: &PassiveProgress, tree: &PassiveTree) -> Self {
        let b = progress.bonuses(tree);
        let shields = Shields::default();
        let flow = FlowState::default();
        Self {
            max_health: (PLAYER_MAX_HEALTH + b.get(PassiveStat::MaxHealth))
                * b.scale(PassiveStat::MaxHealthPercent),
            max_shields: (shields.max + b.get(PassiveStat::MaxShields))
                * b.scale(PassiveStat::MaxShieldsPercent),
            recharge: shields.recharge_rate * b.scale(PassiveStat::ShieldRecharge),
            stamina: flow.stamina_max + b.get(PassiveStat::Stamina),
            melee: b.scale(PassiveStat::MeleeDamage),
            // Reduction is capped: nothing makes the operative untouchable.
            taken: (1.0 - b.get(PassiveStat::DamageReduction) / 100.0).clamp(0.4, 2.0),
            shield_delay: shields.recharge_delay_timer.duration().as_secs_f32()
                * (1.0 - b.get(PassiveStat::ShieldDelay) / 100.0).clamp(0.2, 2.0),
        }
    }
}

/// Keeps the local player's stats in step with the lattice.
#[allow(clippy::type_complexity)]
pub fn apply_lattice(
    mut commands: Commands,
    lattice: Res<Lattice>,
    tree: Res<PassiveTree>,
    mut players: Query<
        (
            Entity,
            &mut Health,
            Option<&mut Shields>,
            Option<&mut FlowState>,
            Option<&AppliedLattice>,
        ),
        With<LocalPredictedPlayer>,
    >,
) {
    let wanted = AppliedLattice::from_progress(&lattice.progress, &tree);
    for (entity, mut health, shields, flow, applied) in &mut players {
        if applied == Some(&wanted) {
            continue;
        }
        health.set_max(wanted.max_health);
        if let Some(mut shields) = shields {
            shields.set_max(wanted.max_shields);
            shields.recharge_rate = wanted.recharge;
            shields
                .recharge_delay_timer
                .set_duration(std::time::Duration::from_secs_f32(wanted.shield_delay));
        }
        if let Some(mut flow) = flow {
            flow.stamina_max = wanted.stamina;
            flow.stamina_current = flow.stamina_current.min(wanted.stamina);
        }
        // Melee bonuses are applied per hit by the armory's weapon damage.
        commands
            .entity(entity)
            .insert((wanted, IncomingDamageScale(wanted.taken)));
    }
}

/// Health regeneration and life on melee hits from the lattice.
pub fn lattice_sustain(
    time: Res<Time>,
    lattice: Res<Lattice>,
    tree: Res<PassiveTree>,
    mut hits: EventReader<MeleeHitEvent>,
    mut players: Query<(Entity, &mut Health), With<LocalPredictedPlayer>>,
    mut cached: Local<Option<(u32, usize, f32, f32)>>,
) {
    // Recompute the two rates only when the lattice changes.
    let key = (lattice.progress.xp, lattice.progress.allocated.len());
    if cached.is_none_or(|c| (c.0, c.1) != key) {
        let b = lattice.progress.bonuses(&tree);
        *cached = Some((
            key.0,
            key.1,
            b.get(PassiveStat::HealthRegen),
            b.get(PassiveStat::LifeOnHit),
        ));
    }
    let (_, _, regen, on_hit) = cached.unwrap_or_default();
    for (entity, mut health) in &mut players {
        if health.is_dead() {
            continue;
        }
        let landed = hits.read().filter(|h| h.attacker == entity).count();
        let amount = regen * time.delta_secs() + on_hit * landed as f32;
        if amount > 0.0 {
            health.heal(amount);
        }
    }
}

/// Writes progress after changes (loading the file first so the
/// tutorial flag and anything else saved stay as they are).
pub fn save_lattice(mut lattice: ResMut<Lattice>, path: Res<ProgressPath>) {
    if !lattice.dirty {
        return;
    }
    lattice.dirty = false;
    let mut progress = load_progress(&path.0);
    progress.passive = lattice.progress.clone();
    if let Err(error) = save_progress(&path.0, &progress) {
        warn!("could not save passive tree progress: {error}");
    }
}

/// P opens and closes the tree; while open the game is paused and the
/// cursor is free.
pub fn toggle_lattice(
    keys: Res<ButtonInput<KeyCode>>,
    mut lattice: ResMut<Lattice>,
    mut overlay: ResMut<ScreenOverlay>,
    mut modal: ResMut<ModalPause>,
    pause: Res<game_ui::PauseMenuState>,
    armory: Option<Res<crate::armory::Armory>>,
) {
    // One full-screen panel at a time.
    if armory.is_some_and(|a| a.open) {
        return;
    }
    let toggle =
        keys.just_pressed(KeyCode::KeyP) || (lattice.open && keys.just_pressed(KeyCode::Escape));
    if toggle && !pause.is_open {
        lattice.open = !lattice.open;
        overlay.0 = lattice.open;
        modal.0 = lattice.open;
    }
}

const BACKDROP: egui::Color32 = egui::Color32::from_rgb(6, 8, 12);
const LINK_OFF: egui::Color32 = egui::Color32::from_rgb(38, 44, 54);
const TEXT: egui::Color32 = egui::Color32::from_rgb(214, 226, 236);
const MUTED: egui::Color32 = egui::Color32::from_rgb(120, 134, 148);

fn branch_color(branch: Branch) -> egui::Color32 {
    match branch {
        Branch::Core => egui::Color32::from_rgb(170, 190, 205),
        Branch::Blade => egui::Color32::from_rgb(255, 82, 96),
        Branch::Ghost => egui::Color32::from_rgb(70, 220, 240),
        Branch::Bulwark => egui::Color32::from_rgb(255, 184, 64),
        Branch::Aegis => egui::Color32::from_rgb(140, 150, 255),
        Branch::Reaver => egui::Color32::from_rgb(200, 80, 230),
        Branch::Pulse => egui::Color32::from_rgb(90, 230, 130),
    }
}

fn node_radius(kind: NodeKind) -> f32 {
    match kind {
        NodeKind::Origin => 16.0,
        NodeKind::Small => 7.0,
        NodeKind::Notable => 12.0,
        NodeKind::Keystone => 17.0,
    }
}

/// Key hints as keycaps: a hairline box with the key, then the action.
pub fn keycap_row(painter: &egui::Painter, at: egui::Pos2, hints: &[(&str, &str)]) {
    let font = egui::FontId::monospace(11.0);
    let mut x = at.x;
    for (key, action) in hints {
        let galley = painter.layout_no_wrap((*key).to_owned(), font.clone(), TEXT);
        let cap = egui::Rect::from_min_size(
            egui::pos2(x, at.y),
            egui::vec2(galley.size().x + 12.0, galley.size().y + 4.0),
        );
        painter.rect_stroke(cap, 4.0, egui::Stroke::new(1.0, TEXT.gamma_multiply(0.16)));
        painter.galley(cap.min + egui::vec2(6.0, 2.0), galley, TEXT);
        let label = painter.layout_no_wrap((*action).to_owned(), font.clone(), MUTED);
        let width = label.size().x;
        painter.galley(egui::pos2(cap.max.x + 6.0, at.y + 2.0), label, MUTED);
        x = cap.max.x + 6.0 + width + 16.0;
    }
}

/// Draws the tree; left click buys, right click refunds, drag pans,
/// wheel zooms.
pub fn draw_lattice(
    mut contexts: EguiContexts,
    mut lattice: ResMut<Lattice>,
    tree: Res<PassiveTree>,
) {
    if !lattice.open {
        return;
    }
    let ctx = contexts.ctx_mut();
    let lattice = &mut *lattice;
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(BACKDROP))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
            if response.dragged() {
                lattice.pan += response.drag_delta();
            }
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 && response.hovered() {
                lattice.zoom = (lattice.zoom * (1.0 + scroll * 0.0015)).clamp(0.1, 2.5);
            }
            let center = rect.center() + lattice.pan;
            let zoom = lattice.zoom;
            let to_screen = |p: Vec2| center + egui::vec2(p.x, -p.y) * zoom;
            let painter = ui.painter_at(rect);
            let progress = &lattice.progress;
            let owned: Vec<bool> = (0..tree.nodes.len())
                .map(|i| progress.owns(&tree, i))
                .collect();
            let can_buy = progress.points_available() > 0;

            let view = rect.expand(40.0);
            for &(a, b) in &tree.links {
                let (pa, pb) = (
                    to_screen(tree.nodes[a].position),
                    to_screen(tree.nodes[b].position),
                );
                if !view.contains(pa) && !view.contains(pb) {
                    continue;
                }
                let lit = owned[a] && owned[b];
                let color = if lit {
                    branch_color(tree.nodes[b].branch)
                } else {
                    LINK_OFF
                };
                painter.line_segment(
                    [pa, pb],
                    egui::Stroke::new(if lit { 3.0 } else { 2.0 } * zoom.sqrt(), color),
                );
            }

            let pointer = response.hover_pos();
            let mut hovered = None;
            for (i, node) in tree.nodes.iter().enumerate() {
                let at = to_screen(node.position);
                if !view.contains(at) {
                    continue;
                }
                let r = node_radius(node.kind) * zoom.sqrt();
                let color = branch_color(node.branch);
                let reachable = progress.reachable(&tree, i);
                let (fill, stroke) = if owned[i] {
                    (color, egui::Stroke::new(2.0, egui::Color32::WHITE))
                } else if reachable && can_buy {
                    (color.gamma_multiply(0.25), egui::Stroke::new(2.0, color))
                } else {
                    (BACKDROP, egui::Stroke::new(1.5, color.gamma_multiply(0.45)))
                };
                if node.kind == NodeKind::Keystone {
                    // Keystones are diamonds.
                    let d = r * 1.25;
                    let points = vec![
                        at + egui::vec2(0.0, -d),
                        at + egui::vec2(d, 0.0),
                        at + egui::vec2(0.0, d),
                        at + egui::vec2(-d, 0.0),
                    ];
                    painter.add(egui::Shape::convex_polygon(points, fill, stroke));
                } else {
                    painter.circle(at, r, fill, stroke);
                }
                if matches!(
                    node.kind,
                    NodeKind::Notable | NodeKind::Keystone | NodeKind::Origin
                ) && zoom > 0.7
                {
                    painter.text(
                        at + egui::vec2(0.0, r + 6.0),
                        egui::Align2::CENTER_TOP,
                        node.name.clone(),
                        egui::FontId::monospace(10.5),
                        if owned[i] { color } else { MUTED },
                    );
                }
                if pointer.is_some_and(|p| p.distance(at) <= r + 3.0) {
                    hovered = Some(i);
                }
            }

            // Buy or refund the node under the pointer.
            if let Some(i) = hovered {
                let id = tree.nodes[i].id.clone();
                if response.clicked() && lattice.progress.allocate(&tree, &id).is_ok() {
                    lattice.dirty = true;
                }
                if response.secondary_clicked() && lattice.progress.refund(&tree, &id).is_ok() {
                    lattice.dirty = true;
                }
                let node = &tree.nodes[i];
                egui::show_tooltip_at_pointer(
                    ui.ctx(),
                    ui.layer_id(),
                    egui::Id::new("lattice_tip"),
                    |ui| {
                        ui.label(
                            egui::RichText::new(node.name.clone())
                                .color(branch_color(node.branch))
                                .size(15.0)
                                .strong(),
                        );
                        let kind = match node.kind {
                            NodeKind::Origin => "Origin",
                            NodeKind::Small => "Node",
                            NodeKind::Notable => "Notable",
                            NodeKind::Keystone => "Keystone",
                        };
                        ui.label(egui::RichText::new(kind).color(MUTED).size(11.0));
                        for &(stat, value) in &node.stats {
                            ui.label(
                                egui::RichText::new(stat.describe(value))
                                    .color(TEXT)
                                    .size(13.0),
                            );
                        }
                        let hint = if lattice.progress.owns(&tree, i) {
                            "Right click to refund"
                        } else if lattice.progress.reachable(&tree, i) {
                            if lattice.progress.points_available() > 0 {
                                "Left click to allocate"
                            } else {
                                "Earn a level to allocate"
                            }
                        } else {
                            "Connect to an owned node first"
                        };
                        ui.label(egui::RichText::new(hint).color(MUTED).size(11.0));
                    },
                );
            }

            // Header: level, XP bar and points; footer: controls.
            let progress = &lattice.progress;
            let (into, needed) = level_progress(progress.xp);
            let header = rect.left_top() + egui::vec2(32.0, 28.0);
            painter.text(
                header,
                egui::Align2::LEFT_TOP,
                "Neural lattice",
                egui::FontId::monospace(20.0),
                TEXT,
            );
            painter.text(
                header + egui::vec2(0.0, 32.0),
                egui::Align2::LEFT_TOP,
                format!(
                    "Level {}  ·  {} / {} xp  ·  {} point{} to spend",
                    progress.level(),
                    into,
                    needed,
                    progress.points_available(),
                    if progress.points_available() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ),
                egui::FontId::monospace(13.0),
                if progress.points_available() > 0 {
                    egui::Color32::from_rgb(255, 184, 64)
                } else {
                    MUTED
                },
            );
            let bar =
                egui::Rect::from_min_size(header + egui::vec2(0.0, 56.0), egui::vec2(320.0, 4.0));
            painter.rect_filled(bar, 0.0, LINK_OFF);
            let mut fill = bar;
            fill.set_width(320.0 * into as f32 / needed.max(1) as f32);
            painter.rect_filled(fill, 0.0, egui::Color32::from_rgb(70, 220, 240));
            keycap_row(
                &painter,
                rect.left_bottom() + egui::vec2(32.0, -40.0),
                &[
                    ("LMB", "Allocate"),
                    ("RMB", "Refund"),
                    ("Drag", "Pan"),
                    ("Wheel", "Zoom"),
                    ("P", "Close"),
                ],
            );
        });
}

pub struct LatticePlugin;

impl Plugin for LatticePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PassiveTree>()
            .init_resource::<Lattice>()
            .init_resource::<ProgressPath>()
            .add_systems(Startup, load_lattice)
            .add_systems(
                Update,
                (
                    toggle_lattice.run_if(in_state(GameFlow::Playing)),
                    grant_experience,
                    apply_lattice,
                    lattice_sustain,
                    save_lattice,
                )
                    .chain(),
            )
            .add_systems(Update, draw_lattice.run_if(in_state(GameFlow::Playing)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kills_level_up_and_bonuses_reach_the_player() {
        let tree = PassiveTree::lattice();
        let mut lattice = Lattice::default();
        let mut level = None;
        for _ in 0..10 {
            level = level.or(lattice.gain(archetype_xp(EnemyArchetype::StreetGrunt)));
        }
        assert_eq!(level, Some(2), "ten grunts reach level 2");
        assert!(lattice.dirty);
        let base = AppliedLattice::from_progress(&lattice.progress, &tree);
        assert_eq!(base.max_health, PLAYER_MAX_HEALTH);
        assert_eq!(base.melee, 1.0);
        // Walk from the origin to the Bulwark gateway: its path grants health.
        let gateway = tree
            .nodes
            .iter()
            .position(|n| n.name == "Bulwark Gateway")
            .expect("gateway");
        let mut route = vec![gateway];
        while !tree.neighbours(*route.last().unwrap()).contains(&0) {
            let at = *route.last().unwrap();
            let closer = *tree
                .neighbours(at)
                .iter()
                .min_by(|a, b| {
                    tree.nodes[**a]
                        .position
                        .length()
                        .total_cmp(&tree.nodes[**b].position.length())
                })
                .expect("towards the origin");
            route.push(closer);
        }
        lattice.progress.xp = 1_000_000;
        for node in route.iter().rev() {
            let id = tree.nodes[*node].id.clone();
            lattice.progress.allocate(&tree, &id).expect("connected");
        }
        let tougher = AppliedLattice::from_progress(&lattice.progress, &tree);
        assert!(tougher.max_health > base.max_health);
    }

    #[test]
    fn the_boss_is_worth_a_lot() {
        assert!(BOSS_XP > archetype_xp(EnemyArchetype::MechaEnforcer) * 3);
    }
}
