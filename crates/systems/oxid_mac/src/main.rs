// crates/systems/oxid_mac/src/main.rs - Macintosh Emulator
mod bus;
mod memory;
mod via;
mod video;

use crate::bus::MacBus;
use crate::video::{MacVideo, SCREEN_HEIGHT, SCREEN_WIDTH};
use minifb::{Key, Window, WindowOptions};
use oxid68k::Oxid68k;
use oxide_core::Cpu;
use std::env;
use std::fs;
use std::time::Duration;

fn detect_model(rom_size: usize) -> (&'static str, usize) {
    match rom_size {
        0..=65536 => ("Macintosh 128K/512K", 512 * 1024),
        65537..=131072 => ("Macintosh Plus", 4 * 1024 * 1024), // Default to 4MB to avoid sizing issues and Sad Mac 03FFFF
        _ => ("Macintosh SE/Classic", 4 * 1024 * 1024),
    }
}

fn get_video_base(ram_size: usize) -> usize {
    match ram_size {
        0x20000 => 0x1A700,
        0x80000 => 0x7A700,
        0x100000 => 0xFA700,
        0x400000 => 0x3FA700,
        _ => ram_size - 0x5900,
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    println!("╔══════════════════════════════════════════╗");
    println!("║     Oxide-Mac - Macintosh Emulator       ║");
    println!("╚══════════════════════════════════════════╝");

    if args.len() < 2 {
        println!("Usage: oxid_mac <path_to_mac_rom>");
        return;
    }

    let rom_path = &args[1];
    let rom_data = match fs::read(rom_path) {
        Ok(data) => data,
        Err(e) => {
            println!("Error reading ROM file: {}", e);
            return;
        }
    };

    if rom_data.len() > 0x400000 {
        println!("Error: ROM too large (max 4MB)");
        return;
    }

    let (model_name, ram_size) = detect_model(rom_data.len());
    let video_base = get_video_base(ram_size);

    println!(
        "ROM: {} bytes | Model: {} | RAM: {}KB | Video: 0x{:06X}",
        rom_data.len(),
        model_name,
        ram_size / 1024,
        video_base
    );

    let mut bus = MacBus::new(rom_data, ram_size);
    let mut cpu = Oxid68k::new();
    let video = MacVideo::new();

    cpu.reset_with_bus(&mut bus);
    println!("Reset: PC={:08X} SP={:08X}", cpu.pc(), cpu.a[7]);

    // TRACE: First 500 instructions to verify boot progress
    println!("\n=== TRACE (first 500 instructions) ===");
    let mut last_overlay = bus.rom_overlay;
    for i in 0..500 {
        let pc = cpu.pc();
        // Just execute, don't flood log unless overlay changes
        cpu.step(&mut bus);

        // Detect overlay change
        if bus.rom_overlay != last_overlay {
            println!(
                ">>> OVERLAY CHANGED at instruction {} PC={:08X}: {} -> {}",
                i, pc, last_overlay, bus.rom_overlay
            );
            last_overlay = bus.rom_overlay;
        }
    }
    println!("=== END INITIAL TRACE ===\n");

    let mut window = Window::new(
        &format!("Oxide-Mac - {}", model_name),
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
        WindowOptions {
            scale: minifb::Scale::X2,
            ..Default::default()
        },
    )
    .expect("Unable to create window");

    window.limit_update_rate(Some(Duration::from_micros(16600)));

    let mut frame_buffer = vec![0u32; SCREEN_WIDTH * SCREEN_HEIGHT];
    let cycles_per_frame = 133_333u32;
    let mut frame_count = 0u64;

    println!("--- Running (D=debug, V=vram, R=regs, ESC=quit) ---");

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let mut cycles = 0u32;
        while cycles < cycles_per_frame {
            let step_cycles = if cpu.stopped || cpu.halted {
                4
            } else {
                cpu.step(&mut bus)
            };
            cycles += step_cycles;

            // Tick VIA timers
            if bus.via.tick(step_cycles) {
                // VIA wants to fire IRQ (level 1)
                cpu.trigger_interrupt(1);
            }
        }

        // VBLANK interrupt (level 1) every frame
        // VBLANK interrupt (level 1) every frame
        // Set VIA interrupt flag for CA1 (VBLANK)
        let current_ifr = bus.via.ifr.get();
        bus.via.ifr.set(current_ifr | 0x02); // CA1 flag
        if bus.via.ier & 0x02 != 0 {
            cpu.trigger_interrupt(1);
        }

        frame_count += 1;

        // Diagnostic every 60 frames (1 second)
        if frame_count % 60 == 0 {
            let slice = bus.ram.dma_slice();
            let nz = slice[video_base..video_base + 21888]
                .iter()
                .filter(|&&b| b != 0)
                .count();
            println!(
                "[F{}] PC={:08X} overlay={} VRAM_nz={}/21888",
                frame_count,
                cpu.pc(),
                bus.rom_overlay,
                nz
            );
        }

        if window.is_key_pressed(Key::D, minifb::KeyRepeat::No) {
            let op = bus.read_u16(cpu.pc());
            println!(
                "[F{}] PC={:08X} SR={:04X} OP={:04X} OVL={}",
                frame_count,
                cpu.pc(),
                cpu.sr.to_u16(),
                op,
                bus.rom_overlay
            );
        }

        if window.is_key_pressed(Key::R, minifb::KeyRepeat::No) {
            println!(
                "D: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
                cpu.d[0], cpu.d[1], cpu.d[2], cpu.d[3], cpu.d[4], cpu.d[5], cpu.d[6], cpu.d[7]
            );
            println!(
                "A: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
                cpu.a[0], cpu.a[1], cpu.a[2], cpu.a[3], cpu.a[4], cpu.a[5], cpu.a[6], cpu.a[7]
            );
        }

        if window.is_key_pressed(Key::V, minifb::KeyRepeat::No) {
            let slice = bus.ram.dma_slice();
            println!(
                "VRAM@{:06X}: {:02X}{:02X}{:02X}{:02X}...",
                video_base,
                slice[video_base],
                slice[video_base + 1],
                slice[video_base + 2],
                slice[video_base + 3]
            );
            let nz = slice[video_base..video_base + 21888]
                .iter()
                .filter(|&&b| b != 0)
                .count();
            println!("Non-zero: {}/21888", nz);
        }

        video.render_screen(
            &bus.ram.dma_slice()[video_base..video_base + 21888],
            &mut frame_buffer,
        );
        window
            .update_with_buffer(&frame_buffer, SCREEN_WIDTH, SCREEN_HEIGHT)
            .unwrap();
    }
    println!("Done. {} frames.", frame_count);
}
