use std::{fs, process::Command};

use playastation::cpu::{Bus, Cpu};

fn create_and_run_program(name: &'static str) -> (Cpu, Bus) {
    let mut memory = vec![0u8; 1024 * 1024];

    let output = format!("{name}.bin");
    fs::File::create(&output).unwrap();
    Command::new("armips")
        .arg(format!("tests/asm/{name}.asm"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
    let program = fs::read(&output).unwrap();
    memory[..program.len()].copy_from_slice(&program[..]);
    fs::remove_file(output).unwrap();

    let mut bus = Bus { memory };
    let mut cpu = Cpu {
        regs: Default::default(),
        pipeline: Default::default(),
    };

    for _ in 0..1000 {
        cpu.cycle(&mut bus);
    }

    (cpu, bus)
}

#[test]
fn test_basic_alu() {
    let (cpu, _) = create_and_run_program("alu_basic");

    assert_eq!(cpu.regs.general[1], 5);
    assert_eq!(cpu.regs.general[2], 7);
    assert_eq!(cpu.regs.general[3], 12);
    assert_eq!(cpu.regs.general[4], 7);
    assert_eq!(cpu.regs.general[5], 7);
    assert_eq!(cpu.regs.general[6], 5);
    assert_eq!(cpu.regs.general[7], 2);
}

#[test]
fn test_basic_shift() {
    let (cpu, _) = create_and_run_program("shift_basic");

    assert_eq!(cpu.regs.general[2], 16);
    assert_eq!(cpu.regs.general[3], 4);
    assert_eq!(cpu.regs.general[4], 4);
    assert_eq!(cpu.regs.general[6], 8);
}

#[test]
fn test_mem_basic() {
    let (cpu, bus) = create_and_run_program("mem_basic");

    assert_eq!(bus.read_word(0x1000), 0x00001234);
    assert_eq!(cpu.regs.general[3], 0x00001234);
}
