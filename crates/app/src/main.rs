//! Audio-Reactive Visual Engine: desktop app entry point.

mod analysis_thread;
mod debug_panel;
mod graph_editor;
mod graph_view;
mod media_pool;
mod mod_editor;
mod offline;
mod perform;
mod rhythm_panel;
mod scene_view;
mod theme;
mod ui;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();

    // `visual-engine --analyse a.mp3 b.flac ...`: offline analysis to CSV, no window.
    if args
        .first()
        .is_some_and(|a| a == "--analyse" || a == "--analyze")
    {
        let mut failed = false;
        for path in &args[1..] {
            let path = std::path::Path::new(path);
            match offline::analyse_file(path) {
                Ok(r) => println!(
                    "{}\n  -> {}\n  {}",
                    path.display(),
                    r.csv_path.display(),
                    r.summary()
                ),
                Err(e) => {
                    eprintln!("{}: {e:#}", path.display());
                    failed = true;
                }
            }
        }
        std::process::exit(if failed { 1 } else { 0 });
    }

    // Optional: a song to open at startup (`visual-engine path/to/song.mp3`).
    let startup_song = args.first().map(std::path::PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Visual Engine")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: wgpu_options(),
        ..Default::default()
    };

    eframe::run_native(
        "Visual Engine",
        options,
        Box::new(|cc| Ok(Box::new(ui::VisualEngineApp::new(cc, startup_song)))),
    )
}

/// egui's default GPU setup, plus GPU timestamps when the GPU has them (for
/// the per-node cost shown in the graph).
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup};
    use eframe::wgpu;
    let mut config = WgpuConfiguration::default();
    if let WgpuSetup::CreateNew(new) = &mut config.wgpu_setup {
        let base = new.device_descriptor.clone();
        new.device_descriptor = std::sync::Arc::new(move |adapter| {
            let mut desc = base(adapter);
            if adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
                desc.required_features |= wgpu::Features::TIMESTAMP_QUERY;
            }
            desc
        });
    }
    config
}
