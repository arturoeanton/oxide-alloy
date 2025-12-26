// crates/oxid_display/src/lib.rs

use minifb::{Window, WindowOptions, Scale, Key, ScaleMode};
use std::time::{Duration, Instant};
use std::thread;

// ============================================================================
//  CONFIGURACIÓN Y ERRORES
// ============================================================================

#[derive(Debug, Clone)]
pub struct DisplayConfig {
    pub title: String,
    pub width: usize,
    pub height: usize,
    pub scale: WindowScale,
    pub target_fps: f64,
    pub resizable: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            title: "Oxide Emulator".to_string(),
            width: 640,
            height: 480,
            scale: WindowScale::X2,
            target_fps: 60.0,
            resizable: false,
        }
    }
}

/// Abstracción de escalas para no depender directamente de los enums de minifb fuera de esta lib
#[derive(Debug, Clone, Copy)]
pub enum WindowScale {
    X1, X2, X4, X8, FitScreen,
}

impl From<WindowScale> for Scale {
    fn from(s: WindowScale) -> Self {
        match s {
            WindowScale::X1 => Scale::X1,
            WindowScale::X2 => Scale::X2,
            WindowScale::X4 => Scale::X4,
            WindowScale::X8 => Scale::X8,
            WindowScale::FitScreen => Scale::FitScreen,
        }
    }
}

// ============================================================================
//  MOTOR DE DISPLAY (HOLY GRAIL ENGINE)
// ============================================================================

pub struct OxidDisplay {
    window: Window,
    
    // Dimensiones nativas del sistema emulado (ej. 320x224 para Genesis)
    width: usize,
    height: usize,
    
    // Control de Tiempo (Frame Limiter)
    target_micro_seconds: u128,
    last_frame_time: Instant,
    
    // Performance stats
    pub fps: usize,
    frame_count: usize,
    last_fps_check: Instant,
}

impl OxidDisplay {
    /// Crea una nueva ventana lista para renderizar
    pub fn new(config: DisplayConfig) -> Self {
        let mut opts = WindowOptions::default();
        opts.scale = config.scale.into();
        opts.resize = config.resizable;
        opts.scale_mode = ScaleMode::AspectRatioStretch; // Mantiene aspect ratio al estirar

        let window = Window::new(
            &config.title,
            config.width,
            config.height,
            opts,
        ).expect("CRITICAL: No se pudo abrir la ventana de video (minifb failure)");

        // Configurar timing
        let target_us = if config.target_fps > 0.0 {
            (1_000_000.0 / config.target_fps) as u128
        } else {
            0 // Sin límite
        };

        Self {
            window,
            width: config.width,
            height: config.height,
            target_micro_seconds: target_us,
            last_frame_time: Instant::now(),
            fps: 0,
            frame_count: 0,
            last_fps_check: Instant::now(),
        }
    }

    /// El corazón del renderizado. Llama a esto una vez por frame del emulador.
    /// buffer: Slice de u32 en formato 0x00RRGGBB.
    pub fn update(&mut self, buffer: &[u32]) {
        // 1. Renderizar buffer a la ventana
        // minifb maneja el doble buffer internamente.
        self.window
            .update_with_buffer(buffer, self.width, self.height)
            .unwrap_or_else(|e| eprintln!("Display Error: {}", e));

        // 2. Frame Limiter (Sincronización)
        // Dormir si el emulador va más rápido que 60Hz (o la tasa target)
        if self.target_micro_seconds > 0 {
            let elapsed = self.last_frame_time.elapsed().as_micros();
            if elapsed < self.target_micro_seconds {
                let sleep_time = self.target_micro_seconds - elapsed;
                // Usamos spin-loop o sleep híbrido para precisión, 
                // pero thread::sleep es 'bueno' para no quemar CPU.
                if sleep_time > 1000 {
                    thread::sleep(Duration::from_micros((sleep_time - 500) as u64));
                }
                // Spin-wait para el último microsegundo preciso
                while self.last_frame_time.elapsed().as_micros() < self.target_micro_seconds {
                    std::hint::spin_loop();
                }
            }
        }
        self.last_frame_time = Instant::now();

        // 3. Calcular FPS reales
        self.frame_count += 1;
        if self.last_fps_check.elapsed().as_secs() >= 1 {
            self.fps = self.frame_count;
            self.frame_count = 0;
            self.last_fps_check = Instant::now();
            
            // Opcional: Actualizar título con FPS (útil para debug)
            // self.window.set_title(&format!("Oxide - FPS: {}", self.fps));
        }
    }

    /// Verifica si la ventana sigue abierta (para el loop principal)
    pub fn is_open(&self) -> bool {
        self.window.is_open() && !self.window.is_key_down(Key::Escape)
    }

    /// Cambiar título dinámicamente (ej. "Sonic 2 - 60 FPS")
    pub fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }
}

// ============================================================================
//  INPUT BRIDGE (Para Oxid_Input)
// ============================================================================

impl OxidDisplay {
    /// Devuelve las teclas presionadas crudas (para que oxid_input las mapee)
    pub fn get_keys(&self) -> Vec<Key> {
        self.window.get_keys()
    }

    /// Verifica una tecla específica (útil para debug rápido o hotkeys)
    pub fn is_key_down(&self, key: Key) -> bool {
        self.window.is_key_down(key)
    }
}

// ============================================================================
//  UTILIDADES DE PIXELES (HELPERS)
// ============================================================================

/// Convierte componentes RGB (0-255) a formato u32 compatible con minifb
#[inline(always)]
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Convierte un valor de 1 bit (0/1) a blanco/negro (Para Mac/Palm)
#[inline(always)]
pub fn mono(bit: bool) -> u32 {
    if bit { 0x000000 } else { 0xFFFFFF } // Negro : Blanco (o viceversa según sistema)
}