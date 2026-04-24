use modular_bitfield::prelude::*;
use strum::FromRepr;

use crate::{
    devices::{Updater, int::InterruptFlags},
    interconnect::Bus,
};

use super::{Mmio, MmioExt};

mod cmd;
mod types;

#[derive(Debug)]
pub struct Gpu {
    pub gpustat: GpuStat,

    vram: Box<[[u16; 1024]; 512]>,
    current_cmd: Option<cmd::Packet>,
    cmd_remaining: cmd::Remain,
}

#[bitfield(bits = 32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuStat {
    pub texture_page_x_base: B4,
    pub texture_page_y_base: bool,
    pub semi_transparency: SemiTransparency,
    pub texture_depth: TextureDepth,
    pub dither_24_to_15: bool,
    pub draw_to_display_area: bool,
    pub set_mask_while_drawing: bool,
    pub draw_to_masked_pixels: bool,
    pub interlace_field: bool,
    pub reverse_flag: bool,
    pub texture_disable: bool,
    pub horizontal_resolution_2: bool,
    pub horizontal_resolution_1: HorizontalResolution,
    pub vertical_resolution: VerticalResolution,
    pub video_mode: VideoMode,
    pub display_depth: DisplayDepth,
    pub vertical_interlace: bool,
    pub display_disabled: bool,
    pub interrupt_request: bool,
    pub dma_data_request: bool,
    pub ready_to_receive_command: bool,
    pub ready_to_send_vram: bool,
    pub ready_to_receive_dma: bool,
    pub dma_direction: GpuDmaDirection,
    pub drawing_even_odd_lines: bool,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum SemiTransparency {
    Average = 0,
    Add = 1,
    Subtract = 2,
    AddQuarter = 3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum TextureDepth {
    Bpp4 = 0,
    Bpp8 = 1,
    Bpp15 = 2,
    Reserved = 3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum HorizontalResolution {
    H256 = 0,
    H320 = 1,
    H512 = 2,
    H640 = 3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum VerticalResolution {
    V240 = 0,
    V480 = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum VideoMode {
    Ntsc = 0,
    Pal = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum DisplayDepth {
    Bpp15 = 0,
    Bpp24 = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum GpuDmaDirection {
    Off = 0,
    Fifo = 1,
    CpuToGp0 = 2,
    VramToCpu = 3,
}

#[derive(FromRepr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Gp0OpcodeGroup {
    Misc = 0x0,
    Polygon = 0x1,
    Line = 0x2,
    Rect = 0x3,
    Vram2Vram = 0x4,
    Cpu2Vram = 0x5,
    Vram2Cpu = 0x6,
    // TODO : Env
}

#[derive(FromRepr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Gp1Opcode {
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

impl Default for GpuStat {
    fn default() -> Self {
        Self::new()
            .with_interlace_field(true)
            .with_display_disabled(true)
            .with_ready_to_receive_command(true)
            .with_ready_to_receive_dma(true)
    }
}

impl Default for Gpu {
    fn default() -> Self {
        Self {
            gpustat: GpuStat::default(),

            vram: Box::new([[0; _]; _]),
            current_cmd: None,
            cmd_remaining: cmd::Remain::Count(0),
        }
    }
}

impl Gpu {
    pub fn dispatch_gp0(&mut self, cmd: u32) {
        let opcode = (cmd >> 24) as u8;
        let group = (cmd >> 29) as u8;
        let group = Gp0OpcodeGroup::from_repr(group);
        println!("{group:?} = {cmd:#x}");
        match (group, opcode) {
            (_, 0x00 | 0x03..=0x1E) => {
                // NOP
            }
            (_, 0x01) => {
                // Clear CLUT AFAIK
            }
            (_, 0x02) => {
                println!("fill vram");
                // FillVram
            }
            (Some(Gp0OpcodeGroup::Polygon), _) => {
                let (cmd, remaining) = cmd::PolygonPacket::init(cmd);
                self.current_cmd.replace(cmd::Packet::Polygon(cmd));
                self.cmd_remaining = remaining;
            }
            (Some(Gp0OpcodeGroup::Line), _) => {
                let (cmd, remaining) = cmd::LinePacket::init(cmd);
                self.current_cmd.replace(cmd::Packet::Line(cmd));
                self.cmd_remaining = remaining;
            }
            (Some(Gp0OpcodeGroup::Rect), _) => {
                let (cmd, remaining) = cmd::RectPacket::init(cmd);
                self.current_cmd.replace(cmd::Packet::Rect(cmd));
                self.cmd_remaining = remaining;
            }
            (Some(Gp0OpcodeGroup::Vram2Vram), _) => {}
            (Some(Gp0OpcodeGroup::Cpu2Vram), _) => {}
            (Some(Gp0OpcodeGroup::Vram2Cpu), _) => {}
            _ => {}
        }
    }

    pub fn dispatch_gp1(&mut self, cmd: u32) {
        let opcode = (cmd >> 24) as u8;
        let Some(opcode) = Gp1Opcode::from_repr(opcode) else {
            return;
        };

        println!("{opcode:?} = {cmd:#x}");
        match opcode {
            Gp1Opcode::ResetGpu => {
                // Reset GPUSTAT to [`Default`]
                self.gpustat = GpuStat::default();
            }
            Gp1Opcode::ResetCommandBuffer => {
                self.current_cmd = None;
                self.cmd_remaining = cmd::Remain::Count(0);
            }
            Gp1Opcode::AcknowledgeInterrupt => self.gpustat.set_interrupt_request(false),
            Gp1Opcode::DisplayEnable => self.gpustat.set_display_disabled(cmd & 0x1 != 0),
            Gp1Opcode::DmaDirection => self.gpustat.set_dma_direction(match cmd & 0x3 {
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
}

impl Mmio for Gpu {
    fn read(&self, dest: &mut [u8], addr: u32) {
        self.read_unaligned(dest, addr, |addr| match addr {
            0x0 => 0,
            0x4 => {
                let mut reg = u32::from_le_bytes(self.gpustat.into_bytes());
                // NB: bugfix
                reg &= !(1 << 19);
                reg
            }
            _ => unreachable!(),
        });
    }

    fn write(&mut self, addr: u32, value: &[u8]) {
        let (addr, value) = self.write_value(addr, value);
        match addr {
            0x0 => self.dispatch_gp0(value),
            0x4 => self.dispatch_gp1(value),
            _ => unreachable!(),
        }
    }
}

impl Updater for Gpu {
    fn tick(bus: &mut Bus) {
        if bus.gpu.gpustat.interrupt_request() {
            bus.int_ctrl.raise(InterruptFlags::GPU);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GpuStat;

    #[test]
    fn verify_default() {
        let gpustat = GpuStat::default();
        let reg = u32::from_le_bytes(gpustat.into_bytes());

        assert_eq!(reg, 0x1480_2000);
    }
}
