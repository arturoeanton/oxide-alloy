use oxide_core::{Cpu, MemoryBus};

mod cycles;
mod tests;

// ============================================================================
//  FLAGS & CONSTANTS
// ============================================================================
pub mod flags {
    pub const S: u8 = 0x80; // Sign
    pub const Z: u8 = 0x40; // Zero
    pub const Y: u8 = 0x20; // Unused/Copy bit 5
    pub const H: u8 = 0x10; // Half Carry
    pub const X: u8 = 0x08; // Unused/Copy bit 3
    pub const P: u8 = 0x04; // Parity/Overflow
    pub const N: u8 = 0x02; // Subtract
    pub const C: u8 = 0x01; // Carry
}

// ============================================================================
//  Z80 CORE STRUCTURE
// ============================================================================

const PARITY_TABLE: [bool; 256] = [
    true, false, false, true, false, true, true, false, false, true, true, false, true, false, false, true, 
    false, true, true, false, true, false, false, true, true, false, false, true, false, true, true, false, 
    false, true, true, false, true, false, false, true, true, false, false, true, false, true, true, false, 
    true, false, false, true, false, true, true, false, false, true, true, false, true, false, false, true, 
    false, true, true, false, true, false, false, true, true, false, false, true, false, true, true, false, 
    true, false, false, true, false, true, true, false, false, true, true, false, true, false, false, true, 
    true, false, false, true, false, true, true, false, false, true, true, false, true, false, false, true, 
    false, true, true, false, true, false, false, true, true, false, false, true, false, true, true, false, 
    false, true, true, false, true, false, false, true, true, false, false, true, false, true, true, false, 
    true, false, false, true, false, true, true, false, false, true, true, false, true, false, false, true, 
    true, false, false, true, false, true, true, false, false, true, true, false, true, false, false, true, 
    false, true, true, false, true, false, false, true, true, false, false, true, false, true, true, false, 
    true, false, false, true, false, true, true, false, false, true, true, false, true, false, false, true, 
    false, true, true, false, true, false, false, true, true, false, false, true, false, true, true, false, 
    false, true, true, false, true, false, false, true, true, false, false, true, false, true, true, false, 
    true, false, false, true, false, true, true, false, false, true, true, false, true, false, false, true 
];

pub struct OxidZ80 {
    // Registros principales
    pub a: u8, pub f: u8,
    pub b: u8, pub c: u8,
    pub d: u8, pub e: u8,
    pub h: u8, pub l: u8,

    // Shadow Registers
    pub a_p: u8, pub f_p: u8,
    pub b_p: u8, pub c_p: u8,
    pub d_p: u8, pub e_p: u8,
    pub h_p: u8, pub l_p: u8,

    // Index & Control
    pub ix: u16, pub iy: u16,
    pub sp: u16, pub pc: u16,
    
    // Interrupts & Refresh
    pub i: u8, pub r: u8,
    pub iff1: bool, pub iff2: bool,
    pub im: u8, // 0, 1, 2
    pub ei_pending: bool,
    
    // State
    pub halted: bool,
    pub cycles: u32,
    
    // Internal use for prefixes
    _displacement: i8, 
}

impl OxidZ80 {
    pub fn new() -> Self {
        Self {
            a: 0xFF, f: 0xFF, b: 0, c: 0, d: 0, e: 0, h: 0, l: 0,
            a_p: 0, f_p: 0, b_p: 0, c_p: 0, d_p: 0, e_p: 0, h_p: 0, l_p: 0,
            ix: 0, iy: 0, sp: 0, pc: 0,
            i: 0, r: 0,
            iff1: false, iff2: false, im: 0, ei_pending: false,
            halted: false, cycles: 0, _displacement: 0,
        }
    }

    pub fn set_internals(&mut self, af_p: u16, bc_p: u16, de_p: u16, hl_p: u16, _wz: u16) {
        self.a_p = (af_p >> 8) as u8;
        self.f_p = (af_p & 0xFF) as u8;
        self.b_p = (bc_p >> 8) as u8;
        self.c_p = (bc_p & 0xFF) as u8;
        self.d_p = (de_p >> 8) as u8;
        self.e_p = (de_p & 0xFF) as u8;
        self.h_p = (hl_p >> 8) as u8;
        self.l_p = (hl_p & 0xFF) as u8;
    }

    // --- Helpers de Lectura ---
    #[inline(always)]
    fn fetch(&mut self, bus: &dyn MemoryBus) -> u8 {
        let val = bus.read(self.pc as u32);
        self.pc = self.pc.wrapping_add(1);
        self.refresh_r(1);
        val
    }

    #[inline(always)]
    fn refresh_r(&mut self, count: u8) {
        for _ in 0..count {
            self.r = (self.r & 0x80) | ((self.r.wrapping_add(1)) & 0x7F);
        }
    }

    #[inline(always)]
    fn fetch_u16(&mut self, bus: &dyn MemoryBus) -> u16 {
        let lo = self.fetch(bus) as u16;
        let hi = self.fetch(bus) as u16;
        (hi << 8) | lo
    }

    // --- Helpers de Stack ---
    fn push(&mut self, bus: &mut dyn MemoryBus, val: u16) {
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp as u32, (val >> 8) as u8); // Hi
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp as u32, (val & 0xFF) as u8); // Lo
    }

    fn pop(&mut self, bus: &dyn MemoryBus) -> u16 {
        let lo = bus.read(self.sp as u32) as u16;
        self.sp = self.sp.wrapping_add(1);
        let hi = bus.read(self.sp as u32) as u16;
        self.sp = self.sp.wrapping_add(1);
        (hi << 8) | lo
    }
}

// ============================================================================
//  CPU TRAIT
// ============================================================================

impl Cpu for OxidZ80 {
    fn reset(&mut self) {
        self.pc = 0; self.sp = 0xFFFF;
        self.iff1 = false; self.iff2 = false; self.im = 0;
        self.halted = false; self.a = 0xFF; self.f = 0xFF;
        self.ix = 0; self.iy = 0;
    }

    fn pc(&self) -> u32 { self.pc as u32 }

    fn step(&mut self, bus: &mut dyn MemoryBus) -> u32 {
        if self.halted {
            return 4; // CPU dormida, consume ciclos esperando IRQ
        }

        // Handle Delayed EI
        if self.ei_pending {
            self.iff1 = true;
            self.iff2 = true;
            self.ei_pending = false;
        }


        let opcode = self.fetch(bus);
        self.cycles = cycles::get_normal_cycles(opcode, true); 

        match opcode {
            0xCB => { self.refresh_r(1); self.exec_cb(bus); },
            0xED => { self.refresh_r(1); self.exec_ed(bus); },
            0xDD => { self.refresh_r(1); self.exec_index(bus, true); },  // IX
            0xFD => { self.refresh_r(1); self.exec_index(bus, false); }, // IY
            _ => self.exec_normal(bus, opcode)
        }
        
        

        
        self.cycles
    }
}

// ============================================================================
//  INTERRUPT SYSTEM
// ============================================================================

impl OxidZ80 {
    /// Non-Maskable Interrupt
    pub fn nmi(&mut self, bus: &mut dyn MemoryBus) -> u32 {
        self.halted = false;
        self.iff2 = self.iff1; 
        self.iff1 = false;    
        self.push(bus, self.pc);
        self.pc = 0x0066;
        11
    }

    /// Maskable Interrupt
    pub fn irq(&mut self, bus: &mut dyn MemoryBus, data_bus: u8) -> u32 {
        if !self.iff1 { return 0; } 

        self.halted = false;
        self.iff1 = false;
        self.iff2 = false;
        let mut cycles = 0;

        match self.im {
            0 => {
                self.exec_normal(bus, data_bus); 
                cycles += 13;
            },
            1 => {
                self.push(bus, self.pc);
                self.pc = 0x0038;
                cycles += 13;
            },
            2 => {
                self.push(bus, self.pc);
                let vec_addr = ((self.i as u16) << 8) | (data_bus as u16);
                let lo = bus.read(vec_addr as u32) as u16;
                let hi = bus.read(vec_addr.wrapping_add(1) as u32) as u16;
                self.pc = (hi << 8) | lo;
                cycles += 19;
            },
            _ => {}
        }
        cycles
    }
}

// ============================================================================
//  OPCODE EXECUTION
// ============================================================================

impl OxidZ80 {
    fn exec_normal(&mut self, bus: &mut dyn MemoryBus, opcode: u8) {
        match opcode {
            0x00 => {}, // NOP
            0x76 => { 
                self.halted = true; 
            },
            
            // 8-bit Loads
            0x40..=0x7F => {
                if opcode == 0x76 { 
                    self.halted = true; 
                    return; 
                }
                let val = self.read_r(bus, opcode & 7);
                self.write_r(bus, (opcode >> 3) & 7, val);
            },
            
            // Imm Loads
            0x06 => self.b = self.fetch(bus), 0x0E => self.c = self.fetch(bus),
            0x16 => self.d = self.fetch(bus), 0x1E => self.e = self.fetch(bus),
            0x26 => self.h = self.fetch(bus), 0x2E => self.l = self.fetch(bus),
            0x3E => self.a = self.fetch(bus),
            0x36 => { let v = self.fetch(bus); bus.write(self.hl() as u32, v); },
            0x37 => { self.f = (self.f & (flags::S|flags::Z|flags::P)) | flags::C | (self.a & (flags::X|flags::Y)); }, // SCF
            0x3F => { // CCF
                let old_c = (self.f & flags::C) != 0;
                self.f = (self.f & (flags::S|flags::Z|flags::P)) | (if old_c { flags::H } else { flags::C }) | (self.a & (flags::X|flags::Y));
            },

            // 16-bit Loads
            0x01 => { let v=self.fetch_u16(bus); self.set_bc(v); },
            0x11 => { let v=self.fetch_u16(bus); self.set_de(v); },
            0x21 => { let v=self.fetch_u16(bus); self.set_hl(v); },
            0x22 => { let a=self.fetch_u16(bus); let v=self.hl(); bus.write(a as u32, v as u8); bus.write((a.wrapping_add(1)) as u32, (v>>8)as u8); }, // LD (nn),HL
            0x2A => { let a=self.fetch_u16(bus); let v=bus.read_u16_le(a as u32); self.set_hl(v); }, // LD HL,(nn)
            0x31 => { self.sp = self.fetch_u16(bus); },
            0x32 => { let a=self.fetch_u16(bus); bus.write(a as u32, self.a); }, // LD (nn),A
            0x3A => { let a=self.fetch_u16(bus); self.a = bus.read(a as u32); }, // LD A,(nn)
            0xF9 => { self.sp = self.hl(); },

            // ALU 8-bit
            0x80..=0xBF => self.alu_opcode(bus, opcode),
            0xC6 => { let v=self.fetch(bus); self.add(v); },
            0xD6 => { let v=self.fetch(bus); self.sub(v); },
            0xE6 => { let v=self.fetch(bus); self.and(v); },
            0xF6 => { let v=self.fetch(bus); self.or(v); },
            0xEE => { let v=self.fetch(bus); self.xor(v); },
            0xFE => { let v=self.fetch(bus); self.cp(v); },

            // Inc/Dec 8-bit
            0x04 => self.b=self.inc(self.b), 0x05 => self.b=self.dec(self.b),
            0x0C => self.c=self.inc(self.c), 0x0D => self.c=self.dec(self.c),
            0x14 => self.d=self.inc(self.d), 0x15 => self.d=self.dec(self.d),
            0x1C => self.e=self.inc(self.e), 0x1D => self.e=self.dec(self.e),
            0x24 => self.h=self.inc(self.h), 0x25 => self.h=self.dec(self.h),
            0x2C => self.l=self.inc(self.l), 0x2D => self.l=self.dec(self.l),
            0x3C => self.a=self.inc(self.a), 0x3D => self.a=self.dec(self.a),
            0x34 => { let addr=self.hl(); let v=self.inc(bus.read(addr as u32)); bus.write(addr as u32, v); },
            0x35 => { let addr=self.hl(); let v=self.dec(bus.read(addr as u32)); bus.write(addr as u32, v); },

            // Misc Loads
            0x02 => bus.write(self.bc() as u32, self.a),
            0x12 => bus.write(self.de() as u32, self.a),
            0x0A => self.a = bus.read(self.bc() as u32),
            0x1A => self.a = bus.read(self.de() as u32),

            // Rotations
            0x07 => { // RLCA
                let c = (self.a & 0x80) != 0;
                self.a = self.a.rotate_left(1);
                self.f = (self.f & (flags::S | flags::Z | flags::P)) | (if c { flags::C } else { 0 }) | (self.a & (flags::X | flags::Y));
            },
            0x17 => { // RLA
                let old_c = (self.f & flags::C) != 0;
                let new_c = (self.a & 0x80) != 0;
                self.a = (self.a << 1) | (if old_c { 1 } else { 0 });
                self.f = (self.f & (flags::S | flags::Z | flags::P)) | (if new_c { flags::C } else { 0 }) | (self.a & (flags::X | flags::Y));
            },
            0x0F => { // RRCA
                let c = (self.a & 0x01) != 0;
                self.a = self.a.rotate_right(1);
                self.f = (self.f & (flags::S | flags::Z | flags::P)) | (if c { flags::C } else { 0 }) | (self.a & (flags::X | flags::Y));
            },
            0x1F => { // RRA
                let old_c = (self.f & flags::C) != 0;
                let new_c = (self.a & 0x01) != 0;
                self.a = (self.a >> 1) | (if old_c { 0x80 } else { 0 });
                self.f = (self.f & (flags::S | flags::Z | flags::P)) | (if new_c { flags::C } else { 0 }) | (self.a & (flags::X | flags::Y));
            },

            // 16-bit Arith
            0x09 => self.add16(self.bc()), 0x19 => self.add16(self.de()),
            0x29 => self.add16(self.hl()), 0x39 => self.add16(self.sp),
            0x03 => { let v=self.bc().wrapping_add(1); self.set_bc(v); },
            0x13 => { let v=self.de().wrapping_add(1); self.set_de(v); },
            0x23 => { let v=self.hl().wrapping_add(1); self.set_hl(v); },
            0x33 => self.sp = self.sp.wrapping_add(1),
            0x0B => { let v=self.bc().wrapping_sub(1); self.set_bc(v); },
            0x1B => { let v=self.de().wrapping_sub(1); self.set_de(v); },
            0x2B => { let v=self.hl().wrapping_sub(1); self.set_hl(v); },
            0x3B => self.sp = self.sp.wrapping_sub(1),

            // Jumps / Calls
            0xC3 => { self.pc = self.fetch_u16(bus); },
            0x18 => { let o=self.fetch(bus) as i8; self.pc = (self.pc as i32 + o as i32) as u16; },
            0x20 => { let t=!self.flag(flags::Z); self.jr(bus, t); self.cycles = cycles::get_normal_cycles(opcode, t); },
            0x28 => { let t=self.flag(flags::Z); self.jr(bus, t); self.cycles = cycles::get_normal_cycles(opcode, t); },
            0x30 => { let t=!self.flag(flags::C); self.jr(bus, t); self.cycles = cycles::get_normal_cycles(opcode, t); },
            0x38 => { let t=self.flag(flags::C); self.jr(bus, t); self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xCD => { let dest=self.fetch_u16(bus); self.push(bus, self.pc); self.pc=dest; },
            0xC9 => { self.pc = self.pop(bus); },
            0xE9 => { self.pc = self.hl(); },
            0xE3 => { // EX (SP), HL
                let low = bus.read(self.sp as u32);
                let high = bus.read((self.sp.wrapping_add(1)) as u32);
                let v = self.hl();
                bus.write(self.sp as u32, v as u8);
                bus.write((self.sp.wrapping_add(1)) as u32, (v>>8) as u8);
                self.set_hl((high as u16) << 8 | low as u16);
            },

            // Conditional Control
            0xC2 => { let d=self.fetch_u16(bus); let t=!self.flag(flags::Z); if t { self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xCA => { let d=self.fetch_u16(bus); let t= self.flag(flags::Z); if t { self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xD2 => { let d=self.fetch_u16(bus); let t=!self.flag(flags::C); if t { self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xDA => { let d=self.fetch_u16(bus); let t= self.flag(flags::C); if t { self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xE2 => { let d=self.fetch_u16(bus); let t=!self.flag(flags::P); if t { self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xEA => { let d=self.fetch_u16(bus); let t= self.flag(flags::P); if t { self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xF2 => { let d=self.fetch_u16(bus); let t=!self.flag(flags::S); if t { self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xFA => { let d=self.fetch_u16(bus); let t= self.flag(flags::S); if t { self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },

            0xC4 => { let d=self.fetch_u16(bus); let t=!self.flag(flags::Z); if t { self.push(bus,self.pc); self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xCC => { let d=self.fetch_u16(bus); let t= self.flag(flags::Z); if t { self.push(bus,self.pc); self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xD4 => { let d=self.fetch_u16(bus); let t=!self.flag(flags::C); if t { self.push(bus,self.pc); self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xDC => { let d=self.fetch_u16(bus); let t= self.flag(flags::C); if t { self.push(bus,self.pc); self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xE4 => { let d=self.fetch_u16(bus); let t=!self.flag(flags::P); if t { self.push(bus,self.pc); self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xEC => { let d=self.fetch_u16(bus); let t= self.flag(flags::P); if t { self.push(bus,self.pc); self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xF4 => { let d=self.fetch_u16(bus); let t=!self.flag(flags::S); if t { self.push(bus,self.pc); self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xFC => { let d=self.fetch_u16(bus); let t= self.flag(flags::S); if t { self.push(bus,self.pc); self.pc=d; } self.cycles = cycles::get_normal_cycles(opcode, t); },

            0xC0 => { let t=!self.flag(flags::Z); if t { self.pc=self.pop(bus); } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xC8 => { let t= self.flag(flags::Z); if t { self.pc=self.pop(bus); } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xD0 => { let t=!self.flag(flags::C); if t { self.pc=self.pop(bus); } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xD8 => { let t= self.flag(flags::C); if t { self.pc=self.pop(bus); } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xE0 => { let t=!self.flag(flags::P); if t { self.pc=self.pop(bus); } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xE8 => { let t= self.flag(flags::P); if t { self.pc=self.pop(bus); } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xF0 => { let t=!self.flag(flags::S); if t { self.pc=self.pop(bus); } self.cycles = cycles::get_normal_cycles(opcode, t); },
            0xF8 => { let t= self.flag(flags::S); if t { self.pc=self.pop(bus); } self.cycles = cycles::get_normal_cycles(opcode, t); },

            // RST
            0xC7 => { self.push(bus, self.pc); self.pc = 0x00; },
            0xCF => { self.push(bus, self.pc); self.pc = 0x08; },
            0xD7 => { self.push(bus, self.pc); self.pc = 0x10; },
            0xDF => { self.push(bus, self.pc); self.pc = 0x18; },
            0xE7 => { self.push(bus, self.pc); self.pc = 0x20; },
            0xEF => { self.push(bus, self.pc); self.pc = 0x28; },
            0xF7 => { self.push(bus, self.pc); self.pc = 0x30; },
            0xFF => { self.push(bus, self.pc); self.pc = 0x38; },

            0x10 => { // DJNZ
                self.b = self.b.wrapping_sub(1);
                let off = self.fetch(bus) as i8;
                if self.b != 0 { self.pc = (self.pc as i32 + off as i32) as u16; self.cycles+=13; }
                else { self.cycles+=8; }
            },

            // Stack
            0xC5 => { let v=self.bc(); self.push(bus,v); }, 0xF5 => { let v=self.af(); self.push(bus,v); },
            0xD5 => { let v=self.de(); self.push(bus,v); }, 0xE5 => { let v=self.hl(); self.push(bus,v); },
            0xC1 => { let v=self.pop(bus); self.set_bc(v); }, 0xF1 => { let v=self.pop(bus); self.set_af(v); },
            0xD1 => { let v=self.pop(bus); self.set_de(v); }, 0xE1 => { let v=self.pop(bus); self.set_hl(v); },

            // IO / Misc
            0xD3 => { let p=self.fetch(bus); bus.port_out((p as u16) | ((self.a as u16)<<8), self.a); },
            0xDB => { let p=self.fetch(bus); self.a = bus.port_in((p as u16) | ((self.a as u16)<<8)); },
            0xEB => { let t=self.de(); self.set_de(self.hl()); self.set_hl(t); },
            0x08 => { let (ta,tf)=(self.a,self.f); self.a=self.a_p; self.f=self.f_p; self.a_p=ta; self.f_p=tf; },
            0xD9 => self.exx(),
            0xF3 => { 
                self.iff1=false; 
                self.iff2=false; 
            },
            0xFB => { 
                // EI: Delay interrupt enable until AFTER next instruction
                self.ei_pending = true; 
            },
            0x27 => self.daa(),
            0x2F => { self.a = !self.a; self.f |= flags::H | flags::N; },
            _ => {}
        }
        self.cycles += 4;
    }

    // --- PREFIX CB: BITS & SHIFTS ---
    fn exec_cb(&mut self, bus: &mut dyn MemoryBus) {
        let op = self.fetch(bus);
        self.cycles = cycles::get_cb_cycles(op);
        let r = op & 7;
        let val = self.read_r(bus, r);
        let res = match (op >> 3) & 0x1F {
            0x00 => self.rot(val, 0, true), // RLC
            0x01 => self.rot(val, 0, false), // RRC
            0x02 => self.rot(val, 1, true), // RL
            0x03 => self.rot(val, 1, false), // RR
            0x04 => self.shift(val, 0, true), // SLA
            0x05 => self.shift(val, 1, false), // SRA
            0x06 => self.shift(val, 2, true), // SLL
            0x07 => self.shift(val, 0, false), // SRL
            0x08..=0x0F => { // BIT
                let b = (op >> 3) & 7;
                let z = (val & (1 << b)) == 0;
                self.f = (self.f & flags::C) | flags::H | (if z {flags::Z|flags::P} else {0});
                if b == 7 && !z { self.f |= flags::S; }
                self.f |= (if r == 6 { self.h } else { val }) & (flags::X | flags::Y);
                return;
            },
            0x10..=0x17 => val & !(1 << ((op >> 3) & 7)), // RES
            0x18..=0x1F => val | (1 << ((op >> 3) & 7)),  // SET
            _ => val
        };
        self.write_r(bus, r, res);
        self.cycles += 8;
    }

    fn exec_cb_index(&mut self, bus: &mut dyn MemoryBus, is_ix: bool) {
        let d = self.fetch(bus) as i8;
        let op = self.fetch(bus);
        self.cycles += 23;

        let idx = if is_ix { self.ix } else { self.iy };
        let addr = idx.wrapping_add(d as u16 as u16) as u32;
        let val = bus.read(addr);

        let res = match (op >> 3) & 0x1F {
            0x00 => self.rot(val, 0, true), // RLC
            0x01 => self.rot(val, 0, false), // RRC
            0x02 => self.rot(val, 1, true), // RL
            0x03 => self.rot(val, 1, false), // RR
            0x04 => self.shift(val, 0, true), // SLA
            0x05 => self.shift(val, 1, false), // SRA
            0x06 => self.shift(val, 2, true), // SLL
            0x07 => self.shift(val, 0, false), // SRL
            0x08..=0x0F => { // BIT
                let b = (op >> 3) & 7;
                let z = (val & (1 << b)) == 0;
                self.f = (self.f & flags::C) | flags::H | (if z {flags::Z|flags::P} else {0});
                if b == 7 && !z { self.f |= flags::S; }
                // Undocumented X/Y for BIT n,(IX+d) come from high byte of address
                self.f |= ((addr >> 8) as u8) & (flags::X | flags::Y);
                return;
            },
            0x10..=0x17 => val & !(1 << ((op >> 3) & 7)), // RES
            0x18..=0x1F => val | (1 << ((op >> 3) & 7)),  // SET
            _ => val
        };
        
        bus.write(addr, res);
        
        // Undocumented: Copy result to register
        let r = op & 7;
        if r != 6 {
            self.write_r(bus, r, res);
        }
    }

    // --- PREFIX ED: EXTENDED ---
    fn exec_ed(&mut self, bus: &mut dyn MemoryBus) {
        let op = self.fetch(bus);
        self.cycles = cycles::get_ed_cycles(op);
        match op {
            // Block Transfer
            0xB0 => self.ldir(bus, true, 1),  0xA0 => self.ldir(bus, false, 1), // LDIR, LDI
            0xB8 => self.ldir(bus, true, -1), 0xA8 => self.ldir(bus, false, -1), // LDDR, LDD
            
            // Block I/O
            0xA2 => self.block_in(bus, true, false),  0xB2 => self.block_in(bus, true, true),  // INI, INIR
            0xAA => self.block_in(bus, false, false), 0xBA => self.block_in(bus, false, true), // IND, INDR
            0xA3 => self.block_out(bus, true, false), 0xB3 => self.block_out(bus, true, true), // OUTI, OTIR
            0xAB => self.block_out(bus, false, false),0xBB => self.block_out(bus, false, true),// OUTD, OTDR
            
            // Interrupt Mode
            // Interrupt Mode
            0x46 | 0x4E | 0x66 | 0x6E => self.im=0, 
            0x56 | 0x76 => self.im=1, 
            0x5E | 0x7E => self.im=2,
            
            // 16-bit Arithmetic
            0x42 => self.sbc16(self.bc()), 0x52 => self.sbc16(self.de()),
            0x62 => self.sbc16(self.hl()), 0x72 => self.sbc16(self.sp),
            0x4A => self.adc16(self.bc()), 0x5A => self.adc16(self.de()),
            0x6A => self.adc16(self.hl()), 0x7A => self.adc16(self.sp),
            
            // I/R Register
            0x47 => self.i = self.a,            // LD I,A
            0x4F => self.r = self.a,            // LD R,A
            0x57 => {                            // LD A,I
                self.a = self.i;
                self.f = (self.f & flags::C) | 
                         (if self.a==0 {flags::Z} else {0}) | 
                         (if self.a&0x80!=0 {flags::S} else {0}) |
                         (if self.iff2 {flags::P} else {0}) |
                         (self.a & (flags::X | flags::Y));
            },
            0x5F => {                            // LD A,R
                self.a = self.r;
                self.f = (self.f & flags::C) | 
                         (if self.a==0 {flags::Z} else {0}) | 
                         (if self.a&0x80!=0 {flags::S} else {0}) |
                         (if self.iff2 {flags::P} else {0}) |
                         (self.a & (flags::X | flags::Y));
            },
            
            // Register I/O
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => { // IN r,(C)
                let r = (op >> 3) & 7;
                let val = bus.port_in(self.bc());
                let _f_old = self.f;
                
                // Flags: S, Z, H=0, P/V=Parity, N=0. C preserved.
                // We use our trusty logic_flags helper which now uses the Lookup Table
                self.f = (self.f & flags::C) | logic_flags(val);
                
                if r != 6 { self.write_r(bus, r, val); }
            },
            0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x71 | 0x79 => { // OUT (C),r
                let r = (op >> 3) & 7;
                let val = if r == 6 { 0 } else { self.read_r(bus, r) };
                bus.port_out(self.bc(), val);
            },
            
            // Load to memory (16-bit)
            0x43 => { let a=self.fetch_u16(bus); let v=self.bc(); bus.write(a as u32, v as u8); bus.write((a.wrapping_add(1)) as u32, (v>>8)as u8); }, // LD (nn),BC
            0x53 => { let a=self.fetch_u16(bus); let v=self.de(); bus.write(a as u32, v as u8); bus.write((a.wrapping_add(1)) as u32, (v>>8)as u8); }, // LD (nn),DE
            0x63 => { let a=self.fetch_u16(bus); let v=self.hl(); bus.write(a as u32, v as u8); bus.write((a.wrapping_add(1)) as u32, (v>>8)as u8); }, // LD (nn),HL
            0x73 => { let a=self.fetch_u16(bus); let v=self.sp;   bus.write(a as u32, v as u8); bus.write((a.wrapping_add(1)) as u32, (v>>8)as u8); }, // LD (nn),SP
            
            // Load from memory (16-bit)
            0x4B => { let a=self.fetch_u16(bus); let v=bus.read_u16_le(a as u32); self.set_bc(v); }, // LD BC,(nn)
            0x5B => { let a=self.fetch_u16(bus); let v=bus.read_u16_le(a as u32); self.set_de(v); }, // LD DE,(nn)
            0x6B => { let a=self.fetch_u16(bus); let v=bus.read_u16_le(a as u32); self.set_hl(v); }, // LD HL,(nn)
            0x7B => { let a=self.fetch_u16(bus); self.sp=bus.read_u16_le(a as u32); }, // LD SP,(nn)
            
            // Negate
            // Negate
            0x44 | 0x4C | 0x54 | 0x5C | 0x64 | 0x6C | 0x74 | 0x7C => { let v=self.a; self.a=0; self.sub(v); }, // NEG
            
            // Block Compare
            0xA1 => self.block_cp(bus, true, false),  0xB1 => self.block_cp(bus, true, true),  // CPI, CPIR
            0xA9 => self.block_cp(bus, false, false), 0xB9 => self.block_cp(bus, false, true), // CPD, CPDR
            
            // BCD
            0x67 => { // RRD
                let v = bus.read(self.hl() as u32);
                let low = self.a & 0x0F;
                self.a = (self.a & 0xF0) | (v & 0x0F);
                bus.write(self.hl() as u32, (v >> 4) | (low << 4));
                self.f = (self.f & flags::C) | logic_flags(self.a);
                self.cycles += 18;
            },
            0x6F => { // RLD
                let v = bus.read(self.hl() as u32);
                let low = self.a & 0x0F;
                self.a = (self.a & 0xF0) | (v >> 4);
                bus.write(self.hl() as u32, (v << 4) | low);
                self.f = (self.f & flags::C) | logic_flags(self.a);
                self.cycles += 18;
            },
            
            // Returns
            0x4D | 0x5D | 0x6D | 0x7D => self.pc = self.pop(bus), // RETI
            0x45 | 0x55 | 0x65 | 0x75 => { self.pc = self.pop(bus); self.iff1=self.iff2; }, // RETN
            
            _ => {}
        }
    }

    // --- PREFIX DD/FD: INDEX IX/IY ---
    fn exec_index(&mut self, bus: &mut dyn MemoryBus, is_ix: bool) {
        let op = self.fetch(bus);
        let idx = if is_ix { self.ix } else { self.iy };
        self.cycles = 8; // Default for most DD/FD opcodes (4 prefix + 4 inner)

        // **FIX**: Split read/write lines for borrow checker (op 0x24/0x2C)
        if op == 0x24 { 
            let val = self.read_idx_8(4, is_ix);
            let res = self.inc(val);
            self.write_idx_8(4, res, is_ix); 
            self.cycles = 8;
            return; 
        } 
        if op == 0x25 { 
            let val = self.read_idx_8(4, is_ix);
            let res = self.dec(val);
            self.write_idx_8(4, res, is_ix); 
            self.cycles = 8;
            return; 
        } 
        if op == 0x2C { 
            let val = self.read_idx_8(5, is_ix);
            let res = self.inc(val);
            self.write_idx_8(5, res, is_ix); 
            self.cycles = 8;
            return; 
        } 
        if op == 0x2D { 
            let val = self.read_idx_8(5, is_ix);
            let res = self.dec(val);
            self.write_idx_8(5, res, is_ix); 
            self.cycles = 8;
            return; 
        } 

        // Undocumented Accessing High/Low bytes of IX/IY
        if (op & 0xC0) == 0x40 {
             let dst = (op >> 3) & 7;
             let src = op & 7;
             if (dst == 4 || dst == 5 || src == 4 || src == 5) && (dst != 6 && src != 6) {
                 let val = self.read_idx_8(src, is_ix);
                 self.write_idx_8(dst, val, is_ix);
                 self.cycles += 4; return;
             }
        }
        
        // Standard Index Logic
        match op {
            0xE5 => self.push(bus, idx),
            0xE1 => { let v=self.pop(bus); if is_ix {self.ix=v} else {self.iy=v} },
            0x21 => { let v=self.fetch_u16(bus); if is_ix {self.ix=v} else {self.iy=v} },
            0x09 => self.add16_idx(is_ix, self.bc()),
            0x19 => self.add16_idx(is_ix, self.de()),
            0x29 => self.add16_idx(is_ix, idx),
            0x39 => self.add16_idx(is_ix, self.sp),
            0x23 => if is_ix { self.ix = self.ix.wrapping_add(1); } else { self.iy = self.iy.wrapping_add(1); },
            0x2B => if is_ix { self.ix = self.ix.wrapping_sub(1); } else { self.iy = self.iy.wrapping_sub(1); },
            0x22 => { let a=self.fetch_u16(bus); bus.write(a as u32, (idx & 0xFF) as u8); bus.write((a.wrapping_add(1)) as u32, (idx >> 8) as u8); },
            0x2A => { let a=self.fetch_u16(bus); let v=bus.read_u16_le(a as u32); if is_ix { self.ix=v; } else { self.iy=v; } },
            0xF9 => self.sp = idx, // LD SP, IX/IY
            0xE9 => { self.pc = idx; }, // JP (IX/IY)
            0xE3 => { // EX (SP), IX/IY
                let lo = bus.read(self.sp as u32);
                let hi = bus.read((self.sp.wrapping_add(1)) as u32);
                bus.write(self.sp as u32, (idx & 0xFF) as u8);
                bus.write((self.sp.wrapping_add(1)) as u32, (idx >> 8) as u8);
                let new_val = ((hi as u16) << 8) | (lo as u16);
                if is_ix { self.ix = new_val; } else { self.iy = new_val; }
            },
            // Opcodes that use (IX+d) displacement - must list explicitly!
            // Only register 6 (normally HL) becomes (IX+d)
            0x34 | 0x35 | 0x36 |  // INC/DEC/LD (IX+d)
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E |  // LD r,(IX+d)
            0x70 | 0x71 | 0x72 | 0x73 | 0x74 | 0x75 | 0x77 |  // LD (IX+d),r
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE  // ALU (IX+d)
            => {
                let d = self.fetch(bus) as i8;
                let addr = idx.wrapping_add(d as u16) as u32;
                self.cycles = 19;
                match op {
                    0x34 => { let v=self.inc(bus.read(addr)); bus.write(addr, v); },
                    0x35 => { let v=self.dec(bus.read(addr)); bus.write(addr, v); },
                    // LD r, (IX+d)
                    0x46 => self.b = bus.read(addr), 0x4E => self.c = bus.read(addr),
                    0x56 => self.d = bus.read(addr), 0x5E => self.e = bus.read(addr),
                    0x66 => self.h = bus.read(addr), 0x6E => self.l = bus.read(addr),
                    0x7E => self.a = bus.read(addr),
                    // LD (IX+d), r
                    0x70 => bus.write(addr, self.b), 0x71 => bus.write(addr, self.c),
                    0x72 => bus.write(addr, self.d), 0x73 => bus.write(addr, self.e),
                    0x74 => bus.write(addr, self.h), 0x75 => bus.write(addr, self.l),
                    0x77 => bus.write(addr, self.a),
                    0x36 => { let n=self.fetch(bus); bus.write(addr, n); },
                    // ALU (IX+d)
                    0x86 => self.add(bus.read(addr)), 0x8E => self.adc(bus.read(addr)),
                    0x96 => self.sub(bus.read(addr)), 0x9E => self.sbc(bus.read(addr)),
                    0xA6 => self.and(bus.read(addr)), 0xAE => self.xor(bus.read(addr)),
                    0xB6 => self.or(bus.read(addr)),  0xBE => self.cp(bus.read(addr)),
                    _ => {}
                }
            },
            0xCB => self.exec_cb_index(bus, is_ix),
            _ => self.exec_normal(bus, op)
        }
    }

    // --- UTILS & HELPERS ---
    
    // **FIXED**: Added `flag` helper
    fn flag(&self, f: u8) -> bool { (self.f & f) != 0 }

    // **FIXED**: Added `alu_opcode` helper
    fn alu_opcode(&mut self, bus: &mut dyn MemoryBus, op: u8) {
        let val = self.read_r(bus, op & 7);
        match (op >> 3) & 7 {
            0 => self.add(val), 
            1 => self.adc(val), 
            2 => self.sub(val), 
            3 => self.sbc(val), 
            4 => self.and(val), 
            5 => self.xor(val),
            6 => self.or(val),  
            7 => self.cp(val), 
            _=>{}
        }
    }

    fn read_r(&mut self, bus: &dyn MemoryBus, r: u8) -> u8 {
        match r { 0=>self.b, 1=>self.c, 2=>self.d, 3=>self.e, 4=>self.h, 5=>self.l, 6=>bus.read(self.hl() as u32), 7=>self.a, _=>0 }
    }
    fn write_r(&mut self, bus: &mut dyn MemoryBus, r: u8, v: u8) {
        match r { 0=>self.b=v, 1=>self.c=v, 2=>self.d=v, 3=>self.e=v, 4=>self.h=v, 5=>self.l=v, 6=>bus.write(self.hl() as u32,v), 7=>self.a=v, _=>{} }
    }

    fn read_idx_8(&self, r: u8, is_ix: bool) -> u8 {
        let val = if is_ix { self.ix } else { self.iy };
        match r {
            4 => (val >> 8) as u8, // High
            5 => (val & 0xFF) as u8, // Low
            _ => if r == 7 { self.a } else { 0 }
        }
    }
    fn write_idx_8(&mut self, r: u8, v: u8, is_ix: bool) {
        let ptr = if is_ix { &mut self.ix } else { &mut self.iy };
        match r {
            4 => *ptr = (*ptr & 0x00FF) | ((v as u16) << 8),
            5 => *ptr = (*ptr & 0xFF00) | (v as u16),
            _ => {}
        }
    }

    #[inline] pub fn bc(&self) -> u16 { ((self.b as u16)<<8)|self.c as u16 }
    #[inline] pub fn de(&self) -> u16 { ((self.d as u16)<<8)|self.e as u16 }
    #[inline] pub fn hl(&self) -> u16 { ((self.h as u16)<<8)|self.l as u16 }
    #[inline] pub fn af(&self) -> u16 { ((self.a as u16)<<8)|self.f as u16 }
    #[inline] pub fn set_bc(&mut self, v:u16) { self.b=(v>>8)as u8; self.c=v as u8; }
    #[inline] pub fn set_de(&mut self, v:u16) { self.d=(v>>8)as u8; self.e=v as u8; }
    #[inline] pub fn set_hl(&mut self, v:u16) { self.h=(v>>8)as u8; self.l=v as u8; }
    #[inline] pub fn set_af(&mut self, v:u16) { self.a=(v>>8)as u8; self.f=v as u8; }

    fn exx(&mut self) {
        let (b,c,d,e,h,l) = (self.b,self.c,self.d,self.e,self.h,self.l);
        self.b=self.b_p; self.c=self.c_p; self.d=self.d_p; self.e=self.e_p; self.h=self.h_p; self.l=self.l_p;
        self.b_p=b; self.c_p=c; self.d_p=d; self.e_p=e; self.h_p=h; self.l_p=l;
    }

    fn inc(&mut self, v: u8) -> u8 {
        let r = v.wrapping_add(1);
        self.f = (self.f & flags::C) | (if r==0{flags::Z}else{0}) | (if r&0x80!=0{flags::S}else{0}) | (if (v&0xF)==0xF{flags::H}else{0}) | (if v==0x7F{flags::P}else{0}) | (r & (flags::X | flags::Y));
        r
    }
    fn dec(&mut self, v: u8) -> u8 {
        let r = v.wrapping_sub(1);
        self.f = (self.f & flags::C) | flags::N | (if r==0{flags::Z}else{0}) | (if r&0x80!=0{flags::S}else{0}) | (if (v&0xF)==0{flags::H}else{0}) | (if v==0x80{flags::P}else{0}) | (r & (flags::X | flags::Y));
        r
    }
    
    // ALU Core
    fn add(&mut self, v: u8) {
        let a = self.a;
        let (r, c) = a.overflowing_add(v);
        let h = (a & 0xF) + (v & 0xF) > 0xF;
        let ov = ((a ^ !v) & (a ^ r) & 0x80) != 0;
        self.f = (if r == 0 { flags::Z } else { 0 }) |
                 (r & 0x80) | // Sign (bit 7)
                 (if h { flags::H } else { 0 }) |
                 (if ov { flags::P } else { 0 }) |
                 (if c { flags::C } else { 0 }) |
                 (r & (flags::X | flags::Y));
        self.a = r;
    }
    fn sub(&mut self, v: u8) {
        let a = self.a;
        let (r, c) = a.overflowing_sub(v);
        let h = (a & 0xF) < (v & 0xF);
        let ov = ((a ^ v) & (a ^ r) & 0x80) != 0;
        self.f = flags::N |
                 (if r == 0 { flags::Z } else { 0 }) |
                 (r & 0x80) | // Sign (bit 7)
                 (if h { flags::H } else { 0 }) |
                 (if ov { flags::P } else { 0 }) |
                 (if c { flags::C } else { 0 }) |
                 (r & (flags::X | flags::Y));
        self.a = r;
    }
    // **FIXED**: Added ADC helper (Precise)
    fn adc(&mut self, v: u8) {
        let c = if (self.f & flags::C) != 0 { 1 } else { 0 };
        let a = self.a;
        let res_wide = (a as u16) + (v as u16) + (c as u16);
        let res = res_wide as u8;
        
        let h = ((a & 0xF) + (v & 0xF) + c) > 0xF;
        let overflow = ((a ^ !v) & (a ^ res) & 0x80) != 0;
        
        self.f = (if res == 0 { flags::Z } else { 0 }) |
                 (res & 0x80) | // Sign (bit 7)
                 (if h { flags::H } else { 0 }) |
                 (if overflow { flags::P } else { 0 }) |
                 (if res_wide > 0xFF { flags::C } else { 0 }) |
                 (res & (flags::X | flags::Y));
        self.a = res;
    }
    // **FIXED**: Added SBC helper (Precise)
    fn sbc(&mut self, v: u8) {
        let c = if (self.f & flags::C) != 0 { 1 } else { 0 };
        let a = self.a;
        let res_wide = (a as i16) - (v as i16) - (c as i16);
        let res = res_wide as u8;
        
        let h = ((a & 0xF) as i16 - (v & 0xF) as i16 - c as i16) < 0;
        let overflow = ((a ^ v) & (a ^ res) & 0x80) != 0;
        
        self.f = flags::N |
                 (if res == 0 { flags::Z } else { 0 }) |
                 (res & 0x80) | // Sign (bit 7)
                 (if h { flags::H } else { 0 }) |
                 (if overflow { flags::P } else { 0 }) |
                 (if res_wide < 0 { flags::C } else { 0 }) |
                 (res & (flags::X | flags::Y));
        self.a = res;
    }
    
    fn and(&mut self, v: u8) { self.a &= v; self.f = flags::H | logic_flags(self.a); }
    fn or(&mut self, v: u8) { self.a |= v; self.f = logic_flags(self.a); }
    fn xor(&mut self, v: u8) { self.a ^= v; self.f = logic_flags(self.a); }
    fn cp(&mut self, v: u8) { 
        let a = self.a; 
        self.sub(v); 
        // CP flags X/Y come from the operand
        self.f = (self.f & !(flags::X | flags::Y)) | (v & (flags::X | flags::Y));
        self.a = a; 
    }
    
    // Bit/Shift Logic
    fn rot(&mut self, v: u8, mode: u8, dir: bool) -> u8 { // mode 0=cyc, 1=thru-C. dir T=L, F=R
        let c = match (mode, dir) {
            (0, true) => (v & 0x80) != 0, (0, false) => (v & 1) != 0,
            (1, true) => (v & 0x80) != 0, (1, false) => (v & 1) != 0, _=>false
        };
        let bit = match (mode, dir) {
             (0, _) => if c {1} else {0}, (1, _) => if (self.f&flags::C)!=0 {1} else {0}, _=>0
        };
        let r = if dir { (v << 1) | bit } else { (v >> 1) | (bit << 7) };
        self.f = logic_flags(r) | (if c {flags::C} else {0});
        r
    }
    fn shift(&mut self, v: u8, mode: u8, left: bool) -> u8 { // 0=logic, 1=arith, 2=SLL
        let c = if left { (v & 0x80) != 0 } else { (v & 1) != 0 };
        let r = match (mode, left) {
            (0, true) => v << 1, (0, false) => v >> 1,
            (1, false) => (v >> 1) | (v & 0x80), // SRA
            (2, true) => (v << 1) | 1, // SLL
            _ => 0
        };
        self.f = logic_flags(r) | (if c {flags::C} else {0});
        r
    }

    // Misc Logic
    fn jr(&mut self, bus: &dyn MemoryBus, c: bool) {
        let o = self.fetch(bus) as i8;
        if c { self.pc = (self.pc as i32 + o as i32) as u16; self.cycles+=12; } else { self.cycles+=7; }
    }
    fn daa(&mut self) {
        let a = self.a;
        let mut diff = 0;
        let mut carry = (self.f & flags::C) != 0;
        let hcc = (self.f & flags::H) != 0;
        let add_sub = (self.f & flags::N) != 0;

        if hcc || (a & 0x0F) > 9 {
            diff |= 0x06;
        }
        if carry || a > 0x99 {
            diff |= 0x60;
            carry = true;
        }

        let res = if add_sub {
            a.wrapping_sub(diff)
        } else {
            a.wrapping_add(diff)
        };

        self.f = (if res == 0 { flags::Z } else { 0 }) |
                 (res & 0x80) |
                 (if add_sub {
                     if (a & 0x0F) < (diff & 0x0F) { flags::H } else { 0 }
                 } else {
                     if (a & 0x0F) > 9 { flags::H } else { 0 }
                 }) |
                 (if carry { flags::C } else { 0 }) |
                 (self.f & flags::N) |
                 (if PARITY_TABLE[res as usize] { flags::P } else { 0 }) |
                 (res & (flags::X | flags::Y));
        self.a = res;
    }
    
    // 16-bit
    fn add16(&mut self, v: u16) {
        let hl=self.hl(); let (r,c)=hl.overflowing_add(v);
        let h=(hl&0xFFF)+(v&0xFFF)>0xFFF;
        self.f=(self.f&(flags::S|flags::Z|flags::P)) | 
               (if h{flags::H}else{0}) | 
               (if c{flags::C}else{0}) |
               (((r >> 8) as u8) & (flags::X | flags::Y));
        self.set_hl(r);
    }
    fn add16_idx(&mut self, ix: bool, v: u16) {
        let b = if ix {self.ix} else {self.iy};
        let (r,c)=b.overflowing_add(v);
        let h=(b&0xFFF)+(v&0xFFF)>0xFFF;
        self.f=(self.f&(flags::S|flags::Z|flags::P))|(if h{flags::H}else{0})|(if c{flags::C}else{0})|(((r >> 8) as u8) & (flags::X | flags::Y));
        if ix {self.ix=r} else {self.iy=r};
    }
    // **FIXED**: Precise SBC16
    fn sbc16(&mut self, v: u16) {
        let hl = self.hl();
        let c = if (self.f & flags::C) != 0 { 1 } else { 0 };
        
        // Use i32 with sign extension for correct overflow calc
        let val_hl = hl as i16 as i32;
        let val_v = v as i16 as i32;
        let res_long = val_hl - val_v - c;
        let res = res_long as u16;
        
        let h = ((hl & 0xFFF) as i32 - (v & 0xFFF) as i32 - c) < 0;
        // Overflow: operands have different signs, result has different sign from HL
        // (HL ^ V) & (HL ^ Res) & 0x8000
        let overflow = ((val_hl ^ val_v) & (val_hl ^ (res as i16 as i32)) & 0x8000) != 0;

        self.f = flags::N |
                 (if res == 0 {flags::Z} else {0}) |
                 (if (res & 0x8000) != 0 {flags::S} else {0}) |
                 (if h {flags::H} else {0}) |
                 (if overflow {flags::P} else {0}) |
                 (if (hl as u32) < (v as u32 + c as u32) { flags::C } else { 0 }) |
                 (((res >> 8) as u8) & (flags::X | flags::Y));
                 
        self.set_hl(res);
    }
    // **NEW**: Precise ADC16 (16-bit add with carry)
    fn adc16(&mut self, v: u16) {
        let hl = self.hl();
        let c = if (self.f & flags::C) != 0 { 1 } else { 0 };
        let res_long = (hl as u32) + (v as u32) + (c as u32);
        let res = res_long as u16;
        
        let h = ((hl & 0xFFF) + (v & 0xFFF) + c as u16) > 0xFFF;
        let overflow = (!(hl ^ v) & (hl ^ res) & 0x8000) != 0;

        self.f = (if res == 0 {flags::Z} else {0}) |
                 (if (res & 0x8000) != 0 {flags::S} else {0}) |
                 (if h {flags::H} else {0}) |
                 (if overflow {flags::P} else {0}) |
                 (if res_long > 0xFFFF {flags::C} else {0}) |
                 (((res >> 8) as u8) & (flags::X | flags::Y)); // X/Y from high byte
                 
        self.set_hl(res);
    }
    
    // Block
    fn ldir(&mut self, bus: &mut dyn MemoryBus, repeat: bool, step: i16) {
        let v = bus.read(self.hl() as u32);
        bus.write(self.de() as u32, v);
        
        self.set_hl(self.hl().wrapping_add(step as u16));
        self.set_de(self.de().wrapping_add(step as u16));
        let bc = self.bc().wrapping_sub(1);
        self.set_bc(bc);
        
        // Flags: N, H cleared. P set if BC != 0.
        self.f &= !(flags::H | flags::N | flags::P);
        if bc != 0 { self.f |= flags::P; }
        
        // Cycle counting and PC adjustment for repeat instructions
        if repeat && bc != 0 {
            self.pc = self.pc.wrapping_sub(2);
            self.cycles += 21;
        } else {
            self.cycles += 16;
        }
    }

    // Block I/O Helpers
    fn block_in(&mut self, bus: &mut dyn MemoryBus, inc: bool, repeat: bool) {
        let port = self.bc();
        let val = bus.port_in(port);
        bus.write(self.hl() as u32, val);
        
        let hl = self.hl();
        if inc { self.set_hl(hl.wrapping_add(1)); } else { self.set_hl(hl.wrapping_sub(1)); }
        self.b = self.b.wrapping_sub(1);
        
        let z = self.b == 0;
        self.f = (self.f & flags::C) | flags::N | (if z {flags::Z} else {0});
        
        if repeat && !z {
            self.pc = self.pc.wrapping_sub(2);
            self.cycles += 21;
        } else {
            self.cycles += 16;
        }
    }

    fn block_out(&mut self, bus: &mut dyn MemoryBus, inc: bool, repeat: bool) {
        let val = bus.read(self.hl() as u32);
        let port = self.bc();
        bus.port_out(port, val);
        
        let hl = self.hl();
        if inc { self.set_hl(hl.wrapping_add(1)); } else { self.set_hl(hl.wrapping_sub(1)); }
        self.b = self.b.wrapping_sub(1);
        
        let z = self.b == 0;
        self.f = (self.f & flags::C) | flags::N | (if z {flags::Z} else {0});
        
        if repeat && !z {
            self.pc = self.pc.wrapping_sub(2);
            self.cycles += 21;
        } else {
            self.cycles += 16;
        }
    }

    fn block_cp(&mut self, bus: &mut dyn MemoryBus, inc: bool, repeat: bool) {
        let v = bus.read(self.hl() as u32);
        let res = self.a.wrapping_sub(v);
        let h = (self.a & 0xF) < (v & 0xF);
        
        let hl = self.hl();
        if inc { self.set_hl(hl.wrapping_add(1)); } else { self.set_hl(hl.wrapping_sub(1)); }
        let bc = self.bc().wrapping_sub(1);
        self.set_bc(bc);
        
        let z = res == 0;
        let s = (res & 0x80) != 0;
        
        // Undocumented Flags for CPI/CPD:
        // Bit 1 (X) = Bit 1 of (A - V - H)
        // Bit 3 (Y) = Bit 3 of (A - V - H)
        let diff = (self.a as i16) - (v as i16) - (if h { 1 } else { 0 });
        
        self.f = (if s { flags::S } else { 0 }) |
                 (if z { flags::Z } else { 0 }) |
                 (if h { flags::H } else { 0 }) |
                 (if bc != 0 { flags::P } else { 0 }) |
                 flags::N |
                 (self.f & flags::C) |
                 ((diff as u8) & flags::Y) | // Bit 5
                 (((diff as u8) << 4) & flags::X); // Bit 3? Wait, bit 3 is bit 3.
        
        // Correcting undocumented flags:
        // Y = bit 1 of (A - V - H) 
        // X = bit 3 of (A - V - H)
        self.f &= !(flags::X | flags::Y);
        if (diff & 0x02) != 0 { self.f |= flags::Y; } // Wait, bit 1 is Y (bit 5)?? No.
        // Y is bit 1 of result? No, typical Z80 CPI flags:
        // Bit 5 (Y) = bit 1 of (A - V - H)
        // Bit 3 (X) = bit 3 of (A - V - H)
        if (diff & 0x02) != 0 { self.f |= flags::Y; }
        if (diff & 0x08) != 0 { self.f |= flags::X; }

        if repeat && bc != 0 && !z {
            self.pc = self.pc.wrapping_sub(2);
            self.cycles += 21;
        } else {
            self.cycles += 16;
        }
    }
}

fn logic_flags(v: u8) -> u8 {
    (if v == 0 { flags::Z } else { 0 }) |
    (v & 0x80) | // S flag is bit 7
    (if PARITY_TABLE[v as usize] { flags::P } else { 0 }) |
    (v & (flags::X | flags::Y))
}