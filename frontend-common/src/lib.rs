pub use eframe;
pub use egui;

use playastation::devices::{
    cdrom::{CdRomMode, CdRomStat, CdRomStatus},
    gpu::GpuStat,
    joy::{JoyCtrl, JoyMode, JoyStat, controller::Button},
};

pub trait EmulatorHost {
    fn send_input(&mut self, input: InputState);
    fn poll_data(&mut self) -> Option<EmulatorData>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputState {
    pub buttons: Button,
}

#[derive(Default, Clone)]
pub struct EmulatorData {
    pub display_width: usize,
    pub display_height: usize,
    pub display_pixels: Vec<u16>,

    pub cdrom_status: CdRomStatus,
    pub cdrom_mode: CdRomMode,
    pub cdrom_stat: CdRomStat,

    pub gpu_stat: GpuStat,

    pub joy_baud: u16,
    pub joy_mode: JoyMode,
    pub joy_ctrl: JoyCtrl,
    pub joy_stat: JoyStat,
}

pub struct App<H> {
    host: H,
    draw_debug_windows: bool,
    latest_data: Option<EmulatorData>,
    texture: Option<egui::TextureHandle>,
}

impl<H> App<H> {
    pub fn new(host: H) -> Self {
        Self {
            host,
            draw_debug_windows: true,
            latest_data: None,
            texture: None,
        }
    }

    fn upload_frame(&mut self, ui: &egui::Ui, data: &EmulatorData) {
        if data.display_width == 0 || data.display_height == 0 {
            return;
        }

        if data.display_pixels.len() != data.display_width * data.display_height {
            return;
        }

        let image = egui::ColorImage::new(
            [data.display_width, data.display_height],
            data.display_pixels
                .iter()
                .copied()
                .map(bgr555_to_color32)
                .collect(),
        );

        match &mut self.texture {
            Some(texture) => {
                texture.set(image, egui::TextureOptions::LINEAR);
            }
            None => {
                self.texture =
                    Some(ui.load_texture("framebuffer", image, egui::TextureOptions::LINEAR));
            }
        }
    }

    fn read_input(&mut self, ui: &egui::Ui) -> InputState {
        ui.input(|input| {
            let mut buttons = Button::empty();

            if input.key_down(egui::Key::W) {
                buttons |= Button::UP;
            }
            if input.key_down(egui::Key::A) {
                buttons |= Button::LEFT;
            }
            if input.key_down(egui::Key::S) {
                buttons |= Button::DOWN;
            }
            if input.key_down(egui::Key::D) {
                buttons |= Button::RIGHT;
            }

            if input.key_down(egui::Key::H) {
                buttons |= Button::SQUARE;
            }
            if input.key_down(egui::Key::J) {
                buttons |= Button::CROSS;
            }
            if input.key_down(egui::Key::K) {
                buttons |= Button::CIRCLE;
            }
            if input.key_down(egui::Key::L) {
                buttons |= Button::TRIANGLE;
            }

            if input.key_pressed(egui::Key::Enter) {
                buttons |= Button::START;
            }

            if input.key_pressed(egui::Key::Z) {
                self.draw_debug_windows = !self.draw_debug_windows;
            }

            InputState { buttons }
        })
    }

    fn draw_display(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if let Some(texture) = &self.texture {
                    ui.image((texture.id(), ui.available_size()));
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("waiting for the first frame");
                    });
                }
            });
    }

    fn draw_cdrom_debug(&mut self, ui: &egui::Ui) {
        let Some(data) = &self.latest_data else {
            return;
        };

        egui::Window::new("CD-ROM Debug").show(ui, |ui| {
            ui.heading("Registers");
            ui.separator();
            egui::Grid::new("cdrom_debug_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Status");
                    ui.monospace(format!("{:#?}", data.cdrom_status));
                    ui.end_row();
                    ui.label("Mode");
                    ui.monospace(format!("{:#?}", data.cdrom_mode));
                    ui.end_row();
                    ui.label("Stat");
                    ui.monospace(format!("{:#?}", data.cdrom_stat));
                    ui.end_row();
                });
        });
    }

    fn draw_gpu_debug(&mut self, ui: &egui::Ui) {
        let Some(data) = &self.latest_data else {
            return;
        };

        egui::Window::new("GPU Debug").show(ui, |ui| {
            ui.heading("Registers");
            ui.separator();
            egui::Grid::new("gpu_debug_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Stat");
                    ui.monospace(format!("{:#?}", data.gpu_stat));
                    ui.end_row();
                });
        });
    }

    fn draw_joy_debug(&mut self, ui: &egui::Ui) {
        let Some(data) = &self.latest_data else {
            return;
        };

        egui::Window::new("Joy Bus Debug").show(ui, |ui| {
            ui.heading("Registers");
            ui.separator();
            egui::Grid::new("joy_debug_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Baud");
                    ui.monospace(format!("{}", data.joy_baud));
                    ui.end_row();
                    ui.label("Mode");
                    ui.monospace(format!("{:#?}", data.joy_mode));
                    ui.end_row();
                    ui.label("Ctrl");
                    ui.monospace(format!("{:#?}", data.joy_ctrl));
                    ui.end_row();
                    ui.label("Stat");
                    ui.monospace(format!("{:#?}", data.joy_stat));
                    ui.end_row();
                });
        });
    }
}

impl<H: EmulatorHost> eframe::App for App<H> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let input = self.read_input(ui);
        self.host.send_input(input);

        if let Some(data) = self.host.poll_data() {
            self.upload_frame(ui, &data);
            self.latest_data = Some(data);
        }

        self.draw_display(ui);

        if self.draw_debug_windows {
            self.draw_cdrom_debug(ui);
            self.draw_gpu_debug(ui);
            self.draw_joy_debug(ui);
        }

        ui.request_repaint();
    }
}

fn bgr555_to_color32(color: u16) -> egui::Color32 {
    let r5 = (color & 0x001f) as u8;
    let g5 = ((color >> 5) & 0x001f) as u8;
    let b5 = ((color >> 10) & 0x001f) as u8;

    let r8 = (r5 << 3) | (r5 >> 2);
    let g8 = (g5 << 3) | (g5 >> 2);
    let b8 = (b5 << 3) | (b5 >> 2);

    egui::Color32::from_rgb(r8, g8, b8)
}
