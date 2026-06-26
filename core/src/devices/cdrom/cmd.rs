use super::{CdRom, CdRomInt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Error {
    BadCommand = 0x40,
    BadParameter = 0x20,
    BadSubFunction = 0x10,
    NoDisc = 0x80,
}

impl CdRom {
    pub(super) fn command(&mut self, cmd: u8) {
        match cmd {
            0x19 => self.cmd_test(),
            other => {
                tracing::warn!(cmd=%format_args!("{other:#X}"), "unknown cdrom command");
                self.cmd_error(Error::BadCommand);
                panic!()
            }
        }
    }

    fn cmd_test(&mut self) {
        let sub = self.param_fifo.pop_front().unwrap_or(0);
        tracing::debug!(arg0=%format_args!("{sub:#X}"), "cdrom test command");

        match sub {
            0x20 => {
                // Fake CD-ROM BIOS/version response.
                //
                // AI says that is convinient:
                // 94 09 19 C0
                self.response_fifo.clear();
                self.response_fifo.extend([0x94, 0x09, 0x19, 0xC0]);
                self.raise_int(CdRomInt::Int3);
            }
            _ => {
                tracing::warn!(sub=%format_args!("{sub:#X}"), "unknown cdrom test subcommand");
                self.cmd_error(Error::BadSubFunction);
            }
        }
    }

    fn cmd_error(&mut self, err: Error) {
        self.response_fifo.clear();
        self.response_fifo.push_back(self.stat().into_bytes()[0]);
        self.response_fifo.push_back(err as u8);
        self.raise_int(CdRomInt::Int5);
    }
}
