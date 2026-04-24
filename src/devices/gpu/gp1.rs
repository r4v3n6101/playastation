use strum::FromRepr;

use super::{Gpu, GpuDmaDirection, GpuStat};

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

pub fn process(gpu: &mut Gpu, cmd: u32) {
    let opcode = (cmd >> 24) as u8;
    let Some(opcode) = Gp1Opcode::from_repr(opcode) else {
        return;
    };

    println!("{opcode:?} = {cmd:#x}");
    match opcode {
        Gp1Opcode::ResetGpu => {
            gpu.gpustat = GpuStat::default();
            gpu.cmdbuf.clear();
        }
        Gp1Opcode::ResetCommandBuffer => {
            gpu.cmdbuf.clear();
        }
        Gp1Opcode::AcknowledgeInterrupt => gpu.gpustat.set_interrupt_request(false),
        Gp1Opcode::DisplayEnable => gpu.gpustat.set_display_disabled(cmd & 0x1 != 0),
        Gp1Opcode::DmaDirection => gpu.gpustat.set_dma_direction(match cmd & 0x3 {
            0x0 => GpuDmaDirection::Off,
            0x1 => GpuDmaDirection::Fifo,
            0x2 => GpuDmaDirection::CpuToGp0,
            0x3 => GpuDmaDirection::VramToCpu,
            _ => unreachable!(),
        }),
        Gp1Opcode::DisplayVramStart => {}
        Gp1Opcode::DisplayHorizontalRange => {}
        Gp1Opcode::DisplayVerticalRange => {}
        Gp1Opcode::DisplayMode => {}
        Gp1Opcode::GetGpuInfo => {}
    }
}
