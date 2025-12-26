use oxide_core::MemoryBus;

pub fn disassemble(pc: u16, bus: &dyn MemoryBus) -> (String, u16) {
    let b0 = bus.read(pc as u32);
    let mut len = 1;

    let mnemonic = match b0 {
        0x00 => "NOP".into(),
        0x01 => { len=3; format!("LD BC, ${:04X}", read16(pc+1, bus)) }
        0x02 => "LD (BC), A".into(),
        0x03 => "INC BC".into(),
        0x04 => "INC B".into(),
        0x05 => "DEC B".into(),
        0x06 => { len=2; format!("LD B, ${:02X}", bus.read((pc+1) as u32)) }
        0x07 => "RLCA".into(),
        0x08 => "EX AF, AF'".into(),
        0x09 => "ADD HL, BC".into(),
        0x0A => "LD A, (BC)".into(),
        0x0B => "DEC BC".into(),
        0x0C => "INC C".into(),
        0x0D => "DEC C".into(),
        0x0E => { len=2; format!("LD C, ${:02X}", bus.read((pc+1) as u32)) }
        0x0F => "RRCA".into(),
        
        0x10 => { len=2; format!("DJNZ ${:04X}", rel(pc, bus)) }
        0x11 => { len=3; format!("LD DE, ${:04X}", read16(pc+1, bus)) }
        0x12 => "LD (DE), A".into(),
        0x13 => "INC DE".into(),
        0x17 => "RLA".into(),
        0x18 => { len=2; format!("JR ${:04X}", rel(pc, bus)) }
        0x19 => "ADD HL, DE".into(),
        0x1A => "LD A, (DE)".into(),
        0x1B => "DEC DE".into(),

        0x20 => { len=2; format!("JR NZ, ${:04X}", rel(pc, bus)) }
        0x21 => { len=3; format!("LD HL, ${:04X}", read16(pc+1, bus)) }
        0x22 => { len=3; format!("LD (${:04X}), HL", read16(pc+1, bus)) }
        0x23 => "INC HL".into(),
        0x27 => "DAA".into(),
        0x28 => { len=2; format!("JR Z, ${:04X}", rel(pc, bus)) }
        0x29 => "ADD HL, HL".into(),
        0x2A => { len=3; format!("LD HL, (${:04X})", read16(pc+1, bus)) }
        0x2F => "CPL".into(),

        0x30 => { len=2; format!("JR NC, ${:04X}", rel(pc, bus)) }
        0x31 => { len=3; format!("LD SP, ${:04X}", read16(pc+1, bus)) }
        0x32 => { len=3; format!("LD (${:04X}), A", read16(pc+1, bus)) }
        0x33 => "INC SP".into(),
        0x36 => { len=2; format!("LD (HL), ${:02X}", bus.read((pc+1) as u32)) }
        0x37 => "SCF".into(),
        0x38 => { len=2; format!("JR C, ${:04X}", rel(pc, bus)) }
        0x39 => "ADD HL, SP".into(),
        0x3A => { len=3; format!("LD A, (${:04X})", read16(pc+1, bus)) }
        0x3B => "DEC SP".into(),
        0x3C => "INC A".into(),
        0x3D => "DEC A".into(),
        0x3E => { len=2; format!("LD A, ${:02X}", bus.read((pc+1) as u32)) }
        0x3F => "CCF".into(),

        0x40..=0x75 | 0x77..=0x7F => {
            let src = reg(b0 & 7);
            let dst = reg((b0 >> 3) & 7);
            format!("LD {}, {}", dst, src)
        }
        0x76 => "HALT".into(),

        0x80..=0x87 => format!("ADD A, {}", reg(b0 & 7)),
        0x88..=0x8F => format!("ADC A, {}", reg(b0 & 7)),
        0x90..=0x97 => format!("SUB {}", reg(b0 & 7)),
        0x98..=0x9F => format!("SBC A, {}", reg(b0 & 7)),
        0xA0..=0xA7 => format!("AND {}", reg(b0 & 7)),
        0xA8..=0xAF => format!("XOR {}", reg(b0 & 7)),
        0xB0..=0xB7 => format!("OR {}", reg(b0 & 7)),
        0xB8..=0xBF => format!("CP {}", reg(b0 & 7)),

        0xC0 => "RET NZ".into(),
        0xC1 => "POP BC".into(),
        0xC2 => { len=3; format!("JP NZ, ${:04X}", read16(pc+1, bus)) }
        0xC3 => { len=3; format!("JP ${:04X}", read16(pc+1, bus)) }
        0xC4 => { len=3; format!("CALL NZ, ${:04X}", read16(pc+1, bus)) }
        0xC5 => "PUSH BC".into(),
        0xC8 => "RET Z".into(),
        0xC9 => "RET".into(),
        0xCA => { len=3; format!("JP Z, ${:04X}", read16(pc+1, bus)) }
        0xCC => { len=3; format!("CALL Z, ${:04X}", read16(pc+1, bus)) }
        0xCD => { len=3; format!("CALL ${:04X}", read16(pc+1, bus)) }

        0xD0 => "RET NC".into(),
        0xD1 => "POP DE".into(),
        0xD3 => { len=2; format!("OUT (${:02X}), A", bus.read((pc+1) as u32)) }
        0xD5 => "PUSH DE".into(),
        0xD8 => "RET C".into(),
        0xD9 => "EXX".into(),
        0xDB => { len=2; format!("IN A, (${:02X})", bus.read((pc+1) as u32)) }

        0xE1 => "POP HL".into(),
        0xE3 => "EX (SP), HL".into(),
        0xE5 => "PUSH HL".into(),
        0xE9 => "JP (HL)".into(),
        0xEB => "EX DE, HL".into(),
        0xF1 => "POP AF".into(),
        0xF3 => "DI".into(),
        0xF5 => "PUSH AF".into(),
        0xF9 => "LD SP, HL".into(),
        0xFB => "EI".into(),

        // ALU Immediate
        0xC6 => { len=2; format!("ADD A, ${:02X}", bus.read((pc+1) as u32)) }
        0xCE => { len=2; format!("ADC A, ${:02X}", bus.read((pc+1) as u32)) }
        0xD6 => { len=2; format!("SUB ${:02X}", bus.read((pc+1) as u32)) }
        0xDE => { len=2; format!("SBC A, ${:02X}", bus.read((pc+1) as u32)) }
        0xE6 => { len=2; format!("AND ${:02X}", bus.read((pc+1) as u32)) }
        0xEE => { len=2; format!("XOR ${:02X}", bus.read((pc+1) as u32)) }
        0xF6 => { len=2; format!("OR ${:02X}", bus.read((pc+1) as u32)) }
        0xFE => { len=2; format!("CP ${:02X}", bus.read((pc+1) as u32)) }

        0xED => {
            let b1 = bus.read((pc+1) as u32);
            len = 2;
            match b1 {
                0x4B => { len=4; format!("LD BC, (${:04X})", read16(pc+2, bus)) }
                0x5B => { len=4; format!("LD DE, (${:04X})", read16(pc+2, bus)) }
                0x7B => { len=4; format!("LD SP, (${:04X})", read16(pc+2, bus)) }
                0x43 => { len=4; format!("LD (${:04X}), BC", read16(pc+2, bus)) }
                0x53 => { len=4; format!("LD (${:04X}), DE", read16(pc+2, bus)) }
                0x73 => { len=4; format!("LD (${:04X}), SP", read16(pc+2, bus)) }
                0xB0 => "LDIR".into(),
                0xB1 => "CPIR".into(),
                0xB8 => "LDDR".into(),
                0xB9 => "CPDR".into(),
                _ => format!("ED ${:02X}", b1)
            }
        }
        
        0xDD | 0xFD => {
            let b1 = bus.read((pc+1) as u32);
            let prefix = if b0 == 0xDD { "IX" } else { "IY" };
            len = 2;
            match b1 {
                0x21 => { len=4; format!("LD {}, ${:04X}", prefix, read16(pc+2, bus)) }
                0x36 => { len=4; format!("LD ({}+${:02X}), ${:02X}", prefix, bus.read((pc+2) as u32), bus.read((pc+3) as u32)) }
                _ => format!("{}-Prefix ${:02X}", prefix, b1)
            }
        }

        0xCB => {
            let b1 = bus.read((pc+1) as u32);
            len = 2;
            format!("CB ${:02X}", b1)
        }

        _ => format!("DB ${:02X}", b0)
    };

    (mnemonic, len)
}

fn reg(r: u8) -> &'static str {
    match r { 0=>"B", 1=>"C", 2=>"D", 3=>"E", 4=>"H", 5=>"L", 6=>"(HL)", 7=>"A", _=>"?" }
}

fn read16(pc: u16, bus: &dyn MemoryBus) -> u16 {
    let l = bus.read(pc as u32) as u16;
    let h = bus.read((pc+1) as u32) as u16;
    (h << 8) | l
}

fn rel(pc: u16, bus: &dyn MemoryBus) -> u16 {
    let offset = bus.read((pc+1) as u32) as i8;
    (pc as i32 + 2 + offset as i32) as u16
}
