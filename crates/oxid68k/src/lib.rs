// crates/oxid68k/src/lib.rs

use oxide_core::{Cpu, MemoryBus};

// ============================================================================
//  DEFINICIONES DE TIPOS Y CONSTANTES
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Size {
    Byte = 1,
    Word = 2,
    Long = 4,
}

impl Size {
    #[inline(always)]
    pub fn bytes(&self) -> u32 { *self as u32 }

    pub fn from_bits(bits: u16) -> Option<Self> {
        match bits & 0x3 {
            0b00 => Some(Size::Byte),
            0b01 => Some(Size::Word),
            0b10 => Some(Size::Long),
            _ => None,
        }
    }
    
    pub fn mask(&self) -> u32 {
        match self { Size::Byte => 0xFF, Size::Word => 0xFFFF, Size::Long => 0xFFFF_FFFF }
    }
    
    pub fn msb(&self) -> u32 {
        match self { Size::Byte => 0x80, Size::Word => 0x8000, Size::Long => 0x8000_0000 }
    }
}

/// Status Register (SR)
#[derive(Debug, Default, Clone, Copy)]
pub struct StatusRegister {
    // System Byte
    pub trace: bool,        // T (15)
    pub supervisor: bool,   // S (13)
    pub int_mask: u8,       // I0-I2 (8-10)
    // User Byte (CCR)
    pub extend: bool,       // X (4)
    pub negative: bool,     // N (3)
    pub zero: bool,         // Z (2)
    pub overflow: bool,     // V (1)
    pub carry: bool,        // C (0)
}

impl StatusRegister {
    pub fn new() -> Self {
        Self { supervisor: true, int_mask: 7, ..Default::default() }
    }

    pub fn to_u16(&self) -> u16 {
        let mut val = 0u16;
        if self.carry { val |= 1; }
        if self.overflow { val |= 2; }
        if self.zero { val |= 4; }
        if self.negative { val |= 8; }
        if self.extend { val |= 16; }
        val |= (self.int_mask as u16 & 0x7) << 8;
        if self.supervisor { val |= 1 << 13; }
        if self.trace { val |= 1 << 15; }
        val
    }

    // Nota: Usar con precaución. Para cambiar SR completo usar cpu.set_sr()
    pub fn from_u16(&mut self, val: u16) {
        self.carry = (val & 1) != 0;
        self.overflow = (val & 2) != 0;
        self.zero = (val & 4) != 0;
        self.negative = (val & 8) != 0;
        self.extend = (val & 16) != 0;
        self.int_mask = ((val >> 8) & 0x7) as u8;
        self.supervisor = (val & (1 << 13)) != 0;
        self.trace = (val & (1 << 15)) != 0;
    }
    
    pub fn update_logic(&mut self, res: u32, sz: Size) {
        self.zero = (res & sz.mask()) == 0;
        self.negative = (res & sz.msb()) != 0;
        self.overflow = false;
        self.carry = false;
    }
}

// ============================================================================
//  ESTRUCTURA PRINCIPAL DE LA CPU
// ============================================================================

pub struct Oxid68k {
    pub d: [u32; 8],
    pub a: [u32; 8], // A7 es siempre el Stack Pointer ACTIVO (sea USP o SSP)
    pub pc: u32,
    pub sr: StatusRegister,
    
    // Shadow Stack Pointers
    pub usp: u32, // User SP guardado
    pub ssp: u32, // Supervisor SP guardado
    
    pub halted: bool,
    pub stopped: bool,
    pub cycles: u32,
}

impl Oxid68k {
    pub fn new() -> Self {
        Self {
            d: [0; 8], a: [0; 8], pc: 0,
            sr: StatusRegister::new(),
            usp: 0, ssp: 0, // Se inicializan en reset
            halted: false, stopped: false, cycles: 0,
        }
    }

    #[inline(always)]
    fn fetch(&mut self, bus: &dyn MemoryBus) -> u16 {
        let val = bus.read_u16(self.pc);
        self.pc = self.pc.wrapping_add(2);
        val
    }

    #[inline(always)]
    fn fetch_long(&mut self, bus: &dyn MemoryBus) -> u32 {
        let hi = self.fetch(bus) as u32;
        let lo = self.fetch(bus) as u32;
        (hi << 16) | lo
    }

    // --- MANEJO CRÍTICO DE MODOS (USP vs SSP) ---
    
    /// Cambia el SR de forma segura, intercambiando A7 si cambia el modo S.
    fn set_sr(&mut self, new_sr_val: u16) {
        let old_s = self.sr.supervisor;
        self.sr.from_u16(new_sr_val);
        let new_s = self.sr.supervisor;

        if old_s != new_s {
            if new_s {
                // User -> Supervisor
                self.usp = self.a[7]; // Guardar USP actual
                self.a[7] = self.ssp; // Cargar SSP
            } else {
                // Supervisor -> User
                self.ssp = self.a[7]; // Guardar SSP actual
                self.a[7] = self.usp; // Cargar USP
            }
        }
    }

    fn exception(&mut self, vector: u8, bus: &mut dyn MemoryBus) {
        // 1. Si estábamos en User mode, cambiar a Supervisor (swap stacks)
        let old_sr = self.sr.to_u16();
        if !self.sr.supervisor {
            self.usp = self.a[7];
            self.a[7] = self.ssp;
            self.sr.supervisor = true;
        }
        self.sr.trace = false; // Deshabilitar trace en excepcion

        // 2. Push PC & SR al Supervisor Stack
        self.a[7] -= 4;
        self.write_mem(bus, self.a[7], self.pc, Size::Long);
        self.a[7] -= 2;
        self.write_mem(bus, self.a[7], old_sr as u32, Size::Word);

        // 3. Jump to Vector
        let vector_addr = (vector as u32) * 4;
        self.pc = self.read_mem(bus, vector_addr, Size::Long);
        
        self.cycles += 34;
    }
}

// ============================================================================
//  IMPLEMENTACIÓN DEL CONTRATO CPU
// ============================================================================

impl Cpu for Oxid68k {
    fn reset(&mut self) {
        self.d = [0; 8]; self.a = [0; 8]; self.pc = 0;
        self.sr = StatusRegister::new(); // Arranca en Supervisor
        self.ssp = 0; self.usp = 0;
        self.halted = false; self.stopped = false;
    }

    fn reset_with_bus(&mut self, bus: &mut dyn MemoryBus) {
         // Cargar SSP inicial y PC
         let sp_hi = bus.read_u16(0) as u32;
         let sp_lo = bus.read_u16(2) as u32;
         self.ssp = (sp_hi << 16) | sp_lo;
         self.a[7] = self.ssp; // Activar SSP en A7
         
         let pc_hi = bus.read_u16(4) as u32;
         let pc_lo = bus.read_u16(6) as u32;
         self.pc = (pc_hi << 16) | pc_lo;
         
         self.sr = StatusRegister::new(); // Supervisor = true
         self.halted = false; self.stopped = false;
         println!("[Oxid68k] Reset: SSP={:08X} PC={:08X}", self.a[7], self.pc);
    }

    fn pc(&self) -> u32 { self.pc }

    fn step(&mut self, bus: &mut dyn MemoryBus) -> u32 {
        if self.halted { return 0; }
        if self.stopped { 
            // TODO: Chequear interrupciones externas aquí para despertar
            return 4; 
        }

        let opcode = self.fetch(bus);
        self.cycles = 0;

        match (opcode >> 12) & 0xF {
            0x0 => self.exec_group_0(opcode, bus),
            0x1 => self.exec_move(opcode, bus, Size::Byte),
            0x2 => self.exec_move(opcode, bus, Size::Long),
            0x3 => self.exec_move(opcode, bus, Size::Word),
            0x4 => self.exec_group_4(opcode, bus),
            0x5 => self.exec_group_5(opcode, bus),
            0x6 => self.exec_group_6(opcode, bus),
            0x7 => self.exec_moveq(opcode),
            0x8 => self.exec_group_8(opcode, bus),
            0x9 => self.exec_group_9(opcode, bus),
            0xB => self.exec_group_B(opcode, bus),
            0xC => self.exec_group_C(opcode, bus),
            0xD => self.exec_group_D(opcode, bus),
            0xE => self.exec_group_E(opcode, bus),
            _   => self.exec_illegal(opcode, bus),
        }
        self.cycles
    }
}

// ============================================================================
//  EFFECTIVE ADDRESS (EA) ENGINE
// ============================================================================

impl Oxid68k {
    fn read_ea(&mut self, bus: &dyn MemoryBus, mode: u8, reg: u8, size: Size) -> u32 {
        match mode {
            0 => self.d[reg as usize] & size.mask(),
            1 => self.a[reg as usize] & size.mask(),
            2 => self.read_mem(bus, self.a[reg as usize], size),
            3 => { 
                let addr = self.a[reg as usize];
                let val = self.read_mem(bus, addr, size);
                let inc = if reg == 7 && size == Size::Byte { 2 } else { size.bytes() };
                self.a[reg as usize] = self.a[reg as usize].wrapping_add(inc);
                val
            },
            4 => { 
                let dec = if reg == 7 && size == Size::Byte { 2 } else { size.bytes() };
                self.a[reg as usize] = self.a[reg as usize].wrapping_sub(dec);
                self.read_mem(bus, self.a[reg as usize], size)
            },
            5 => { 
                let disp = self.fetch(bus) as i16 as i32;
                let addr = (self.a[reg as usize] as i32 + disp) as u32;
                self.read_mem(bus, addr, size)
            },
            7 => match reg {
                0 => self.fetch(bus) as i16 as i32 as u32, // Abs.W
                1 => self.fetch_long(bus), // Abs.L
                2 => { // PC Rel
                    let base = self.pc.wrapping_sub(2);
                    let disp = self.fetch(bus) as i16 as i32;
                    self.read_mem(bus, (base as i32 + disp) as u32, size)
                },
                4 => match size {
                    Size::Byte => (self.fetch(bus) & 0xFF) as u32,
                    Size::Word => self.fetch(bus) as u32,
                    Size::Long => self.fetch_long(bus),
                },
                _ => panic!("Read EA 7:{} no impl", reg),
            },
            _ => panic!("Read EA Ilegal {}", mode),
        }
    }

    fn write_ea(&mut self, bus: &mut dyn MemoryBus, mode: u8, reg: u8, size: Size, val: u32) {
        match mode {
            0 => self.set_reg_d(reg as usize, val, size),
            1 => {
                let v = if size == Size::Word { (val as u16) as i16 as i32 as u32 } else { val };
                self.a[reg as usize] = v;
            },
            2 => self.write_mem(bus, self.a[reg as usize], val, size),
            3 => { 
                let addr = self.a[reg as usize];
                self.write_mem(bus, addr, val, size);
                let inc = if reg == 7 && size == Size::Byte { 2 } else { size.bytes() };
                self.a[reg as usize] = self.a[reg as usize].wrapping_add(inc);
            },
            4 => { 
                let dec = if reg == 7 && size == Size::Byte { 2 } else { size.bytes() };
                self.a[reg as usize] = self.a[reg as usize].wrapping_sub(dec);
                self.write_mem(bus, self.a[reg as usize], val, size);
            },
            5 => { 
                let disp = self.fetch(bus) as i16 as i32; 
                let addr = (self.a[reg as usize] as i32 + disp) as u32;
                self.write_mem(bus, addr, val, size);
            },
            7 => match reg {
                1 => { let addr = self.fetch_long(bus); self.write_mem(bus, addr, val, size); },
                _ => panic!("Write EA 7:{} no impl", reg),
            },
            _ => panic!("Write EA Ilegal {}", mode),
        }
    }

    fn calculate_ea_addr(&mut self, bus: &dyn MemoryBus, mode: u8, reg: u8) -> u32 {
        match mode {
            2 => self.a[reg as usize],
            5 => {
                let disp = self.fetch(bus) as i16 as i32;
                (self.a[reg as usize] as i32 + disp) as u32
            },
            7 => match reg {
                0 => self.fetch(bus) as i16 as i32 as u32,
                1 => self.fetch_long(bus),
                2 => {
                     let base = self.pc.wrapping_sub(2);
                     let disp = self.fetch(bus) as i16 as i32;
                     (base as i32 + disp) as u32
                },
                _ => panic!("Calc EA 7:{} no impl", reg),
            },
            _ => panic!("Calc EA Ilegal {}", mode),
        }
    }

    fn read_mem(&self, bus: &dyn MemoryBus, addr: u32, size: Size) -> u32 {
        match size {
            Size::Byte => bus.read(addr) as u32,
            Size::Word => bus.read_u16(addr) as u32,
            Size::Long => {
                let hi = bus.read_u16(addr) as u32;
                let lo = bus.read_u16(addr + 2) as u32;
                (hi << 16) | lo
            }
        }
    }

    fn write_mem(&mut self, bus: &mut dyn MemoryBus, addr: u32, val: u32, size: Size) {
        match size {
            Size::Byte => bus.write(addr, val as u8),
            Size::Word => { bus.write(addr, (val >> 8) as u8); bus.write(addr + 1, val as u8); },
            Size::Long => {
                bus.write(addr, (val >> 24) as u8); bus.write(addr + 1, (val >> 16) as u8);
                bus.write(addr + 2, (val >> 8) as u8); bus.write(addr + 3, val as u8);
            }
        }
    }
    
    fn set_reg_d(&mut self, idx: usize, val: u32, size: Size) {
        match size {
            Size::Byte => self.d[idx] = (self.d[idx] & 0xFFFF_FF00) | (val & 0xFF),
            Size::Word => self.d[idx] = (self.d[idx] & 0xFFFF_0000) | (val & 0xFFFF),
            Size::Long => self.d[idx] = val,
        }
    }
}

// ============================================================================
//  LÓGICA DE INSTRUCCIONES POR GRUPOS
// ============================================================================

impl Oxid68k {
    // --- GRUPO 0: Bit, Imm, SR, MOVEP ---
    fn exec_group_0(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        if (opcode & 0x0100) != 0 || (opcode & 0x0800) == 0x0800 {
            if (opcode & 0xC0) != 0xC0 { 
                 self.exec_bit_op(opcode, bus);
                 return;
            }
        }

        let size = Size::from_bits((opcode >> 6) & 0x3).unwrap();
        let ea_mode = (opcode >> 3) & 0x7;
        let ea_reg = (opcode & 0x7) as u8;

        match (opcode >> 9) & 0x7 {
            0 => { // ORI
                if opcode == 0x007C { // ORI to SR (Privileged)
                    let val = self.fetch(bus);
                    if self.sr.supervisor { self.set_sr(self.sr.to_u16() | val); } else { self.exception(8, bus); }
                    self.cycles += 20; return;
                }
                let imm = self.fetch_val(bus, size);
                let dst = self.read_ea(bus, ea_mode as u8, ea_reg, size);
                let res = dst | imm;
                self.sr.update_logic(res, size);
                self.write_ea(bus, ea_mode as u8, ea_reg, size, res);
                self.cycles += 8;
            },
            1 => { // ANDI
                if opcode == 0x027C { // ANDI to SR (Privileged)
                    let val = self.fetch(bus);
                    if self.sr.supervisor { self.set_sr(self.sr.to_u16() & val); } else { self.exception(8, bus); }
                    self.cycles += 20; return;
                }
                let imm = self.fetch_val(bus, size);
                let dst = self.read_ea(bus, ea_mode as u8, ea_reg, size);
                let res = dst & imm;
                self.sr.update_logic(res, size);
                self.write_ea(bus, ea_mode as u8, ea_reg, size, res);
                self.cycles += 8;
            },
            2 => { // SUBI
                let imm = self.fetch_val(bus, size);
                let dst = self.read_ea(bus, ea_mode as u8, ea_reg, size);
                let (res, c, v) = self.sub_generic(dst, imm, size);
                self.update_flags_sub(res, c, v, size);
                self.sr.extend = c;
                self.write_ea(bus, ea_mode as u8, ea_reg, size, res);
                self.cycles += 8;
            },
            3 => { // ADDI
                let imm = self.fetch_val(bus, size);
                let dst = self.read_ea(bus, ea_mode as u8, ea_reg, size);
                let (res, c, v) = self.add_generic(dst, imm, size);
                self.update_flags_sub(res, c, v, size);
                self.sr.extend = c;
                self.write_ea(bus, ea_mode as u8, ea_reg, size, res);
                self.cycles += 8;
            },
            5 => { // EORI
                 if opcode == 0x0A7C { // EORI to SR (Privileged)
                    let val = self.fetch(bus);
                    if self.sr.supervisor { self.set_sr(self.sr.to_u16() ^ val); } else { self.exception(8, bus); }
                    self.cycles += 20; return;
                }
                let imm = self.fetch_val(bus, size);
                let dst = self.read_ea(bus, ea_mode as u8, ea_reg, size);
                let res = dst ^ imm;
                self.sr.update_logic(res, size);
                self.write_ea(bus, ea_mode as u8, ea_reg, size, res);
                self.cycles += 8;
            },
            6 => { // CMPI
                let imm = self.fetch_val(bus, size);
                let dst = self.read_ea(bus, ea_mode as u8, ea_reg, size);
                let (res, c, v) = self.sub_generic(dst, imm, size);
                self.update_flags_sub(res, c, v, size);
                self.cycles += 8;
            },
            _ => { }
        }
    }

    fn exec_bit_op(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        let dynamic = (opcode & 0x0100) != 0;
        let ea_mode = (opcode >> 3) & 0x7;
        let ea_reg = (opcode & 0x7) as u8;
        
        let bit = if dynamic {
            let reg = ((opcode >> 9) & 0x7) as usize;
            self.d[reg] & 31
        } else {
            (self.fetch(bus) & 0xFF) as u32
        };

        let is_reg = ea_mode == 0;
        let val = if is_reg { self.d[ea_reg as usize] } 
                  else { self.read_ea(bus, ea_mode as u8, ea_reg, Size::Byte) as u32 };

        let target = if is_reg { bit % 32 } else { bit % 8 };
        let mask = 1 << target;
        
        self.sr.zero = (val & mask) == 0; 

        let op = (opcode >> 6) & 0x3;
        if op == 0 { self.cycles += if dynamic {6} else {10}; return; } // BTST

        let new_val = match op {
            1 => val ^ mask,  // BCHG
            2 => val & !mask, // BCLR
            3 => val | mask,  // BSET
            _ => val,
        };

        if is_reg {
            self.set_reg_d(ea_reg as usize, new_val, Size::Long);
            self.cycles += if dynamic {8} else {12};
        } else {
            self.write_ea(bus, ea_mode as u8, ea_reg, Size::Byte, new_val);
            self.cycles += if dynamic {8} else {12};
        }
    }

    fn fetch_val(&mut self, bus: &dyn MemoryBus, size: Size) -> u32 {
        match size {
            Size::Byte => (self.fetch(bus) & 0xFF) as u32,
            Size::Word => self.fetch(bus) as u32,
            Size::Long => self.fetch_long(bus),
        }
    }

    // --- GROUP 1, 2, 3: MOVE ---
    fn exec_move(&mut self, opcode: u16, bus: &mut dyn MemoryBus, size: Size) {
        let src_mode = (opcode >> 3) & 0x7;
        let src_reg = (opcode & 0x7) as u8;
        let dst_mode = (opcode >> 6) & 0x7;
        let dst_reg = (opcode >> 9) & 0x7;

        let val = self.read_ea(bus, src_mode as u8, src_reg, size);
        
        // MOVEA does NOT update flags
        if dst_mode != 1 {
            self.sr.update_logic(val, size);
        }
        
        self.write_ea(bus, dst_mode as u8, dst_reg as u8, size, val);
        self.cycles += 4;
    }

    // --- GROUP 4: Misc ---
    fn exec_group_4(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        if (opcode & 0xFB80) == 0x4880 { // MOVEM
            let dr = (opcode & 0x0400) != 0; 
            let sz = if (opcode & 0x0040) != 0 { Size::Long } else { Size::Word };
            let mode = (opcode >> 3) & 0x7;
            let reg = (opcode & 0x7) as u8;
            let mask = self.fetch(bus);
            self.exec_movem(bus, mode as u8, reg, mask, dr, sz);
            return;
        }

        // MOVE USP (Privileged)
        if (opcode & 0xFFF0) == 0x4E60 {
            if !self.sr.supervisor { self.exception(8, bus); return; }
            let reg = (opcode & 0x7) as usize;
            if (opcode & 0x8) != 0 { // MOVE USP, An (0x4E68)
                self.a[reg] = self.usp;
            } else { // MOVE An, USP (0x4E60)
                self.usp = self.a[reg];
            }
            self.cycles += 4; return;
        }
        
        // NBCD
        if (opcode & 0xFFC0) == 0x4800 {
            let mode = (opcode >> 3) & 0x7;
            let reg = (opcode & 0x7) as u8;
            let dst = self.read_ea(bus, mode as u8, reg, Size::Byte);
            let res = self.bcd_add(0, dst as u8, 0, true); // 0 - dst - X
            self.write_ea(bus, mode as u8, reg, Size::Byte, res as u32);
            self.cycles += 8; return;
        }

        if (opcode & 0xFFF0) == 0x4880 || (opcode & 0xFFF0) == 0x48C0 { // EXT
             let reg = (opcode & 0x7) as usize;
             let op = (opcode >> 6) & 0x7;
             if op == 2 { 
                 let val = self.d[reg] as i8 as i16 as u32;
                 self.set_reg_d(reg, val, Size::Word);
                 self.sr.update_logic(val, Size::Word);
             } else if op == 3 {
                 let val = self.d[reg] as i16 as i32 as u32;
                 self.d[reg] = val;
                 self.sr.update_logic(val, Size::Long);
             }
             self.cycles += 4; return;
        }
        if (opcode & 0xFFF8) == 0x4840 { // SWAP
             let reg = (opcode & 0x7) as usize;
             let val = self.d[reg];
             let res = (val << 16) | (val >> 16);
             self.d[reg] = res;
             self.sr.update_logic(res, Size::Long);
             self.cycles += 4; return;
        }

        if opcode == 0x4E71 { self.cycles += 4; return; } // NOP
        if opcode == 0x4E73 { self.exec_rte(bus); return; } // RTE
        if opcode == 0x4E75 { // RTS
            let ret = self.read_mem(bus, self.a[7], Size::Long);
            self.a[7] += 4;
            self.pc = ret;
            self.cycles += 16; return;
        }
        
        if (opcode & 0xFFF0) == 0x4E40 { // TRAP
            let vec = (opcode & 0xF) as u8;
            self.exception(32 + vec, bus); return;
        }

        if (opcode & 0xFFF8) == 0x4E50 { // LINK
            let reg = (opcode & 0x7) as usize;
            let disp = self.fetch(bus) as i16 as i32;
            self.a[7] -= 4;
            self.write_mem(bus, self.a[7], self.a[reg], Size::Long);
            self.a[reg] = self.a[7];
            self.a[7] = (self.a[7] as i32 + disp) as u32;
            self.cycles += 16; return;
        }

        if (opcode & 0xFFF8) == 0x4E58 { // UNLK
            let reg = (opcode & 0x7) as usize;
            self.a[7] = self.a[reg];
            let val = self.read_mem(bus, self.a[7], Size::Long);
            self.a[reg] = val;
            self.a[7] += 4;
            self.cycles += 12; return;
        }
        
        if (opcode & 0xFFC0) == 0x4AC0 { // TAS
             let mode = (opcode >> 3) & 0x7;
             let reg = (opcode & 0x7) as u8;
             let val = self.read_ea(bus, mode as u8, reg, Size::Byte);
             self.sr.update_logic(val as u32, Size::Byte);
             self.write_ea(bus, mode as u8, reg, Size::Byte, val | 0x80); 
             self.cycles += 4; return;
        }

        // JMP, JSR, LEA, PEA
        let mode = (opcode >> 3) & 0x7;
        let reg = (opcode & 0x7) as u8;
        match (opcode >> 6) & 0x3F {
            0x39 => { self.pc = self.calculate_ea_addr(bus, mode as u8, reg); self.cycles += 8; }, // JMP
            0x3A => { // JSR
                let tgt = self.calculate_ea_addr(bus, mode as u8, reg);
                self.a[7] -= 4;
                self.write_mem(bus, self.a[7], self.pc, Size::Long);
                self.pc = tgt;
                self.cycles += 16;
            },
            _ => {
                if (opcode & 0x01C0) == 0x01C0 { // LEA
                    let rd = ((opcode >> 9) & 0x7) as usize;
                    let addr = self.calculate_ea_addr(bus, mode as u8, reg);
                    self.a[rd] = addr; self.cycles += 4;
                } else if (opcode & 0x01C0) == 0x0140 { // PEA
                    let addr = self.calculate_ea_addr(bus, mode as u8, reg);
                    self.a[7] -= 4;
                    self.write_mem(bus, self.a[7], addr, Size::Long);
                    self.cycles += 8;
                }
            }
        }
    }

    fn exec_movem(&mut self, bus: &mut dyn MemoryBus, mode: u8, reg: u8, mask: u16, dr: bool, size: Size) {
        let mut addr = self.calculate_ea_addr(bus, mode, reg);
        if dr { // Mem -> Reg
            for i in 0..16 {
                if (mask & (1 << i)) != 0 {
                    let val = if size == Size::Word {
                        (self.read_mem(bus, addr, Size::Word) as i16) as i32 as u32
                    } else { self.read_mem(bus, addr, Size::Long) };
                    if i < 8 { self.d[i] = val; } else { self.a[i-8] = val; }
                    addr += size.bytes();
                }
            }
            if mode == 3 { self.a[reg as usize] = addr; }
        } else { // Reg -> Mem
            if mode == 4 { // Predecrement
                let mut temp = self.a[reg as usize];
                for i in 0..16 { 
                    if (mask & (1 << i)) != 0 {
                         temp -= size.bytes();
                         let val = if i < 8 { self.d[i] } else { self.a[i-8] };
                         self.write_mem(bus, temp, val, size);
                    }
                }
                self.a[reg as usize] = temp;
            } else {
                 for i in 0..16 {
                    if (mask & (1 << i)) != 0 {
                        let val = if i < 8 { self.d[i] } else { self.a[i-8] };
                        self.write_mem(bus, addr, val, size);
                        addr += size.bytes();
                    }
                }
            }
        }
        self.cycles += 8;
    }

    fn exec_rte(&mut self, bus: &dyn MemoryBus) {
        let sr_val = self.read_mem(bus, self.a[7], Size::Word) as u16;
        self.a[7] += 2;
        // CRÍTICO: set_sr gestiona el cambio de stack USP/SSP si el bit S cambia
        self.set_sr(sr_val); 
        let pc_val = self.read_mem(bus, self.a[7], Size::Long);
        self.a[7] += 4;
        self.pc = pc_val;
        self.cycles += 20;
    }

    // --- GROUP 5: ADDQ, SUBQ, Scc, DBcc ---
    fn exec_group_5(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        if (opcode & 0xC0) == 0 { // ADDQ/SUBQ
            let imm = (opcode >> 9) & 0x7;
            let val = if imm == 0 { 8 } else { imm as u32 };
            let sub = (opcode & 0x0100) != 0;
            let sz = Size::from_bits((opcode >> 6) & 0x3).unwrap();
            let mode = (opcode >> 3) & 0x7;
            let reg = (opcode & 0x7) as u8;
            let dst = self.read_ea(bus, mode as u8, reg, sz);
            let (res, c, v) = if sub { self.sub_generic(dst, val, sz) } else { self.add_generic(dst, val, sz) };
            if mode != 1 {
                self.sr.zero = (res & sz.mask()) == 0;
                self.sr.negative = (res & sz.msb()) != 0;
                self.sr.overflow = v; self.sr.carry = c; self.sr.extend = c;
            }
            self.write_ea(bus, mode as u8, reg, sz, res);
            self.cycles += 4;
            return;
        }
        
        let cond = (opcode >> 8) & 0xF;
        let mode = (opcode >> 3) & 0x7;
        let reg = (opcode & 0x7) as u8;
        let met = self.check_condition(cond as u8);

        if mode == 1 { // DBcc
            let disp = self.fetch(bus) as i16 as i32;
            if !met {
                let r = (opcode & 0x7) as usize;
                let val = (self.d[r] as i16).wrapping_sub(1);
                self.set_reg_d(r, val as u32, Size::Word);
                if val != -1 {
                    self.pc = (self.pc.wrapping_sub(2) as i32 + disp) as u32;
                    self.cycles += 10;
                } else { self.cycles += 14; }
            } else { self.cycles += 12; }
        } else { // Scc
            let val = if met { 0xFF } else { 0x00 };
            self.write_ea(bus, mode as u8, reg, Size::Byte, val);
            self.cycles += 8;
        }
    }

    // --- GROUP 6: Branches ---
    fn exec_group_6(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        let cond = (opcode >> 8) & 0xF;
        let disp = (opcode & 0xFF) as i8 as i32;
        let offset = if disp == 0 { self.fetch(bus) as i16 as i32 } else { disp };
        
        if (opcode & 0xFF) == 0 && cond == 1 { // BSR
            self.a[7] -= 4;
            self.write_mem(bus, self.a[7], self.pc, Size::Long);
            self.pc = (self.pc.wrapping_sub(2) as i32 + offset) as u32;
        } else if self.check_condition(cond as u8) {
            self.pc = (self.pc.wrapping_sub(2) as i32 + offset) as u32;
            self.cycles += 10;
        } else {
            self.cycles += 8;
        }
    }

    // --- GROUP 7: MOVEQ ---
    fn exec_moveq(&mut self, opcode: u16) {
        let reg = ((opcode >> 9) & 0x7) as usize;
        let val = (opcode & 0xFF) as i8 as i32 as u32;
        self.d[reg] = val;
        self.sr.update_logic(val, Size::Long);
        self.cycles += 4;
    }

    // --- GROUP 8: DIV, OR, SBCD ---
    fn exec_group_8(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        let mode = (opcode >> 3) & 0x7;
        let reg = (opcode & 0x7) as u8;
        let op = (opcode >> 6) & 0x7;
        let idx = ((opcode >> 9) & 0x7) as usize;

        if op == 4 { // SBCD
             let mem = (opcode & 0x8) != 0;
             let ry = (opcode & 0x7) as usize;
             let (src, dst) = if mem {
                  // -(Ax) - -(Ay)
                  self.a[ry] -= 1; self.a[idx] -= 1;
                  (self.read_mem(bus, self.a[ry], Size::Byte) as u8, self.read_mem(bus, self.a[idx], Size::Byte) as u8)
             } else {
                  (self.d[ry] as u8, self.d[idx] as u8)
             };
             let res = self.bcd_add(dst, src, 0, true); // Sub using negate add? Simplified bcd_sub here
             if mem { self.write_mem(bus, self.a[idx], res as u32, Size::Byte); } else { self.set_reg_d(idx, res as u32, Size::Byte); }
             self.cycles += 6; return;
        }

        match op {
            3 => { // DIVU
                let div = self.read_ea(bus, mode as u8, reg, Size::Word) as u16;
                if div == 0 { self.exception(5, bus); return; }
                let dvd = self.d[idx];
                let q = dvd / (div as u32);
                let r = dvd % (div as u32);
                if q > 0xFFFF { self.sr.overflow = true; self.sr.carry = false; }
                else {
                    self.d[idx] = (r << 16) | q;
                    self.sr.negative = (q as i16) < 0; self.sr.zero = q == 0;
                    self.sr.overflow = false; self.sr.carry = false;
                }
                self.cycles += 140;
            },
            7 => { // DIVS
                let div = self.read_ea(bus, mode as u8, reg, Size::Word) as i16;
                if div == 0 { self.exception(5, bus); return; }
                let dvd = self.d[idx] as i32;
                let q = dvd / (div as i32);
                let r = dvd % (div as i32);
                if q > 32767 || q < -32768 { self.sr.overflow = true; self.sr.carry = false; }
                else {
                    self.d[idx] = ((r as u32 & 0xFFFF) << 16) | (q as u32 & 0xFFFF);
                    self.sr.negative = q < 0; self.sr.zero = q == 0;
                    self.sr.overflow = false; self.sr.carry = false;
                }
                self.cycles += 158;
            },
            _ => { // OR
                 let (sz, mem) = decode_opmode(op);
                 if sz == Size::Byte && op > 6 { return; }
                 let ea = self.read_ea(bus, mode as u8, reg, sz);
                 let res = if !mem { self.d[idx] | ea } else { ea | self.d[idx] };
                 if !mem { self.set_reg_d(idx, res, sz); } else { self.write_ea(bus, mode as u8, reg, sz, res); }
                 self.sr.update_logic(res, sz);
                 self.cycles += 4;
            }
        }
    }

    // --- GROUP 9: SUB ---
    fn exec_group_9(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        let idx = ((opcode >> 9) & 0x7) as usize;
        let op = (opcode >> 6) & 0x7;
        let mode = (opcode >> 3) & 0x7;
        let reg = (opcode & 0x7) as u8;
        let (sz, mem) = decode_opmode(op);
        
        let src = self.read_ea(bus, mode as u8, reg, sz);
        let dst = if !mem { self.d[idx] } else { self.read_ea(bus, mode as u8, reg, sz) };
        let (res, c, v) = self.sub_generic(dst, src, sz);
        self.update_flags_sub(res, c, v, sz);
        self.sr.extend = c;
        if !mem { self.set_reg_d(idx, res, sz); } else { self.write_ea(bus, mode as u8, reg, sz, res); }
        self.cycles += 4;
    }

    // --- GROUP B: CMP, EOR ---
    fn exec_group_B(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        let idx = ((opcode >> 9) & 0x7) as usize;
        let op = (opcode >> 6) & 0x7;
        let mode = (opcode >> 3) & 0x7;
        let reg = (opcode & 0x7) as u8;

        if op >= 4 && op <= 6 { // EOR
            let sz = match op { 4=>Size::Byte, 5=>Size::Word, 6=>Size::Long, _=>unreachable!() };
            let src = self.d[idx];
            let dst = self.read_ea(bus, mode as u8, reg, sz);
            let res = src ^ dst;
            self.sr.update_logic(res, sz);
            self.write_ea(bus, mode as u8, reg, sz, res);
            self.cycles += 8;
            return;
        }
        // CMP
        let sz = match op { 0=>Size::Byte, 1=>Size::Word, 2=>Size::Long, 3|7=>Size::Long, _=>return };
        let src = self.read_ea(bus, mode as u8, reg, if op==3 {Size::Word} else {sz});
        let src_ext = if op==3 { src as i16 as i32 as u32 } else { src };
        let dst = if op==3 || op==7 { self.a[idx] } else { self.d[idx] };
        
        let (res, c, v) = self.sub_generic(dst, src_ext, sz);
        self.update_flags_sub(res, c, v, sz);
        self.cycles += 4;
    }

    // --- GROUP C: AND, MUL, ABCD, EXG ---
    fn exec_group_C(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        let mode = (opcode >> 3) & 0x7;
        let reg = (opcode & 0x7) as u8;
        let op = (opcode >> 6) & 0x7;
        let idx = ((opcode >> 9) & 0x7) as usize;
        
        if op == 4 { // ABCD
             let mem = (opcode & 0x8) != 0;
             let ry = (opcode & 0x7) as usize;
             let (src, dst) = if mem {
                  self.a[ry] -= 1; self.a[idx] -= 1;
                  (self.read_mem(bus, self.a[ry], Size::Byte) as u8, self.read_mem(bus, self.a[idx], Size::Byte) as u8)
             } else {
                  (self.d[ry] as u8, self.d[idx] as u8)
             };
             let res = self.bcd_add(dst, src, 0, false);
             if mem { self.write_mem(bus, self.a[idx], res as u32, Size::Byte); } else { self.set_reg_d(idx, res as u32, Size::Byte); }
             self.cycles += 6; return;
        }

        match op {
            3 => { // MULU
                let src = self.read_ea(bus, mode as u8, reg, Size::Word) as u16;
                let dst = self.d[idx] as u16;
                let res = (dst as u32) * (src as u32);
                self.d[idx] = res;
                self.sr.negative = (res as i32) < 0; self.sr.zero = res==0;
                self.sr.overflow = false; self.sr.carry = false;
                self.cycles += 70;
            },
            7 => { // MULS
                let src = self.read_ea(bus, mode as u8, reg, Size::Word) as i16;
                let dst = self.d[idx] as i16;
                let res = (dst as i32) * (src as i32);
                self.d[idx] = res as u32;
                self.sr.negative = res < 0; self.sr.zero = res==0;
                self.sr.overflow = false; self.sr.carry = false;
                self.cycles += 70;
            },
            _ => { // AND, EXG
                let op_full = (opcode >> 3) & 0x1F;
                if op_full == 0x08 || op_full == 0x09 || op_full == 0x11 { // EXG
                     let ry = (opcode & 0x7) as usize;
                     match op_full {
                         0x08 => { let t = self.d[idx]; self.d[idx]=self.d[ry]; self.d[ry]=t; },
                         0x09 => { let t = self.a[idx]; self.a[idx]=self.a[ry]; self.a[ry]=t; },
                         0x11 => { let t = self.d[idx]; self.d[idx]=self.a[ry]; self.a[ry]=t; },
                         _ => {}
                     }
                     self.cycles += 6; return;
                }
                let (sz, mem) = decode_opmode(op);
                if sz == Size::Byte && op > 6 { return; }
                let ea = self.read_ea(bus, mode as u8, reg, sz);
                let res = if !mem { self.d[idx] & ea } else { ea & self.d[idx] };
                if !mem { self.set_reg_d(idx, res, sz); } else { self.write_ea(bus, mode as u8, reg, sz, res); }
                self.sr.update_logic(res, sz);
                self.cycles += 4;
            }
        }
    }

    // --- GROUP D: ADD ---
    fn exec_group_D(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        let idx = ((opcode >> 9) & 0x7) as usize;
        let op = (opcode >> 6) & 0x7;
        let mode = (opcode >> 3) & 0x7;
        let reg = (opcode & 0x7) as u8;
        let (sz, mem) = decode_opmode(op);
        
        let src = self.read_ea(bus, mode as u8, reg, sz);
        let dst = if !mem { self.d[idx] } else { self.read_ea(bus, mode as u8, reg, sz) };
        let (res, c, v) = self.add_generic(dst, src, sz);
        self.sr.zero = (res & sz.mask()) == 0;
        self.sr.negative = (res & sz.msb()) != 0;
        self.sr.overflow = v; self.sr.carry = c; self.sr.extend = c;
        if !mem { self.set_reg_d(idx, res, sz); } else { self.write_ea(bus, mode as u8, reg, sz, res); }
        self.cycles += 4;
    }

    // --- GROUP E: Shifts ---
    fn exec_group_E(&mut self, opcode: u16, _bus: &mut dyn MemoryBus) {
        let sz = Size::from_bits((opcode >> 6) & 0x3).unwrap();
        let stype = (opcode >> 3) & 0x3;
        let left = (opcode & 0x0100) != 0;
        let reg = (opcode & 0x7) as usize;
        let ir = (opcode & 0x0020) != 0;
        
        let cnt = if ir { (self.d[((opcode >> 9) & 0x7) as usize] & 63) as u32 }
                  else { let c = (opcode >> 9) & 0x7; if c==0 {8} else {c as u32} };
        
        let val = self.d[reg];
        let (res, c, v) = match stype {
            0 => self.shift_asr(val, cnt, left, sz),
            1 => self.shift_lsr(val, cnt, left, sz),
            2 => (val, self.sr.carry, false), // ROX stub
            3 => self.rot(val, cnt, left, sz),
            _ => (val, false, false),
        };
        self.set_reg_d(reg, res, sz);
        self.sr.zero = (res & sz.mask()) == 0;
        self.sr.negative = (res & sz.msb()) != 0;
        self.sr.carry = c; self.sr.overflow = v;
        if stype != 3 { self.sr.extend = c; }
        self.cycles += 8 + (2*cnt);
    }
    
    // Shift logic helpers
    fn shift_lsr(&self, val: u32, cnt: u32, left: bool, sz: Size) -> (u32, bool, bool) {
        let mask = sz.mask();
        let val = val & mask;
        if cnt == 0 { return (val, false, false); }
        if left {
            let res = (val << cnt) & mask;
            let c = ((val << (cnt - 1)) & sz.msb()) != 0;
            (res, c, false)
        } else {
            let res = (val >> cnt) & mask;
            let c = ((val >> (cnt - 1)) & 1) != 0;
            (res, c, false)
        }
    }
    fn shift_asr(&self, val: u32, cnt: u32, left: bool, sz: Size) -> (u32, bool, bool) {
        let (res, c, _) = self.shift_lsr(val, cnt, left, sz);
        if left {
            let msb = sz.msb();
            let v = ((val & msb) != 0) != ((res & msb) != 0);
            (res, c, v)
        } else {
            let mask = sz.mask();
            let sval = match sz { Size::Byte => (val as i8) as i32, Size::Word => (val as i16) as i32, Size::Long => val as i32 };
            let res = (sval >> cnt) as u32 & mask;
            let c = if cnt > 0 { ((sval >> (cnt-1)) & 1) != 0 } else { false };
            (res, c, false)
        }
    }
    fn rot(&self, val: u32, cnt: u32, left: bool, sz: Size) -> (u32, bool, bool) {
        let bits = sz.bytes() * 8;
        let c = cnt % bits;
        let mask = sz.mask();
        let val = val & mask;
        if c == 0 { return (val, false, false); }
        let res = if left { ((val << c) | (val >> (bits - c))) & mask } 
                  else { ((val >> c) | (val << (bits - c))) & mask };
        let carry = if left { (res & 1) != 0 } else { (res & sz.msb()) != 0 };
        (res, carry, false)
    }

    fn exec_illegal(&mut self, opcode: u16, bus: &mut dyn MemoryBus) {
        println!("[Oxid68k] ILLEGAL: {:04X}", opcode);
        self.exception(4, bus);
    }
}

// ============================================================================
//  MATH & BCD HELPERS
// ============================================================================

fn decode_opmode(op: u16) -> (Size, bool) {
    match op {
        0 => (Size::Byte, false), 1 => (Size::Word, false), 2 => (Size::Long, false),
        4 => (Size::Byte, true),  5 => (Size::Word, true),  6 => (Size::Long, true),
        _ => (Size::Long, false),
    }
}

impl Oxid68k {
    fn sub_generic(&self, dst: u32, src: u32, sz: Size) -> (u32, bool, bool) {
        match sz {
            Size::Byte => self.sub_8(dst as u8, src as u8),
            Size::Word => self.sub_16(dst as u16, src as u16),
            Size::Long => self.sub_32(dst, src),
        }
    }
    fn add_generic(&self, dst: u32, src: u32, sz: Size) -> (u32, bool, bool) {
        match sz {
            Size::Byte => self.add_8(dst as u8, src as u8),
            Size::Word => self.add_16(dst as u16, src as u16),
            Size::Long => self.add_32(dst, src),
        }
    }
    
    // BCD Logic (Critical for Genesis games)
    fn bcd_add(&mut self, dst: u8, src: u8, _x: u8, sub: bool) -> u8 {
        let mut lo = (dst & 0xF) as i16;
        let mut hi = ((dst >> 4) & 0xF) as i16;
        let slo = (src & 0xF) as i16;
        let shi = ((src >> 4) & 0xF) as i16;
        let x_val = if self.sr.extend { 1 } else { 0 };

        if sub { lo = lo - slo - x_val; hi = hi - shi; } 
        else { lo = lo + slo + x_val; hi = hi + shi; }
        
        let mut c = false;
        if lo > 9 { lo -= 10; hi += 1; } 
        else if lo < 0 { lo += 10; hi -= 1; }
        
        if hi > 9 { hi -= 10; c = true; }
        else if hi < 0 { hi += 10; c = true; } // borrow set C
        
        self.sr.extend = c; self.sr.carry = c;
        if (hi | lo) != 0 { self.sr.zero = false; } // Z cleared if non-zero, else unchanged
        
        ((hi << 4) | lo) as u8
    }

    fn sub_32(&self, dst: u32, src: u32) -> (u32, bool, bool) {
        let res = dst.wrapping_sub(src);
        let c = src > dst;
        let v = ((dst ^ src) & (dst ^ res) & 0x8000_0000) != 0;
        (res, c, v)
    }
    fn sub_16(&self, dst: u16, src: u16) -> (u32, bool, bool) {
        let res = dst.wrapping_sub(src);
        let c = src > dst;
        let v = ((dst ^ src) & (dst ^ res) & 0x8000) != 0;
        (res as u32, c, v)
    }
    fn sub_8(&self, dst: u8, src: u8) -> (u32, bool, bool) {
        let res = dst.wrapping_sub(src);
        let c = src > dst;
        let v = ((dst ^ src) & (dst ^ res) & 0x80) != 0;
        (res as u32, c, v)
    }
    fn add_32(&self, dst: u32, src: u32) -> (u32, bool, bool) {
        let res = dst.wrapping_add(src);
        let c = res < dst;
        let v = (!(dst ^ src) & (dst ^ res) & 0x8000_0000) != 0;
        (res, c, v)
    }
    fn add_16(&self, dst: u16, src: u16) -> (u32, bool, bool) {
        let res = dst.wrapping_add(src);
        let c = res < dst;
        let v = (!(dst ^ src) & (dst ^ res) & 0x8000) != 0;
        (res as u32, c, v)
    }
    fn add_8(&self, dst: u8, src: u8) -> (u32, bool, bool) {
        let res = dst.wrapping_add(src);
        let c = res < dst;
        let v = (!(dst ^ src) & (dst ^ res) & 0x80) != 0;
        (res as u32, c, v)
    }
    fn update_flags_sub(&mut self, res: u32, c: bool, v: bool, sz: Size) {
        self.sr.zero = (res & sz.mask()) == 0;
        self.sr.negative = (res & sz.msb()) != 0;
        self.sr.overflow = v; self.sr.carry = c;
    }

    fn check_condition(&self, cond: u8) -> bool {
        match cond {
            0x0 => true, // T (True)
            0x1 => false, // F (False)
            0x2 => !self.sr.carry && !self.sr.zero, // HI
            0x3 => self.sr.carry || self.sr.zero, // LS
            0x4 => !self.sr.carry, // CC
            0x5 => self.sr.carry, // CS
            0x6 => !self.sr.zero, // NE
            0x7 => self.sr.zero, // EQ
            0x8 => !self.sr.overflow, // VC
            0x9 => self.sr.overflow, // VS
            0xA => !self.sr.negative, // PL
            0xB => self.sr.negative, // MI
            0xC => self.sr.negative == self.sr.overflow, // GE
            0xD => self.sr.negative != self.sr.overflow, // LT
            0xE => !self.sr.zero && (self.sr.negative == self.sr.overflow), // GT
            0xF => self.sr.zero || (self.sr.negative != self.sr.overflow), // LE
            _ => false,
        }
    }
}