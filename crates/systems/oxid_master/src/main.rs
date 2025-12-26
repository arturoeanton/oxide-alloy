// crates/systems/oxid_master/src/main.rs
mod bus;
mod vdp;

use oxide_core::{Cpu, Rom};
use oxidz80::OxidZ80;
use crate::bus::MasterSystemBus;
use minifb::{Window, WindowOptions, Key};
use std::env;

const WIDTH: usize = 256;
const HEIGHT: usize = 192;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: oxid_master <rom_path>");
        return;
    }

    let rom_path = &args[1];
    let rom = Rom::from_file(rom_path).expect("Failed to load ROM");
    
    let mut bus = MasterSystemBus::new(rom.data);
    let mut cpu = OxidZ80::new();
    cpu.reset();

    let mut window = Window::new(
        "Oxide-Master - Sonic The Hedgehog",
        WIDTH * 3,
        HEIGHT * 3,
        WindowOptions::default(),
    ).expect("Failed to create window");

    window.limit_update_rate(Some(std::time::Duration::from_micros(16666))); // ~60fps

    let mut frame_buffer = vec![0u32; WIDTH * HEIGHT];

    println!("SMS Emulator started with ROM: {}", rom_path);

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Actualizar input al inicio del frame (más responsivo)
        let mut pad = 0xFFu8;
        if window.is_key_down(Key::Up)    { pad &= !0x01; }
        if window.is_key_down(Key::Down)  { pad &= !0x02; }
        if window.is_key_down(Key::Left)  { pad &= !0x04; }
        if window.is_key_down(Key::Right) { pad &= !0x08; }
        if window.is_key_down(Key::Z)     { pad &= !0x10; } // Button 1
        if window.is_key_down(Key::X)     { pad &= !0x20; } // Button 2
        bus.joypad = pad;

        for y in 0..262 {
            // Execute cycles for one scanline: ~3.58MHz / 60 / 262 = ~228 cycles
            let mut cycles_this_line = 0;
            while cycles_this_line < 228 { 
                cycles_this_line += cpu.step(&mut bus);
            }

            if y < 192 {
                let mut line_buf = [0u32; WIDTH];
                bus.vdp.render_scanline(y, &mut line_buf);
                for x in 0..WIDTH {
                    frame_buffer[y * WIDTH + x] = line_buf[x];
                }
            }

            // V-Counter mapping for NTSC: 00-DA, then jumps to D5-FF
            let v_cnt = if y <= 218 {
                y as u8
            } else {
                (y as i32 - 6) as u8
            };
            bus.v_counter = v_cnt;

            bus.vdp.tick_scanline(y);
            if bus.vdp.is_interrupting() {
                cpu.irq(&mut bus, 0xFF);
            }
        }

        window.update_with_buffer(&frame_buffer, WIDTH, HEIGHT).unwrap();
    }
}