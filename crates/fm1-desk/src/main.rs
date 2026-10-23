//! FM-1 / SMK-37 Desk: both emulated synths in one window. The synth's panel fills the window
//! while it plays; the firmware picker (F1) comes over it, full window: both boards' firmware
//! lists, downloads, updates. The emulator, the boards and the panels are upstream's own
//! (fm1-core, fm1-soc, fm1-live's panel drawings, included from there as they are).

// (no console window behind the app in a release build on Windows)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[path = "../../fm1-live/src/panel.rs"]
#[allow(dead_code)]
mod panel;
#[path = "../../fm1-live/src/panel_smk.rs"]
#[allow(dead_code)]
mod panel_smk;

mod app;
mod catalog;
mod engine;
mod keys;

fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("fm1-desk {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(app::TITLE)
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([640.0, 420.0])
            .with_icon(eframe::egui::IconData { rgba: include_bytes!("../assets/icon-64.rgba").to_vec(), width: 64, height: 64 }),
        ..Default::default()
    };
    eframe::run_native(app::TITLE, options, Box::new(|cc| Ok(Box::new(app::Desk::new(cc)))))
}
