// crates/systems/oxid_master/src/bus.rs
use oxide_core::MemoryBus;
use crate::vdp::Vdp;

/// Implementación densa del Bus del Master System.
/// Maneja mapeo de memoria, espejos (mirrors) y despacho de puertos I/O.
pub struct MasterSystemBus {
    /// ROM del cartucho (cargada completa).
    pub rom: Vec<u8>,
    /// RAM del sistema (8KB). Mapeada en $C000-$DFFF y espejada en $E000-$FFFF.
    pub ram: [u8; 0x2000],
    /// Procesador de Video (VDP).
    pub vdp: Vdp,
    /// Bancos de ROM paginados.
    /// Slot 0: $0000-$3FFF (Fijo o Banco 0)
    /// Slot 1: $4000-$7FFF (Banco seleccionable)
    /// Slot 2: $8000-$BFFF (Banco seleccionable)
    pub paged_rom: [usize; 3],
    /// Máscara para evitar accesos fuera de rango en la ROM.
    pub rom_mask: usize,
    /// Estado del Joypad (puertos $DC-$DD).
    pub joypad: u8,
    /// Valor del V-Counter (simulado para puerto $7E).
    pub v_counter: u8,
}

impl MasterSystemBus {
    pub fn new(rom: Vec<u8>) -> Self {
        let mask = if rom.len() > 0 {
            (1 << (rom.len().next_power_of_two().trailing_zeros())) - 1
        } else {
            0
        };
        
        Self {
            rom,
            ram: [0; 0x2000],
            vdp: Vdp::new(),
            // Inicialización típica de mappers Sega:
            // Slot 0 -> Banco 0
            // Slot 1 -> Banco 1
            // Slot 2 -> Banco 2
            paged_rom: [0, 0x4000, 0x8000], 
            rom_mask: mask,
            joypad: 0xFF, // Pull-up resistors (1=no pulsado)
            v_counter: 0,
        }
    }

    /// Escribe en los registros del Mapper (Frame Control).
    /// Los mappers de Sega usan $FFFC-$FFFF para seleccionar bancos.
    fn write_mapper(&mut self, address: u32, value: u8) {
        // Asumimos Mapper SEGA estándar por ahora.
        let bank_addr = (value as usize * 0x4000) & self.rom_mask;
        match address {
            0xFFFD => self.paged_rom[0] = bank_addr, // Control Slot 0 ($0400-$3FFF)
            0xFFFE => self.paged_rom[1] = bank_addr, // Control Slot 1 ($4000-$7FFF)
            0xFFFF => self.paged_rom[2] = bank_addr, // Control Slot 2 ($8000-$BFFF)
            _ => {}
        }
    }
}

impl MemoryBus for MasterSystemBus {
    fn read(&self, address: u32) -> u8 {
        match address & 0xFFFF {
            // --- ROM Slots ---
            // Slot 0: Los primeros 1KB ($0000-$03FF) son fijos al principio de la ROM (header/vectores).
            0x0000..=0x03FF => {
                if self.rom.is_empty() { return 0xFF; }
                self.rom[(address as usize) & self.rom_mask]
            }
            0x0400..=0x3FFF => {
                if self.rom.is_empty() { return 0xFF; }
                let offset = (address as usize) & 0x3FFF;
                self.rom[(self.paged_rom[0] + offset) & self.rom_mask]
            }
            // Slot 1
            0x4000..=0x7FFF => {
                if self.rom.is_empty() { return 0xFF; }
                let offset = (address as usize) & 0x3FFF;
                self.rom[(self.paged_rom[1] + offset) & self.rom_mask]
            }
            // Slot 2
            0x8000..=0xBFFF => {
                if self.rom.is_empty() { return 0xFF; }
                let offset = (address as usize) & 0x3FFF;
                self.rom[(self.paged_rom[2] + offset) & self.rom_mask]
            }

            // --- RAM & Mirrors ---
            // RAM Principal (8KB)
            0xC000..=0xDFFF => self.ram[(address as usize) & 0x1FFF],
            
            // Espejo de RAM (Mirror) $E000-$FFFF
            // Nota: Los últimos bytes pueden ser registros de mapper writes, pero se leen como RAM.
            0xE000..=0xFFFF => self.ram[(address as usize) & 0x1FFF],

            _ => 0xFF,
        }
    }

    fn write(&mut self, address: u32, value: u8) {
        match address & 0xFFFF {
            // ROM no es escribible (normalmente), pero algunos mappers raros sí.
            0x0000..=0xBFFF => {} 

            // RAM Principal
            0xC000..=0xDFFF => self.ram[(address as usize) & 0x1FFF] = value,

            // Espejo de RAM ($E000-$FFFF)
            // Aquí se solapan las escrituras de los registros de Mapper de Sega.
            0xE000..=0xFFFF => {
                self.ram[(address as usize) & 0x1FFF] = value; // Escribe en RAM subyacente
                
                // Mapeo de Registros de Paginación (Mapper Writes)
                if address >= 0xFFFC {
                    self.write_mapper(address, value);
                }
            }
            _ => {}
        }
    }

    fn port_in(&mut self, port: u16) -> u8 {
        // El puerto se decodifica usualmente con los bits bajos.
        let p = port & 0xFF;
        
        match p {
            // $7E: V-Counter (Línea vertical actual)
            0x7E => self.v_counter,
            
            // $7F: H-Counter (Posición horizontal aprox). Stub.
            0x7F => 0x00,

            // $BE: VDP Data Port (Buffer de lectura)
            0xBE => self.vdp.read_data(),

            // $BF: VDP Control/Status Port (Flags de interrupción, overflow, etc)
            0xBF => self.vdp.read_status(),

            // $DC: Joypad 1 / 2 Port A
            0xDC => self.joypad,

            // $DD: Joypad 2 Port B / Misc
            0xDD => 0xFF, 

            // Otros puertos pueden devolver 0xFF o bus flotante
            _ => 0xFF
        }
    }

    fn port_out(&mut self, port: u16, value: u8) {
        let p = port & 0xFF;
        match p {
            // $7E-$7F: PSG (Programmable Sound Generator)
            0x7E | 0x7F => {
                // TODO: Implementar PSG
            }

            // $BE: VDP Data Port
            0xBE => self.vdp.write_data(value),

            // $BF: VDP Control Port
            0xBF => self.vdp.write_control(value),

            // $3F: I/O Control (Bios/Ram enable, etc - Avanzado)
            0x3F => {
                // No implementado en esta versión básica
            }
            
            _ => {}
        }
    }
}
