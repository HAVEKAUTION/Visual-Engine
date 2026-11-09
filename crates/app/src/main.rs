//! Audio-Reactive Visual Engine: desktop app entry point.

mod feed_thread;
mod ui;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Visual Engine")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Visual Engine",
        options,
        Box::new(|cc| Ok(Box::new(ui::VisualEngineApp::new(cc)))),
    )
}
