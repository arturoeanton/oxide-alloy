// crates/systems/oxid_mac/src/main.rs
use oxide_core::{Cpu, MemoryBus};
use oxid68k::Oxid68k;

/// Bus de memoria del Macintosh Classic
struct MacBus {
    // 1MB de RAM (simulado con un Vector para no llenar el stack)
    ram: Vec<u8>,
}

impl MacBus {
    fn new(size_kb: usize) -> Self {
        Self {
            ram: vec![0; size_kb * 1024],
        }
    }
}

impl MemoryBus for MacBus {
    fn read(&self, address: u32) -> u8 {
        // Validación básica de límites para evitar crasheos (seguridad Rust)
        self.ram.get(address as usize).cloned().unwrap_or(0)
    }

    fn write(&mut self, address: u32, value: u8) {
        if let Some(byte) = self.ram.get_mut(address as usize) {
            *byte = value;
        }
    }
}

fn main() {
    println!("--- Oxide-Mac (Classic) ---");

    // 1. Inicializar Hardware: 1024 KB de RAM
    let mut bus = MacBus::new(1024);
    let mut cpu = Oxid68k::new();

    // 2. Inyectar código 68k de prueba:
    // En el Macintosh real, el PC empieza donde indique el vector de reset.
    // Aquí pondremos un NOP (0x4E71) en la dirección 0x00
    bus.write(0, 0x4E); // Byte alto del NOP
    bus.write(1, 0x71); // Byte bajo del NOP
    bus.write(2, 0x4E); // Otro NOP para el siguiente paso
    bus.write(3, 0x71);

    println!("Status: Motorola 68000 Core Online.");
    println!("Memory: 1024 KB RAM Allocated.");

    // 3. Ejecutar ciclo de prueba
    for _ in 0..2 {
        let current_pc = cpu.pc();
        let cycles = cpu.step(&mut bus);
        println!("PC: 0x{:08X} | Instrucción ejecutada | Ciclos: {}", current_pc, cycles);
    }

    println!("--- Emulación de Macintosh en pausa ---");
}