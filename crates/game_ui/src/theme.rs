//! One look for every egui panel (creator, skills, inventory, settings):
//! the HUD's palette and typeface, square edges, quiet chrome.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

const FONT: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Medium.ttf");
const TITLE_FONT: &[u8] = include_bytes!("../../../assets/fonts/Rajdhani-Bold.ttf");

pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0, 229, 255);
pub const WARN: egui::Color32 = egui::Color32::from_rgb(255, 170, 0);
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(232, 240, 250);
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(140, 158, 178);
const PANEL: egui::Color32 = egui::Color32::from_rgba_premultiplied(6, 10, 15, 238);
const WIDGET: egui::Color32 = egui::Color32::from_rgb(18, 26, 36);
const WIDGET_HOVER: egui::Color32 = egui::Color32::from_rgb(24, 40, 52);

/// Name of the display face registered with egui.
pub const TITLE_FAMILY: &str = "title";

pub fn style() -> (egui::FontDefinitions, egui::Visuals) {
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("mono".into(), egui::FontData::from_static(FONT));
    fonts
        .font_data
        .insert("title".into(), egui::FontData::from_static(TITLE_FONT));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "mono".into());
    }
    fonts.families.insert(
        egui::FontFamily::Name(TITLE_FAMILY.into()),
        vec!["title".into()],
    );

    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.window_fill = PANEL;
    v.panel_fill = PANEL;
    v.window_stroke = egui::Stroke::new(1.0, ACCENT.gamma_multiply(0.45));
    v.window_rounding = egui::Rounding::ZERO;
    v.menu_rounding = egui::Rounding::ZERO;
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.extreme_bg_color = egui::Color32::from_rgb(4, 7, 11);
    v.slider_trailing_fill = true;
    for (w, fill) in [
        (&mut v.widgets.noninteractive, PANEL),
        (&mut v.widgets.inactive, WIDGET),
        (&mut v.widgets.hovered, WIDGET_HOVER),
        (&mut v.widgets.active, WIDGET_HOVER),
        (&mut v.widgets.open, WIDGET),
    ] {
        w.rounding = egui::Rounding::ZERO;
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
    }
    v.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 40, 52));
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    (fonts, v)
}

/// Installs the theme once an egui context exists.
pub fn apply_egui_theme(mut contexts: EguiContexts, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let (fonts, visuals) = style();
    ctx.set_fonts(fonts);
    ctx.set_visuals(visuals);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.slider_width = 400.0;
    });
    *done = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_registers_fonts_and_square_chrome() {
        let (fonts, visuals) = style();
        assert!(fonts.font_data.contains_key("mono") && fonts.font_data.contains_key("title"));
        assert!(fonts
            .families
            .contains_key(&egui::FontFamily::Name(TITLE_FAMILY.into())));
        assert_eq!(visuals.window_rounding, egui::Rounding::ZERO);
        assert_eq!(visuals.override_text_color, Some(TEXT));
    }
}
