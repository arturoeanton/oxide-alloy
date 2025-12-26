// crates/systems/oxid_master/src/vdp.rs

// --- Constantes del VDP ---
const VRAM_SIZE: usize = 0x4000; // 16KB Video RAM
const CRAM_SIZE: usize = 0x20;   // 32 Bytes Color RAM (16 BG + 16 Sprite)
const FRAME_WIDTH: usize = 256;
// const FRAME_HEIGHT: usize = 192; // Altura visible estándar NTSC (Unused)

// Banderas de Registro de Estado
const STATUS_VBLANK: u8    = 0x80; // Frame Interrupt Pending
const STATUS_OVERFLOW: u8  = 0x40; // Sprite Overflow (> 8 sprites per line)
const STATUS_COLLISION: u8 = 0x20; // Sprite Collision

/// Implementación del SMS VDP (Video Display Processor).
/// Basado en el TMS9918a pero con extensiones de Sega (Modo 4).
pub struct Vdp {
    // Memorias
    pub vram: [u8; VRAM_SIZE],
    pub cram: [u8; CRAM_SIZE],
    pub regs: [u8; 16],

    // Estado Interno
    pub status: u8,
    pub address: u16,        // Registro de Dirección (Internal Address Register)
    pub code: u8,            // Código de Acceso (0=Read, 1=Write, 2=Reg, 3=CRAM)
    pub read_buffer: u8,     // Buffer de pre-lectura (Read Buffer)
    pub address_latch: bool, // Toggle primer/segundo byte de escritura

    // Contadores de Interrupción
    pub line_counter: u8,       // Reg 10 Down Counter
    pub interrupt_pending: bool,// Line Interrupt Request
}

impl Vdp {
    pub fn new() -> Self {
        Self {
            vram: [0; VRAM_SIZE],
            cram: [0; CRAM_SIZE],
            regs: [0; 16],
            status: 0,
            address: 0,
            code: 0,
            read_buffer: 0,
            address_latch: false,
            line_counter: 0,
            interrupt_pending: false,
        }
    }

    /// Ejecuta la lógica al final de una scanline.
    /// Maneja el Line Counter y la bandera de VBlank.
    pub fn tick_scanline(&mut self, y: usize) {
        // En NTSC SMS, las líneas visibles son 0-191.
        // VBlank comienza en la línea 192.
        
        if y < 192 {
            // Reg 10 contiene el valor de recarga para el Line Counter.
            if self.line_counter == 0 {
                self.line_counter = self.regs[10];
                self.interrupt_pending = true; // Se activa al llegar a cero
            } else {
                self.line_counter = self.line_counter.saturating_sub(1);
            }
        } else {
            // Fuera de la zona visible, el contador se recarga constantemente.
            self.line_counter = self.regs[10];
        }

        // Interrupt de VBlank ocurre precisamente al incio de la línea 192.
        if y == 192 {
            self.status |= STATUS_VBLANK;
        }
    }

    /// Verifica si hay alguna interrupción pendiente (IRQ) hacia el Z80.
    pub fn is_interrupting(&self) -> bool {
        // Frame Interrupt (VBlank): Habilitado si Reg 1 Bit 5 está activo
        let vblank_irq = (self.status & STATUS_VBLANK) != 0 && (self.regs[1] & 0x20) != 0;
        
        // Line Interrupt: Habilitado si Reg 0 Bit 4 está activo
        let line_irq = self.interrupt_pending && (self.regs[0] & 0x10) != 0;
        
        vblank_irq || line_irq
    }

    /// Escribe en el Puerto de Control ($BF).
    /// Maneja la máquina de estados del Address Latch (1er byte / 2do byte).
    pub fn write_control(&mut self, val: u8) {
        if !self.address_latch {
            // Primer byte: 8 bits bajos de la dirección
            self.address = (self.address & 0xFF00) | (val as u16);
            self.address_latch = true;
        } else {
            // Segundo byte: 6 bits altos de la dirección + 2 bits de código
            self.address = (self.address & 0x00FF) | ((val as u16 & 0x3F) << 8);
            self.code = (val >> 6) & 0x03;
            self.address_latch = false;

            // VDP Quirk: Updating address register (Code 0, 1, 3) updates read buffer immediately.
            if self.code != 2 {
                self.read_buffer = self.vram[(self.address & 0x3FFF) as usize];
            }

            match self.code {
                0 => { // Read VRAM Request
                    self.address = self.address.wrapping_add(1) & 0x3FFF;
                }
                2 => { // Write VDP Register
                    // Formato: 10xx rrrr (Code 2, Reg index en lower 4 bits of high byte? No, it's specific)
                    // En realidad, para escribir registro:
                    // Byte 1: Data
                    // Byte 2: 1000 rrrr (0x80 | Reg)
                    // El "Code 2" viene de los bits altos 10 (binario) en el 2do byte.
                    
                    // La dirección capturada en 'address' tiene: dddd dddd (low) | 1000 rrrr (high)
                    // El dato a escribir es el byte bajo de 'address'.
                    // El registro es los 4 bits bajos del byte alto.
                    let reg = val & 0x0F; 
                    // El dato estaba en el primer byte, que ahora es self.address & 0xFF
                    let data = (self.address & 0xFF) as u8;
                    
                    if reg < 16 {
                        self.regs[reg as usize] = data;
                    }
                }
                _ => {} // Otros modos (Write VRAM/CRAM) solo setean la dirección
            }
        }
    }

    /// Escribe en el Puerto de Datos ($BE).
    pub fn write_data(&mut self, val: u8) {
        self.address_latch = false; // Escribir datos resetea el latch
        self.read_buffer = val; // Actualiza el buffer de lectura también

        match self.code {
            0..=2 => { // Write VRAM
                self.vram[(self.address & 0x3FFF) as usize] = val;
            }
            3 => { // Write CRAM
                // CRAM address es solo los 5 bits bajos (32 colores)
                self.cram[(self.address & 0x1F) as usize] = val;
            }
            _ => {}
        }
        self.address = self.address.wrapping_add(1) & 0x3FFF;
    }

    /// Lee del Puerto de Datos ($BE).
    pub fn read_data(&mut self) -> u8 {
        self.address_latch = false;
        
        // Devuelve el contenido del buffer
        let res = self.read_buffer;
        
        // Pre-carga el siguiente byte en el buffer
        self.read_buffer = self.vram[(self.address & 0x3FFF) as usize];
        self.address = self.address.wrapping_add(1) & 0x3FFF;
        
        res
    }

    /// Lee del Puerto de Estado ($BF).
    pub fn read_status(&mut self) -> u8 {
        let res = self.status;
        
        // Lectura limpia banderas y latch
        self.status &= 0x1F; // Limpia bits 7 (F), 6 (O), 5 (C)
        self.interrupt_pending = false; // Limpia flag de interrupción de línea
        self.address_latch = false;
        
        res
    }

    /// Renderiza una línea de scanline (0-191).
    pub fn render_scanline(&mut self, y: usize, line_buffer: &mut [u32]) {
        if y >= 192 { return; }

        let mut bg_buffer = [(0u8, false); FRAME_WIDTH]; // (color_idx, priority_bit)
        let mut spr_buffer = [(0u8, 0u8); FRAME_WIDTH];   // (color_idx, sprite_index) - index not strictly needed for color, but debugging

        // 1. Render Background
        self.render_background(y, &mut bg_buffer);

        // 2. Render Sprites
        self.render_sprites(y, &mut spr_buffer);

        // 3. Composition
        let backdrop_color_idx = (self.regs[7] & 0x0F) + 16; // Backdrop uses Sprite Palette? No, Reg 7 lower nibble. 
        // Docs: "Background color register... bits 0-3 select color from sub-palette 2 (sprite palette)" -> +16.
        // Actually it depends on the mode, but for SMS it's usually +16 unless Bit 4 of Reg 0 is set?
        // Let's assume +16 for now as standard SMS.
        
        let mask_col0 = (self.regs[0] & 0x20) != 0;

        for x in 0..FRAME_WIDTH {
            // Masking Column 0
            if mask_col0 && x < 8 {
                line_buffer[x] = 0xFF000000;
                continue;
            }

            let (bg_idx, bg_priority) = bg_buffer[x];
            let (spr_idx, _spr_id) = spr_buffer[x];

            // Logic:
            // - Sprite trumps BG, UNLESS BG has Priority bit SET and BG pixel is opaque.
            // - Transparent pixels (index%16 == 0) don't draw.
            // - If both transparent, draw Backdrop.
            
            let bg_transparent = (bg_idx & 0x0F) == 0;
            let spr_transparent = (spr_idx & 0x0F) == 0;

            let final_idx = if !spr_transparent {
                if bg_priority && !bg_transparent {
                    bg_idx // BG Priority wins
                } else {
                    spr_idx // Sprite wins
                }
            } else {
                if !bg_transparent {
                    bg_idx // BG Normal
                } else {
                    backdrop_color_idx // Backdrop
                }
            };

            // Palette Lookup
            let val = self.cram[(final_idx & 0x1F) as usize];
            let r = (val & 0x03) * 85;
            let g = ((val >> 2) & 0x03) * 85;
            let b = ((val >> 4) & 0x03) * 85;
            
            line_buffer[x] = 0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        }
    }

    fn render_background(&mut self, y: usize, buffer: &mut [(u8, bool)]) {
        let scroll_x = self.regs[8] as usize;
        let scroll_y = self.regs[9] as usize;
        let name_table_base = ((self.regs[2] as usize & 0x0E) << 10); // $3800
        
        // Scroll Locking
        let h_scroll_inh = (self.regs[0] & 0x40) != 0 && y < 16;
        let v_scroll_inh = (self.regs[0] & 0x80) != 0; 

        // Mask Column 0 is handled in composition, but we render fully here.

        for x in 0..FRAME_WIDTH {
            let cur_scroll_x = if h_scroll_inh { 0 } else { scroll_x };
            let cur_scroll_y = if v_scroll_inh && x >= 192 { 0 } else { scroll_y };

            // Virtual Coords
            // In SMS Mode 4: 256 x 224 virtual map.
            // BG Y Wrapping: 224 lines.
            // BG X: Subtractive scroll (x - scroll) shifts background appropriately
            let bg_x = (x.wrapping_add(256).wrapping_sub(cur_scroll_x)) % 256;
            let bg_y = (y + cur_scroll_y) % 224; 

            let tx = bg_x / 8;
            let ty = bg_y / 8;
            let nt_addr = name_table_base + (ty * 64) + (tx * 2);

            let low = self.vram[nt_addr];
            let high = self.vram[nt_addr + 1];
            let entry = (high as u16) << 8 | (low as u16);

            let priority = (entry & 0x1000) != 0;
            let palette_sel = (entry & 0x0800) != 0;
            let v_flip = (entry & 0x0400) != 0;
            let h_flip = (entry & 0x0200) != 0;
            let tile_idx = entry & 0x01FF;

            let py = if v_flip { 7 - (bg_y % 8) } else { bg_y % 8 };
            let px = if h_flip { 7 - (bg_x % 8) } else { bg_x % 8 };

            let tile_addr = (tile_idx as usize * 32) + (py as usize * 4);
            // Optimization: read 4 bytes at once? No, vram is u8 array.
            
            let b0 = self.vram[tile_addr];
            let b1 = self.vram[tile_addr + 1];
            let b2 = self.vram[tile_addr + 2];
            let b3 = self.vram[tile_addr + 3];

            let shift = 7 - px;
            let color_val = 
                (((b0 >> shift) & 1) << 0) |
                (((b1 >> shift) & 1) << 1) |
                (((b2 >> shift) & 1) << 2) |
                (((b3 >> shift) & 1) << 3);

            let final_idx = if palette_sel { 16 + color_val } else { color_val };
            
            buffer[x] = (final_idx, priority);
        }
    }

    fn render_sprites(&mut self, y: usize, buffer: &mut [(u8, u8)]) {
        let sprite_attr_base = ((self.regs[5] as usize & 0x7E) << 7);
        let sprite_pattern_base = if (self.regs[6] & 0x04) != 0 { 0x2000 } else { 0x0000 };
        let sprite_size_16 = (self.regs[1] & 0x02) != 0;
        let sprite_shift = (self.regs[0] & 0x08) != 0;
        
        let sprite_height = if sprite_size_16 { 16 } else { 8 };
        let mut sprites_drawn = 0;

        for i in 0..64 {
            let y_addr = sprite_attr_base + i;
            let sy_raw = self.vram[y_addr];
            if sy_raw == 0xD0 { break; } // Terminator
            
            // Y Coordinate logic
            // SMS VDP Mode 4 applies a +1 offset to the Y coordinate.
            // A sprite at Y=0 starts drawing at line 1.
            let mut sy = sy_raw as i32;
            if sy > 240 { sy -= 256; }
            sy += 1; // Correct Mode 4 Offset

            let line_y = y as i32;
            if line_y >= sy && line_y < (sy + sprite_height) {
                if sprites_drawn >= 8 {
                    self.status |= STATUS_OVERFLOW;
                    break; 
                }
                
                // Read X and Tile from SAT (second half, offset 0x80)
                // SAT format: Y table (64 bytes), then X/N table (128 bytes: X, N interleaved)
                let xn_addr = sprite_attr_base + 0x80 + (i * 2);
                let sx_raw = self.vram[xn_addr];
                let tile_raw = self.vram[xn_addr + 1];

                let sx = (sx_raw as i32) - (if sprite_shift { 8 } else { 0 });
                let tile_idx = if sprite_size_16 { tile_raw & 0xFE } else { tile_raw } as usize;
                
                let py = (line_y - sy) as usize;
                let pat_addr = (sprite_pattern_base + (tile_idx * 32) + (py * 4)) & 0x3FFF;
                
                let b0 = self.vram[pat_addr];
                let b1 = self.vram[(pat_addr + 1) & 0x3FFF];
                let b2 = self.vram[(pat_addr + 2) & 0x3FFF];
                let b3 = self.vram[(pat_addr + 3) & 0x3FFF];

                for px in 0..8 {
                    let screen_x = sx + px;
                    if screen_x < 0 || screen_x >= 256 { continue; }
                    let screen_x_u = screen_x as usize;

                    // Already drawn a sprite here? SMS shows first sprite in list.
                    if buffer[screen_x_u].0 != 0 {
                        // Collision Check: New sprite pixel overlaps existing sprite pixel
                        // Logic: If we seek to draw a non-transparent pixel, and one is already there..
                        // But wait, the loop iterates front-to-back.
                        // If buffer has a pixel, it came from a higher priority sprite (lower index).
                        // Collision flag is set when two non-transparent sprite pixels overlap.
                        let shift = 7 - px;
                        let color_val = 
                             (((b0 >> shift) & 1) << 0) |
                             (((b1 >> shift) & 1) << 1) |
                             (((b2 >> shift) & 1) << 2) |
                             (((b3 >> shift) & 1) << 3);

                        if color_val != 0 {
                            self.status |= STATUS_COLLISION;
                        }
                        continue; 
                    }

                    let shift = 7 - px;
                    let color_val = 
                        (((b0 >> shift) & 1) << 0) |
                        (((b1 >> shift) & 1) << 1) |
                        (((b2 >> shift) & 1) << 2) |
                        (((b3 >> shift) & 1) << 3);

                    if color_val != 0 {
                        buffer[screen_x_u] = (color_val + 16, i as u8);
                    }
                }
                sprites_drawn += 1;
            }
        }
    }
}
