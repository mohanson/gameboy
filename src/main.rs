// Note: Game BoyTM, Game Boy PocketTM, Super Game BoyTM and Game Boy ColorTM are registered trademarks of
// Nintendo CO., LTD. © 1989 to 1999 by Nintendo CO., LTD.
use cpal::Sample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use gameboy::apu::Apu;
use gameboy::convention::{Memory, STEP_CYCLES, Stable};
use gameboy::gameboy::GameBoy;
use gameboy::gpu::{SCREEN_H, SCREEN_W};
use std::io::Write;

struct Argument {
    audio: bool,
    mode: String,
    rom: String,
    scale: u32,
    speed: u32,
}

fn main() {
    rog::reg("gameboy");
    rog::reg("gameboy::cartridge");
    rog::reg("gameboy::mmunit");
    let mut argu = Argument { audio: false, mode: String::from("minifb"), rom: String::from(""), scale: 2, speed: 1 };
    {
        let mut ap = argparse::ArgumentParser::new();
        ap.set_description("Gameboy emulator");
        ap.refer(&mut argu.audio).add_option(&["-a", "--enable-audio"], argparse::StoreTrue, "Enable audio");
        ap.refer(&mut argu.mode).add_option(
            &["-m", "--mode"],
            argparse::Store,
            "Set the emulator mode (blargg-memory-output, blargg-serial-output, minifb, mts)",
        );
        ap.refer(&mut argu.scale).add_option(
            &["-x", "--scale-factor"],
            argparse::Store,
            "Scale the video by a factor of 1, 2, 4, or 8",
        );
        ap.refer(&mut argu.speed).add_option(
            &["-s", "--speed-factor"],
            argparse::Store,
            "Set the emulator speed (1 for normal speed, 2 for double speed, etc.)",
        );
        ap.refer(&mut argu.rom).add_argument("rom", argparse::Store, "Rom name");
        ap.parse_args_or_exit();
    }

    match argu.mode.as_str() {
        "blargg-memory-output" => mode_blargg_memory_output(&argu),
        "blargg-serial-output" => mode_blargg_serial_output(&argu),
        "minifb" => mode_minifb(&argu),
        "mts" => mode_mts(&argu),
        _ => unreachable!(),
    }
}

fn mode_blargg_serial_output(argu: &Argument) {
    let mut mbrd = GameBoy::power_up(&argu.rom);
    mbrd.spd = argu.speed;
    let mut buff = String::new();
    loop {
        mbrd.step();
        if mbrd.mmu.borrow().serial.ctrl == 0x81 {
            print!("{}", char::from(mbrd.mmu.borrow().serial.data));
            buff.push(char::from(mbrd.mmu.borrow().serial.data));
            // Clear the transfer start flag to indicate that the transfer is complete.
            mbrd.mmu.borrow_mut().serial.ctrl = 0x01;
            std::io::stdout().flush().unwrap();
            if buff.contains("Passed") {
                print!("\n");
                std::process::exit(0);
            }
            if buff.contains("Failed") {
                print!("\n");
                std::process::exit(1);
            }
        }
    }
}

fn mode_blargg_memory_output(argu: &Argument) {
    let mut mbrd = GameBoy::power_up(&argu.rom);
    mbrd.spd = argu.speed;
    loop {
        mbrd.step();
        let a = [mbrd.mmu.borrow().lb(0xa001), mbrd.mmu.borrow().lb(0xa002), mbrd.mmu.borrow().lb(0xa003)];
        let b = [0xde, 0xb0, 0x61];
        if a == b {
            break;
        }
    }
    loop {
        mbrd.step();
        if mbrd.mmu.borrow().lb(0xa000) == 0x80 {
            break;
        }
    }
    let mut i: usize = 0;
    loop {
        mbrd.step();
        let ch = mbrd.mmu.borrow().lb(0xa004 + i as u16);
        if ch != 0 {
            print!("{}", char::from(ch));
            i += 1;
        }
        let ex = mbrd.mmu.borrow().lb(0xa000);
        if ex != 0x80 {
            std::io::stdout().flush().unwrap();
            std::process::exit(ex as i32);
        }
    }
}

fn mode_minifb(argu: &Argument) {
    let mut mbrd = GameBoy::power_up(&argu.rom);
    mbrd.spd = argu.speed;
    let rom_name = mbrd.mmu.borrow().cartridge.title.clone();

    let mut option = minifb::WindowOptions::default();
    option.resize = true;
    option.scale = match argu.scale {
        1 => minifb::Scale::X1,
        2 => minifb::Scale::X2,
        4 => minifb::Scale::X4,
        8 => minifb::Scale::X8,
        _ => panic!("Supported scale: 1, 2, 4 or 8"),
    };
    let mut window =
        minifb::Window::new(format!("Gameboy - {}", rom_name).as_str(), SCREEN_W, SCREEN_H, option).unwrap();
    let mut window_buffer = vec![0x00; SCREEN_W * SCREEN_H];
    window.update_with_buffer(window_buffer.as_slice(), SCREEN_W, SCREEN_H).unwrap();

    // Initialize audio related. It is necessary to ensure that the stream object remains alive.
    let stream: cpal::Stream;
    if argu.audio {
        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        rog::debugln!("Open the audio player: {}", device.name().unwrap());
        let config = device.default_output_config().unwrap();
        let sample_format = config.sample_format();
        rog::debugln!("Sample format: {}", sample_format);
        let config: cpal::StreamConfig = config.into();
        rog::debugln!("Stream config: {:?}", config);

        let apu = Apu::power_up(config.sample_rate.0);
        let apu_data = apu.buffer.clone();
        mbrd.mmu.borrow_mut().apu = apu;

        stream = match sample_format {
            cpal::SampleFormat::F32 => device
                .build_output_stream(
                    &config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let len = std::cmp::min(data.len() / 2, apu_data.lock().unwrap().len());
                        for (i, (data_l, data_r)) in apu_data.lock().unwrap().drain(..len).enumerate() {
                            data[i * 2 + 0] = data_l;
                            data[i * 2 + 1] = data_r;
                        }
                    },
                    move |err| rog::debugln!("{}", err),
                    None,
                )
                .unwrap(),
            cpal::SampleFormat::F64 => device
                .build_output_stream(
                    &config,
                    move |data: &mut [f64], _: &cpal::OutputCallbackInfo| {
                        let len = std::cmp::min(data.len() / 2, apu_data.lock().unwrap().len());
                        for (i, (data_l, data_r)) in apu_data.lock().unwrap().drain(..len).enumerate() {
                            data[i * 2 + 0] = data_l.to_sample::<f64>();
                            data[i * 2 + 1] = data_r.to_sample::<f64>();
                        }
                    },
                    move |err| rog::debugln!("{}", err),
                    None,
                )
                .unwrap(),
            _ => panic!("unreachable"),
        };
        stream.play().unwrap();
    }
    let _ = stream;

    let mut cycles = 0;

    loop {
        // Stop the program, if the GUI is closed by the user
        if !window.is_open() {
            break;
        }

        // Execute an instruction
        cycles += mbrd.step();

        // Update the window
        if mbrd.mmu.borrow_mut().gpu.check_and_reset_gpu_updated() {
            let mut i: usize = 0;
            for l in mbrd.mmu.borrow().gpu.data.iter() {
                for w in l.iter() {
                    let b = u32::from(w[0]) << 16;
                    let g = u32::from(w[1]) << 8;
                    let r = u32::from(w[2]);
                    let a = 0xff00_0000;

                    window_buffer[i] = a | b | g | r;
                    i += 1;
                }
            }
            window.update_with_buffer(window_buffer.as_slice(), SCREEN_W, SCREEN_H).unwrap();
        }

        if cycles < STEP_CYCLES {
            continue;
        }
        cycles -= STEP_CYCLES;

        // Handling keyboard events
        if window.is_key_down(minifb::Key::Escape) {
            break;
        }
        let keys = vec![
            (minifb::Key::Right, gameboy::joypad::JoypadKey::Right),
            (minifb::Key::Up, gameboy::joypad::JoypadKey::Up),
            (minifb::Key::Left, gameboy::joypad::JoypadKey::Left),
            (minifb::Key::Down, gameboy::joypad::JoypadKey::Down),
            (minifb::Key::Z, gameboy::joypad::JoypadKey::A),
            (minifb::Key::X, gameboy::joypad::JoypadKey::B),
            (minifb::Key::Space, gameboy::joypad::JoypadKey::Select),
            (minifb::Key::Enter, gameboy::joypad::JoypadKey::Start),
        ];
        for (rk, vk) in &keys {
            if window.is_key_down(*rk) {
                mbrd.mmu.borrow_mut().joypad.key_down(vk.clone());
            } else {
                mbrd.mmu.borrow_mut().joypad.key_free(vk.clone());
            }
        }
    }

    mbrd.mmu.borrow_mut().cartridge.save();
}

fn mode_mts(argu: &Argument) {
    let mut mbrd = GameBoy::power_up(&argu.rom);
    mbrd.spd = argu.speed;
    let passed = [0x03, 0x05, 0x08, 0x0d, 0x15, 0x22];
    let failed = [0x42, 0x42, 0x42, 0x42, 0x42, 0x42];
    loop {
        mbrd.step();
        let reg = &mbrd.cpu.reg;
        let sig = [reg.b, reg.c, reg.d, reg.e, reg.h, reg.l];
        if sig != passed && sig != failed {
            continue;
        }
        let pc = reg.pc;
        if mbrd.mmu.borrow().lb(pc) != 0x18 || mbrd.mmu.borrow().lb(pc.wrapping_add(1)) != 0xfe {
            continue;
        }
        if sig == passed {
            rog::println!("Passed");
            std::process::exit(0);
        } else {
            rog::println!("Failed");
            std::process::exit(1);
        }
    }
}
