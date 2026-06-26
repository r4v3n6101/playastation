#![allow(dead_code)]

use modular_bitfield::bitfield;
use strum::FromRepr;

use super::{
    DisplayDepth, DmaDirection, Gpu, HorizontalResolution, VerticalResolution, VideoMode, gp0,
};

#[derive(FromRepr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Gp1Opcode {
    ResetGpu = 0x00,
    ResetCommandBuffer = 0x01,
    AcknowledgeInterrupt = 0x02,
    DisplayEnable = 0x03,
    DmaDirection = 0x04,
    DisplayVramStart = 0x05,
    DisplayHorizontalRange = 0x06,
    DisplayVerticalRange = 0x07,
    DisplayMode = 0x08,
    GetGpuInfo = 0x10,
}

#[bitfield(bits = 8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayMode {
    hres: HorizontalResolution,
    vres: VerticalResolution,
    vmode: VideoMode,
    depth: DisplayDepth,
    interlace: bool,
    special_hres: bool,
    reverse: bool,
}

impl Gpu {
    #[tracing::instrument(
        target = "gpu.gp1",
        level = "DEBUG",
        "dispatch",
        skip(self),
        fields(cmd=%format_args!("{cmd:#X}"))
    )]
    pub fn dispatch_gp1(&mut self, cmd: u32) {
        let opcode = (cmd >> 24) as u8;
        let Some(opcode) = Gp1Opcode::from_repr(opcode) else {
            return;
        };
        tracing::trace!(?opcode, "command decoded");

        match opcode {
            Gp1Opcode::ResetGpu => {
                self.cmdbuf = gp0::CmdBuf::default();
                self.int_flag = false;
                self.dma_direction = DmaDirection::default();
                self.display.enabled = false;
                self.display.vram_start = (0, 0);
                self.display.hrange = (0, 0);
                self.display.vrange = (0, 0);
                self.display.hres = HorizontalResolution::default();
                self.display.vmode = VideoMode::default();
                self.display.special_368_hres = false;
                self.clock.set_display_mode(
                    self.display.vmode,
                    self.display.hres,
                    self.display.special_368_hres,
                );
            }
            Gp1Opcode::ResetCommandBuffer => {
                self.cmdbuf = Default::default();
            }
            Gp1Opcode::AcknowledgeInterrupt => {
                self.int_flag = false;
            }
            Gp1Opcode::DisplayEnable => {
                self.display.enabled = (cmd & 1) == 0;
            }
            Gp1Opcode::DmaDirection => {
                self.dma_direction = match cmd & 0x3 {
                    0x0 => DmaDirection::Off,
                    0x1 => DmaDirection::Fifo,
                    0x2 => DmaDirection::CpuToGp0,
                    0x3 => DmaDirection::VramToCpu,
                    _ => unreachable!(),
                }
            }
            Gp1Opcode::DisplayVramStart => {
                self.display.vram_start = ((cmd & 0x3FF) as u16, ((cmd >> 10) & 0x1FF) as u16);
            }
            Gp1Opcode::DisplayHorizontalRange => {
                self.display.hrange = ((cmd & 0x0FFF) as u16, ((cmd >> 12) & 0x0FFF) as u16);
            }
            Gp1Opcode::DisplayVerticalRange => {
                self.display.vrange = ((cmd & 0x03FF) as u16, ((cmd >> 10) & 0x03FF) as u16);
            }
            Gp1Opcode::DisplayMode => {
                let mode = DisplayMode::from_bytes([cmd as u8]);

                self.display.hres = mode.hres();
                self.display.vres = mode.vres();
                self.display.vmode = mode.vmode();
                self.display.display_depth = mode.depth();
                self.display.interlaced = mode.interlace();
                self.display.special_368_hres = mode.special_hres();
                self.display.reversed = mode.reverse();

                self.clock.set_display_mode(
                    self.display.vmode,
                    self.display.hres,
                    self.display.special_368_hres,
                );
            }
            Gp1Opcode::GetGpuInfo => {}
        }
    }
}
