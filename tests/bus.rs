use playastation::{
    cpu::Cop0,
    devices::int::InterruptFlags,
    interconnect::Bus,
    run::{CpuExecutor, interpreter::Interpreter},
};

#[test]
fn test_bus_interrupt_triggers_cpu_exception() {
    let mut bus = Bus::default();
    let mut executor = CpuExecutor::<Interpreter>::default();

    executor.cpu.pc = 4;
    executor.block_size = 1024;

    // Enable CPU interrupts:
    // IEc = bit 0
    // IM bit 2 = hardware IRQ lane used by cop0.set_hw_irq()
    executor.cpu.cop0.regs[Cop0::STATUS_IDX] = 0x0401;

    // Make the interrupt controller pending.
    bus.int_ctrl.i_mask = InterruptFlags::VBLANK;
    bus.int_ctrl.raise(InterruptFlags::VBLANK);

    executor.run(&mut bus);

    assert_eq!(executor.cpu.cop0.cause().excode(), 0);
    assert!(!executor.cpu.cop0.cause().bd());
    assert_eq!(executor.cpu.cop0.regs[Cop0::EPC_IDX], 0x1000);
    assert_eq!(executor.cpu.pc, 0x8000_0080);
}
