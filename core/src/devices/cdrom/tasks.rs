use super::{CdRom, ErrorCode, IrqFlag, Task};

pub struct Test;

impl Task for Test {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.push_response(&[0x94, 0x09, 0x19, 0xC0]);
        cdrom.raise_int(IrqFlag::Int3);
    }
}

pub struct BadCommand {
    pub command: u8,
}

impl Task for BadCommand {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.raise_err(ErrorCode::BadCommand);
    }
}
