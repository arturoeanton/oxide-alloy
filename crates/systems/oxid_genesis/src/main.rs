use oxide_core::{Cpu, MemoryBus};
use oxid68k::Oxid68k;
use oxidz80::OxidZ80;

fn main() {
    println!("--- Oxide-Genesis (Mega Drive) ---");

    // 1. Inicializar el Hardware
    struct GenesisBus {
        _cartridge_rom: Vec<u8>,
        _work_ram: [u8; 65536],
        _z80_ram: [u8; 8192],
    }

    impl MemoryBus for GenesisBus {
        fn read(&self, _addr: u32) -> u8 { 0 }
        fn write(&mut self, _addr: u32, _val: u8) { }
    }

    let mut bus = GenesisBus {
        _cartridge_rom: vec![0; 1024 * 1024], // 1MB ROM ficticia
        _work_ram: [0; 65536],
        _z80_ram: [0; 8192],
    };

    let mut main_cpu = Oxid68k::new(); // El jefe (Juego)
    let mut sound_cpu = OxidZ80::new(); // El asistente (Audio)

    // 2. Simular el "Boot"
    main_cpu.reset();
    sound_cpu.reset();

    println!("Status: Dual CPU Setup Complete.");
    println!("- Main CPU: Motorola 68000");
    println!("- Sound CPU: Zilog Z80");

    // 3. Ejecutar un paso en ambas (Sincronización básica)
    // En un emulador real, el 68k corre más rápido que el Z80
    main_cpu.step(&mut bus);
    sound_cpu.step(&mut bus); // El Z80 lee de su región mapeada

    println!("PC 68k: 0x{:08X}", main_cpu.pc());
    println!("PC Z80: 0x{:04X}", sound_cpu.pc() as u16);
}