// crates/systems/oxid_mac/src/bus.rs
use crate::memory::MacRam;
use crate::via::{MacVia, ViaAction};
use oxide_core::MemoryBus;
use std::cell::Cell;

// Macintosh Memory Map (Strict)
// $000000 - $3FFFFF: RAM (128KB-4MB)
// $400000 - $4FFFFF: ROM (Read Only)
// $580000 - $5FFFFF: SCSI (Read/Write)
// $900000 - $BFFFFF: SCC (Read/Write) -> Stubs or partial
// $C00000 - $DFFFFF: IWM (Read/Write) -> Stubs
// $E80000 - $EFFFFF: VIA (Read/Write)
// Everything else -> Bus Error

pub struct MacBus {
    pub ram: MacRam,
    pub rom: Vec<u8>,
    pub rom_overlay: bool,
    pub via: MacVia,
    pub fault_addr: Cell<Option<u32>>,
}

impl MacBus {
    pub fn new(rom_data: Vec<u8>, ram_size: usize) -> Self {
        Self {
            ram: MacRam::new(ram_size),
            rom: rom_data,
            rom_overlay: true,
            via: MacVia::new(),
            fault_addr: Cell::new(None),
        }
    }

    // Helper for Big Endian Word Read
    pub fn read_u16(&self, addr: u32) -> u16 {
        let hi = self.read(addr) as u16;
        let lo = self.read(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }
}

impl MemoryBus for MacBus {
    fn bus_error(&self) -> Option<u32> {
        self.fault_addr.get()
    }

    fn ack_bus_error(&mut self) {
        self.fault_addr.set(None);
    }

    fn read(&self, address: u32) -> u8 {
        // Overlay logic: ROM at 0x0 at boot
        if self.rom_overlay && address < self.rom.len() as u32 {
            return self.rom[address as usize];
        }

        let high = (address >> 20) & 0xF;

        match high {
            0x0..=0x3 => self.ram.read(address),
            0x4 => {
                let offset = (address & 0x0FFFFF) as usize % self.rom.len();
                self.rom[offset]
            }
            // SCSI: 580000-5FFFFF
            0x5 => {
                if address >= 0x580000 {
                    0x00 // SCSI Stub
                } else {
                    self.fault_addr.set(Some(address));
                    0xFF
                }
            }
            0x6..=0x8 => {
                self.fault_addr.set(Some(address));
                0xFF
            }
            // SCC: 900000-BFFFFF
            0x9..=0xB => 0x04,
            // IWM: C00000-DFFFFF
            0xC..=0xD => 0x1F,
            // VIA: E80000-EFFFFF (E0-E7 is usually invalid/mirror?)
            0xE => {
                if address >= 0xE80000 {
                    self.via.read(address & 0xFFFF)
                } else {
                    self.fault_addr.set(Some(address));
                    0xFF
                }
            }
            0xF => 0x00, // Phase/Test
            _ => {
                // Invalid / Unmapped -> Bus Error
                self.fault_addr.set(Some(address));
                0xFF
            }
        }
    }

    fn write(&mut self, address: u32, value: u8) {
        let high = (address >> 20) & 0xF;

        match high {
            0x0..=0x3 => {
                self.ram.write(address, value);
                // Auto-disable overlay on low RAM write
                if self.rom_overlay && address < 0x8000 {
                    self.rom_overlay = false;
                }
            }
            0x4 => {
                // Write to ROM -> STRICT BUS ERROR
                self.fault_addr.set(Some(address));
            }
            0x5 => {
                if address < 0x580000 {
                    self.fault_addr.set(Some(address));
                }
            }
            0x9..=0xB => {} // SCC
            0xC..=0xD => {} // IWM
            0xE => {
                if address >= 0xE80000 {
                    if let Some(action) = self.via.write(address & 0xFFFF, value) {
                        match action {
                            ViaAction::SetOverlay(enable) => self.rom_overlay = enable,
                        }
                    }
                } else {
                    self.fault_addr.set(Some(address));
                }
            }
            0xF => {}
            _ => {
                self.fault_addr.set(Some(address));
            }
        }
    }
}
