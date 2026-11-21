//! The 8-skill metrics panel (hotkey K).

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use game_core::identifiers::PlayerCharacterMarker;
use game_rpg::xp_table::{level_to_xp, xp_to_level, MAX_LEVEL, XP_CAP};
use game_rpg::{CharacterSkills, SkillType};

pub const MAX_TOTAL_LEVEL: u32 = 8 * MAX_LEVEL as u32;
pub const MAX_TOTAL_XP: u64 = 8 * XP_CAP as u64;

#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct SkillsModalState {
    pub is_open: bool,
    pub selected_skill: Option<SkillType>,
}

impl SkillsModalState {
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }
}

pub fn skill_display_name(skill: SkillType) -> &'static str {
    match skill {
        SkillType::Agility => "Agility",
        SkillType::CyberMelee => "Cyber-Melee",
        SkillType::Ballistics => "Ballistics",
        SkillType::Netrunning => "Netrunning",
        SkillType::Engineering => "Engineering",
        SkillType::Shadow => "Shadow",
        SkillType::Endurance => "Endurance",
        SkillType::StreetCredit => "Street Credit",
    }
}

/// One row of the panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillRow {
    pub skill: SkillType,
    pub level: u8,
    pub xp: u32,
    /// XP at which the current level began.
    pub level_start_xp: u32,
    /// XP needed for the next level; `None` at 99.
    pub next_level_xp: Option<u32>,
    /// Progress through the current level in `[0, 1]`; 1.0 at 99.
    pub progress: f32,
    pub mastered: bool,
}

impl SkillRow {
    pub fn from_xp(skill: SkillType, xp: u32) -> Self {
        let level = xp_to_level(xp);
        let level_start_xp = level_to_xp(level);
        if level >= MAX_LEVEL {
            return Self {
                skill,
                level,
                xp,
                level_start_xp,
                next_level_xp: None,
                progress: 1.0,
                mastered: true,
            };
        }
        let next = level_to_xp(level + 1);
        let span = (next - level_start_xp).max(1) as f32;
        Self {
            skill,
            level,
            xp,
            level_start_xp,
            next_level_xp: Some(next),
            progress: ((xp - level_start_xp) as f32 / span).clamp(0.0, 1.0),
            mastered: false,
        }
    }

    pub fn xp_to_next_level(&self) -> u32 {
        self.next_level_xp
            .map_or(0, |next| next.saturating_sub(self.xp))
    }

    pub fn badge(&self) -> Option<&'static str> {
        self.mastered.then_some("[MASTERED] - Cape Unlocked")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkillsSummary {
    pub rows: [SkillRow; 8],
    pub total_level: u32,
    pub total_xp: u64,
}

impl SkillsSummary {
    pub fn new(skills: &CharacterSkills) -> Self {
        let rows = SkillType::ALL.map(|skill| SkillRow::from_xp(skill, skills.get_xp(skill)));
        Self {
            total_level: rows.iter().map(|r| u32::from(r.level)).sum(),
            total_xp: rows.iter().map(|r| u64::from(r.xp)).sum(),
            rows,
        }
    }

    pub fn mastered_count(&self) -> usize {
        self.rows.iter().filter(|r| r.mastered).count()
    }
}

fn group_digits(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn render_skills_modal_egui(
    mut contexts: EguiContexts,
    mut state: ResMut<SkillsModalState>,
    players: Query<&CharacterSkills, With<PlayerCharacterMarker>>,
) {
    if !state.is_open {
        return;
    }
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let summary = players.iter().next().map(SkillsSummary::new);
    let mut open = true;
    egui::Window::new("NEON VEIL // NEURAL SKILL METRICS")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            let Some(summary) = summary else {
                ui.label("No neural link.");
                return;
            };
            ui.horizontal(|ui| {
                ui.strong(format!(
                    "TOTAL LEVEL {} / {MAX_TOTAL_LEVEL}",
                    summary.total_level
                ));
                ui.separator();
                ui.label(format!("TOTAL XP {}", group_digits(summary.total_xp)));
            });
            ui.separator();
            egui::Grid::new("skills_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    for (i, row) in summary.rows.iter().enumerate() {
                        ui.vertical(|ui| {
                            let selected = state.selected_skill == Some(row.skill);
                            let label =
                                format!("{}  {:>2}", skill_display_name(row.skill), row.level);
                            let response = ui.selectable_label(selected, label);
                            if response.clicked() {
                                state.selected_skill = (!selected).then_some(row.skill);
                            }
                            response.on_hover_ui(|ui| {
                                ui.label(format!("XP {}", group_digits(u64::from(row.xp))));
                                match row.next_level_xp {
                                    Some(next) => {
                                        ui.label(format!(
                                            "Level {} at {}",
                                            row.level + 1,
                                            group_digits(u64::from(next))
                                        ));
                                        ui.label(format!(
                                            "{} XP to go",
                                            group_digits(u64::from(row.xp_to_next_level()))
                                        ));
                                    }
                                    None => {
                                        ui.label("Maximum level reached.");
                                    }
                                }
                            });
                            ui.add(
                                egui::ProgressBar::new(row.progress)
                                    .desired_width(220.0)
                                    .show_percentage(),
                            );
                            if let Some(badge) = row.badge() {
                                ui.colored_label(egui::Color32::from_rgb(255, 170, 0), badge);
                            }
                        });
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
        });
    if !open {
        state.is_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_rpg::xp_table::HALFWAY_XP;

    #[test]
    fn toggle() {
        let mut state = SkillsModalState::default();
        assert!(!state.is_open);
        state.toggle();
        assert!(state.is_open);
        state.toggle();
        assert!(!state.is_open);
    }

    #[test]
    fn level_one_with_no_xp() {
        let row = SkillRow::from_xp(SkillType::Agility, 0);
        assert_eq!(
            (row.level, row.progress, row.next_level_xp),
            (1, 0.0, Some(83))
        );
        assert_eq!(row.xp_to_next_level(), 83);
        assert!(row.badge().is_none());
    }

    #[test]
    fn level_92_progress_toward_93() {
        let row = SkillRow::from_xp(SkillType::Shadow, HALFWAY_XP);
        assert_eq!(row.level, 92);
        assert_eq!(row.level_start_xp, 6_517_253);
        assert_eq!(row.next_level_xp, Some(level_to_xp(93)));
        assert_eq!(row.progress, 0.0, "exactly at the level 92 threshold");
        let span = level_to_xp(93) - level_to_xp(92);
        let midway = SkillRow::from_xp(SkillType::Shadow, HALFWAY_XP + span / 2);
        assert_eq!(midway.level, 92);
        assert!((midway.progress - 0.5).abs() < 1e-4, "{}", midway.progress);
        let almost = SkillRow::from_xp(SkillType::Shadow, level_to_xp(93) - 1);
        assert!(almost.progress < 1.0 && almost.progress > 0.999);
    }

    #[test]
    fn level_99_is_clamped_and_mastered() {
        for xp in [level_to_xp(99), 50_000_000, XP_CAP] {
            let row = SkillRow::from_xp(SkillType::Endurance, xp);
            assert_eq!((row.level, row.progress, row.mastered), (99, 1.0, true));
            assert_eq!(row.next_level_xp, None);
            assert_eq!(row.badge(), Some("[MASTERED] - Cape Unlocked"));
            assert_eq!(row.xp_to_next_level(), 0);
        }
    }

    #[test]
    fn totals_sum_all_eight_skills() {
        let fresh = SkillsSummary::new(&CharacterSkills::default());
        assert_eq!(fresh.total_level, 8);
        assert_eq!(fresh.total_xp, 0);
        let mut skills = CharacterSkills::default();
        skills.add_xp(SkillType::Agility, level_to_xp(45));
        skills.add_xp(SkillType::Ballistics, XP_CAP);
        let summary = SkillsSummary::new(&skills);
        assert_eq!(summary.total_level, 45 + 99 + 6);
        assert_eq!(
            summary.total_xp,
            u64::from(level_to_xp(45)) + u64::from(XP_CAP)
        );
        assert_eq!(summary.mastered_count(), 1);
        let order: Vec<SkillType> = summary.rows.iter().map(|r| r.skill).collect();
        assert_eq!(order, SkillType::ALL.to_vec());
        let mut maxed = CharacterSkills::default();
        for skill in SkillType::ALL {
            maxed.add_xp(skill, XP_CAP);
        }
        let max = SkillsSummary::new(&maxed);
        assert_eq!(
            (max.total_level, max.total_xp),
            (MAX_TOTAL_LEVEL, MAX_TOTAL_XP)
        );
        assert_eq!(max.total_level, 792);
    }

    #[test]
    fn digit_grouping() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1_000), "1,000");
        assert_eq!(group_digits(13_034_431), "13,034,431");
        assert_eq!(group_digits(MAX_TOTAL_XP), "1,600,000,000");
    }
}
