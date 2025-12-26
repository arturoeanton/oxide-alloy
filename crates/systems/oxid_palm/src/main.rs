// crates/systems/oxid_palm/src/main.rs
use oxide_core::{Cpu, MemoryBus};
use oxid68k::Oxid68k;

/// Bus de memoria de una Palm Pilot (DragonBall CPU)
struct PalmBus {
    rom: Vec<u8>, // Almacena el Palm OS
    ram: Vec<u8>, // Almacena datos y apps
}

impl PalmBus {
    fn new() -> Self {
        Self {
            // Típico: 512KB de ROM y 512KB de RAM en modelos iniciales
            rom: vec![0; 512 * 1024],
            ram: vec![0; 512 * 1024],
        }
    }
}

impl MemoryBus for PalmBus {
    fn read(&self, address: u32) -> u8 {
        match address {
            // Rango de ROM: 0x00000000 - 0x0007FFFF
            0x0000_0000..=0x0007_FFFF => {
                self.rom[address as usize]
            }
            // Rango de RAM: 0x00FF0000 en adelante (ejemplo simplificado)
            0x00FF_0000..=0x0106_FFFF => {
                let offset = (address - 0x00FF_0000) as usize;
                self.ram[offset]
            }
            _ => 0, // Dirección no mapeada
        }
    }

    fn write(&mut self, address: u32, value: u8) {
        match address {
            0x0000_0000..=0x0007_FFFF => {
                // ¡La ROM es de solo lectura! 
                // Ignoramos la escritura o podríamos lanzar un log de advertencia.
            }
            0x00FF_0000..=0x0106_FFFF => {
                let offset = (address - 0x00FF_0000) as usize;
                self.ram[offset] = value;
            }
            _ => {} // Dirección no mapeada
        }
    }
}

fn main() {
    println!("--- Oxide-Palm (Pilot) ---");

    let mut bus = PalmBus::new();
    let mut cpu = Oxid68k::new();

    // En una Palm real, al bootear, el 68k lee el stack pointer 
    // de la dirección 0 y el PC de la dirección 4.
    // Inyectemos un NOP en la ROM para probar:
    bus.rom[0] = 0x4E; 
    bus.rom[1] = 0x71;

    println!("Status: DragonBall (68k) Core Online.");
    println!("Memory Map: ROM @ 0x00000000, RAM @ 0x00FF0000");

    // Ejecutar un paso
    cpu.step(&mut bus);

    println!("PC: 0x{:08X} | Palm OS Boot Sequence Initiated...", cpu.pc());
}