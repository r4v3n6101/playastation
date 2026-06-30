mod app;

#[cfg(not(target_arch = "wasm32"))]
mod desktop;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    use std::sync::Arc;

    use clap::Parser;
    use crossbeam_utils::atomic::AtomicCell;
    use playastation::devices::joy::controller::Button;

    let args = desktop::Args::parse();

    let (data_tx, data_rx) = triple_buffer::triple_buffer(&app::EmulatorData::default());

    let button_state = Arc::new(AtomicCell::new(Button::empty()));

    desktop::spawn_emulator_thread(args, Arc::clone(&button_state), data_tx);

    let host = desktop::Host::new(button_state, data_rx);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PlayaStation")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        "PlayaStation",
        options,
        Box::new(move |_cc| Ok(Box::new(app::App::new(host)))),
    )
}
