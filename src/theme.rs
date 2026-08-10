use egui::{epaint::Hsva, Color32, Context, CornerRadius, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle, Visuals};

pub const BG_DARKEST: Color32 = Color32::from_rgb(5, 14, 18);
pub const BG_DARK: Color32 = Color32::from_rgb(10, 24, 31);
pub const PANEL: Color32 = Color32::from_rgb(15, 34, 41);
pub const PANEL_ALT: Color32 = Color32::from_rgb(22, 49, 59);
pub const TURQUOISE: Color32 = Color32::from_rgb(34, 219, 196);
pub const TURQUOISE_SOFT: Color32 = Color32::from_rgb(123, 255, 232);
pub const DANGER: Color32 = Color32::from_rgb(255, 97, 118);
pub const WARNING: Color32 = Color32::from_rgb(255, 194, 92);
pub const TEXT: Color32 = Color32::from_rgb(229, 245, 244);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(138, 175, 175);

pub fn install(ctx: &Context) {
    let fonts = FontDefinitions::default();
    ctx.set_fonts(fonts);

    let mut style: Style = (*ctx.style()).clone();
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
    ctx.set_style(style);
}

fn visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.extreme_bg_color = BG_DARKEST;
    visuals.panel_fill = BG_DARK;
    visuals.window_fill = BG_DARK;
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgba_premultiplied(34, 219, 196, 32));
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(18);
    visuals.widgets.inactive.bg_fill = PANEL;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(18);
    visuals.widgets.hovered.bg_fill = PANEL_ALT;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, TURQUOISE);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(18);
    visuals.widgets.active.bg_fill = tint(TURQUOISE, 0.25);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, TURQUOISE_SOFT);
    visuals.widgets.active.corner_radius = CornerRadius::same(18);
    visuals.selection.bg_fill = tint(TURQUOISE, 0.35);
    visuals.selection.stroke = Stroke::new(1.0, TURQUOISE_SOFT);
    visuals.hyperlink_color = TURQUOISE_SOFT;
    visuals.faint_bg_color = PANEL;
    visuals.code_bg_color = PANEL_ALT;
    visuals.window_shadow.color = Color32::from_black_alpha(90);
    visuals
}

fn tint(color: Color32, value_mult: f32) -> Color32 {
    let hsva = Hsva::from(color);
    Color32::from(Hsva {
        v: (hsva.v * (1.0 + value_mult)).min(1.0),
        s: hsva.s,
        h: hsva.h,
        a: hsva.a,
    })
}

