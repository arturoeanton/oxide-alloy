// crates/oxid68k/src/lib.rs - Motorola 68000 Complete Implementation
use oxide_core::{Cpu, MemoryBus};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Size {
    Byte = 1,
    Word = 2,
    Long = 4,
}
impl Size {
    #[inline]
    pub fn bytes(&self) -> u32 {
        *self as u32
    }
    #[inline]
    pub fn bits(&self) -> u32 {
        self.bytes() * 8
    }
    #[inline]
    pub fn mask(&self) -> u32 {
        match self {
            Size::Byte => 0xFF,
            Size::Word => 0xFFFF,
            Size::Long => 0xFFFFFFFF,
        }
    }
    #[inline]
    pub fn msb(&self) -> u32 {
        match self {
            Size::Byte => 0x80,
            Size::Word => 0x8000,
            Size::Long => 0x80000000,
        }
    }
    pub fn from_bits(b: u16) -> Option<Self> {
        match b & 3 {
            0 => Some(Size::Byte),
            1 => Some(Size::Word),
            2 => Some(Size::Long),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StatusRegister {
    pub trace: bool,
    pub supervisor: bool,
    pub int_mask: u8,
    pub extend: bool,
    pub negative: bool,
    pub zero: bool,
    pub overflow: bool,
    pub carry: bool,
}
impl StatusRegister {
    pub fn new() -> Self {
        Self {
            supervisor: true,
            int_mask: 7,
            ..Default::default()
        }
    }
    pub fn to_u16(&self) -> u16 {
        (if self.carry { 1 } else { 0 })
            | (if self.overflow { 2 } else { 0 })
            | (if self.zero { 4 } else { 0 })
            | (if self.negative { 8 } else { 0 })
            | (if self.extend { 16 } else { 0 })
            | ((self.int_mask as u16 & 7) << 8)
            | (if self.supervisor { 0x2000 } else { 0 })
            | (if self.trace { 0x8000 } else { 0 })
    }
    pub fn from_u16(&mut self, v: u16) {
        self.carry = v & 1 != 0;
        self.overflow = v & 2 != 0;
        self.zero = v & 4 != 0;
        self.negative = v & 8 != 0;
        self.extend = v & 16 != 0;
        self.int_mask = ((v >> 8) & 7) as u8;
        self.supervisor = v & 0x2000 != 0;
        self.trace = v & 0x8000 != 0;
    }
    #[inline]
    pub fn set_nz(&mut self, v: u32, s: Size) {
        self.zero = (v & s.mask()) == 0;
        self.negative = (v & s.msb()) != 0;
    }
    #[inline]
    pub fn set_logic(&mut self, v: u32, s: Size) {
        self.set_nz(v, s);
        self.overflow = false;
        self.carry = false;
    }
}

pub struct Oxid68k {
    pub d: [u32; 8],
    pub a: [u32; 8],
    pub pc: u32,
    pub sr: StatusRegister,
    pub usp: u32,
    pub ssp: u32,
    pub halted: bool,
    pub stopped: bool,
    pub cycles: u32,
    pub pending_int: Option<u8>,
}

impl Oxid68k {
    pub fn new() -> Self {
        Self {
            d: [0; 8],
            a: [0; 8],
            pc: 0,
            sr: StatusRegister::new(),
            usp: 0,
            ssp: 0,
            halted: false,
            stopped: false,
            cycles: 0,
            pending_int: None,
        }
    }
    #[inline]
    fn fetch(&mut self, bus: &dyn MemoryBus) -> u16 {
        let v = bus.read_u16(self.pc);
        self.pc = self.pc.wrapping_add(2);
        v
    }
    #[inline]
    fn fetch_long(&mut self, bus: &dyn MemoryBus) -> u32 {
        let h = self.fetch(bus) as u32;
        let l = self.fetch(bus) as u32;
        (h << 16) | l
    }
    fn set_sr(&mut self, v: u16) {
        let old = self.sr.supervisor;
        self.sr.from_u16(v);
        if old != self.sr.supervisor {
            if self.sr.supervisor {
                self.usp = self.a[7];
                self.a[7] = self.ssp;
            } else {
                self.ssp = self.a[7];
                self.a[7] = self.usp;
            }
        }
    }
    fn set_ccr(&mut self, v: u8) {
        self.sr.carry = v & 1 != 0;
        self.sr.overflow = v & 2 != 0;
        self.sr.zero = v & 4 != 0;
        self.sr.negative = v & 8 != 0;
        self.sr.extend = v & 16 != 0;
    }
    fn exception(&mut self, vec: u8, bus: &mut dyn MemoryBus) {
        let old_sr = self.sr.to_u16();
        if !self.sr.supervisor {
            self.usp = self.a[7];
            self.a[7] = self.ssp;
            self.sr.supervisor = true;
        }
        self.sr.trace = false;
        self.a[7] = self.a[7].wrapping_sub(4);
        self.write_long(bus, self.a[7], self.pc);
        self.a[7] = self.a[7].wrapping_sub(2);
        self.write_word(bus, self.a[7], old_sr);
        self.pc = self.read_long(bus, (vec as u32) * 4);
        self.cycles += 34;
    }

    fn exception_bus_error(&mut self, bus: &mut dyn MemoryBus, fault_addr: u32, ir: u16) {
        println!(
            "[Oxid68k] Bus Error at PC={:08X} Access={:08X} IR={:04X}",
            self.pc, fault_addr, ir
        );
        let old_sr = self.sr.to_u16();
        if !self.sr.supervisor {
            self.usp = self.a[7];
            self.a[7] = self.ssp;
            self.sr.supervisor = true;
        }
        self.sr.trace = false;

        // Group 0 Exception (14 bytes)
        // PC (4)
        self.a[7] = self.a[7].wrapping_sub(4);
        self.write_long(bus, self.a[7], self.pc);
        // SR (2)
        self.a[7] = self.a[7].wrapping_sub(2);
        self.write_word(bus, self.a[7], old_sr);
        // IR (2)
        self.a[7] = self.a[7].wrapping_sub(2);
        self.write_word(bus, self.a[7], ir);
        // Access Address (4)
        self.a[7] = self.a[7].wrapping_sub(4);
        self.write_long(bus, self.a[7], fault_addr);
        // Function Code (2) - Placeholder 0x5
        self.a[7] = self.a[7].wrapping_sub(2);
        self.write_word(bus, self.a[7], 0x0005);

        self.pc = self.read_long(bus, 8); // Vector 2 (Address 8)
        self.cycles += 50;
    }
    pub fn trigger_interrupt(&mut self, lv: u8) {
        if lv > self.sr.int_mask {
            self.pending_int = Some(lv);
            self.stopped = false;
        }
    }
    fn process_int(&mut self, bus: &mut dyn MemoryBus) {
        if let Some(lv) = self.pending_int.take() {
            if lv > self.sr.int_mask {
                let old_sr = self.sr.to_u16();
                if !self.sr.supervisor {
                    self.usp = self.a[7];
                    self.a[7] = self.ssp;
                    self.sr.supervisor = true;
                }
                self.sr.trace = false;
                self.sr.int_mask = lv;
                self.a[7] = self.a[7].wrapping_sub(4);
                self.write_long(bus, self.a[7], self.pc);
                self.a[7] = self.a[7].wrapping_sub(2);
                self.write_word(bus, self.a[7], old_sr);
                self.pc = self.read_long(bus, ((24 + lv) as u32) * 4);
                self.cycles += 44;
            }
        }
    }
    #[inline]
    fn read_byte(&self, bus: &dyn MemoryBus, a: u32) -> u8 {
        bus.read(a)
    }
    #[inline]
    fn read_word(&self, bus: &dyn MemoryBus, a: u32) -> u16 {
        bus.read_u16(a)
    }
    #[inline]
    fn read_long(&self, bus: &dyn MemoryBus, a: u32) -> u32 {
        ((bus.read_u16(a) as u32) << 16) | bus.read_u16(a.wrapping_add(2)) as u32
    }
    #[inline]
    fn write_byte(&self, bus: &mut dyn MemoryBus, a: u32, v: u8) {
        bus.write(a, v);
    }
    #[inline]
    fn write_word(&self, bus: &mut dyn MemoryBus, a: u32, v: u16) {
        bus.write(a, (v >> 8) as u8);
        bus.write(a.wrapping_add(1), v as u8);
    }
    #[inline]
    fn write_long(&self, bus: &mut dyn MemoryBus, a: u32, v: u32) {
        bus.write(a, (v >> 24) as u8);
        bus.write(a.wrapping_add(1), (v >> 16) as u8);
        bus.write(a.wrapping_add(2), (v >> 8) as u8);
        bus.write(a.wrapping_add(3), v as u8);
    }
    fn read_sz(&self, bus: &dyn MemoryBus, a: u32, s: Size) -> u32 {
        match s {
            Size::Byte => self.read_byte(bus, a) as u32,
            Size::Word => self.read_word(bus, a) as u32,
            Size::Long => self.read_long(bus, a),
        }
    }
    fn write_sz(&self, bus: &mut dyn MemoryBus, a: u32, v: u32, s: Size) {
        match s {
            Size::Byte => self.write_byte(bus, a, v as u8),
            Size::Word => self.write_word(bus, a, v as u16),
            Size::Long => self.write_long(bus, a, v),
        }
    }
    #[inline]
    fn set_d(&mut self, r: usize, v: u32, s: Size) {
        match s {
            Size::Byte => self.d[r] = (self.d[r] & 0xFFFFFF00) | (v & 0xFF),
            Size::Word => self.d[r] = (self.d[r] & 0xFFFF0000) | (v & 0xFFFF),
            Size::Long => self.d[r] = v,
        }
    }
}

impl Oxid68k {
    fn read_ea(&mut self, bus: &dyn MemoryBus, m: u8, r: u8, s: Size) -> u32 {
        match m {
            0 => self.d[r as usize] & s.mask(),
            1 => self.a[r as usize],
            2 => self.read_sz(bus, self.a[r as usize], s),
            3 => {
                let a = self.a[r as usize];
                let inc = if r == 7 && s == Size::Byte {
                    2
                } else {
                    s.bytes()
                };
                self.a[r as usize] = a.wrapping_add(inc);
                self.read_sz(bus, a, s)
            }
            4 => {
                let dec = if r == 7 && s == Size::Byte {
                    2
                } else {
                    s.bytes()
                };
                self.a[r as usize] = self.a[r as usize].wrapping_sub(dec);
                self.read_sz(bus, self.a[r as usize], s)
            }
            5 => {
                let d = self.fetch(bus) as i16 as i32;
                self.read_sz(bus, (self.a[r as usize] as i32).wrapping_add(d) as u32, s)
            }
            6 => {
                let a = self.calc_idx(bus, self.a[r as usize]);
                self.read_sz(bus, a, s)
            }
            7 => match r {
                0 => {
                    let a = self.fetch(bus) as i16 as i32 as u32;
                    self.read_sz(bus, a, s)
                }
                1 => {
                    let a = self.fetch_long(bus);
                    self.read_sz(bus, a, s)
                }
                2 => {
                    let b = self.pc;
                    let d = self.fetch(bus) as i16 as i32;
                    self.read_sz(bus, (b as i32).wrapping_add(d) as u32, s)
                }
                3 => {
                    let b = self.pc;
                    let a = self.calc_idx(bus, b);
                    self.read_sz(bus, a, s)
                }
                4 => match s {
                    Size::Byte => (self.fetch(bus) & 0xFF) as u32,
                    Size::Word => self.fetch(bus) as u32,
                    Size::Long => self.fetch_long(bus),
                },
                _ => 0,
            },
            _ => 0,
        }
    }
    fn write_ea(&mut self, bus: &mut dyn MemoryBus, m: u8, r: u8, s: Size, v: u32) {
        match m {
            0 => self.set_d(r as usize, v, s),
            1 => {
                self.a[r as usize] = if s == Size::Word {
                    (v as i16) as i32 as u32
                } else {
                    v
                }
            }
            2 => self.write_sz(bus, self.a[r as usize], v, s),
            3 => {
                let a = self.a[r as usize];
                self.write_sz(bus, a, v, s);
                let inc = if r == 7 && s == Size::Byte {
                    2
                } else {
                    s.bytes()
                };
                self.a[r as usize] = a.wrapping_add(inc);
            }
            4 => {
                let dec = if r == 7 && s == Size::Byte {
                    2
                } else {
                    s.bytes()
                };
                self.a[r as usize] = self.a[r as usize].wrapping_sub(dec);
                self.write_sz(bus, self.a[r as usize], v, s);
            }
            5 => {
                let d = self.fetch(bus) as i16 as i32;
                self.write_sz(
                    bus,
                    (self.a[r as usize] as i32).wrapping_add(d) as u32,
                    v,
                    s,
                );
            }
            6 => {
                let a = self.calc_idx(bus, self.a[r as usize]);
                self.write_sz(bus, a, v, s);
            }
            7 => match r {
                0 => {
                    let a = self.fetch(bus) as i16 as i32 as u32;
                    self.write_sz(bus, a, v, s);
                }
                1 => {
                    let a = self.fetch_long(bus);
                    self.write_sz(bus, a, v, s);
                }
                _ => {}
            },
            _ => {}
        }
    }
    fn calc_ea(&mut self, bus: &dyn MemoryBus, m: u8, r: u8) -> u32 {
        match m {
            2 | 3 | 4 => self.a[r as usize],
            5 => {
                let d = self.fetch(bus) as i16 as i32;
                (self.a[r as usize] as i32).wrapping_add(d) as u32
            }
            6 => self.calc_idx(bus, self.a[r as usize]),
            7 => match r {
                0 => self.fetch(bus) as i16 as i32 as u32,
                1 => self.fetch_long(bus),
                2 => {
                    let b = self.pc;
                    let d = self.fetch(bus) as i16 as i32;
                    (b as i32).wrapping_add(d) as u32
                }
                3 => {
                    let b = self.pc;
                    self.calc_idx(bus, b)
                }
                _ => 0,
            },
            _ => 0,
        }
    }
    fn calc_idx(&mut self, bus: &dyn MemoryBus, base: u32) -> u32 {
        let ext = self.fetch(bus);
        let ir = ((ext >> 12) & 7) as usize;
        let ia = ext & 0x8000 != 0;
        let il = ext & 0x0800 != 0;
        let disp = (ext & 0xFF) as i8 as i32;
        let idx = if ia { self.a[ir] } else { self.d[ir] };
        let idx = if il { idx as i32 } else { (idx as i16) as i32 };
        (base as i32).wrapping_add(idx).wrapping_add(disp) as u32
    }
    fn add_flags(&mut self, d: u32, s: u32, sz: Size) -> u32 {
        let m = sz.mask();
        let msb = sz.msb();
        let r = (d & m).wrapping_add(s & m) & m;
        self.sr.carry = r < (d & m);
        self.sr.overflow = (!(d ^ s) & (d ^ r) & msb) != 0;
        self.sr.zero = r == 0;
        self.sr.negative = (r & msb) != 0;
        r
    }
    fn sub_flags(&mut self, d: u32, s: u32, sz: Size) -> u32 {
        let m = sz.mask();
        let msb = sz.msb();
        let r = (d & m).wrapping_sub(s & m) & m;
        self.sr.carry = (s & m) > (d & m);
        self.sr.overflow = ((d ^ s) & (d ^ r) & msb) != 0;
        self.sr.zero = r == 0;
        self.sr.negative = (r & msb) != 0;
        r
    }
    fn test_cc(&self, c: u8) -> bool {
        match c {
            0 => true,
            1 => false,
            2 => !self.sr.carry && !self.sr.zero,
            3 => self.sr.carry || self.sr.zero,
            4 => !self.sr.carry,
            5 => self.sr.carry,
            6 => !self.sr.zero,
            7 => self.sr.zero,
            8 => !self.sr.overflow,
            9 => self.sr.overflow,
            10 => !self.sr.negative,
            11 => self.sr.negative,
            12 => self.sr.negative == self.sr.overflow,
            13 => self.sr.negative != self.sr.overflow,
            14 => !self.sr.zero && self.sr.negative == self.sr.overflow,
            15 => self.sr.zero || self.sr.negative != self.sr.overflow,
            _ => false,
        }
    }
    fn abcd(&mut self, d: u8, s: u8, x: u8) -> u8 {
        let mut lo = (d & 0xF) + (s & 0xF) + x;
        let mut hi = (d >> 4) + (s >> 4);
        if lo > 9 {
            lo -= 10;
            hi += 1;
        }
        let c = hi > 9;
        if c {
            hi -= 10;
        }
        self.sr.carry = c;
        self.sr.extend = c;
        let r = ((hi & 0xF) << 4) | (lo & 0xF);
        if r != 0 {
            self.sr.zero = false;
        }
        r
    }
    fn sbcd(&mut self, d: u8, s: u8, x: u8) -> u8 {
        let mut lo = (d & 0xF) as i16 - (s & 0xF) as i16 - x as i16;
        let mut hi = (d >> 4) as i16 - (s >> 4) as i16;
        if lo < 0 {
            lo += 10;
            hi -= 1;
        }
        let c = hi < 0;
        if c {
            hi += 10;
        }
        self.sr.carry = c;
        self.sr.extend = c;
        let r = ((hi as u8 & 0xF) << 4) | (lo as u8 & 0xF);
        if r != 0 {
            self.sr.zero = false;
        }
        r
    }
}

impl Cpu for Oxid68k {
    fn reset(&mut self) {
        *self = Self::new();
    }
    fn reset_with_bus(&mut self, bus: &mut dyn MemoryBus) {
        let raw = self.read_long(bus, 0);
        self.ssp = if raw > 0x100000 { 0x80000 } else { raw };
        self.a[7] = self.ssp;
        self.pc = self.read_long(bus, 4);
        self.sr = StatusRegister::new();
        self.halted = false;
        self.stopped = false;
        println!(
            "[Oxid68k] Reset: SSP={:08X} (raw={:08X}) PC={:08X}",
            self.a[7], raw, self.pc
        );
    }
    fn pc(&self) -> u32 {
        self.pc
    }
    fn step(&mut self, bus: &mut dyn MemoryBus) -> u32 {
        if self.halted {
            return 0;
        }
        if self.pending_int.is_some() {
            self.process_int(bus);
        }
        if self.stopped {
            return 4;
        }
        let op = self.fetch(bus);
        self.cycles = 4;
        self.exec(op, bus);

        if let Some(fault_addr) = bus.bus_error() {
            bus.ack_bus_error();
            self.exception_bus_error(bus, fault_addr, op);
        }

        self.cycles
    }
}

impl Oxid68k {
    fn exec(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        match (op >> 12) & 0xF {
            0x0 => self.g0(op, bus),
            0x1 => self.mov(op, bus, Size::Byte),
            0x2 => self.mov(op, bus, Size::Long),
            0x3 => self.mov(op, bus, Size::Word),
            0x4 => self.g4(op, bus),
            0x5 => self.g5(op, bus),
            0x6 => self.g6(op, bus),
            0x7 => self.moveq(op),
            0x8 => self.g8(op, bus),
            0x9 => self.g9(op, bus),
            0xA => self.exception(10, bus),
            0xB => self.gb(op, bus),
            0xC => self.gc(op, bus),
            0xD => self.gd(op, bus),
            0xE => self.ge(op, bus),
            0xF => self.exception(11, bus),
            _ => {}
        }
    }
    fn g0(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        match op {
            0x003C => {
                let v = self.fetch(bus) as u8;
                self.set_ccr((self.sr.to_u16() as u8) | v);
                self.cycles = 20;
                return;
            }
            0x007C => {
                if !self.sr.supervisor {
                    self.exception(8, bus);
                    return;
                }
                let v = self.fetch(bus);
                self.set_sr(self.sr.to_u16() | v);
                self.cycles = 20;
                return;
            }
            0x023C => {
                let v = self.fetch(bus) as u8;
                self.set_ccr((self.sr.to_u16() as u8) & v);
                self.cycles = 20;
                return;
            }
            0x027C => {
                if !self.sr.supervisor {
                    self.exception(8, bus);
                    return;
                }
                let v = self.fetch(bus);
                self.set_sr(self.sr.to_u16() & v);
                self.cycles = 20;
                return;
            }
            0x0A3C => {
                let v = self.fetch(bus) as u8;
                self.set_ccr((self.sr.to_u16() as u8) ^ v);
                self.cycles = 20;
                return;
            }
            0x0A7C => {
                if !self.sr.supervisor {
                    self.exception(8, bus);
                    return;
                }
                let v = self.fetch(bus);
                self.set_sr(self.sr.to_u16() ^ v);
                self.cycles = 20;
                return;
            }
            _ => {}
        }
        if (op & 0x0138) == 0x0108 {
            self.movep(op, bus);
            return;
        }
        // Bit operations with register: 0000 rrr1 xxmm mrrr where xx != 11 for memory (BTST) or any for register
        // Actually: bit 8 must be 1, and if mode=0 (register) size bits can be anything,
        // but for memory operations size=11 is still valid (BSET)
        if (op & 0x0100) != 0 {
            let size_bits = (op >> 6) & 3;
            let mode = (op >> 3) & 7;
            // For memory (mode != 0), all size values 00-11 are valid bit operations
            // For register (mode == 0), we need to check it's not MOVEP (already handled above)
            if mode == 0 || size_bits <= 3 {
                self.bitd(op, bus);
                return;
            }
        }
        if (op & 0x0F00) == 0x0800 {
            self.bits(op, bus);
            return;
        }
        let sz = match Size::from_bits((op >> 6) & 3) {
            Some(s) => s,
            None => {
                self.exception(4, bus);
                return;
            }
        };
        match (op >> 9) & 7 {
            0 => {
                let i = self.imm(bus, sz);
                let d = self.read_ea(bus, m, r, sz);
                let res = d | i;
                self.sr.set_logic(res, sz);
                self.write_ea(bus, m, r, sz, res);
                self.cycles = 8;
            }
            1 => {
                let i = self.imm(bus, sz);
                let d = self.read_ea(bus, m, r, sz);
                let res = d & i;
                self.sr.set_logic(res, sz);
                self.write_ea(bus, m, r, sz, res);
                self.cycles = 8;
            }
            2 => {
                let i = self.imm(bus, sz);
                let d = self.read_ea(bus, m, r, sz);
                let res = self.sub_flags(d, i, sz);
                self.sr.extend = self.sr.carry;
                self.write_ea(bus, m, r, sz, res);
                self.cycles = 8;
            }
            3 => {
                let i = self.imm(bus, sz);
                let d = self.read_ea(bus, m, r, sz);
                let res = self.add_flags(d, i, sz);
                self.sr.extend = self.sr.carry;
                self.write_ea(bus, m, r, sz, res);
                self.cycles = 8;
            }
            5 => {
                let i = self.imm(bus, sz);
                let d = self.read_ea(bus, m, r, sz);
                let res = d ^ i;
                self.sr.set_logic(res, sz);
                self.write_ea(bus, m, r, sz, res);
                self.cycles = 8;
            }
            6 => {
                let i = self.imm(bus, sz);
                let d = self.read_ea(bus, m, r, sz);
                self.sub_flags(d, i, sz);
                self.cycles = 8;
            }
            _ => self.exception(4, bus),
        }
    }
    fn imm(&mut self, bus: &dyn MemoryBus, s: Size) -> u32 {
        match s {
            Size::Byte => (self.fetch(bus) & 0xFF) as u32,
            Size::Word => self.fetch(bus) as u32,
            Size::Long => self.fetch_long(bus),
        }
    }
    fn bitd(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let br = ((op >> 9) & 7) as usize;
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        let bit = self.d[br];
        let ir = m == 0;
        let msk = if ir { 31 } else { 7 };
        let bn = bit & msk;
        let mk = 1u32 << bn;

        if ir {
            // Register direct - operate on Dn
            let v = self.d[r as usize];
            self.sr.zero = (v & mk) == 0;
            match (op >> 6) & 3 {
                0 => {
                    self.cycles = 6;
                } // BTST
                1 => {
                    self.d[r as usize] = v ^ mk;
                    self.cycles = 8;
                } // BCHG
                2 => {
                    self.d[r as usize] = v & !mk;
                    self.cycles = 10;
                } // BCLR
                3 => {
                    self.d[r as usize] = v | mk;
                    self.cycles = 8;
                } // BSET
                _ => {}
            }
        } else {
            // Memory - calculate address ONCE, then read-modify-write
            let addr = self.calc_ea(bus, m, r);
            let v = self.read_byte(bus, addr) as u32;
            self.sr.zero = (v & mk) == 0;
            match (op >> 6) & 3 {
                0 => {
                    self.cycles = 8;
                } // BTST - no write
                1 => {
                    self.write_byte(bus, addr, (v ^ mk) as u8);
                    self.cycles = 12;
                } // BCHG
                2 => {
                    self.write_byte(bus, addr, (v & !mk) as u8);
                    self.cycles = 12;
                } // BCLR
                3 => {
                    self.write_byte(bus, addr, (v | mk) as u8);
                    self.cycles = 12;
                } // BSET
                _ => {}
            }
        }
    }
    fn bits(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        let bit = (self.fetch(bus) & 0xFF) as u32;
        let ir = m == 0;
        let msk = if ir { 31 } else { 7 };
        let bn = bit & msk;
        let mk = 1u32 << bn;

        if ir {
            let v = self.d[r as usize];
            self.sr.zero = (v & mk) == 0;
            match (op >> 6) & 3 {
                0 => {
                    self.cycles = 10;
                }
                1 => {
                    self.d[r as usize] = v ^ mk;
                    self.cycles = 12;
                }
                2 => {
                    self.d[r as usize] = v & !mk;
                    self.cycles = 14;
                }
                3 => {
                    self.d[r as usize] = v | mk;
                    self.cycles = 12;
                }
                _ => {}
            }
        } else {
            let addr = self.calc_ea(bus, m, r);
            let v = self.read_byte(bus, addr) as u32;
            self.sr.zero = (v & mk) == 0;
            match (op >> 6) & 3 {
                0 => {
                    self.cycles = 12;
                }
                1 => {
                    self.write_byte(bus, addr, (v ^ mk) as u8);
                    self.cycles = 16;
                }
                2 => {
                    self.write_byte(bus, addr, (v & !mk) as u8);
                    self.cycles = 16;
                }
                3 => {
                    self.write_byte(bus, addr, (v | mk) as u8);
                    self.cycles = 16;
                }
                _ => {}
            }
        }
    }
    fn movep(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let dr = ((op >> 9) & 7) as usize;
        let ar = (op & 7) as usize;
        let disp = self.fetch(bus) as i16 as i32;
        let a = (self.a[ar] as i32).wrapping_add(disp) as u32;
        match (op >> 6) & 7 {
            4 => {
                let h = self.read_byte(bus, a) as u32;
                let l = self.read_byte(bus, a.wrapping_add(2)) as u32;
                self.d[dr] = (self.d[dr] & 0xFFFF0000) | (h << 8) | l;
                self.cycles = 16;
            }
            5 => {
                let b0 = self.read_byte(bus, a) as u32;
                let b1 = self.read_byte(bus, a.wrapping_add(2)) as u32;
                let b2 = self.read_byte(bus, a.wrapping_add(4)) as u32;
                let b3 = self.read_byte(bus, a.wrapping_add(6)) as u32;
                self.d[dr] = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;
                self.cycles = 24;
            }
            6 => {
                let v = self.d[dr];
                self.write_byte(bus, a, (v >> 8) as u8);
                self.write_byte(bus, a.wrapping_add(2), v as u8);
                self.cycles = 16;
            }
            7 => {
                let v = self.d[dr];
                self.write_byte(bus, a, (v >> 24) as u8);
                self.write_byte(bus, a.wrapping_add(2), (v >> 16) as u8);
                self.write_byte(bus, a.wrapping_add(4), (v >> 8) as u8);
                self.write_byte(bus, a.wrapping_add(6), v as u8);
                self.cycles = 24;
            }
            _ => {}
        }
    }
    fn mov(&mut self, op: u16, bus: &mut dyn MemoryBus, sz: Size) {
        let sm = ((op >> 3) & 7) as u8;
        let sr = (op & 7) as u8;
        let dr = ((op >> 9) & 7) as u8;
        let dm = ((op >> 6) & 7) as u8;
        let v = self.read_ea(bus, sm, sr, sz);
        if dm != 1 {
            self.sr.set_logic(v, sz);
        }
        self.write_ea(bus, dm, dr, sz, v);
        self.cycles = 4;
    }
    fn moveq(&mut self, op: u16) {
        let r = ((op >> 9) & 7) as usize;
        let v = (op & 0xFF) as i8 as i32 as u32;
        self.d[r] = v;
        self.sr.set_logic(v, Size::Long);
        self.cycles = 4;
    }
    fn g4(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        match op {
            0x4E70 => {
                self.cycles = 132;
                return;
            }
            0x4E71 => {
                self.cycles = 4;
                return;
            }
            0x4E72 => {
                if !self.sr.supervisor {
                    self.exception(8, bus);
                    return;
                }
                let v = self.fetch(bus);
                self.set_sr(v);
                self.stopped = true;
                self.cycles = 4;
                return;
            }
            0x4E73 => {
                if !self.sr.supervisor {
                    self.exception(8, bus);
                    return;
                }
                let sr = self.read_word(bus, self.a[7]);
                self.a[7] = self.a[7].wrapping_add(2);
                let pc = self.read_long(bus, self.a[7]);
                self.a[7] = self.a[7].wrapping_add(4);
                self.set_sr(sr);
                self.pc = pc;
                self.cycles = 20;
                return;
            }
            0x4E75 => {
                self.pc = self.read_long(bus, self.a[7]);
                self.a[7] = self.a[7].wrapping_add(4);
                self.cycles = 16;
                return;
            }
            0x4E76 => {
                if self.sr.overflow {
                    self.exception(7, bus);
                }
                self.cycles = 4;
                return;
            }
            0x4E77 => {
                let c = self.read_word(bus, self.a[7]) as u8;
                self.a[7] = self.a[7].wrapping_add(2);
                self.set_ccr(c);
                self.pc = self.read_long(bus, self.a[7]);
                self.a[7] = self.a[7].wrapping_add(4);
                self.cycles = 20;
                return;
            }
            _ => {}
        }
        if (op & 0xFFF0) == 0x4E60 {
            if !self.sr.supervisor {
                self.exception(8, bus);
                return;
            }
            let rg = (op & 7) as usize;
            if op & 8 != 0 {
                self.a[rg] = self.usp;
            } else {
                self.usp = self.a[rg];
            }
            self.cycles = 4;
            return;
        }
        if (op & 0xFFF0) == 0x4E40 {
            self.exception(32 + (op & 0xF) as u8, bus);
            return;
        }
        if (op & 0xFFF8) == 0x4E50 {
            let rg = (op & 7) as usize;
            let d = self.fetch(bus) as i16 as i32;
            self.a[7] = self.a[7].wrapping_sub(4);
            self.write_long(bus, self.a[7], self.a[rg]);
            self.a[rg] = self.a[7];
            self.a[7] = (self.a[7] as i32).wrapping_add(d) as u32;
            self.cycles = 16;
            return;
        }
        if (op & 0xFFF8) == 0x4E58 {
            let rg = (op & 7) as usize;
            self.a[7] = self.a[rg];
            self.a[rg] = self.read_long(bus, self.a[7]);
            self.a[7] = self.a[7].wrapping_add(4);
            self.cycles = 12;
            return;
        }
        if (op & 0xFB80) == 0x4880 {
            self.movem(op, bus);
            return;
        }
        if (op & 0xFFF8) == 0x4880 {
            let rg = (op & 7) as usize;
            let v = (self.d[rg] as i8) as i16 as u16;
            self.d[rg] = (self.d[rg] & 0xFFFF0000) | v as u32;
            self.sr.set_logic(v as u32, Size::Word);
            self.cycles = 4;
            return;
        }
        if (op & 0xFFF8) == 0x48C0 {
            let rg = (op & 7) as usize;
            let v = (self.d[rg] as i16) as i32 as u32;
            self.d[rg] = v;
            self.sr.set_logic(v, Size::Long);
            self.cycles = 4;
            return;
        }
        if (op & 0xFFF8) == 0x4840 && m == 0 {
            let rg = (op & 7) as usize;
            let v = self.d[rg];
            self.d[rg] = (v >> 16) | (v << 16);
            self.sr.set_logic(self.d[rg], Size::Long);
            self.cycles = 4;
            return;
        }
        if (op & 0xFFC0) == 0x4840 && m != 0 {
            let a = self.calc_ea(bus, m, r);
            self.a[7] = self.a[7].wrapping_sub(4);
            self.write_long(bus, self.a[7], a);
            self.cycles = 12;
            return;
        }
        if (op & 0xF1C0) == 0x41C0 {
            let ar = ((op >> 9) & 7) as usize;
            let a = self.calc_ea(bus, m, r);
            self.a[ar] = a;
            self.cycles = 4;
            return;
        }
        if (op & 0xF1C0) == 0x4180 {
            let dr = ((op >> 9) & 7) as usize;
            let bnd = self.read_ea(bus, m, r, Size::Word) as i16;
            let v = self.d[dr] as i16;
            if v < 0 {
                self.sr.negative = true;
                self.exception(6, bus);
            } else if v > bnd {
                self.sr.negative = false;
                self.exception(6, bus);
            }
            self.cycles = 10;
            return;
        }
        let sz = match (op >> 6) & 3 {
            0 => Size::Byte,
            1 => Size::Word,
            2 => Size::Long,
            _ => match (op >> 8) & 0xF {
                0x0 => {
                    let v = self.sr.to_u16();
                    self.write_ea(bus, m, r, Size::Word, v as u32);
                    self.cycles = 8;
                    return;
                }
                0x4 => {
                    let v = self.read_ea(bus, m, r, Size::Word);
                    self.set_ccr(v as u8);
                    self.cycles = 12;
                    return;
                }
                0x6 => {
                    if !self.sr.supervisor {
                        self.exception(8, bus);
                        return;
                    }
                    let v = self.read_ea(bus, m, r, Size::Word) as u16;
                    self.set_sr(v);
                    self.cycles = 12;
                    return;
                }
                0xA => {
                    let v = self.read_ea(bus, m, r, Size::Byte);
                    self.sr.set_logic(v, Size::Byte);
                    self.write_ea(bus, m, r, Size::Byte, v | 0x80);
                    self.cycles = 4;
                    return;
                }
                0xE => {
                    if m >= 2 {
                        self.pc = self.calc_ea(bus, m, r);
                        self.cycles = 8;
                    } else {
                        let t = self.calc_ea(bus, m, r);
                        self.a[7] = self.a[7].wrapping_sub(4);
                        self.write_long(bus, self.a[7], self.pc);
                        self.pc = t;
                        self.cycles = 16;
                    }
                    return;
                }
                _ => {
                    self.exception(4, bus);
                    return;
                }
            },
        };
        match (op >> 8) & 0xF {
            0x0 => {
                // NEGX - Calculate EA once for read-modify-write
                if m == 0 {
                    let d = self.d[r as usize] & sz.mask();
                    let x = if self.sr.extend { 1 } else { 0 };
                    let res = self.sub_flags(0, d.wrapping_add(x), sz);
                    if (res & sz.mask()) != 0 {
                        self.sr.zero = false;
                    }
                    self.sr.extend = self.sr.carry;
                    self.set_d(r as usize, res, sz);
                } else {
                    let addr = self.calc_ea(bus, m, r);
                    let d = self.read_sz(bus, addr, sz);
                    let x = if self.sr.extend { 1 } else { 0 };
                    let res = self.sub_flags(0, d.wrapping_add(x), sz);
                    if (res & sz.mask()) != 0 {
                        self.sr.zero = false;
                    }
                    self.sr.extend = self.sr.carry;
                    self.write_sz(bus, addr, res, sz);
                }
                self.cycles = 4;
            }
            0x2 => {
                // CLR - Calculate EA once, then do read-modify-write
                if m == 0 {
                    // Data register direct - just clear it
                    self.set_d(r as usize, 0, sz);
                } else {
                    // Memory - calculate address ONCE, then read (dummy) and write
                    let addr = self.calc_ea(bus, m, r);
                    let _ = self.read_sz(bus, addr, sz); // Dummy read (68k behavior)
                    self.write_sz(bus, addr, 0, sz);
                }
                self.sr.zero = true;
                self.sr.negative = false;
                self.sr.overflow = false;
                self.sr.carry = false;
                self.cycles = 4;
            }
            0x4 => {
                // NEG - Calculate EA once for read-modify-write
                if m == 0 {
                    let d = self.d[r as usize] & sz.mask();
                    let res = self.sub_flags(0, d, sz);
                    self.sr.extend = self.sr.carry;
                    self.set_d(r as usize, res, sz);
                } else {
                    let addr = self.calc_ea(bus, m, r);
                    let d = self.read_sz(bus, addr, sz);
                    let res = self.sub_flags(0, d, sz);
                    self.sr.extend = self.sr.carry;
                    self.write_sz(bus, addr, res, sz);
                }
                self.cycles = 4;
            }
            0x6 => {
                // NOT - Calculate EA once for read-modify-write
                if m == 0 {
                    let d = self.d[r as usize] & sz.mask();
                    let res = !d & sz.mask();
                    self.sr.set_logic(res, sz);
                    self.set_d(r as usize, res, sz);
                } else {
                    let addr = self.calc_ea(bus, m, r);
                    let d = self.read_sz(bus, addr, sz);
                    let res = !d & sz.mask();
                    self.sr.set_logic(res, sz);
                    self.write_sz(bus, addr, res, sz);
                }
                self.cycles = 4;
            }
            0x8 => {
                // NBCD - Calculate EA once for read-modify-write
                if m == 0 {
                    let d = self.d[r as usize] as u8;
                    let x = if self.sr.extend { 1 } else { 0 };
                    let res = self.sbcd(0, d, x);
                    self.d[r as usize] = (self.d[r as usize] & 0xFFFFFF00) | res as u32;
                } else {
                    let addr = self.calc_ea(bus, m, r);
                    let d = self.read_byte(bus, addr);
                    let x = if self.sr.extend { 1 } else { 0 };
                    let res = self.sbcd(0, d, x);
                    self.write_byte(bus, addr, res);
                }
                self.cycles = 8;
            }
            0xA => {
                let v = self.read_ea(bus, m, r, sz);
                self.sr.set_logic(v, sz);
                self.cycles = 4;
            }
            _ => self.exception(4, bus),
        }
    }
    fn movem(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let dir = (op & 0x0400) != 0;
        let sz = if op & 0x0040 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        let (base, mask) = if m == 4 {
            let mk = self.fetch(bus);
            (self.a[r as usize], mk)
        } else {
            let a = self.calc_ea(bus, m, r);
            let mk = self.fetch(bus);
            (a, mk)
        };
        if dir {
            let mut a = base;
            for i in 0..16 {
                if mask & (1 << i) != 0 {
                    let v = if sz == Size::Word {
                        (self.read_word(bus, a) as i16) as i32 as u32
                    } else {
                        self.read_long(bus, a)
                    };
                    if i < 8 {
                        self.d[i] = v;
                    } else {
                        self.a[i - 8] = v;
                    }
                    a = a.wrapping_add(sz.bytes());
                }
            }
            if m == 3 {
                self.a[r as usize] = a;
            }
        } else {
            if m == 4 {
                let mut a = base;
                for i in 0..16 {
                    let b = 15 - i;
                    if mask & (1 << b) != 0 {
                        a = a.wrapping_sub(sz.bytes());
                        let v = if i < 8 { self.a[7 - i] } else { self.d[15 - i] };
                        self.write_sz(bus, a, v, sz);
                    }
                }
                self.a[r as usize] = a;
            } else {
                let mut a = base;
                for i in 0..16 {
                    if mask & (1 << i) != 0 {
                        let v = if i < 8 { self.d[i] } else { self.a[i - 8] };
                        self.write_sz(bus, a, v, sz);
                        a = a.wrapping_add(sz.bytes());
                    }
                }
            }
        }
        self.cycles = 8 + (mask.count_ones() * if sz == Size::Long { 8 } else { 4 });
    }
}

impl Oxid68k {
    fn g5(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        if (op >> 6) & 3 == 3 {
            let cc = ((op >> 8) & 0xF) as u8;
            if m == 1 {
                let disp = self.fetch(bus) as i16 as i32;
                if !self.test_cc(cc) {
                    let v = ((self.d[r as usize] as u16).wrapping_sub(1)) as u16;
                    self.d[r as usize] = (self.d[r as usize] & 0xFFFF0000) | v as u32;
                    if v != 0xFFFF {
                        self.pc = (self.pc.wrapping_sub(2) as i32).wrapping_add(disp) as u32;
                        self.cycles = 10;
                    } else {
                        self.cycles = 14;
                    }
                } else {
                    self.cycles = 12;
                }
            } else {
                let v = if self.test_cc(cc) { 0xFF } else { 0x00 };
                self.write_ea(bus, m, r, Size::Byte, v);
                self.cycles = 8;
            }
        } else {
            let sz = Size::from_bits((op >> 6) & 3).unwrap();
            let d = ((op >> 9) & 7) as u32;
            let d = if d == 0 { 8 } else { d };
            if (op & 0x0100) != 0 {
                if m == 1 {
                    self.a[r as usize] = self.a[r as usize].wrapping_sub(d);
                } else {
                    let dst = self.read_ea(bus, m, r, sz);
                    let res = self.sub_flags(dst, d, sz);
                    self.sr.extend = self.sr.carry;
                    self.write_ea(bus, m, r, sz, res);
                }
            } else {
                if m == 1 {
                    self.a[r as usize] = self.a[r as usize].wrapping_add(d);
                } else {
                    let dst = self.read_ea(bus, m, r, sz);
                    let res = self.add_flags(dst, d, sz);
                    self.sr.extend = self.sr.carry;
                    self.write_ea(bus, m, r, sz, res);
                }
            }
            self.cycles = 4;
        }
    }
    fn g6(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let cc = ((op >> 8) & 0xF) as u8;
        let d8 = (op & 0xFF) as i8 as i32;
        let disp = if d8 == 0 {
            self.fetch(bus) as i16 as i32
        } else {
            d8
        };
        let base = if d8 == 0 {
            self.pc.wrapping_sub(2)
        } else {
            self.pc
        };
        match cc {
            0 => {
                self.pc = (base as i32).wrapping_add(disp) as u32;
                self.cycles = 10;
            }
            1 => {
                self.a[7] = self.a[7].wrapping_sub(4);
                self.write_long(bus, self.a[7], self.pc);
                self.pc = (base as i32).wrapping_add(disp) as u32;
                self.cycles = 18;
            }
            _ => {
                if self.test_cc(cc) {
                    self.pc = (base as i32).wrapping_add(disp) as u32;
                    self.cycles = 10;
                } else {
                    self.cycles = if d8 == 0 { 12 } else { 8 };
                }
            }
        }
    }
    fn g8(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let dr = ((op >> 9) & 7) as usize;
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        match (op >> 6) & 7 {
            0 | 1 | 2 => {
                let sz = Size::from_bits((op >> 6) & 3).unwrap();
                let s = self.read_ea(bus, m, r, sz);
                let res = self.d[dr] | s;
                self.set_d(dr, res, sz);
                self.sr.set_logic(res, sz);
                self.cycles = 4;
            }
            3 => {
                let div = self.read_ea(bus, m, r, Size::Word) as u32;
                if div == 0 {
                    self.exception(5, bus);
                    return;
                }
                let dvd = self.d[dr];
                let q = dvd / div;
                let rm = dvd % div;
                self.sr.carry = false;
                if q > 0xFFFF {
                    self.sr.overflow = true;
                } else {
                    self.sr.overflow = false;
                    self.sr.zero = q == 0;
                    self.sr.negative = (q & 0x8000) != 0;
                    self.d[dr] = (rm << 16) | (q & 0xFFFF);
                }
                self.cycles = 140;
            }
            4 => {
                let rx = dr;
                let ry = (op & 7) as usize;
                let rm = op & 8 != 0;
                let x = if self.sr.extend { 1 } else { 0 };
                if rm {
                    self.a[ry] = self.a[ry].wrapping_sub(1);
                    self.a[rx] = self.a[rx].wrapping_sub(1);
                    let s = self.read_byte(bus, self.a[ry]);
                    let d = self.read_byte(bus, self.a[rx]);
                    let res = self.sbcd(d, s, x);
                    self.write_byte(bus, self.a[rx], res);
                    self.cycles = 18;
                } else {
                    let s = self.d[ry] as u8;
                    let d = self.d[rx] as u8;
                    let res = self.sbcd(d, s, x);
                    self.d[rx] = (self.d[rx] & 0xFFFFFF00) | res as u32;
                    self.cycles = 6;
                }
            }
            5 | 6 => {
                let sz = match (op >> 6) & 7 {
                    4 => Size::Byte,
                    5 => Size::Word,
                    6 => Size::Long,
                    _ => Size::Word,
                };
                let s = self.d[dr];
                let d = self.read_ea(bus, m, r, sz);
                let res = s | d;
                self.write_ea(bus, m, r, sz, res);
                self.sr.set_logic(res, sz);
                self.cycles = 8;
            }
            7 => {
                let div = self.read_ea(bus, m, r, Size::Word) as i16 as i32;
                if div == 0 {
                    self.exception(5, bus);
                    return;
                }
                let dvd = self.d[dr] as i32;
                let q = dvd / div;
                let rm = dvd % div;
                self.sr.carry = false;
                if q > 32767 || q < -32768 {
                    self.sr.overflow = true;
                } else {
                    self.sr.overflow = false;
                    self.sr.zero = q == 0;
                    self.sr.negative = q < 0;
                    self.d[dr] = ((rm as u32 & 0xFFFF) << 16) | (q as u32 & 0xFFFF);
                }
                self.cycles = 158;
            }
            _ => {}
        }
    }
    fn g9(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let dr = ((op >> 9) & 7) as usize;
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        match (op >> 6) & 7 {
            0 | 1 | 2 => {
                let sz = Size::from_bits((op >> 6) & 3).unwrap();
                let s = self.read_ea(bus, m, r, sz);
                let d = self.d[dr];
                let res = self.sub_flags(d, s, sz);
                self.sr.extend = self.sr.carry;
                self.set_d(dr, res, sz);
                self.cycles = 4;
            }
            3 => {
                let s = self.read_ea(bus, m, r, Size::Word) as i16 as i32 as u32;
                self.a[dr] = self.a[dr].wrapping_sub(s);
                self.cycles = 8;
            }
            4 | 5 | 6 => {
                if m == 0 || m == 1 {
                    self.subx(op, bus);
                } else {
                    let sz = match (op >> 6) & 7 {
                        4 => Size::Byte,
                        5 => Size::Word,
                        6 => Size::Long,
                        _ => Size::Word,
                    };
                    let s = self.d[dr];
                    let d = self.read_ea(bus, m, r, sz);
                    let res = self.sub_flags(d, s, sz);
                    self.sr.extend = self.sr.carry;
                    self.write_ea(bus, m, r, sz, res);
                    self.cycles = 8;
                }
            }
            7 => {
                let s = self.read_ea(bus, m, r, Size::Long);
                self.a[dr] = self.a[dr].wrapping_sub(s);
                self.cycles = 8;
            }
            _ => {}
        }
    }
    fn subx(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let rx = ((op >> 9) & 7) as usize;
        let ry = (op & 7) as usize;
        let rm = op & 8 != 0;
        let sz = Size::from_bits((op >> 6) & 3).unwrap();
        let x = if self.sr.extend { 1 } else { 0 };
        if rm {
            let dec = sz.bytes();
            self.a[ry] = self.a[ry].wrapping_sub(dec);
            self.a[rx] = self.a[rx].wrapping_sub(dec);
            let s = self.read_sz(bus, self.a[ry], sz);
            let d = self.read_sz(bus, self.a[rx], sz);
            let res = d.wrapping_sub(s).wrapping_sub(x) & sz.mask();
            self.sr.carry = (s + x) > d;
            self.sr.overflow = ((d ^ s) & (d ^ res) & sz.msb()) != 0;
            self.sr.extend = self.sr.carry;
            if res != 0 {
                self.sr.zero = false;
            }
            self.sr.negative = (res & sz.msb()) != 0;
            self.write_sz(bus, self.a[rx], res, sz);
            self.cycles = 18;
        } else {
            let s = self.d[ry] & sz.mask();
            let d = self.d[rx] & sz.mask();
            let res = d.wrapping_sub(s).wrapping_sub(x) & sz.mask();
            self.sr.carry = (s + x) > d;
            self.sr.overflow = ((d ^ s) & (d ^ res) & sz.msb()) != 0;
            self.sr.extend = self.sr.carry;
            if res != 0 {
                self.sr.zero = false;
            }
            self.sr.negative = (res & sz.msb()) != 0;
            self.set_d(rx, res, sz);
            self.cycles = 4;
        }
    }
    fn gb(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let dr = ((op >> 9) & 7) as usize;
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        match (op >> 6) & 7 {
            0 | 1 | 2 => {
                let sz = Size::from_bits((op >> 6) & 3).unwrap();
                let s = self.read_ea(bus, m, r, sz);
                let d = self.d[dr];
                self.sub_flags(d, s, sz);
                self.cycles = 4;
            }
            3 => {
                let s = self.read_ea(bus, m, r, Size::Word) as i16 as i32 as u32;
                let d = self.a[dr];
                self.sub_flags(d, s, Size::Long);
                self.cycles = 6;
            }
            4 | 5 | 6 => {
                if m == 1 {
                    let ax = dr;
                    let ay = r as usize;
                    let sz = Size::from_bits((op >> 6) & 3).unwrap();
                    let s = self.read_sz(bus, self.a[ay], sz);
                    self.a[ay] = self.a[ay].wrapping_add(sz.bytes());
                    let d = self.read_sz(bus, self.a[ax], sz);
                    self.a[ax] = self.a[ax].wrapping_add(sz.bytes());
                    self.sub_flags(d, s, sz);
                    self.cycles = 12;
                } else {
                    let sz = match (op >> 6) & 7 {
                        4 => Size::Byte,
                        5 => Size::Word,
                        6 => Size::Long,
                        _ => Size::Word,
                    };
                    let s = self.d[dr];
                    let d = self.read_ea(bus, m, r, sz);
                    let res = s ^ d;
                    self.sr.set_logic(res, sz);
                    self.write_ea(bus, m, r, sz, res);
                    self.cycles = 8;
                }
            }
            7 => {
                let s = self.read_ea(bus, m, r, Size::Long);
                let d = self.a[dr];
                self.sub_flags(d, s, Size::Long);
                self.cycles = 6;
            }
            _ => {}
        }
    }
    fn gc(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let dr = ((op >> 9) & 7) as usize;
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        match (op >> 6) & 7 {
            0 | 1 | 2 => {
                let sz = Size::from_bits((op >> 6) & 3).unwrap();
                let s = self.read_ea(bus, m, r, sz);
                let res = self.d[dr] & s;
                self.set_d(dr, res, sz);
                self.sr.set_logic(res, sz);
                self.cycles = 4;
            }
            3 => {
                let s = self.read_ea(bus, m, r, Size::Word) as u32;
                let d = self.d[dr] as u16 as u32;
                let res = s * d;
                self.d[dr] = res;
                self.sr.carry = false;
                self.sr.overflow = false;
                self.sr.zero = res == 0;
                self.sr.negative = (res & 0x80000000) != 0;
                self.cycles = 70;
            }
            4 => {
                if m == 0 || m == 1 {
                    let rx = dr;
                    let ry = (op & 7) as usize;
                    let rm = op & 8 != 0;
                    let x = if self.sr.extend { 1 } else { 0 };
                    if rm {
                        self.a[ry] = self.a[ry].wrapping_sub(1);
                        self.a[rx] = self.a[rx].wrapping_sub(1);
                        let s = self.read_byte(bus, self.a[ry]);
                        let d = self.read_byte(bus, self.a[rx]);
                        let res = self.abcd(d, s, x);
                        self.write_byte(bus, self.a[rx], res);
                        self.cycles = 18;
                    } else {
                        let s = self.d[ry] as u8;
                        let d = self.d[rx] as u8;
                        let res = self.abcd(d, s, x);
                        self.d[rx] = (self.d[rx] & 0xFFFFFF00) | res as u32;
                        self.cycles = 6;
                    }
                } else {
                    let s = self.d[dr];
                    let d = self.read_ea(bus, m, r, Size::Byte);
                    let res = s & d;
                    self.write_ea(bus, m, r, Size::Byte, res);
                    self.sr.set_logic(res, Size::Byte);
                    self.cycles = 8;
                }
            }
            5 => {
                if m == 0 {
                    let ry = r as usize;
                    let t = self.d[dr];
                    self.d[dr] = self.d[ry];
                    self.d[ry] = t;
                    self.cycles = 6;
                } else if m == 1 {
                    let ry = r as usize;
                    let t = self.a[dr];
                    self.a[dr] = self.a[ry];
                    self.a[ry] = t;
                    self.cycles = 6;
                } else {
                    let s = self.d[dr];
                    let d = self.read_ea(bus, m, r, Size::Word);
                    let res = s & d;
                    self.write_ea(bus, m, r, Size::Word, res);
                    self.sr.set_logic(res, Size::Word);
                    self.cycles = 8;
                }
            }
            6 => {
                if m == 1 {
                    let ry = r as usize;
                    let t = self.d[dr];
                    self.d[dr] = self.a[ry];
                    self.a[ry] = t;
                    self.cycles = 6;
                } else {
                    let s = self.d[dr];
                    let d = self.read_ea(bus, m, r, Size::Long);
                    let res = s & d;
                    self.write_ea(bus, m, r, Size::Long, res);
                    self.sr.set_logic(res, Size::Long);
                    self.cycles = 12;
                }
            }
            7 => {
                let s = self.read_ea(bus, m, r, Size::Word) as i16 as i32;
                let d = self.d[dr] as i16 as i32;
                let res = (s * d) as u32;
                self.d[dr] = res;
                self.sr.carry = false;
                self.sr.overflow = false;
                self.sr.zero = res == 0;
                self.sr.negative = (res & 0x80000000) != 0;
                self.cycles = 70;
            }
            _ => {}
        }
    }
    fn gd(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let dr = ((op >> 9) & 7) as usize;
        let m = ((op >> 3) & 7) as u8;
        let r = (op & 7) as u8;
        match (op >> 6) & 7 {
            0 | 1 | 2 => {
                let sz = Size::from_bits((op >> 6) & 3).unwrap();
                let s = self.read_ea(bus, m, r, sz);
                let d = self.d[dr];
                let res = self.add_flags(d, s, sz);
                self.sr.extend = self.sr.carry;
                self.set_d(dr, res, sz);
                self.cycles = 4;
            }
            3 => {
                let s = self.read_ea(bus, m, r, Size::Word) as i16 as i32 as u32;
                self.a[dr] = self.a[dr].wrapping_add(s);
                self.cycles = 8;
            }
            4 | 5 | 6 => {
                if m == 0 || m == 1 {
                    self.addx(op, bus);
                } else {
                    let sz = match (op >> 6) & 7 {
                        4 => Size::Byte,
                        5 => Size::Word,
                        6 => Size::Long,
                        _ => Size::Word,
                    };
                    let s = self.d[dr];
                    let d = self.read_ea(bus, m, r, sz);
                    let res = self.add_flags(d, s, sz);
                    self.sr.extend = self.sr.carry;
                    self.write_ea(bus, m, r, sz, res);
                    self.cycles = 8;
                }
            }
            7 => {
                let s = self.read_ea(bus, m, r, Size::Long);
                self.a[dr] = self.a[dr].wrapping_add(s);
                self.cycles = 8;
            }
            _ => {}
        }
    }
    fn addx(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        let rx = ((op >> 9) & 7) as usize;
        let ry = (op & 7) as usize;
        let rm = op & 8 != 0;
        let sz = Size::from_bits((op >> 6) & 3).unwrap();
        let x = if self.sr.extend { 1 } else { 0 };
        if rm {
            let dec = sz.bytes();
            self.a[ry] = self.a[ry].wrapping_sub(dec);
            self.a[rx] = self.a[rx].wrapping_sub(dec);
            let s = self.read_sz(bus, self.a[ry], sz);
            let d = self.read_sz(bus, self.a[rx], sz);
            let res = d.wrapping_add(s).wrapping_add(x) & sz.mask();
            self.sr.carry = res < d || (x == 1 && res == d);
            self.sr.overflow = (!(d ^ s) & (d ^ res) & sz.msb()) != 0;
            self.sr.extend = self.sr.carry;
            if res != 0 {
                self.sr.zero = false;
            }
            self.sr.negative = (res & sz.msb()) != 0;
            self.write_sz(bus, self.a[rx], res, sz);
            self.cycles = 18;
        } else {
            let s = self.d[ry] & sz.mask();
            let d = self.d[rx] & sz.mask();
            let res = d.wrapping_add(s).wrapping_add(x) & sz.mask();
            self.sr.carry = res < d || (x == 1 && res == d);
            self.sr.overflow = (!(d ^ s) & (d ^ res) & sz.msb()) != 0;
            self.sr.extend = self.sr.carry;
            if res != 0 {
                self.sr.zero = false;
            }
            self.sr.negative = (res & sz.msb()) != 0;
            self.set_d(rx, res, sz);
            self.cycles = 4;
        }
    }
    fn ge(&mut self, op: u16, bus: &mut dyn MemoryBus) {
        if (op >> 6) & 3 == 3 {
            let m = ((op >> 3) & 7) as u8;
            let r = (op & 7) as u8;
            let dr = (op & 0x0100) != 0;
            let ty = (op >> 9) & 3;
            let v = self.read_ea(bus, m, r, Size::Word);
            let res = match ty {
                0 => self.asx(v, 1, dr, Size::Word),
                1 => self.lsx(v, 1, dr, Size::Word),
                2 => self.roxx(v, 1, dr, Size::Word),
                3 => self.rox(v, 1, dr, Size::Word),
                _ => v,
            };
            self.write_ea(bus, m, r, Size::Word, res);
            self.cycles = 8;
        } else {
            let sz = Size::from_bits((op >> 6) & 3).unwrap();
            let ir = (op & 0x0020) != 0;
            let dr = (op & 0x0100) != 0;
            let ty = (op >> 3) & 3;
            let rg = (op & 7) as usize;
            let cnt = if ir {
                let cr = ((op >> 9) & 7) as usize;
                (self.d[cr] % 64) as u32
            } else {
                let c = (op >> 9) & 7;
                if c == 0 {
                    8
                } else {
                    c as u32
                }
            };
            let v = self.d[rg] & sz.mask();
            let res = match ty {
                0 => self.asx(v, cnt, dr, sz),
                1 => self.lsx(v, cnt, dr, sz),
                2 => self.roxx(v, cnt, dr, sz),
                3 => self.rox(v, cnt, dr, sz),
                _ => v,
            };
            self.set_d(rg, res, sz);
            self.cycles = 6 + 2 * cnt;
        }
    }
    fn asx(&mut self, v: u32, c: u32, l: bool, sz: Size) -> u32 {
        if c == 0 {
            self.sr.carry = false;
            self.sr.overflow = false;
            self.sr.set_nz(v, sz);
            return v;
        }
        let m = sz.mask();
        let msb = sz.msb();
        if l {
            let mut x = v & m;
            let mut car = false;
            let mut ov = false;
            for _ in 0..c {
                car = (x & msb) != 0;
                let om = x & msb;
                x = (x << 1) & m;
                if (x & msb) != om {
                    ov = true;
                }
            }
            self.sr.carry = car;
            self.sr.extend = car;
            self.sr.overflow = ov;
            self.sr.set_nz(x, sz);
            x
        } else {
            let sign = v & msb;
            let mut x = v & m;
            let mut car = false;
            for _ in 0..c {
                car = (x & 1) != 0;
                x = (x >> 1) | sign;
            }
            self.sr.carry = car;
            self.sr.extend = car;
            self.sr.overflow = false;
            self.sr.set_nz(x, sz);
            x
        }
    }
    fn lsx(&mut self, v: u32, c: u32, l: bool, sz: Size) -> u32 {
        if c == 0 {
            self.sr.carry = false;
            self.sr.overflow = false;
            self.sr.set_nz(v, sz);
            return v;
        }
        let m = sz.mask();
        let msb = sz.msb();
        let bits = sz.bits();
        let (res, car) = if l {
            if c >= bits {
                (0, if c == bits { (v & 1) != 0 } else { false })
            } else {
                ((v << c) & m, (v & (msb >> (c - 1))) != 0)
            }
        } else {
            if c >= bits {
                (0, if c == bits { (v & msb) != 0 } else { false })
            } else {
                ((v >> c) & m, (v & (1 << (c - 1))) != 0)
            }
        };
        self.sr.carry = car;
        self.sr.extend = car;
        self.sr.overflow = false;
        self.sr.set_nz(res, sz);
        res
    }
    fn roxx(&mut self, v: u32, c: u32, l: bool, sz: Size) -> u32 {
        let m = sz.mask();
        let msb = sz.msb();
        let bits = sz.bits();
        let c = c % (bits + 1);
        if c == 0 {
            self.sr.carry = self.sr.extend;
            self.sr.overflow = false;
            self.sr.set_nz(v, sz);
            return v;
        }
        let mut x = v & m;
        let mut ext = if self.sr.extend { 1u32 } else { 0 };
        for _ in 0..c {
            if l {
                let nx = if (x & msb) != 0 { 1 } else { 0 };
                x = ((x << 1) | ext) & m;
                ext = nx;
            } else {
                let nx = if (x & 1) != 0 { 1 } else { 0 };
                x = (x >> 1) | (if ext != 0 { msb } else { 0 });
                ext = nx;
            }
        }
        self.sr.carry = ext != 0;
        self.sr.extend = ext != 0;
        self.sr.overflow = false;
        self.sr.set_nz(x, sz);
        x
    }
    fn rox(&mut self, v: u32, c: u32, l: bool, sz: Size) -> u32 {
        let m = sz.mask();
        let msb = sz.msb();
        let bits = sz.bits();

        if c == 0 {
            self.sr.carry = false;
            self.sr.overflow = false;
            self.sr.set_nz(v, sz);
            return v;
        }

        let shift = c % bits;
        let x = v & m;

        let res = if shift == 0 {
            x
        } else {
            if l {
                ((x << shift) | (x >> (bits - shift))) & m
            } else {
                ((x >> shift) | (x << (bits - shift))) & m
            }
        };

        // For ROL, last bit out is bit 0 of result (which was shifted around).
        // For ROR, last bit out is MSB of result.
        // Even if shift == 0 (e.g. rotate 32), the last bit out logic on 'res' holds:
        // L: bit 0 was shifted out on step 32.
        // R: bit 31 was shifted out on step 32.
        self.sr.carry = if l { (res & 1) != 0 } else { (res & msb) != 0 };
        self.sr.overflow = false;
        self.sr.set_nz(res, sz);
        res
    }
}
