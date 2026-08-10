mod app;
mod pkcs11;
mod theme;

use app::TokenStudioApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([780.0, 640.0])
            .with_min_inner_size([760.0, 620.0])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_title("PKCS11 Token Studio"),
        ..Default::default()
    };

    eframe::run_native(
        "PKCS11 Token Studio",
        options,
        Box::new(|cc| Ok(Box::new(TokenStudioApp::new(cc)))),
    )
}
