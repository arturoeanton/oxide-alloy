// crates/systems/oxid_mac/src/video.rs

/// Ancho de pantalla del Macintosh Classic/SE/Plus
pub const SCREEN_WIDTH: usize = 512;
/// Alto de pantalla del Macintosh Classic/SE/Plus
pub const SCREEN_HEIGHT: usize = 342;
/// Ancho del buffer de video en bytes (512 pixels / 8 bits)
pub const ROW_BYTES: usize = SCREEN_WIDTH / 8;

/// Representa el subsistema de video del Macintosh (Shifter).
/// En el Mac original, el video lee directamente de la RAM principal.
pub struct MacVideo;

impl MacVideo {
    pub fn new() -> Self {
        Self
    }

    /// Renderiza el buffer de video del Mac (1-bit por pixel) a un buffer de 32-bit (ARGB) para minifb.
    /// 
    /// `vram`: Slice de la RAM que contiene los datos de video.
    /// `buffer`: Buffer de salida de 32 bits (size = 512 * 342).
    pub fn render_screen(&self, vram: &[u8], buffer: &mut [u32]) {
        // El video buffer usualmente comienza en $3FA700 en un Mac de 4MB o algo así,
        // pero aquí pasamos solo el slice relevante.
        // Asumimos que `vram` empieza exactamente donde empieza la memoria de video.
        
        // Color 0 = Blanco (en Mac 0 es blanco en VRAM para pantalla? No, 0 es blanco, 1 es negro).
        // Wait, Macintosh 1-bit: 0=White, 1=Black.
        const COLOR_WHITE: u32 = 0xFFDDDDDD; // Un blanco 'papel' no tan brillante
        const COLOR_BLACK: u32 = 0xFF222222; // Un negro no tan absoluto

        for y in 0..SCREEN_HEIGHT {
            for col_byte in 0..ROW_BYTES {
                let byte = vram[y * ROW_BYTES + col_byte];
                
                // Procesar 8 pixeles
                for bit in 0..8 {
                    // El bit más significativo (0x80) es el pixel de más a la izquierda (pixel 0 del byte).
                    let is_black = (byte & (0x80 >> bit)) != 0;
                    
                    let color = if is_black { COLOR_BLACK } else { COLOR_WHITE };
                    
                    buffer[y * SCREEN_WIDTH + (col_byte * 8 + bit)] = color;
                }
            }
        }
    }
}
