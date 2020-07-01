// Note: Game BoyTM, Game Boy PocketTM, Super Game BoyTM and Game Boy ColorTM are registered trademarks of
// Nintendo CO., LTD. © 1989 to 1999 by Nintendo CO., LTD.
extern crate crossterm;
extern crate crossterm_input;
use gameboy::apu::Apu;
use gameboy::gpu::{SCREEN_H, SCREEN_W};
use gameboy::motherboard::MotherBoard;
use std::cmp;
use std::thread;
use std::process::exit;
use blockish::{render_write_eol, current_terminal_is_supported};
use crossterm_input::{input, InputEvent, KeyEvent, RawScreen};
use crossterm::{terminal, cursor, execute};
use std::io::{stdout, Write};

fn main() {
    rog::reg("gameboy");
    rog::reg("gameboy::cartridge");

    let mut rom = String::from("");
    let mut c_audio = false;
    let mut c_terminal = false;
    let mut c_scale = 2;
    {
        let mut ap = argparse::ArgumentParser::new();
        ap.set_description("Gameboy emulator");
        ap.refer(&mut c_audio)
            .add_option(&["-a", "--enable-audio"],
                        argparse::StoreTrue,
                        "Enable audio");
        ap.refer(&mut c_terminal)
            .add_option(&["-t", "--terminal"],
                        argparse::StoreTrue,
                        "Render inside terminal");
        ap.refer(&mut c_scale)
            .add_option(&["-x", "--scale-factor"],
                        argparse::Store,
                        "Scale the video by a factor of 1, 2, 4, or 8");
        ap.refer(&mut rom).add_argument("rom", argparse::Store, "Rom name");
        ap.parse_args_or_exit();
    }

    let mut mbrd = MotherBoard::power_up(rom);
    let rom_name = mbrd.mmu.borrow().cartridge.title();

    let mut option = minifb::WindowOptions::default();
    option.resize = true;
    option.scale = match c_scale {
        1 => minifb::Scale::X1,
        2 => minifb::Scale::X2,
        4 => minifb::Scale::X4,
        8 => minifb::Scale::X8,
        _ => panic!("Supported scale: 1, 2, 4 or 8"),
    };
    let mut window_opt = None;
    if !c_terminal {
        if !current_terminal_is_supported() {
            println!("your terminal is not supported");
            exit(1);
        }
        window_opt = Some(minifb::Window::new(format!("Gameboy - {}", rom_name).as_str(), SCREEN_W, SCREEN_H, option).unwrap());
    }
    let mut window_buffer = vec![0x00; SCREEN_W * SCREEN_H];
    if let Some(window) = &mut window_opt {
        window.update_with_buffer(window_buffer.as_slice()).unwrap();
    }

    // Initialize audio related
    if c_audio {
        let device = cpal::default_output_device().unwrap();
        rog::debugln!("Open the audio player: {}", device.name());
        let format = device.default_output_format().unwrap();
        let format = cpal::Format {
            channels: 2,
            sample_rate: format.sample_rate,
            data_type: cpal::SampleFormat::F32,
        };

        let event_loop = cpal::EventLoop::new();
        let stream_id = event_loop.build_output_stream(&device, &format).unwrap();
        event_loop.play_stream(stream_id);

        let apu = Apu::power_up(format.sample_rate.0);
        let apu_data = apu.buffer.clone();
        mbrd.mmu.borrow_mut().apu = Some(apu);

        thread::spawn(move || {
            event_loop.run(move |_, stream_data| {
                let mut apu_data = apu_data.lock().unwrap();
                if let cpal::StreamData::Output { buffer } = stream_data {
                    let len = cmp::min(buffer.len() / 2, apu_data.len());
                    match buffer {
                        cpal::UnknownTypeOutputBuffer::F32(mut buffer) => {
                            for (i, (data_l, data_r)) in apu_data.drain(..len).enumerate() {
                                buffer[i * 2] = data_l;
                                buffer[i * 2 + 1] = data_r;
                            }
                        }
                        cpal::UnknownTypeOutputBuffer::U16(mut buffer) => {
                            for (i, (data_l, data_r)) in apu_data.drain(..len).enumerate() {
                                buffer[i * 2] =
                                    (data_l * f32::from(std::i16::MAX) + f32::from(std::u16::MAX) / 2.0) as u16;
                                buffer[i * 2 + 1] =
                                    (data_r * f32::from(std::i16::MAX) + f32::from(std::u16::MAX) / 2.0) as u16;
                            }
                        }
                        cpal::UnknownTypeOutputBuffer::I16(mut buffer) => {
                            for (i, (data_l, data_r)) in apu_data.drain(..len).enumerate() {
                                buffer[i * 2] = (data_l * f32::from(std::i16::MAX)) as i16;
                                buffer[i * 2 + 1] = (data_r * f32::from(std::i16::MAX)) as i16;
                            }
                        }
                    }
                }
            });
        });
    }
    let mut term_width = 20 * 8;
    let mut term_height = 20 * 8;
    let _screen = RawScreen::into_raw_mode();
    let input = input();
    let mut reader = input.read_async();
    match crossterm::terminal::size() {
        Ok(res) => {
            term_width = res.0 as u32 * 8;
            term_height = res.1 as u32 * 8 * 2;
        }
        Err(_) => {
        } }


    match window_opt {
        None => {let _ = execute!(stdout(),terminal::EnterAlternateScreen);},
        Some(_) => {},
    }
    loop {
        // Stop the program, if the GUI is closed by the user
        if let Some(window) = &mut window_opt {
            if !window.is_open() {
                break;
            }
        }
        // Execute an instruction
        mbrd.next();

        // Update the window
        if mbrd.check_and_reset_gpu_updated() {
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
            let original_width = SCREEN_W as u32;
            let original_height = SCREEN_H as u32;
            if let Some(window) = &mut window_opt {
                window.update_with_buffer(window_buffer.as_slice()).unwrap();
            }
            else {
                let _ = execute!(stdout(),cursor::MoveTo(0,0));
                render_write_eol(term_width, term_height, &|x, y| {
                    let start = (y * original_height / term_height * original_width
                                + (x * original_width / term_width)) as usize;
                    let pixel = window_buffer[start];
                    ((pixel >> 16 & 0xff) as u8, (pixel >> 8 & 0xff) as u8, (pixel & 0xff) as u8)
                }, false);
            }
        }

        if !mbrd.cpu.flip() {
            continue;
        }

        // Handling keyboard events
        if let Some(window) = &mut window_opt {
            if window.is_key_down(minifb::Key::Escape) {
                break;
            }
        }

        if let Some(window) = &mut window_opt {
            let keys = vec![(minifb::Key::Right, gameboy::joypad::JoypadKey::Right),
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
                    mbrd.mmu.borrow_mut().joypad.keydown(vk.clone());
                } else {
                    mbrd.mmu.borrow_mut().joypad.keyup(vk.clone());
                }
            }
        }
        else {
            let keys = vec![
                (KeyEvent::Right, gameboy::joypad::JoypadKey::Right),
                (KeyEvent::Up, gameboy::joypad::JoypadKey::Up),
                (KeyEvent::Left, gameboy::joypad::JoypadKey::Left),
                (KeyEvent::Down, gameboy::joypad::JoypadKey::Down),
                (KeyEvent::Char('z'), gameboy::joypad::JoypadKey::A),
                (KeyEvent::Char('x'), gameboy::joypad::JoypadKey::B),
                (KeyEvent::Char(' '), gameboy::joypad::JoypadKey::Select),
                (KeyEvent::Enter, gameboy::joypad::JoypadKey::Start),
            ];
                let option_event =  reader.next();
                if Some(InputEvent::Keyboard(KeyEvent::Esc)) == option_event {
                    break;
                }
                for (rk, vk) in &keys {
                    if Some(InputEvent::Keyboard(rk.clone())) == option_event {
                        mbrd.mmu.borrow_mut().joypad.keydown(vk.clone());
                    } else {
                        mbrd.mmu.borrow_mut().joypad.keyup(vk.clone());
                    }
                }
        }
    }

    mbrd.mmu.borrow_mut().cartridge.sav();
    match window_opt {
        None => {let _ = execute!(stdout(),terminal::LeaveAlternateScreen);},
        Some(_) => {},
    }
}
