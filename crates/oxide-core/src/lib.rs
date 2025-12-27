use std::fs;
use std::path::Path;
use thiserror::Error;

// ============================================================================
//  CONTRACTS (TRAITS)
// ============================================================================

/// Representa cualquier dispositivo capaz de ejecutar instrucciones (CPU)
pub trait Cpu {
    /// Reinicio en frío (Power On)
    fn reset(&mut self);

    /// Reinicio con acceso al bus (necesario para 68k que lee vectores de reset)
    fn reset_with_bus(&mut self, _bus: &mut dyn MemoryBus) {
        self.reset();
    }

    /// Ejecuta una instrucción o paso atómico.
    /// Retorna la cantidad de ciclos consumidos.
    fn step(&mut self, bus: &mut dyn MemoryBus) -> u32;

    /// Debugging: Obtener el Program Counter actual
    fn pc(&self) -> u32;
}

/// Contrato UNIFICADO para el Bus (Memoria + I/O).
pub trait MemoryBus {
    // --- Métodos Obligatorios (Memoria) ---
    fn read(&self, addr: u32) -> u8;
    fn write(&mut self, addr: u32, val: u8);

    // --- Métodos de I/O (Puertos) ---
    // Tienen implementación por defecto para sistemas que no usan puertos (como consolas puras memory-mapped)
    // o para no obligar a implementarlos si no se necesitan.
    fn port_in(&mut self, _port: u16) -> u8 {
        0xFF
    } // Bus flotante devuelve FF
    fn port_out(&mut self, _port: u16, _val: u8) {} // Escritura al vacío

    // --- Helpers Automáticos (Default Impls) ---

    // Lectura 16-bit Big Endian (Motorola 68k)
    fn read_u16_be(&self, addr: u32) -> u16 {
        let hi = self.read(addr) as u16;
        let lo = self.read(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    // Lectura 16-bit Little Endian (Zilog Z80, Intel)
    fn read_u16_le(&self, addr: u32) -> u16 {
        let lo = self.read(addr) as u16;
        let hi = self.read(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    // Lectura 32-bit Big Endian (Motorola 68k)
    fn read_u32_be(&self, addr: u32) -> u32 {
        let b0 = self.read(addr) as u32;
        let b1 = self.read(addr.wrapping_add(1)) as u32;
        let b2 = self.read(addr.wrapping_add(2)) as u32;
        let b3 = self.read(addr.wrapping_add(3)) as u32;
        (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
    }

    // Escritura 16-bit Big Endian
    fn write_u16_be(&mut self, addr: u32, val: u16) {
        self.write(addr, (val >> 8) as u8);
        self.write(addr.wrapping_add(1), (val & 0xFF) as u8);
    }

    // Escritura 32-bit Big Endian
    fn write_u32_be(&mut self, addr: u32, val: u32) {
        self.write(addr, (val >> 24) as u8);
        self.write(addr.wrapping_add(1), (val >> 16) as u8);
        self.write(addr.wrapping_add(2), (val >> 8) as u8);
        self.write(addr.wrapping_add(3), (val & 0xFF) as u8);
    }

    // Compatibilidad Legacy para oxid68k (Asume Big Endian por defecto)
    fn read_u16(&self, addr: u32) -> u16 {
        self.read_u16_be(addr)
    }

    // --- Bus Error Signaling (Optional) ---
    // Returns Some(address) if the last operation failed
    fn bus_error(&self) -> Option<u32> {
        None
    }
    fn ack_bus_error(&mut self) {}
}

// Eliminamos el trait IoBus separado porque ahora vive dentro de MemoryBus.

// ============================================================================
//  ROM LOADER (UTILIDAD)
// ============================================================================

#[derive(Error, Debug)]
pub enum RomError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ROM file is too small or empty")]
    Empty,
}

pub struct Rom {
    pub data: Vec<u8>,
}

impl Rom {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, RomError> {
        let data = fs::read(path)?;
        if data.is_empty() {
            return Err(RomError::Empty);
        }
        Ok(Self { data })
    }

    /// Crea una ROM vacía de tamaño fijo (útil para tests)
    pub fn new_empty(size: usize) -> Self {
        Self {
            data: vec![0; size],
        }
    }
}
