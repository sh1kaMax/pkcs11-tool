use egui::{Color32, Context, CornerRadius, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle, Theme, Visuals};

pub const BG_DARKEST: Color32 = Color32::from_rgb(255, 255, 255);
pub const BG_DARK: Color32 = Color32::from_rgb(255, 255, 255);
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(252, 252, 252, 248);
pub const PANEL_ALT: Color32 = Color32::from_rgba_premultiplied(240, 240, 240, 252);
pub const TURQUOISE: Color32 = Color32::from_rgb(20, 20, 20);
pub const TURQUOISE_SOFT: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 160);
pub const DANGER: Color32 = Color32::from_rgb(20, 20, 20);
pub const WARNING: Color32 = Color32::from_rgb(110, 110, 110);
pub const TEXT: Color32 = Color32::from_rgb(18, 18, 18);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(110, 110, 110);

pub fn install(ctx: &Context) {
    let fonts = FontDefinitions::default();
    ctx.set_fonts(fonts);
    ctx.set_theme(Theme::Light);

    let mut style: Style = (*ctx.style_of(Theme::Light)).clone();
    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.button_padding = egui::vec2(18.0, 12.0);
    style.visuals = visuals();
    style.text_styles = [
        (TextStyle::Heading, FontId::new(30.0, FontFamily::Proportional)),
        (TextStyle::Name("Hero".into()), FontId::new(42.0, FontFamily::Proportional)),
        (TextStyle::Name("Section".into()), FontId::new(20.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(16.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(15.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(14.0, FontFamily::Monospace)),
    ]
    .into();
    ctx.set_style_of(Theme::Light, style.clone());
    ctx.set_style_of(Theme::Dark, style);
}

fn visuals() -> Visuals {
    let mut visuals = Visuals::light();
    visuals.override_text_color = Some(TEXT);
    visuals.extreme_bg_color = BG_DARKEST;
    visuals.panel_fill = BG_DARK;
    visuals.window_fill = BG_DARK;
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 18));
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(18);
    visuals.widgets.inactive.bg_fill = PANEL;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(18);
    visuals.widgets.hovered.bg_fill = PANEL_ALT;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 80));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(18);
    visuals.widgets.active.bg_fill = Color32::from_rgba_premultiplied(20, 20, 20, 240);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, PANEL);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 200));
    visuals.widgets.active.corner_radius = CornerRadius::same(18);
    visuals.selection.bg_fill = Color32::from_rgba_premultiplied(20, 20, 20, 50);
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 120));
    visuals.hyperlink_color = TEXT;
    visuals.faint_bg_color = PANEL;
    visuals.code_bg_color = PANEL_ALT;
    visuals.window_shadow.color = Color32::from_black_alpha(0);
    visuals
}
