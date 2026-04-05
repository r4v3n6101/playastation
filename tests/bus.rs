use playastation::{
    cpu::{Cop0, Cpu, CpuCtx},
    devices::int::InterruptFlags,
    interconnect::Bus,
};

#[test]
fn test_bus_interrupt_triggers_cpu_exception() {
    let mut bus = Bus::default();
    let mut cpu = Cpu::default();
    let mut cpu_ctx = CpuCtx::default();

    cpu.regs.pc = 0x1000;

    // Enable CPU interrupts:
    // IEc = bit 0
    // IM bit 2 = hardware IRQ lane used by cop0.set_hw_irq()
    cpu.cop0.regs[Cop0::STATUS_IDX] = 0x0401;

    // Make the interrupt controller pending.
    bus.int_ctrl.i_mask = InterruptFlags::VBLANK;
    bus.int_ctrl.raise(InterruptFlags::VBLANK);

    cpu.run(&mut cpu_ctx, &mut bus);

    assert_eq!(cpu.cop0.cause().excode(), 0);
    assert!(!cpu.cop0.cause().bd());
    assert_eq!(cpu.cop0.regs[Cop0::EPC_IDX], 0x1000);
    assert_eq!(cpu.regs.pc, 0x8000_0080);
}
