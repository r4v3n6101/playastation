use playastation::devices::{
    cdrom::{CdRomMode, CdRomStat, CdRomStatus},
    joy::controller::Button,
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
    pub display: Display,
    pub cdrom: Cdrom,
}

#[derive(Default, Clone)]
pub struct Display {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u16>,
}

#[derive(Default, Clone)]
pub struct Cdrom {
    pub status: CdRomStatus,
    pub mode: CdRomMode,
    pub stat: CdRomStat,
}

pub struct App<H> {
    host: H,
    texture: Option<egui::TextureHandle>,
    show_cdrom_debug: bool,
}

impl<H> App<H> {
    pub fn new(host: H) -> Self {
        Self {
            host,
            texture: None,
            show_cdrom_debug: true,
        }
    }

    fn upload_frame(&mut self, ui: &egui::Ui, data: &EmulatorData) {
        if data.display.width == 0 || data.display.height == 0 {
            return;
        }

        if data.display.pixels.len() != data.display.width * data.display.height {
            return;
        }

        let image = egui::ColorImage::new(
            [data.display.width, data.display.height],
            data.display
                .pixels
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

    fn draw_cdrom_debug(&mut self, ui: &egui::Ui, data: &EmulatorData) {
        egui::Window::new("CD-ROM Debug")
            .open(&mut self.show_cdrom_debug)
            .default_width(360.0)
            .show(ui, |ui| {
                ui.heading("CD-ROM");

                ui.separator();

                egui::Grid::new("cdrom_debug_grid")
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Status");
                        ui.monospace(format!("{:#?}", data.cdrom.status));
                        ui.end_row();
                        ui.label("Mode");
                        ui.monospace(format!("{:#?}", data.cdrom.mode));
                        ui.end_row();
                        ui.label("Stat");
                        ui.monospace(format!("{:#?}", data.cdrom.stat));
                        ui.end_row();
                    });
            });
    }
}

impl<H: EmulatorHost> eframe::App for App<H> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.host.send_input(read_input(ui));

        let Some(data) = self.host.poll_data() else {
            return;
        };
        self.upload_frame(ui, &data);

        self.draw_display(ui);
        if self.show_cdrom_debug {
            self.draw_cdrom_debug(ui, &data);
        }

        ui.request_repaint();
    }
}

fn read_input(ui: &egui::Ui) -> InputState {
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

        if input.key_down(egui::Key::Enter) {
            buttons |= Button::START;
        }

        InputState { buttons }
    })
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
