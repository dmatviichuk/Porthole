pub use porthole_kubernetes::{
    channel, clusters, error, exec, logs, metrics, pods, quantity, resources, sessions, shell_env, summary, watch,
};
mod data;
mod model;
mod preferences;
mod ui;

pub fn run() -> eframe::Result {
    shell_env::import_login_shell_env();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "porthole_lib=info,kube=warn".into()),
        )
        .init();
    let runtime = tokio::runtime::Runtime::new().expect("create async runtime");
    let _enter = runtime.enter();
    let icon = image::load_from_memory(include_bytes!("../../../assets/porthole.png"))
        .ok()
        .map(|image| {
            let rgba = image.into_rgba8();
            std::sync::Arc::new(eframe::egui::IconData {
                width: rgba.width(),
                height: rgba.height(),
                rgba: rgba.into_raw(),
            })
        });
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Porthole")
            .with_icon(icon.unwrap_or_default())
            .with_inner_size([1320.0, 840.0])
            .with_min_inner_size([960.0, 600.0])
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false),
        ..Default::default()
    };
    eframe::run_native("Porthole", options, Box::new(|cc| Ok(Box::new(ui::App::new(cc)))))
}
