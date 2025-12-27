# Oxide Alloy - Estado del Proyecto

Bienvenido a **Oxide Alloy**, un emulador multi-sistema experimental escrito en Rust. Este proyecto busca implementar emuladores para sistemas basados en Z80 y Motorola 68000.

## Estado General
El proyecto está en desarrollo activo se encuentra en una fase "pre-alpha" para la mayoría de los sistemas.

### Componentes Principales (Cores)
*   **oxidz80**: Núcleo de CPU Z80.
    *   **Estado**: Avanzado.
    *   **Características**: Implementación completa de instrucciones (oficiales + indocumentadas), soporte de interrupciones (IM 0, 1, 2) y Block I/O (`LDIR`, `OTIR`).
    *   **Problemas Conocidos**: Problemas de temporización e integración de interrupciones en sistemas complejos (deadlocks en algunos escenarios).
*   **oxid68k**: Núcleo de CPU Motorola 68000.
    *   **Estado**: En desarrollo. Soporte básico de instrucciones.

## Estado de los Sistemas

### 1. Oxid Master (Sega Master System)
*   **Estado**: Ejecutable con fallos gráficos/lógicos.
*   **Rom Testeadas**: *Sonic The Hedgehog*, *Wonder Boy*.
*   **Funcionalidad**:
    *   **CPU**: Z80 funcionando.
    *   **VDP**: Renderizado por scanline implementado. Backgrounds se muestran correctamente.
    *   **Timing**: Implementado H-Counter ($7F) y V-Counter ($7E) para sincronización.
    *   **Input**: Implementado con soporte para Joypad 1 y 2 (y mirrors).
*   **Problemas Actuales**:
    *   **Juegos**: *Sonic* presenta fallos en la física (cae al vacío), sugiriendo problemas sutiles de sincronización o flags.
    *   **Sprites**: Renderizan pero no actualizan su posición correctamente (fallo en interrupciones VBlank o DMA).
    *   **Audio**: PSG no implementado (Stub).

### 2. Oxid Spec (ZX Spectrum)
*   **Estado**: Boot a BASIC (parcial).
*   **Funcionalidad**:
    *   Carga la BIOS del 48k.
    *   Muestra el logo y llega al prompt.
*   **Problemas Actuales**:
    *   **Input Deadlock**: El teclado funciona en menús iniciales pero deja de responder en el prompt de BASIC debido a un bloqueo en la rutina de escaneo de interrupciones (IFF1 falso persistentemente).

### 3. Oxid Genesis (Sega Mega Drive / Genesis)
*   **Estado**: Experimental / Stub.
*   **CPU**: Usa `oxid68k` (Main) y `oxidz80` (Sound).
*   **Estado**: Inicialización básica. No apto para juegos comerciales aún.

### 4. Oxid Mac (Macintosh 128k/Plus)
*   **Estado**: Experimental.
*   **CPU**: 68000.
*   **Objetivo**: Emular hardware básico de Macintosh clásico.

### 5. Oxid Palm (Palm Pilot)
*   **Estado**: Experimental.
*   **CPU**: 68000 / 68328 (Dragonball).
*   **Objetivo**: Cargar Palm OS 1.x/2.x.

## Cómo Ejecutar
Para correr un sistema específico, usa `cargo run`:

```bash
# Sega Master System
cargo run -p oxid_master "ruta/al/juego.sms"

# ZX Spectrum
cargo run -p oxid_spec -- -rom "ruta/a/48.rom"
```

## Próximos Pasos (Roadmap)
1.  **Debugging Z80 Interrupts**: Solucionar definitivamente el manejo de IRQ en `oxidz80` para estabilizar SMS y Spectrum.
2.  **VDP Timing**: Refinar ciclos por línea y estados de VBlank en Master System.
3.  **Audio**: Implementar SN76489 (PSG) para SMS.
4.  **Genesis**: Avanzar en la implementación del VDP de 16-bits.
