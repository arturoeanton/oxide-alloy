use bitflags::bitflags;
use minifb::{Key, MouseMode, Window};
use std::collections::HashMap;

// ============================================================================
//  DEFINICIÓN DE CONTROLADOR UNIVERSAL (RETROPAD)
// ============================================================================

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct GamepadButtons: u16 {
        const UP       = 1 << 0;
        const DOWN     = 1 << 1;
        const LEFT     = 1 << 2;
        const RIGHT    = 1 << 3;
        const A        = 1 << 4; // Botón principal (Action/Confirm)
        const B        = 1 << 5; // Botón secundario (Back/Cancel)
        const X        = 1 << 6;
        const Y        = 1 << 7;
        const START    = 1 << 8;
        const SELECT   = 1 << 9; // O "Mode" en Genesis
        const L1       = 1 << 10;
        const R1       = 1 << 11;
    }
}

// ============================================================================
//  ESTADO DEL MOUSE
// ============================================================================

#[derive(Debug, Clone, Copy, Default)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

// ============================================================================
//  GESTOR DE INPUT (INPUT MANAGER)
// ============================================================================

pub struct OxidInput {
    // Estado actual de los dispositivos virtuales
    pub player1: GamepadButtons,
    pub player2: GamepadButtons,
    pub mouse: MouseState,

    // Configuración de Mapeo (Teclado -> Botón Virtual)
    key_map_p1: HashMap<Key, GamepadButtons>,
    key_map_p2: HashMap<Key, GamepadButtons>,
}

impl OxidInput {
    /// Crea un nuevo gestor de entrada con un mapeo por defecto inteligente
    pub fn new() -> Self {
        let mut input = Self {
            player1: GamepadButtons::empty(),
            player2: GamepadButtons::empty(),
            mouse: MouseState::default(),
            key_map_p1: HashMap::new(),
            key_map_p2: HashMap::new(),
        };
        input.load_default_mapping();
        input
    }

    /// Carga un esquema de controles estándar (WASD/Flechas)
    fn load_default_mapping(&mut self) {
        // --- Jugador 1 (Teclado: Flechas + Z/X/A/S/Enter) ---
        self.map_p1(Key::Up, GamepadButtons::UP);
        self.map_p1(Key::Down, GamepadButtons::DOWN);
        self.map_p1(Key::Left, GamepadButtons::LEFT);
        self.map_p1(Key::Right, GamepadButtons::RIGHT);
        
        self.map_p1(Key::X, GamepadButtons::A);      // Genesis A / SNES B
        self.map_p1(Key::Z, GamepadButtons::B);      // Genesis B / SNES Y
        self.map_p1(Key::C, GamepadButtons::X);      // Genesis C / SNES A
        self.map_p1(Key::A, GamepadButtons::Y);      // SNES X
        
        self.map_p1(Key::Enter, GamepadButtons::START);
        self.map_p1(Key::RightShift, GamepadButtons::SELECT);
        
        // Teclas extra para Spectrum (Mapeo rápido de prueba)
        self.map_p1(Key::Space, GamepadButtons::A); // Space suele ser Fire
    }

    /// Asocia una tecla física a un botón virtual del Jugador 1
    pub fn map_p1(&mut self, key: Key, button: GamepadButtons) {
        self.key_map_p1.insert(key, button);
    }

    /// El corazón del Input: Lee la ventana física y actualiza los estados virtuales
    pub fn update(&mut self, window: &Window) {
        // 1. Resetear estados
        self.player1 = GamepadButtons::empty();
        self.player2 = GamepadButtons::empty();

        // 2. Obtener teclas presionadas (FIXED for minifb 0.24)
        // window.get_keys() retorna Vec<Key> directamente, no Option.
        let keys = window.get_keys();
        
        for key in keys {
            // Chequear mapeo Jugador 1
            if let Some(btn) = self.key_map_p1.get(&key) {
                self.player1.insert(*btn);
            }
            // Chequear mapeo Jugador 2
            if let Some(btn) = self.key_map_p2.get(&key) {
                self.player2.insert(*btn);
            }
        }

        // 3. Actualizar Mouse
        if let Some((x, y)) = window.get_mouse_pos(MouseMode::Pass) {
            self.mouse.x = x;
            self.mouse.y = y;
            self.mouse.left = window.get_mouse_down(minifb::MouseButton::Left);
            self.mouse.right = window.get_mouse_down(minifb::MouseButton::Right);
            self.mouse.middle = window.get_mouse_down(minifb::MouseButton::Middle);
        }
    }
    
    /// Helper directo para verificar una tecla específica (bypass mapeo)
    /// Útil para emuladores de teclado completo como Spectrum
    pub fn is_key_down(&self, window: &Window, key: Key) -> bool {
        window.is_key_down(key)
    }
}

// ============================================================================
//  TRAIT PARA SISTEMAS (CONTRACT)
// ============================================================================

pub trait InputProvider {
    fn get_gamepad(&self, player: usize) -> GamepadButtons;
    fn get_mouse(&self) -> MouseState;
}

impl InputProvider for OxidInput {
    fn get_gamepad(&self, player: usize) -> GamepadButtons {
        match player {
            0 => self.player1,
            1 => self.player2,
            _ => GamepadButtons::empty(),
        }
    }

    fn get_mouse(&self) -> MouseState {
        self.mouse
    }
}