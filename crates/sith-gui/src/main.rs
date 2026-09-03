//! `sith-gui` -- a graphical browser for 16-bit Windows NE binaries.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icons;
mod keys;
mod paths;
mod palette;
mod state;
mod theme;
mod ui;
mod views;
mod widgets;
mod wizard;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let initial = std::env::args().nth(1).map(std::path::PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1500.0, 950.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("sith"),
        ..Default::default()
    };

    eframe::run_native(
        "sith",
        options,
        Box::new(move |cc| Ok(Box::new(state::SithApp::new(cc, initial.clone())))),
    )
}

impl eframe::App for state::SithApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui::frame(self, ui);
    }
}
