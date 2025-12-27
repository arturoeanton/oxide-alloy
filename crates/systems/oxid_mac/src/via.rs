// crates/systems/oxid_mac/src/via.rs
// Macintosh VIA (Versatile Interface Adapter) - 6522 emulation

use std::cell::Cell;

#[derive(Clone)]
pub struct MacVia {
    pub ora: u8,  // Output Register A (vBufA)
    pub orb: u8,  // Output Register B (vBufB)
    pub ddra: u8, // Data Direction A
    pub ddrb: u8, // Data Direction B
    pub t1c: u16, // Timer 1 Counter
    pub t1l: u16, // Timer 1 Latch
    pub t2c: u16, // Timer 2 Counter
    pub acr: u8,  // Auxiliary Control Register
    pub ier: u8,  // Interrupt Enable Register

    // IFR needs interior mutability because reads can clear flags
    pub ifr: Cell<u8>, // Interrupt Flag Register

    // RTC state
    rtc_enabled: bool,
    rtc_clock: bool,
    rtc_bit_count: u8,
    rtc_shift_reg: u32,
    rtc_data_out: u8,

    // Keyboard simulation state
    kbd_last_cmd: u8,

    // Video timing simulation (Cell for interior mutability)
    hblank_counter: Cell<u32>,
}

impl MacVia {
    pub fn new() -> Self {
        Self {
            ora: 0x00,
            orb: 0,
            ddra: 0,
            ddrb: 0,
            t1c: 0xFFFF,
            t1l: 0xFFFF,
            t2c: 0xFFFF,
            acr: 0,
            ier: 0,
            ifr: Cell::new(0),
            rtc_enabled: false,
            rtc_clock: false,
            rtc_bit_count: 0,
            rtc_shift_reg: 0,
            rtc_data_out: 0xFF,
            kbd_last_cmd: 0,
            hblank_counter: Cell::new(0),
        }
    }

    pub fn read(&self, offset: u32) -> u8 {
        // VIA registers are at 512-byte intervals
        // Register = (offset >> 9) & 0xF
        let reg = (offset >> 9) & 0xF;
        match reg {
            0 => {
                // ORB (vBufB) - includes RTC data bit
                // Bit 0: RTC data out (active low)
                // Bit 3: Mouse button (active low, 1=up)
                // Bit 5: Mouse Y2 (1=no change)
                // Bit 6: Horizontal blanking (toggles to simulate video timing)
                // Bit 7: Sound volume (not used here)
                let rtc_data = if self.rtc_data_out != 0 { 0x01 } else { 0x00 };
                let mouse_up = 0x08; // Mouse button not pressed (1=up)

                // Simulate HBlank toggling - The Mac ROM polls this bit
                let count = self.hblank_counter.get().wrapping_add(1);
                self.hblank_counter.set(count);
                let hblank = if (count % 5) < 1 { 0x00 } else { 0x40 };

                self.orb | rtc_data | mouse_up | hblank
            }
            1 | 15 => self.ora, // ORA (vBufA)
            2 => self.ddrb,
            3 => self.ddra,
            4 => (self.t1c & 0xFF) as u8,        // T1C-L
            5 => ((self.t1c >> 8) & 0xFF) as u8, // T1C-H
            6 => (self.t1l & 0xFF) as u8,        // T1L-L
            7 => ((self.t1l >> 8) & 0xFF) as u8, // T1L-H
            8 => (self.t2c & 0xFF) as u8,        // T2C-L
            9 => ((self.t2c >> 8) & 0xFF) as u8, // T2C-H
            10 => {
                // Shift Register (Reg 10)
                // Reading SR usually clears the interrupt flag (bit 2)
                let current_ifr = self.ifr.get();
                self.ifr.set(current_ifr & !0x04);

                // Keyboard Protocol Simulation
                // If last command was Inquiry (0x10), return Mac Plus Keypad ID (0x0B)
                match self.kbd_last_cmd {
                    0x10 => 0x0B, // Mac Plus Keypad
                    0x12 => 0x0B, // Another Inquiry?
                    _ => 0xFF,    // Idle / No response
                }
            }
            11 => self.acr,
            12 => 0, // PCR (not used)
            13 => self.ifr.get(),
            14 => self.ier | 0x80,
            _ => 0,
        }
    }

    pub fn write(&mut self, offset: u32, val: u8) -> Option<ViaAction> {
        let reg = (offset >> 9) & 0xF;

        match reg {
            0 => {
                // ORB (vBufB)
                self.orb = val;

                let rtc_enable = (val & 0x04) == 0; // Active low
                let rtc_clock = (val & 0x02) != 0;
                let rtc_data = val & 0x01;

                if rtc_enable {
                    if !self.rtc_clock && rtc_clock {
                        // Rising edge
                        self.rtc_shift_reg = (self.rtc_shift_reg << 1) | (rtc_data as u32);
                        self.rtc_bit_count += 1;
                        if self.rtc_bit_count >= 8 {
                            self.rtc_data_out = 0xFF;
                        }
                    }
                    self.rtc_clock = rtc_clock;
                }

                if !rtc_enable && self.rtc_enabled {
                    self.rtc_bit_count = 0;
                    self.rtc_shift_reg = 0;
                    self.rtc_data_out = 0xFF;
                }
                self.rtc_enabled = rtc_enable;

                None
            }
            1 | 15 => {
                self.ora = val;
                // Mac Plus compatibility: disable overlay
                Some(ViaAction::SetOverlay(false))
            }
            2 => {
                self.ddrb = val;
                None
            }
            3 => {
                self.ddra = val;
                None
            }
            4 => {
                self.t1l = (self.t1l & 0xFF00) | val as u16;
                None
            }
            5 => {
                self.t1l = (self.t1l & 0x00FF) | ((val as u16) << 8);
                self.t1c = self.t1l;
                // Writing T1C-H clears T1 interrupt
                let ifr = self.ifr.get();
                self.ifr.set(ifr & !0x40);
                None
            }
            6 => {
                self.t1l = (self.t1l & 0xFF00) | val as u16;
                None
            }
            7 => {
                self.t1l = (self.t1l & 0x00FF) | ((val as u16) << 8);
                None
            }
            8 => {
                self.t2c = (self.t2c & 0xFF00) | val as u16;
                None
            }
            9 => {
                self.t2c = (self.t2c & 0x00FF) | ((val as u16) << 8);
                // Writing T2C-H clears T2 interrupt
                let ifr = self.ifr.get();
                self.ifr.set(ifr & !0x20);
                None
            }
            10 => {
                // Shift Register Write
                self.kbd_last_cmd = val;

                // Simulate "Transfer Complete" immediately by setting bit 2
                let ifr = self.ifr.get();
                self.ifr.set(ifr | 0x04);
                None
            }
            11 => {
                self.acr = val;
                None
            }
            13 => {
                // IFR Write: 1 to clear
                let current = self.ifr.get();
                self.ifr.set(current & !val);
                None
            }
            14 => {
                if val & 0x80 != 0 {
                    self.ier |= val & 0x7F;
                } else {
                    self.ier &= !(val & 0x7F);
                }
                None
            }
            _ => None,
        }
    }

    /// Tick the VIA timers. Returns true if an interrupt line state changes or is active.
    pub fn tick(&mut self, cycles: u32) -> bool {
        let cycle_u16 = cycles as u16;
        let mut ifr = self.ifr.get();
        let mut ifr_changed = false;

        // Timer 1
        let (new_t1, overflow1) = self.t1c.overflowing_sub(cycle_u16);
        self.t1c = new_t1;

        if overflow1 {
            if (ifr & 0x40) == 0 {
                ifr |= 0x40;
                ifr_changed = true;
            }
            if (self.acr & 0x40) != 0 {
                self.t1c = self.t1l;
            }
        }

        // Timer 2
        let (new_t2, overflow2) = self.t2c.overflowing_sub(cycle_u16);
        if overflow2 && self.t2c > 0 {
            if (ifr & 0x20) == 0 {
                ifr |= 0x20;
                ifr_changed = true;
            }
        }
        self.t2c = new_t2;

        if ifr_changed {
            self.ifr.set(ifr);
        }

        (ifr & self.ier & 0x7F) != 0
    }

    // Check pending IRQ
    #[allow(dead_code)]
    pub fn irq_pending(&self) -> bool {
        (self.ifr.get() & self.ier & 0x7F) != 0
    }
}

pub enum ViaAction {
    SetOverlay(bool),
}
