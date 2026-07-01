use playastation_frontend_common::{App, EmulatorData, EmulatorHost, InputState, eframe};
use wasm_bindgen::prelude::*;

const WIDTH: usize = 320;
const HEIGHT: usize = 240;

pub struct Host {
    frame_index: u32,
}

impl Host {
    pub fn new() -> Result<Self, wasm_bindgen::JsValue> {
        Ok(Self { frame_index: 0 })
    }
}

impl EmulatorHost for Host {
    fn send_input(&mut self, _input: InputState) {}

    fn poll_data(&mut self) -> Option<EmulatorData> {
        self.frame_index = self.frame_index.wrapping_add(1);

        let mut pixels = vec![0u16; WIDTH * HEIGHT];

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let r = (((x + self.frame_index as usize) & 31) as u16) << 0;
                let g = (((y + self.frame_index as usize) & 31) as u16) << 5;
                let b = (((x ^ y) & 31) as u16) << 10;

                pixels[y * WIDTH + x] = r | g | b;
            }
        }

        Some(EmulatorData {
            vram: pixels,

            ..Default::default()
        })
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let window = web_sys::window().ok_or("missing window")?;
    let document = window.document().ok_or("missing document")?;

    let canvas = document
        .get_element_by_id("canvas")
        .ok_or("missing canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let host = Host::new()?;

    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |_cc| Ok(Box::new(App::new(host)))),
        )
        .await
}
