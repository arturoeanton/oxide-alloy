// crates/oxidz80/src/tests.rs
#[cfg(test)]
mod tests {
    use crate::*;
    use oxide_core::MemoryBus;

    struct TestBus { ram: [u8; 65536] }
    impl MemoryBus for TestBus {
        fn read(&self, addr: u32) -> u8 { self.ram[addr as usize] }
        fn write(&mut self, addr: u32, val: u8) { self.ram[addr as usize] = val; }
    }

    fn run_opcode(cpu: &mut OxidZ80, bus: &mut TestBus, op: u8) {
        bus.ram[cpu.pc as usize] = op;
        cpu.step(bus);
    }

    #[test]
    fn test_daa() {
        let mut cpu = OxidZ80::new();
        // Addition cases
        cpu.a = 0x99; cpu.f = 0;
        cpu.add(0x01);
        cpu.daa();
        assert_eq!(cpu.a, 0x00);
        assert!((cpu.f & flags::C) != 0);

        cpu.a = 0x05; cpu.f = 0;
        cpu.add(0x05);
        cpu.daa();
        assert_eq!(cpu.a, 0x10);

        // Subtraction cases
        cpu.a = 0x00; cpu.f = 0;
        cpu.sub(0x01); // 0xFF, C=1, N=1
        cpu.daa();
        assert_eq!(cpu.a, 0x99);
        assert!((cpu.f & flags::C) != 0);
    }

    #[test]
    fn test_ccf_scf() {
        let mut cpu = OxidZ80::new();
        let mut bus = TestBus { ram: [0; 65536] };
        cpu.a = 0xA5; // 1010 0101 -> X=0, Y=1 (bits 3/5)
        cpu.f = 0;
        cpu.pc = 0x1000;
        run_opcode(&mut cpu, &mut bus, 0x37); // SCF
        assert!((cpu.f & flags::C) != 0);
        assert!((cpu.f & flags::Y) != 0);
        assert!((cpu.f & flags::X) == 0);

        cpu.f = flags::C;
        cpu.pc = 0x2000;
        run_opcode(&mut cpu, &mut bus, 0x3F); // CCF
        assert!((cpu.f & flags::C) == 0);
        assert!((cpu.f & flags::H) != 0); // H = old C
    }

    #[test]
    fn test_bit_xy_flags() {
        let mut cpu = OxidZ80::new();
        let mut bus = TestBus { ram: [0; 65536] };
        
        cpu.a = 0x08;
        cpu.pc = 0x1000;
        bus.ram[0x1000] = 0xCB;
        bus.ram[0x1001] = 0x5F; // BIT 3, A
        cpu.step(&mut bus); 
        assert!((cpu.f & flags::Z) == 0);
        assert!((cpu.f & flags::X) != 0); // Bit 3 of A is 1

        cpu.h = 0x20; // H high byte of address
        cpu.l = 0x00;
        cpu.pc = 0x1002;
        bus.ram[0x1002] = 0xCB;
        bus.ram[0x1003] = 0x76; // BIT 6, (HL)
        bus.ram[0x2000] = 0x00; // Value at (HL)
        cpu.step(&mut bus);
        assert!((cpu.f & flags::Z) != 0);
        assert!((cpu.f & flags::Y) != 0); // Y comes from H (bit 5 of 0x20)
    }
}
