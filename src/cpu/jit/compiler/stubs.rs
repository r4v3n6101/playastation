use crate::interconnect::Bus;

use super::super::super::Cpu;

pub extern "C" fn bus_store(cpu: *mut Cpu, bus: *mut Bus) {
    // Safety: ptr-s are valid, since passed from compiled code.
    let cpu = unsafe { &mut *cpu };
    let bus = unsafe { &mut *bus };

    if cpu.cop0.status().isc() {
        return;
    }
}
