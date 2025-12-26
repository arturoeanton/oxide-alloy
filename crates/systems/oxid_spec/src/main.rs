use oxidz80::OxidZ80;
use oxide_core::{Cpu, MemoryBus, Rom};
use oxid_display::{OxidDisplay, DisplayConfig, WindowScale};
use minifb::Key;

// ============================================================================
//  CONSTANTS
// ============================================================================
const SCREEN_WIDTH: usize = 256;
const SCREEN_HEIGHT: usize = 192;
const CYCLES_PER_FRAME: u32 = 69888; // 3.5MHz / 50.08 Hz

// Paleta Oficial (0-7 Normal, 8-15 Bright)
const PALETTE: [u32; 16] = [
    0x000000, 0x0000CD, 0xCD0000, 0xCD00CD, 0x00CD00, 0x00CDCD, 0xCDCD00, 0xCDCDCD, // Normal
    0x000000, 0x0000FF, 0xFF0000, 0xFF00FF, 0x00FF00, 0x00FFFF, 0xFFFF00, 0xFFFFFF, // Bright
];

// ============================================================================
//  BUS implementation
// ============================================================================
struct SpectrumBus {
    rom: Vec<u8>,
    ram: Vec<u8>,
    border_color: u8,
    keys: Vec<Key>,
    flash_frame: u32,
}

impl SpectrumBus {
    fn new(rom: Rom) -> Self {
        // Ensure ROM is exactly 16KB
        let mut rom_data = rom.data;
        if rom_data.len() > 16384 { rom_data.truncate(16384); }
        if rom_data.len() < 16384 { rom_data.resize(16384, 0xFF); }

        Self {
            rom: rom_data,
            ram: vec![0; 49152], // 48KB RAM
            border_color: 7,
            keys: Vec::new(),
            flash_frame: 0,
        }
    }

    fn read_keyboard(&self, row_mask: u8) -> u8 {
        let mut data = 0xFF; // All keys released (1)

        // Debug: Print mask
        // if row_mask == 0 { println!("Keyboard Scan: Mask 0"); }

        // Check for convenience keys
        let backspace = self.keys.contains(&Key::Backspace);
        let left = self.keys.contains(&Key::Left);
        let down = self.keys.contains(&Key::Down);
        let up = self.keys.contains(&Key::Up);
        let right = self.keys.contains(&Key::Right);
        
        let force_caps = backspace || left || down || up || right;

        // Helper to check key and pull bit low (0)
        let check = |key: Key, bit: u8, current: u8| -> u8 {
            if self.keys.contains(&key) { current & !(1 << bit) } else { current }
        };

        // Row 0 (Bit 0 low): SHIFT, Z, X, C, V
        if (row_mask & 0x01) == 0 {
            // CAPS SHIFT (Bit 0)
            if self.keys.contains(&Key::LeftShift) || force_caps {
                data &= !0x01;
            }
            data = check(Key::Z, 1, data);
            data = check(Key::X, 2, data);
            data = check(Key::C, 3, data);
            data = check(Key::V, 4, data);
        }
        // Row 1 (Bit 1 low): A, S, D, F, G
        if (row_mask & 0x02) == 0 {
            data = check(Key::A, 0, data);
            data = check(Key::S, 1, data);
            data = check(Key::D, 2, data);
            data = check(Key::F, 3, data);
            data = check(Key::G, 4, data);
        }
        // Row 2: Q, W, E, R, T
        if (row_mask & 0x04) == 0 {
            data = check(Key::Q, 0, data);
            data = check(Key::W, 1, data);
            data = check(Key::E, 2, data);
            data = check(Key::R, 3, data);
            data = check(Key::T, 4, data);
        }
        // Row 3: 1, 2, 3, 4, 5
        if (row_mask & 0x08) == 0 {
            data = check(Key::Key1, 0, data);
            data = check(Key::Key2, 1, data);
            data = check(Key::Key3, 2, data);
            data = check(Key::Key4, 3, data);
            // 5 (Bit 4) -> Left
            if self.keys.contains(&Key::Key5) || left { data &= !0x10; }
        }
        // Row 4: 0, 9, 8, 7, 6
        if (row_mask & 0x10) == 0 {
            // 0 (Bit 0) -> Backspace
            if self.keys.contains(&Key::Key0) || backspace { data &= !0x01; }
            data = check(Key::Key9, 1, data);
            // 8 (Bit 2) -> Right
            if self.keys.contains(&Key::Key8) || right { data &= !0x04; }
            // 7 (Bit 3) -> Up
            if self.keys.contains(&Key::Key7) || up { data &= !0x08; }
            // 6 (Bit 4) -> Down
            if self.keys.contains(&Key::Key6) || down { data &= !0x10; }
        }
        // Row 5: P, O, I, U, Y
        if (row_mask & 0x20) == 0 {
            data = check(Key::P, 0, data);
            data = check(Key::O, 1, data);
            data = check(Key::I, 2, data);
            data = check(Key::U, 3, data);
            data = check(Key::Y, 4, data);
        }
        // Row 6: ENTER, L, K, J, H
        if (row_mask & 0x40) == 0 {
            data = check(Key::Enter, 0, data);
            data = check(Key::L, 1, data);
            data = check(Key::K, 2, data);
            data = check(Key::J, 3, data);
            data = check(Key::H, 4, data);
        }
        // Row 7: SPACE, SYM, M, N, B
        if (row_mask & 0x80) == 0 {
            data = check(Key::Space, 0, data);
            data = check(Key::RightShift, 1, data); // Sym Shift
            data = check(Key::M, 2, data);
            data = check(Key::N, 3, data);
            data = check(Key::B, 4, data);
        }
        
        if data != 0xFF {
             // println!("SCAN RowMask:{:02X} Data:{:02X}", row_mask, data);
        }
        data
    }


    // I/O methods moved to Trait Implementation
}

impl MemoryBus for SpectrumBus {
    fn read(&self, addr: u32) -> u8 {
        let a = addr & 0xFFFF;
        if a < 0x4000 {
            // ROM (0x0000 - 0x3FFF)
            unsafe { *self.rom.get_unchecked(a as usize) }
        } else {
            // RAM (0x4000 - 0xFFFF) -> Offset 0
            unsafe { *self.ram.get_unchecked((a - 0x4000) as usize) }
        }
    }

    fn write(&mut self, addr: u32, val: u8) {
        let a = addr & 0xFFFF;
        if a >= 0x4000 {
            // RAM
            unsafe { *self.ram.get_unchecked_mut((a - 0x4000) as usize) = val; }
        }
        // ROM Writes ignored
    }

    fn port_in(&mut self, port: u16) -> u8 {
        // ULA Port 0xFE
        if (port & 1) == 0 {
            let row_mask = (port >> 8) as u8;
            return self.read_keyboard(row_mask);
        }
        // Kempston Joystick (0x1F) - TODO
        0xFF
    }

    fn port_out(&mut self, port: u16, val: u8) {
        // ULA Port 0xFE: Border + MIC/EAR
        if (port & 1) == 0 {
            self.border_color = val & 0x07;
            // TODO: Audio (Bit 3 MIC, Bit 4 EAR)
        }
    }
}

mod disasm;

use std::fs::File;
use std::io::Write;
use std::path::Path;

struct Config {
    rom_path: String,
    log_path: Option<String>,
    verbosity: u32,
}

struct LogManager {
    base_path: String,
    current_file: Option<File>,
    current_size: u64,
    rotation_count: u32,
    max_size: u64,
}

impl LogManager {
    fn new(path: &str) -> Self {
        let max_size = 50 * 1024 * 1024; // 50MB
        let mut mgr = Self {
            base_path: path.to_string(),
            current_file: None,
            current_size: 0,
            rotation_count: 0,
            max_size,
        };
        mgr.rotate();
        mgr
    }

    fn rotate(&mut self) {
        let path = Path::new(&self.base_path);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("log");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("log");
        
        let new_filename = format!("{:}_{:03}.{:}", stem, self.rotation_count, ext);
        println!("Rotating to log file: {}", new_filename);
        
        self.current_file = Some(File::create(new_filename).unwrap());
        self.current_size = 0;
        self.rotation_count += 1;
    }

    fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        if let Some(ref mut f) = self.current_file {
            let bytes = line.as_bytes();
            f.write_all(bytes)?;
            f.write_all(b"\n")?;
            self.current_size += (bytes.len() + 1) as u64;

            if self.current_size >= self.max_size {
                self.rotate();
            }
        }
        Ok(())
    }
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();
    let mut config = Config {
        rom_path: "roms/48.rom".into(),
        log_path: None,
        verbosity: 0,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-rom" if i + 1 < args.len() => {
                config.rom_path = args[i + 1].clone();
                i += 2;
            }
            "-log" if i + 1 < args.len() => {
                config.log_path = Some(args[i + 1].clone());
                i += 2;
            }
            "-v" => { config.verbosity = 1; i += 1; }
            "-vv" => { config.verbosity = 2; i += 1; }
            "-vvv" => { config.verbosity = 3; i += 1; }
            _ => i += 1,
        }
    }
    config
}

// ============================================================================
//  MAIN
// ============================================================================
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_args();
    println!("--- Oxide Spectrum (ZX Spectrum 48K) ---");
    println!("ROM: {}", config.rom_path);
    if let Some(ref p) = config.log_path { println!("Logging execution base: {}", p); }
    println!("Verbosity level: {}", config.verbosity);

    let rom = Rom::from_file(&config.rom_path)?;
    let mut bus = SpectrumBus::new(rom);
    let mut cpu = OxidZ80::new();
    let mut display = OxidDisplay::new(DisplayConfig {
        title: format!("Oxide Spectrum - {}", config.rom_path),
        width: SCREEN_WIDTH,
        height: SCREEN_HEIGHT,
        scale: WindowScale::X2,
        target_fps: 50.0,
        resizable: false,
    });
    
    let mut frame_buffer = vec![0u32; SCREEN_WIDTH * SCREEN_HEIGHT];
    let mut log_mgr = config.log_path.as_ref().map(|p| LogManager::new(p));

    cpu.reset();
    
    while display.is_open() {
        bus.keys = display.get_keys();
        if !bus.keys.is_empty() {
            println!("KEYS PRESSED: {:?}", bus.keys);
        }
        bus.flash_frame = bus.flash_frame.wrapping_add(1);

        if bus.flash_frame % 50 == 0 {
             // Logic removed
        }

        if bus.flash_frame % 60 == 0 {
            // [DIAGNOSTIC] Check if FRAMES system variable is incrementing
            // FRAMES is at 0x5C78 (Low) and 0x5C79 (High)
            // 0x5C78 - 0x4000 = 0x1C78 offset in RAM
             if let Some(low) = bus.ram.get(0x1C78) {
                 if let Some(high) = bus.ram.get(0x1C79) {
                     let frames = (*low as u16) | ((*high as u16) << 8);
                     
                     // Check LAST_K (0x5C08 = 0x1C08)
                     let last_k = *bus.ram.get(0x1C08).unwrap_or(&0);
                     // Check FLAGS (0x5C3B = 0x1C3B)
                     let flags = *bus.ram.get(0x1C3B).unwrap_or(&0);
                     
                     println!("SYS FRAMES:{} IFF1:{} LAST_K:{:02X} FLAGS:{:02X}", frames, cpu.iff1, last_k, flags);
                 }
             }
        }

        // Run Frame
        let mut cycles = 0;
        while cycles < CYCLES_PER_FRAME {
            // Tracing / Logging logic
            if config.verbosity > 0 || log_mgr.is_some() {
                let pc = cpu.pc;
                // Filter out Screen Clear Loop to avoid massive logs
                if pc >= 0x0E4D && pc <= 0x0E66 { 
                    cycles += cpu.step(&mut bus); 
                    continue; 
                }

                let (mnemonic, len) = disasm::disassemble(pc, &bus);
                let mut bytes_str = String::new();
                for i in 0..len {
                    bytes_str.push_str(&format!("{:02X} ", bus.read((pc + i) as u32)));
                }

                let mut line = format!("{:04X}: {:<12} {:<20}", pc, bytes_str, mnemonic);
                
                if config.verbosity >= 2 {
                    line.push_str(&format!(" AF:{:04X} BC:{:04X} DE:{:04X} HL:{:04X}", 
                        cpu.af(), cpu.bc(), cpu.de(), cpu.hl()));
                }
                if config.verbosity >= 3 {
                    line.push_str(&format!(" IX:{:04X} IY:{:04X} SP:{:04X} I:{:02X} R:{:02X}", 
                        cpu.ix, cpu.iy, cpu.sp, cpu.i, cpu.r));
                }

                if let Some(ref mut mgr) = log_mgr {
                    mgr.write_line(&line).ok();
                } else if config.verbosity > 0 {
                    println!("{}", line);
                }
                
                cycles += cpu.step(&mut bus);
            } else {
                if cpu.halted {
                    println!("CPU HALTED at frame, IFF1={}", cpu.iff1);
                }
                cycles += cpu.step(&mut bus);
            }
        }

        // VBLANK Interrupt
        if cpu.iff1 {
            cpu.irq(&mut bus, 0xFF);
        }
        
        // Render
        render_screen(&bus, &mut frame_buffer);
        display.update(&frame_buffer);
    }

    Ok(())
}

fn render_screen(bus: &SpectrumBus, buffer: &mut [u32]) {
    // VRAM is at 0x4000 in System Map.
    // In our new bus.ram, 0x4000 maps to index 0.
    const VRAM_OFFSET: usize = 0; 
    const ATTR_OFFSET: usize = 0x1800; // 0x5800 - 0x4000
    
    let flash_on = (bus.flash_frame & 16) != 0; // Blink ~3 Hz

    for y in 0..192 {
        // Line translation logic
        let line = y & 0x07;
        let row = (y >> 3) & 0x07;
        let sector = (y >> 6) & 0x03;
        
        // Relative to RAM start
        // Spectrum Layout: SS LLL RRR CCCCC (Sector, Line, Row, Col)
        // My previous code had swapped Line (L) and Row (R).
        // Correct: Line << 8, Row << 5.
        let pixel_idx = VRAM_OFFSET | (sector << 11) | (line << 8) | (row << 5);
        let attr_idx = ATTR_OFFSET | (sector << 8) | (row << 5);

        for x_byte in 0..32 {
            let pixels = unsafe { *bus.ram.get_unchecked(pixel_idx + x_byte) };
            let attr = unsafe { *bus.ram.get_unchecked(attr_idx + x_byte) };

            let mut ink = PALETTE[(attr & 0x07) as usize + if (attr & 0x40)!=0 {8} else {0}];
            let mut paper = PALETTE[((attr >> 3) & 0x07) as usize + if (attr & 0x40)!=0 {8} else {0}];
            
            if (attr & 0x80) != 0 && flash_on {
                std::mem::swap(&mut ink, &mut paper);
            }

            for bit in 0..8 {
                let color = if (pixels & (0x80 >> bit)) != 0 { ink } else { paper };
                buffer[y * SCREEN_WIDTH + (x_byte * 8 + bit)] = color;
            }
        }
    }
}
