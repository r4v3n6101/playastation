use strum::FromRepr;

use super::{DmaDirection, Gpu};

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

#[tracing::instrument(
    target = "gpu.gp1",
    level = "DEBUG",
    "dispatch",
    skip(gpu),
    fields(cmd=%format_args!("{cmd:#X}"))
)]
pub fn dispatch(gpu: &mut Gpu, cmd: u32) {
    let opcode = (cmd >> 24) as u8;
    let Some(opcode) = Gp1Opcode::from_repr(opcode) else {
        return;
    };
    tracing::trace!(?opcode, "command decoded");

    match opcode {
        Gp1Opcode::ResetGpu => {
            gpu.cmdbuf = Default::default();
            gpu.renderer.reset();
        }
        Gp1Opcode::ResetCommandBuffer => {
            gpu.cmdbuf = Default::default();
        }
        Gp1Opcode::AcknowledgeInterrupt => {
            gpu.int_flag = false;
        }
        Gp1Opcode::DisplayEnable => {}
        Gp1Opcode::DmaDirection => {
            gpu.dma_direction = match cmd & 0x3 {
                0x0 => DmaDirection::Off,
                0x1 => DmaDirection::Fifo,
                0x2 => DmaDirection::CpuToGp0,
                0x3 => DmaDirection::VramToCpu,
                _ => unreachable!(),
            }
        }
        Gp1Opcode::DisplayVramStart => {}
        Gp1Opcode::DisplayHorizontalRange => {}
        Gp1Opcode::DisplayVerticalRange => {}
        Gp1Opcode::DisplayMode => {}
        Gp1Opcode::GetGpuInfo => {}
    }
}
